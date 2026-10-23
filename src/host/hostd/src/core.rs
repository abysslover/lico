//! Host operations shared by the local IPC and the optional loopback HTTP listener.
//! Every function validates its target and returns either a reply body or a v1
//! error code; nothing here depends on how the request arrived.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::payload;
use crate::session::{obj, rec_line, InputState, LogView, Session, State, Sub};

pub const MAX_REQUEST: usize = 4096;
pub const OPS: [&str; 13] = [
    "hello", "status", "snapshot", "subscribe", "unsubscribe", "session.create", "session.list", "input",
    "input.status", "approval.answer", "view.attach", "view.detach", "resources",
];
pub const REPLAY_CHUNK: usize = 256;
const MAX_EVENTS: usize = 256;

pub struct Config {
    pub log_dir: PathBuf,
    pub agent: PathBuf,
    pub retain: usize,
    pub window: usize,
    pub queue_bytes: usize,
    pub payload: usize,
    pub max_sessions: usize,
}

pub struct ViewEntry {
    pub session: String,
    pub conn: Option<Arc<Sub>>,
    pub created: Instant,
}

pub struct Shared {
    pub cfg: Config,
    pub sessions: RwLock<Vec<Arc<Mutex<Session>>>>,
    pub views: Mutex<HashMap<String, ViewEntry>>,
    pub started: Instant,
}

pub type Reply = Result<Map<String, Value>, &'static str>;

extern "C" {
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).expect("urandom");
    payload::hex(&buf)
}

pub fn state_name(s: &Session) -> &'static str {
    if s.state == State::Running {
        "running"
    } else {
        "exited"
    }
}

// ------------------------------------------------------------------- agents
/// Start a synthetic agent owned by this host; it ends when the host does.
pub fn start_session(shared: &Arc<Shared>, producing: bool, rate: u64, history: u64, size: usize, stay: bool) -> Arc<Mutex<Session>> {
    let id = format!("s{}", random_hex(8));
    let session = Arc::new(Mutex::new(Session::new(id, producing, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window)));
    let mut command = Command::new(&shared.cfg.agent);
    command
        .args(["--history", &history.to_string(), "--rate", &rate.to_string(), "--payload", &size.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if stay {
        command.arg("--stay");
    }
    unsafe {
        command.pre_exec(|| {
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG, SIGKILL: no agent outlives its host
            Ok(())
        });
    }
    let mut child = command.spawn().expect("spawn the synthetic agent");
    let stdout = child.stdout.take().expect("agent stdout");
    let mut stdin = child.stdin.take().expect("agent stdin");
    let (tx, rx) = mpsc::channel::<String>();
    {
        let mut s = session.lock().unwrap();
        s.agent_pid = child.id();
        s.input_tx = Some(tx);
    }
    // a writer thread owns the agent's input pipe, so a stalled agent never blocks the host
    let writer_session = session.clone();
    thread::spawn(move || {
        for line in rx {
            let request_id = line.strip_prefix("I ").and_then(|rest| rest.split(' ').next()).map(str::to_string);
            if stdin.write_all(line.as_bytes()).and_then(|_| stdin.write_all(b"\n")).and_then(|_| stdin.flush()).is_err() {
                return;
            }
            if let Some(rid) = request_id {
                writer_session.lock().unwrap().advance_input(&rid, InputState::Submitted);
            }
        }
    });
    let owner = session.clone();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let mut parts = line.splitn(3, ' ');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("R"), Some(data), None) => {
                    if let Some(bytes) = payload::unhex(data) {
                        owner.lock().unwrap().append(bytes);
                    }
                }
                (Some("E"), Some(rid), None) => owner.lock().unwrap().advance_input(rid, InputState::Observed),
                (Some("A"), Some(n), Some(prompt)) => {
                    if let Ok(number) = n.parse::<i64>() {
                        owner.lock().unwrap().add_approval(number, prompt.to_string());
                    }
                }
                _ => {}
            }
        }
        let _ = child.wait();
        let mut s = owner.lock().unwrap();
        s.state = State::Exited;
        s.input_tx = None;
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

pub fn find(shared: &Shared, wanted: &str) -> Option<Arc<Mutex<Session>>> {
    shared.sessions.read().unwrap().iter().find(|s| s.lock().unwrap().id == wanted).cloned()
}

fn check_generation(generation: Option<i64>) -> Result<(), &'static str> {
    match generation {
        Some(g) if g != 1 => Err("stale_generation"),
        _ => Ok(()),
    }
}

// --------------------------------------------------------------- operations
pub fn hello(shared: &Shared) -> Reply {
    Ok(obj(json!({"protocol": 1, "host": "lico-host", "max_request": MAX_REQUEST, "retain": shared.cfg.retain, "ops": OPS})))
}

