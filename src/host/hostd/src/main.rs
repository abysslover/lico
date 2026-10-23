//! The Lico session host (synthetic phase): owns agent processes, their output and
//! the durable log independently of any GUI, and serves protocol v1 on an
//! owner-only Unix socket.

mod payload;
mod session;

use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use session::{encode, rec_line, LogView, Session, State, Sub};

const MAX_REQUEST: usize = 4096;
const OPS: [&str; 7] = ["hello", "status", "snapshot", "subscribe", "unsubscribe", "session.create", "session.list"];
const REPLAY_CHUNK: usize = 256;

struct Config {
    log_dir: PathBuf,
    agent: PathBuf,
    retain: usize,
    window: usize,
    queue_bytes: usize,
    payload: usize,
}

struct Shared {
    cfg: Config,
    sessions: RwLock<Vec<Arc<Mutex<Session>>>>,
}

#[repr(C)]
struct Ucred {
    pid: i32,
    uid: u32,
    gid: u32,
}

extern "C" {
    fn getsockopt(fd: i32, level: i32, name: i32, value: *mut Ucred, len: *mut u32) -> i32;
    fn getuid() -> u32;
    fn umask(mask: u32) -> u32;
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
}

fn peer_is_owner(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred = Ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<Ucred>() as u32;
    // SOL_SOCKET = 1, SO_PEERCRED = 17 on Linux
    let rc = unsafe { getsockopt(stream.as_raw_fd(), 1, 17, &mut cred, &mut len) };
    rc == 0 && cred.uid == unsafe { getuid() }
}

fn random_id() -> String {
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("urandom");
    format!("s{}", payload::hex(&bytes))
}

// ------------------------------------------------------------------- agents
/// Start a synthetic agent owned by this host; it ends when the host does.
fn start_session(shared: &Arc<Shared>, producing: bool, rate: u64, history: u64, size: usize) -> Arc<Mutex<Session>> {
    let id = random_id();
    let session = Arc::new(Mutex::new(Session::new(id, producing, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window)));
    let mut command = Command::new(&shared.cfg.agent);
    command
        .args(["--history", &history.to_string(), "--rate", &rate.to_string(), "--payload", &size.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG, SIGKILL: no agent outlives its host
            Ok(())
        });
    }
    let mut child = command.spawn().expect("spawn the synthetic agent");
    let stdout = child.stdout.take().expect("agent stdout");
    let _keep_stdin = child.stdin.take();
    let owner = session.clone();
    thread::spawn(move || {
        let _stdin = _keep_stdin; // the pipe stays open for the life of the session
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(data) = payload::unhex(&line) {
                owner.lock().unwrap().append(data);
            }
        }
        let _ = child.wait();
        owner.lock().unwrap().state = State::Exited;
    });
    // the first records exist before the session is announced
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let ready = {
            let s = session.lock().unwrap();
            s.next_seq as u64 >= history || s.state == State::Exited
        };
        if ready {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    shared.sessions.write().unwrap().push(session.clone());
    session
}

// ----------------------------------------------------------------- protocol
fn reply_fields(rid: &Option<Value>, mut body: Map<String, Value>) -> Map<String, Value> {
    let mut all = Map::new();
    all.insert("ok".into(), json!(1));
    if let Some(id) = rid {
        all.insert("id".into(), id.clone());
    }
    all.append(&mut body);
    all
}

fn fail(sub: &Arc<Sub>, code: &str, rid: Option<&Value>) {
    let mut body = Map::new();
    body.insert("ok".into(), json!(0));
    body.insert("error".into(), json!(code));
    if let Some(Value::String(s)) = rid {
        if s.chars().count() <= 64 {
            body.insert("id".into(), Value::String(s.clone()));
        }
    }
    sub.push(encode(body));
}

fn int(map: &Map<String, Value>, key: &str) -> Option<i64> {
    map.get(key).and_then(Value::as_i64)
}

fn valid_str(value: &Value, max: usize) -> bool {
    matches!(value, Value::String(s) if s.chars().count() <= max)
}

