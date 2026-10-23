//! Protocol v1 core in Rust, built two ways: standard library only (default) and
//! with serde_json (`--features serde`). It exists to compare size and resident
//! memory with other implementations of the same contract; the conformance suite
//! decides whether it is a correct host. Record payloads are a deterministic
//! function of the sequence number; the retained window stores them.

use std::collections::{HashSet, VecDeque};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const MAX_REQUEST: usize = 4096;
const SNAPSHOT_LINES: usize = 40;
const RECORD_BYTES: usize = 1024;
const OPS: [&str; 5] = ["hello", "status", "snapshot", "subscribe", "unsubscribe"];

#[derive(Clone, Debug)]
enum J {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(&'static str, J)>),
}

// ---------------------------------------------------------------- json layer
#[cfg(not(feature = "serde"))]
mod json {
    use super::J;

    pub fn encode(value: &J, out: &mut String) {
        match value {
            J::Null => out.push_str("null"),
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Int(i) => out.push_str(&i.to_string()),
            J::Str(s) => {
                out.push('"');
                for ch in s.chars() {
                    match ch {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            J::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    encode(item, out);
                }
                out.push(']');
            }
            J::Obj(fields) => {
                out.push('{');
                for (i, (key, item)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    out.push_str(key);
                    out.push_str("\":");
                    encode(item, out);
                }
                out.push('}');
            }
        }
    }

    /// A flat object of scalar values: anything else is not a valid request.
    pub fn parse_flat(bytes: &[u8]) -> Option<Vec<(String, J)>> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut chars = text.trim().chars().peekable();
        let skip = |c: &mut std::iter::Peekable<std::str::Chars>| {
            while matches!(c.peek(), Some(' ' | '\t' | '\r' | '\n')) {
                c.next();
            }
        };
        if chars.next()? != '{' {
            return None;
        }
        let mut fields = Vec::new();
        skip(&mut chars);
        if chars.peek() == Some(&'}') {
            chars.next();
        } else {
            loop {
                skip(&mut chars);
                let key = string(&mut chars)?;
                skip(&mut chars);
                if chars.next()? != ':' {
                    return None;
                }
                skip(&mut chars);
                let value = scalar(&mut chars)?;
                fields.push((key, value));
                skip(&mut chars);
                match chars.next()? {
                    ',' => continue,
                    '}' => break,
                    _ => return None,
                }
            }
        }
        skip(&mut chars);
        if chars.next().is_some() {
            return None;
        }
        Some(fields)
    }

    fn string(c: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
        if c.next()? != '"' {
            return None;
        }
        let mut out = String::new();
        loop {
            match c.next()? {
                '"' => return Some(out),
                '\\' => match c.next()? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let hex: String = (0..4).filter_map(|_| c.next()).collect();
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    _ => return None,
                },
                ch if (ch as u32) < 0x20 => return None,
                ch => out.push(ch),
            }
        }
    }

    fn scalar(c: &mut std::iter::Peekable<std::str::Chars>) -> Option<J> {
        match c.peek()? {
            '"' => Some(J::Str(string(c)?)),
            't' | 'f' | 'n' => {
                let word: String = std::iter::from_fn(|| c.next_if(|x| x.is_ascii_alphabetic())).collect();
                match word.as_str() {
                    "true" => Some(J::Bool(true)),
                    "false" => Some(J::Bool(false)),
                    "null" => Some(J::Null),
                    _ => None,
                }
            }
            '-' | '0'..='9' => {
                let digits: String = std::iter::from_fn(|| c.next_if(|x| x.is_ascii_digit() || *x == '-')).collect();
                if matches!(c.peek(), Some('.' | 'e' | 'E')) {
                    return None; // only integers are valid in this protocol
                }
                digits.parse().ok().map(J::Int)
            }
            _ => None,
        }
    }
}

#[cfg(feature = "serde")]
mod json {
    use super::J;
    use serde_json::{Map, Value};