fn status_entry(s: &Session) -> Value {
    let mut entry = json!({
        "session": s.id, "generation": 1, "seq": s.last_seq(), "producing": s.producing,
        "subscribers": s.subs.len(), "bytes_total": s.bytes_total, "oldest": s.oldest(), "state": state_name(s),
    });
    if !s.approvals.is_empty() {
        entry["approvals"] = Value::Array(s.approvals.iter().map(|(n, p)| json!({"approval_id": n, "prompt": p})).collect());
    }
    entry
}

pub fn status(shared: &Shared) -> Reply {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let entries: Vec<Value> = list.iter().map(|s| status_entry(&s.lock().unwrap())).collect();
    Ok(obj(json!({"pid": std::process::id(), "sessions": entries})))
}

pub fn session_one(shared: &Shared, session: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    Ok(obj(status_entry(&s)))
}

pub fn session_list(shared: &Shared) -> Reply {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let entries: Vec<Value> = list
        .iter()
        .map(|s| {
            let s = s.lock().unwrap();
            json!({"session": s.id, "generation": 1, "seq": s.last_seq(), "state": state_name(&s)})
        })
        .collect();
    Ok(obj(json!({"sessions": entries})))
}

pub fn session_create(shared: &Arc<Shared>, rate: u64, history: u64, size: usize) -> Reply {
    if shared.sessions.read().unwrap().len() >= shared.cfg.max_sessions {
        return Err("busy"); // a bounded number of sessions, and so of agent processes
    }
    let created = start_session(shared, rate > 0, rate, history, size, false);
    let id = created.lock().unwrap().id.clone();
    Ok(obj(json!({"session": id, "generation": 1})))
}

pub fn snapshot(shared: &Shared, session: &str, generation: Option<i64>) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    check_generation(generation)?;
    let s = found.lock().unwrap();
    Ok(s.snapshot())
}

/// An input is dispatched at most once per request id: a repeat returns its state.
pub fn input(shared: &Shared, session: &str, generation: i64, request_id: &str, text: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    check_generation(Some(generation))?;
    let mut s = found.lock().unwrap();
    if let Some(state) = s.input_state(request_id) {
        return Ok(obj(json!({"session": s.id, "request_id": request_id, "state": state.name(), "duplicate": true})));
    }
    if s.state == State::Exited || s.input_tx.is_none() {
        return Err("session_exited");
    }
    s.record_input(request_id);
    let line = format!("I {request_id} {}", payload::hex(text.as_bytes()));
    if s.input_tx.as_ref().map_or(true, |tx| tx.send(line).is_err()) {
        return Err("session_exited");
    }
    let state = s.input_state(request_id).unwrap_or(InputState::Accepted);
    Ok(obj(json!({"session": s.id, "request_id": request_id, "state": state.name(), "duplicate": false})))
}

pub fn input_status(shared: &Shared, session: &str, request_id: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    let state = s.input_state(request_id).ok_or("unknown_request")?;
    Ok(obj(json!({"session": s.id, "request_id": request_id, "state": state.name()})))
}

pub fn approval_answer(shared: &Shared, session: &str, generation: i64, approval_id: i64, answer: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    check_generation(Some(generation))?;
    let mut s = found.lock().unwrap();
    let position = s.approvals.iter().position(|(n, _)| *n == approval_id).ok_or("unknown_approval")?;
    if s.input_tx.as_ref().map_or(true, |tx| tx.send(format!("P {approval_id} {answer}")).is_err()) {
        return Err("session_exited");
    }
    s.approvals.remove(position);
    Ok(obj(json!({"session": s.id, "approval_id": approval_id, "state": "answered"})))
}

/// Attach a view: the latest screen now and, for a connected view, live records after it.
pub fn view_attach(shared: &Shared, conn: Option<&Arc<Sub>>, session: &str, generation: Option<i64>, after: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    check_generation(generation)?;
    let mut s = found.lock().unwrap();
    let (oldest, last) = (s.oldest(), s.last_seq());
    let valid = oldest <= after + 1 && after <= last;
    let view = format!("w{}", random_hex(8));
    let mut body = obj(json!({"view": view, "session": s.id, "generation": 1, "gap": !valid, "from": last + 1}));
    body.insert("snapshot".into(), Value::Object(s.snapshot()));
    body.insert("missed".into(), if valid { json!(last - after) } else { Value::Null });
    if let Some(sub) = conn {
        sub.clear_overflow(&s.id);
        s.subs.push(sub.clone());
    }
    {
        // a view that has no connection (the HTTP listener) expires if it is never detached
        let mut views = shared.views.lock().unwrap();
        views.retain(|_, v| v.conn.is_some() || v.created.elapsed() < Duration::from_secs(600));
    }
    shared.views.lock().unwrap().insert(view, ViewEntry { session: s.id.clone(), conn: conn.cloned(), created: Instant::now() });
    Ok(body)
}