fn in_range(value: &Value, min: i64, max: i64) -> bool {
    value.as_i64().map_or(false, |i| i >= min && i <= max)
}

/// Strict validation mirroring the v1 request schemas: unknown fields are refused.
fn valid_request(op: &str, request: &Map<String, Value>) -> bool {
    let required: &[&str] = match op {
        "hello" | "status" | "session.list" => &[],
        "snapshot" | "unsubscribe" => &["session"],
        "subscribe" => &["session", "after"],
        _ => &["kind"],
    };
    for (key, value) in request {
        let ok = match (op, key.as_str()) {
            (_, "op") => true,
            (_, "v") => value.as_i64() == Some(1),
            (_, "id") => valid_str(value, 64),
            ("snapshot" | "subscribe" | "unsubscribe", "session") => valid_str(value, 64),
            ("subscribe", "after") => in_range(value, -1, i64::MAX),
            ("snapshot" | "subscribe", "generation") => in_range(value, 1, i64::MAX),
            ("subscribe", "snapshot") => value.is_boolean(),
            ("session.create", "kind") => value.as_str() == Some("synthetic"),
            ("session.create", "rate") => in_range(value, 0, 10_000),
            ("session.create", "history") => in_range(value, 0, 100_000),
            ("session.create", "payload") => in_range(value, 1, 4096),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    required.iter().all(|key| request.contains_key(*key))
}

fn status_entry(s: &Session) -> Value {
    json!({
        "session": s.id, "generation": 1, "seq": s.last_seq(), "producing": s.producing,
        "subscribers": s.subs.len(), "bytes_total": s.bytes_total, "oldest": s.oldest(),
        "state": if s.state == State::Running { "running" } else { "exited" },
    })
}

fn handle(shared: &Arc<Shared>, sub: &Arc<Sub>, raw: &[u8]) {
    let parsed: Option<Value> = serde_json::from_slice(raw).ok();
    let Some(Value::Object(request)) = parsed else { return fail(sub, "bad_request", None) };
    let rid = request.get("id").cloned();
    if let Some(v) = request.get("v").and_then(Value::as_i64) {
        if v != 1 {
            return fail(sub, "version_unsupported", rid.as_ref());
        }
    }
    let Some(Value::String(op)) = request.get("op").cloned() else { return fail(sub, "bad_request", rid.as_ref()) };
    if !OPS.contains(&op.as_str()) {
        return fail(sub, "unknown_op", rid.as_ref());
    }
    if !valid_request(&op, &request) {
        return fail(sub, "bad_request", rid.as_ref());
    }
    let reply = |body: Value| sub.push(encode(reply_fields(&rid, body.as_object().cloned().unwrap_or_default())));
    match op.as_str() {
        "hello" => reply(json!({
            "protocol": 1, "host": "lico-host", "max_request": MAX_REQUEST, "retain": shared.cfg.retain,
            "ops": OPS,
        })),
        "status" => {
            let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
            let entries: Vec<Value> = list.iter().map(|s| status_entry(&s.lock().unwrap())).collect();
            reply(json!({"pid": std::process::id(), "sessions": entries}));
        }
        "session.list" => {
            let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
            let entries: Vec<Value> = list
                .iter()
                .map(|s| {
                    let s = s.lock().unwrap();
                    json!({"session": s.id, "generation": 1, "seq": s.last_seq(),
                           "state": if s.state == State::Running { "running" } else { "exited" }})
                })
                .collect();
            reply(json!({"sessions": entries}));
        }
        "session.create" => {
            let rate = int(&request, "rate").unwrap_or(0) as u64;
            let history = int(&request, "history").unwrap_or(0) as u64;
            let size = int(&request, "payload").unwrap_or(shared.cfg.payload as i64) as usize;
            let created = start_session(shared, rate > 0, rate, history, size);
            let id = created.lock().unwrap().id.clone();
            reply(json!({"session": id, "generation": 1}));
        }
        _ => session_op(shared, sub, &request, &op, &rid),
    }
}

fn session_op(shared: &Arc<Shared>, sub: &Arc<Sub>, request: &Map<String, Value>, op: &str, rid: &Option<Value>) {
    let reply = |body: Value| sub.push(encode(reply_fields(rid, body.as_object().cloned().unwrap_or_default())));
    let wanted = request.get("session").and_then(Value::as_str).unwrap_or_default();
    let found = shared.sessions.read().unwrap().iter().find(|s| s.lock().unwrap().id == wanted).cloned();
    let Some(session) = found else { return fail(sub, "unknown_session", rid.as_ref()) };
    if let Some(g) = int(request, "generation") {
        if g != 1 {
            return fail(sub, "stale_generation", rid.as_ref());
        }
    }
    let mut guard = session.lock().unwrap();
    match op {
        "snapshot" => reply(Value::Object(guard.snapshot())),
        "unsubscribe" => {
            guard.subs.retain(|s| !Arc::ptr_eq(s, sub));
            sub.clear_overflow(&guard.id);
            reply(json!({"session": guard.id, "seq": guard.last_seq()}));
        }
        _ => {
            let after = int(request, "after").unwrap_or(-1);
            let want_snapshot = request.get("snapshot").and_then(Value::as_bool) == Some(true);
            let (oldest, last) = (guard.oldest(), guard.last_seq());
            let id = guard.id.clone();
            let mut body = json!({"session": id, "generation": 1});
            let valid = oldest <= after + 1 && after <= last;
            if want_snapshot {
                // reactivation: the latest screen now, live records after it
                body["gap"] = json!(!valid);
                body["snapshot"] = Value::Object(guard.snapshot());
                body["from"] = json!(last + 1);
                body["missed"] = if valid { json!(last - after) } else { Value::Null };
                reply(body);
                sub.clear_overflow(&id);
                guard.subs.push(sub.clone());
            } else if !valid {
                body["gap"] = json!(true);
                body["snapshot"] = Value::Object(guard.snapshot());
                body["from"] = json!(last + 1);
                reply(body);
                sub.clear_overflow(&id);
                guard.subs.push(sub.clone());
            } else {
                body["gap"] = json!(false);
                body["from"] = json!(after + 1);
                reply(body);
                sub.clear_overflow(&id);
                drop(guard);
                replay(&session, sub, &id, after);
            }
        }
    }
}

/// Send every record after `cursor`, then join the live stream with no gap: older
/// records come from the log in chunks (outside the session lock), the recent ones
/// from the in-memory window, and the join happens under the lock.
fn replay(session: &Arc<Mutex<Session>>, sub: &Arc<Sub>, id: &str, mut cursor: i64) {
    loop {
        let (mem_start, view): (i64, LogView) = {
            let mut s = session.lock().unwrap();
            let mem_start = s.mem_start();
            if cursor + 1 >= mem_start {
                for (seq, data) in s.window_after(cursor) {
                    if sub.offer(id, rec_line(id, seq, &data)) {
                        return;
                    }
                }
                s.subs.push(sub.clone()); // live records follow with no gap
                return;
            }
            (mem_start, s.log_view())
        };
        if !sub.wait_room() {
            // a reader that does not drain a replay is told to resynchronise
            sub.push(encode(json!({"ev": "overflow", "session": id}).as_object().cloned().unwrap()));
            return;
        }
        let chunk = match view.read(cursor + 1, mem_start, REPLAY_CHUNK) {
            Ok(chunk) if !chunk.is_empty() => chunk,
            _ => {
                // the records were pruned while replaying: tell the reader to resynchronise
                sub.push(encode(json!({"ev": "overflow", "session": id}).as_object().cloned().unwrap()));
                return;
            }
        };
        for (seq, data) in chunk {
            if sub.offer(id, rec_line(id, seq, &data)) {
                return;
            }
            cursor = seq;
        }
    }
}

// -------------------------------------------------------------- connections
fn connection(stream: UnixStream, shared: Arc<Shared>) {
    if !peer_is_owner(&stream) {
        return;
    }
    let sub = Sub::new(shared.cfg.queue_bytes);
    let writer = {
        let sub = sub.clone();
        let mut out = stream.try_clone().expect("clone");
        thread::spawn(move || {
            while let Some(line) = sub.pop_wait() {
                if line.is_empty() {
                    continue;
                }
                if out.write_all(&line).is_err() {
                    return;
                }
            }
        })
    };
    let mut input = stream;
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let too_large = |sub: &Arc<Sub>| {
        let mut body = Map::new();
        body.insert("ok".into(), json!(0));
        body.insert("error".into(), json!("too_large"));
        sub.push(encode(body));
    };
    'read: loop {
        let n = match input.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&chunk[..n]);
        while let Some(pos) = pending.iter().position(|b| *b == b'\n') {
            let raw: Vec<u8> = pending.drain(..=pos).collect();
            if raw.len() > MAX_REQUEST {
                too_large(&sub);
                break 'read;
            }
            handle(&shared, &sub, &raw[..raw.len() - 1]);
        }
        if pending.len() > MAX_REQUEST {
            too_large(&sub);
            break;
        }
    }
    for session in shared.sessions.read().unwrap().iter() {
        session.lock().unwrap().subs.retain(|s| !Arc::ptr_eq(s, &sub));
    }
    thread::sleep(Duration::from_millis(20)); // let the writer flush a final error line
    sub.closed.store(true, Ordering::Relaxed);
    sub.wake.notify_one();
    let _ = writer.join();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, default: &str| args.windows(2).find(|w| w[0] == name).map_or(default.to_string(), |w| w[1].clone());
    let socket = arg("--socket", "");
    if socket.is_empty() {
        eprintln!("usage: lico-host --socket PATH [--log-dir DIR] [--agent PATH] [--sessions N] [--producing N] [--rate R] [--history N] [--retain N] [--window N] [--queue-bytes N] [--payload N]");
        std::process::exit(2);
    }
    let socket_dir = std::path::Path::new(&socket).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let log_dir = PathBuf::from(arg("--log-dir", socket_dir.join("lico-log").to_str().unwrap_or("lico-log")));
    let agent_default = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("lico-synth-agent"))).unwrap_or_default();
    let retain: usize = arg("--retain", "2048").parse().expect("retain");
    let cfg = Config {
        log_dir: log_dir.clone(),
        agent: PathBuf::from(arg("--agent", agent_default.to_str().unwrap_or(""))),
        retain,
        window: arg("--window", "64").parse::<usize>().expect("window").max(session::SNAPSHOT_LINES),
        queue_bytes: arg("--queue-bytes", "1048576").parse().expect("queue-bytes"),
        payload: arg("--payload", "1024").parse().expect("payload"),
    };
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::remove_dir_all(&log_dir); // a fresh log for each run of the synthetic host
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&log_dir).expect("log dir");
    }
    let sessions: usize = arg("--sessions", "1").parse().expect("sessions");
    let producing: usize = arg("--producing", "1").parse().expect("producing");
    let rate: u64 = arg("--rate", "100").parse::<f64>().expect("rate") as u64;
    let history: u64 = arg("--history", "5000").parse().expect("history");
    let size = cfg.payload;
    let shared = Arc::new(Shared { cfg, sessions: RwLock::new(Vec::new()) });
    for index in 0..sessions {
        let live = index < producing;
        start_session(&shared, live, if live { rate } else { 0 }, history, size);
    }
    let ids: Vec<String> = shared.sessions.read().unwrap().iter().map(|s| s.lock().unwrap().id.clone()).collect();
    let _ = std::fs::remove_file(&socket);
    let previous = unsafe { umask(0o077) }; // never group- or world-accessible, even briefly
    let listener = UnixListener::bind(&socket).expect("bind");
    unsafe { umask(previous) };
    println!("{}", json!({"ready": 1, "pid": std::process::id(), "sessions": ids}));
    let _ = std::io::stdout().flush();
    for stream in listener.incoming().flatten() {
        let shared = shared.clone();
        thread::spawn(move || connection(stream, shared));
    }
}