    fn to_value(value: &J) -> Value {
        match value {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Int(i) => Value::from(*i),
            J::Str(s) => Value::String(s.clone()),
            J::Arr(items) => Value::Array(items.iter().map(to_value).collect()),
            J::Obj(fields) => {
                let mut map = Map::new();
                for (key, item) in fields {
                    map.insert((*key).to_string(), to_value(item));
                }
                Value::Object(map)
            }
        }
    }

    pub fn encode(value: &J, out: &mut String) {
        out.push_str(&serde_json::to_string(&to_value(value)).unwrap_or_default());
    }

    pub fn parse_flat(bytes: &[u8]) -> Option<Vec<(String, J)>> {
        let value: Value = serde_json::from_slice(bytes).ok()?;
        let mut fields = Vec::new();
        for (key, item) in value.as_object()? {
            let scalar = match item {
                Value::Null => J::Null,
                Value::Bool(b) => J::Bool(*b),
                Value::Number(n) => J::Int(n.as_i64()?),
                Value::String(s) => J::Str(s.clone()),
                _ => return None,
            };
            fields.push((key.clone(), scalar));
        }
        Some(fields)
    }
}

// ----------------------------------------------------------------- host state
fn payload(seq: i64) -> Vec<u8> {
    let mut state = (seq as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0xD1B5_4A32_D192_ED03);
    let mut out = Vec::with_capacity(RECORD_BYTES);
    while out.len() < RECORD_BYTES {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    out.truncate(RECORD_BYTES);
    out
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

fn random_id() -> String {
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("urandom");
    format!("s{}", hex(&bytes))
}

struct Sub {
    queue: Mutex<VecDeque<Vec<u8>>>,
    wake: Condvar,
    cap: usize,
    closed: AtomicBool,
    overflowed: Mutex<HashSet<String>>,
}

struct Session {
    id: String,
    producing: bool,
    ring: VecDeque<(i64, Vec<u8>)>,
    next_seq: i64,
    bytes_total: i64,
    subs: Vec<Arc<Sub>>,
    retain: usize,
}

impl Session {
    fn last_seq(&self) -> i64 {
        self.next_seq - 1
    }

    fn oldest(&self) -> i64 {
        self.ring.front().map_or(self.next_seq, |r| r.0)
    }

    fn append(&mut self) {
        let seq = self.next_seq;
        let data = payload(seq);
        self.bytes_total += data.len() as i64;
        if self.ring.len() == self.retain {
            self.ring.pop_front();
        }
        let encoded = rec_line(&self.id, seq, &data);
        self.ring.push_back((seq, data));
        self.next_seq += 1;
        let id = self.id.clone();
        self.subs.retain(|sub| !offer(sub, &id, encoded.clone()));
    }

    fn snapshot(&self) -> Vec<(&'static str, J)> {
        let skip = self.ring.len().saturating_sub(SNAPSHOT_LINES);
        let lines = self
            .ring
            .iter()
            .skip(skip)
            .map(|(seq, data)| J::Arr(vec![J::Int(*seq), J::Str(format!("{:08} {}", seq, hex(&data[..44])))]))
            .collect();
        vec![("session", J::Str(self.id.clone())), ("generation", J::Int(1)), ("seq", J::Int(self.last_seq())), ("lines", J::Arr(lines))]
    }
}

fn line(fields: Vec<(&'static str, J)>) -> Vec<u8> {
    let mut all = vec![("v", J::Int(1))];
    all.extend(fields);
    let mut out = String::new();
    json::encode(&J::Obj(all), &mut out);
    out.push('\n');
    out.into_bytes()
}

fn rec_line(session: &str, seq: i64, data: &[u8]) -> Vec<u8> {
    line(vec![("ev", J::Str("rec".into())), ("session", J::Str(session.into())), ("generation", J::Int(1)), ("seq", J::Int(seq)), ("data", J::Str(hex(data)))])
}

/// Queue a record for a subscriber; returns true when the subscriber must be dropped.
fn offer(sub: &Arc<Sub>, session: &str, encoded: Vec<u8>) -> bool {
    if sub.overflowed.lock().unwrap().contains(session) {
        return true;
    }
    let mut queue = sub.queue.lock().unwrap();
    if queue.len() >= sub.cap {
        sub.overflowed.lock().unwrap().insert(session.to_string());
        queue.push_back(line(vec![("ev", J::Str("overflow".into())), ("session", J::Str(session.into()))]));
        sub.wake.notify_one();
        return true; // never wait for a slow reader
    }
    queue.push_back(encoded);
    sub.wake.notify_one();
    false
}

fn send(sub: &Arc<Sub>, fields: Vec<(&'static str, J)>) {
    sub.queue.lock().unwrap().push_back(line(fields));
    sub.wake.notify_one();
}

struct Host {
    sessions: Vec<Session>,
    retain: usize,
}

fn get<'a>(fields: &'a [(String, J)], key: &str) -> Option<&'a J> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn valid_str(value: &J, max: usize) -> bool {
    matches!(value, J::Str(s) if s.chars().count() <= max)
}

fn valid_int(value: &J, min: i64) -> bool {
    matches!(value, J::Int(i) if *i >= min)
}

/// Strict request validation, mirroring the v1 schemas: unknown fields are refused.
fn valid_request(op: &str, fields: &[(String, J)]) -> bool {
    let (required, optional): (&[&str], &[&str]) = match op {
        "hello" | "status" => (&[], &[]),
        "snapshot" => (&["session"], &["generation"]),
        "subscribe" => (&["session", "after"], &["generation", "snapshot"]),
        _ => (&["session"], &[]),
    };
    for (key, value) in fields {
        let ok = match key.as_str() {
            "op" => true,
            "v" => matches!(value, J::Int(1)),
            "id" => valid_str(value, 64),
            "session" if required.contains(&"session") => valid_str(value, 64),
            "after" if required.contains(&"after") => valid_int(value, -1),
            "generation" if optional.contains(&"generation") => valid_int(value, 1),
            "snapshot" if optional.contains(&"snapshot") => matches!(value, J::Bool(_)),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    required.iter().all(|key| get(fields, key).is_some())
}

fn handle(host: &Mutex<Host>, sub: &Arc<Sub>, request: Option<Vec<(String, J)>>) {
    let fail = |code: &str, rid: Option<&J>| {
        let mut f = vec![("ok", J::Int(0)), ("error", J::Str(code.into()))];
        if let Some(id @ J::Str(s)) = rid {
            if s.chars().count() <= 64 {
                f.push(("id", id.clone()));
            }
        }
        send(sub, f);
    };
    let Some(request) = request else { return fail("bad_request", None) };
    let rid = get(&request, "id").cloned();
    if let Some(J::Int(v)) = get(&request, "v") {
        if *v != 1 {
            return fail("version_unsupported", rid.as_ref());
        }
    }
    let Some(J::Str(op)) = get(&request, "op").cloned() else { return fail("bad_request", rid.as_ref()) };
    if !OPS.contains(&op.as_str()) {
        return fail("unknown_op", rid.as_ref());
    }
    if !valid_request(&op, &request) {
        return fail("bad_request", rid.as_ref());
    }
    let reply = |mut body: Vec<(&'static str, J)>| {
        let mut f = vec![("ok", J::Int(1))];
        if let Some(id) = &rid {
            f.push(("id", id.clone()));
        }
        f.append(&mut body);
        send(sub, f);
    };
    let mut host = host.lock().unwrap();
    match op.as_str() {
        "hello" => reply(vec![
            ("protocol", J::Int(1)),
            ("host", J::Str("lico-core".into())),
            ("max_request", J::Int(MAX_REQUEST as i64)),
            ("retain", J::Int(host.retain as i64)),
            ("ops", J::Arr(OPS.iter().map(|o| J::Str((*o).into())).collect())),
        ]),
        "status" => {
            let sessions = host
                .sessions
                .iter()
                .map(|s| {
                    J::Obj(vec![
                        ("session", J::Str(s.id.clone())),
                        ("generation", J::Int(1)),
                        ("seq", J::Int(s.last_seq())),
                        ("producing", J::Bool(s.producing)),
                        ("subscribers", J::Int(s.subs.len() as i64)),
                        ("bytes_total", J::Int(s.bytes_total)),
                    ])
                })
                .collect();
            reply(vec![("pid", J::Int(std::process::id() as i64)), ("sessions", J::Arr(sessions))]);
        }
        _ => {
            let Some(J::Str(wanted)) = get(&request, "session") else { return };
            let Some(session) = host.sessions.iter_mut().find(|s| &s.id == wanted) else {
                return fail("unknown_session", rid.as_ref());
            };
            if let Some(J::Int(g)) = get(&request, "generation") {
                if *g != 1 {
                    return fail("stale_generation", rid.as_ref());
                }
            }
            match op.as_str() {
                "snapshot" => reply(session.snapshot()),
                "unsubscribe" => {
                    session.subs.retain(|s| !Arc::ptr_eq(s, sub));
                    sub.overflowed.lock().unwrap().remove(&session.id);
                    reply(vec![("session", J::Str(session.id.clone())), ("seq", J::Int(session.last_seq()))]);
                }
                _ => {
                    let Some(J::Int(after)) = get(&request, "after").cloned() else { return };
                    let want_snapshot = matches!(get(&request, "snapshot"), Some(J::Bool(true)));
                    let oldest = session.oldest();
                    let mut body = vec![("session", J::Str(session.id.clone())), ("generation", J::Int(1))];
                    let mut replay_after = after;
                    if want_snapshot {
                        let snap = session.snapshot();
                        let valid = oldest <= after + 1 && after <= session.last_seq();
                        body.push(("gap", J::Bool(!valid)));
                        body.push(("from", J::Int(session.last_seq() + 1)));
                        body.push(("missed", if valid { J::Int(session.last_seq() - after) } else { J::Null }));
                        body.push(("snapshot", J::Obj(snap)));
                        replay_after = session.last_seq();
                    } else if after + 1 < oldest || after > session.last_seq() {
                        body.push(("gap", J::Bool(true)));
                        body.push(("snapshot", J::Obj(session.snapshot())));
                        replay_after = session.last_seq();
                        body.push(("from", J::Int(replay_after + 1)));
                    } else {
                        body.push(("gap", J::Bool(false)));
                        body.push(("from", J::Int(after + 1)));
                    }
                    reply(body);
                    sub.overflowed.lock().unwrap().remove(&session.id);
                    let id = session.id.clone();
                    let replay: Vec<Vec<u8>> = session.ring.iter().filter(|(seq, _)| *seq > replay_after).map(|(seq, data)| rec_line(&id, *seq, data)).collect();
                    session.subs.push(sub.clone()); // live records follow the replay with no gap
                    for encoded in replay {
                        if offer(sub, &id, encoded) {
                            session.subs.retain(|s| !Arc::ptr_eq(s, sub));
                            break;
                        }
                    }
                }
            }
        }
    }
}

// --------------------------------------------------------- peer and sockets
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
}

fn peer_is_owner(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred = Ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<Ucred>() as u32;
    // SOL_SOCKET = 1, SO_PEERCRED = 17 on Linux
    let rc = unsafe { getsockopt(stream.as_raw_fd(), 1, 17, &mut cred, &mut len) };
    rc == 0 && cred.uid == unsafe { getuid() }
}

fn connection(stream: UnixStream, host: Arc<Mutex<Host>>, cap: usize) {
    if !peer_is_owner(&stream) {
        return;
    }
    let sub = Arc::new(Sub { queue: Mutex::new(VecDeque::new()), wake: Condvar::new(), cap, closed: AtomicBool::new(false), overflowed: Mutex::new(HashSet::new()) });
    let writer = {
        let sub = sub.clone();
        let mut out = stream.try_clone().expect("clone");
        thread::spawn(move || loop {
            let next = {
                let mut queue = sub.queue.lock().unwrap();
                while queue.is_empty() && !sub.closed.load(Ordering::Relaxed) {
                    queue = sub.wake.wait(queue).unwrap();
                }
                queue.pop_front()
            };
            match next {
                Some(bytes) => {
                    if out.write_all(&bytes).is_err() {
                        return;
                    }
                }
                None => return,
            }
        })
    };
    let mut input = stream;
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    'read: loop {
        let n = match input.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        pending.extend_from_slice(&chunk[..n]);
        while let Some(pos) = pending.iter().position(|b| *b == b'\n') {
            let raw: Vec<u8> = pending.drain(..=pos).collect();
            if raw.len() > MAX_REQUEST {
                send(&sub, vec![("ok", J::Int(0)), ("error", J::Str("too_large".into()))]);
                break 'read;
            }
            handle(&host, &sub, json::parse_flat(&raw[..raw.len() - 1]));
        }
        if pending.len() > MAX_REQUEST {
            send(&sub, vec![("ok", J::Int(0)), ("error", J::Str("too_large".into()))]);
            break;
        }
    }
    for session in host.lock().unwrap().sessions.iter_mut() {
        session.subs.retain(|s| !Arc::ptr_eq(s, &sub));
    }
    // let the writer flush a final error line, then stop it
    thread::sleep(Duration::from_millis(20));
    sub.closed.store(true, Ordering::Relaxed);
    sub.wake.notify_one();
    let _ = writer.join();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, default: &str| args.windows(2).find(|w| w[0] == name).map_or(default.to_string(), |w| w[1].clone());
    let path = arg("--socket", "");
    if path.is_empty() {
        eprintln!("usage: lico-host --socket PATH [--sessions N] [--producing N] [--rate R] [--history N] [--retain N]");
        std::process::exit(2);
    }
    let sessions: usize = arg("--sessions", "1").parse().expect("sessions");
    let producing: usize = arg("--producing", "1").parse().expect("producing");
    let rate: f64 = arg("--rate", "100").parse().expect("rate");
    let history: usize = arg("--history", "5000").parse().expect("history");
    let retain: usize = arg("--retain", "2048").parse().expect("retain");
    let mut list = Vec::new();
    for index in 0..sessions {
        let mut s = Session { id: random_id(), producing: index < producing, ring: VecDeque::new(), next_seq: 0, bytes_total: 0, subs: Vec::new(), retain };
        for _ in 0..history {
            s.append();
        }
        list.push(s);
    }
    let ids: Vec<String> = list.iter().map(|s| s.id.clone()).collect();
    let host = Arc::new(Mutex::new(Host { sessions: list, retain }));
    for index in 0..producing.min(sessions) {
        let host = host.clone();
        thread::spawn(move || {
            let interval = Duration::from_secs_f64(1.0 / rate);
            let mut next_at = Instant::now();
            loop {
                next_at += interval;
                host.lock().unwrap().sessions[index].append();
                let now = Instant::now();
                if next_at > now {
                    thread::sleep(next_at - now);
                }
            }
        });
    }
    let _ = std::fs::remove_file(&path);
    let previous = unsafe { umask(0o077) }; // never group- or world-accessible, even briefly
    let listener = UnixListener::bind(&path).expect("bind");
    unsafe { umask(previous) };
    let mut ready = String::new();
    json::encode(&J::Obj(vec![("ready", J::Int(1)), ("pid", J::Int(std::process::id() as i64)), ("sessions", J::Arr(ids.into_iter().map(J::Str).collect()))]), &mut ready);
    println!("{ready}");
    let _ = std::io::stdout().flush();
    let cap = retain + 1024;
    for stream in listener.incoming().flatten() {
        let host = host.clone();
        thread::spawn(move || connection(stream, host, cap));
    }
}