pub fn view_detach(shared: &Shared, view: &str) -> Reply {
    let entry = shared.views.lock().unwrap().remove(view).ok_or("unknown_view")?;
    let found = find(shared, &entry.session).ok_or("unknown_session")?;
    let mut s = found.lock().unwrap();
    if let Some(sub) = &entry.conn {
        s.subs.retain(|x| !Arc::ptr_eq(x, sub));
        sub.clear_overflow(&s.id);
    }
    Ok(obj(json!({"view": view, "session": s.id, "seq": s.last_seq()})))
}

/// Views of a vanished connection are removed; the sessions and their agents go on.
pub fn drop_connection(shared: &Shared, conn: &Arc<Sub>) {
    shared.views.lock().unwrap().retain(|_, v| v.conn.as_ref().map_or(true, |c| !Arc::ptr_eq(c, conn)));
    for session in shared.sessions.read().unwrap().iter() {
        session.lock().unwrap().subs.retain(|s| !Arc::ptr_eq(s, conn));
    }
}

fn proc_kb(path: &str, key: &str) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.lines().find(|l| l.starts_with(key))?.split_whitespace().nth(1)?.parse().ok()
}

pub fn resources(shared: &Shared) -> Reply {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let (mut running, mut subscribers, mut log_bytes) = (0, 0, 0usize);
    let mut agents = Vec::new();
    for s in &list {
        let s = s.lock().unwrap();
        if s.state == State::Running {
            running += 1;
            if let Some(rss) = proc_kb(&format!("/proc/{}/status", s.agent_pid), "VmRSS:") {
                agents.push(json!({"session": s.id, "pid": s.agent_pid, "rss_kb": rss}));
            }
        }
        subscribers += s.subs.len();
        log_bytes += s.disk_bytes();
    }
    let views = shared.views.lock().unwrap().len();
    Ok(obj(json!({
        "pid": std::process::id(), "uptime_s": shared.started.elapsed().as_secs(),
        "rss_kb": proc_kb("/proc/self/smaps_rollup", "Rss:").unwrap_or(0),
        "pss_kb": proc_kb("/proc/self/smaps_rollup", "Pss:").unwrap_or(0),
        "threads": proc_kb("/proc/self/status", "Threads:").unwrap_or(1).max(1),
        "sessions": list.len(), "running_sessions": running, "subscribers": subscribers, "views": views,
        "log_bytes": log_bytes, "agents": agents,
    })))
}

/// Records after `after` for a caller that cannot hold a stream (the HTTP listener):
/// at most one bounded page, or a gap with a snapshot when the cursor is unusable.
pub fn events(shared: &Shared, session: &str, after: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let (oldest, last, mem_start, view, id): (i64, i64, i64, LogView, String) = {
        let s = found.lock().unwrap();
        (s.oldest(), s.last_seq(), s.mem_start(), s.log_view(), s.id.clone())
    };
    let valid = oldest <= after + 1 && after <= last;
    if !valid {
        let s = found.lock().unwrap();
        return Ok(obj(json!({"session": id, "generation": 1, "gap": true, "from": last + 1,
                              "snapshot": Value::Object(s.snapshot()), "events": []})));
    }
    let records: Vec<(i64, Vec<u8>)> = if after + 1 >= mem_start {
        found.lock().unwrap().window_after(after).into_iter().take(MAX_EVENTS).collect()
    } else {
        view.read(after + 1, mem_start, MAX_EVENTS).map_err(|_| "unknown_session")?
    };
    let events: Vec<Value> = records
        .iter()
        .map(|(seq, data)| json!({"v": 1, "ev": "rec", "session": id, "generation": 1, "seq": seq, "data": payload::hex(data)}))
        .collect();
    Ok(obj(json!({"session": id, "generation": 1, "gap": false, "from": after + 1, "events": events})))
}

/// Send every record after `cursor`, then join the live stream with no gap: older
/// records come from the log in chunks (outside the session lock), the recent ones
/// from the in-memory window, and the join happens under the lock.
pub fn replay(session: &Arc<Mutex<Session>>, sub: &Arc<Sub>, id: &str, mut cursor: i64) {
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
            sub.push(crate::session::encode(obj(json!({"ev": "overflow", "session": id}))));
            return;
        }
        let chunk = match view.read(cursor + 1, mem_start, REPLAY_CHUNK) {
            Ok(chunk) if !chunk.is_empty() => chunk,
            _ => {
                // the records were pruned while replaying: tell the reader to resynchronise
                sub.push(crate::session::encode(obj(json!({"ev": "overflow", "session": id}))));
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
