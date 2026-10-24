//! Protocol v1 over the owner-only Unix socket: strict requests, one reply per
//! request, records and notices as events on the same connection.

use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::core::{self, Shared, MAX_REQUEST, OPS};
use crate::session::{encode, obj, Sub};

#[repr(C)]
struct Ucred {
    pid: i32,
    uid: u32,
    gid: u32,
}

extern "C" {
    fn getsockopt(fd: i32, level: i32, name: i32, value: *mut Ucred, len: *mut u32) -> i32;
    fn getuid() -> u32;
}

fn peer_is_owner(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred = Ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<Ucred>() as u32;
    // SOL_SOCKET = 1, SO_PEERCRED = 17 on Linux
    let rc = unsafe { getsockopt(stream.as_raw_fd(), 1, 17, &mut cred, &mut len) };
    rc == 0 && cred.uid == unsafe { getuid() }
}

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

pub fn int(map: &Map<String, Value>, key: &str) -> Option<i64> {
    map.get(key).and_then(Value::as_i64)
}

pub fn text<'a>(map: &'a Map<String, Value>, key: &str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn valid_str(value: &Value, max: usize) -> bool {
    matches!(value, Value::String(s) if s.chars().count() <= max)
}

fn in_range(value: &Value, min: i64, max: i64) -> bool {
    value.as_i64().map_or(false, |i| i >= min && i <= max)
}

/// A request id is short and made of letters, digits, dash and underscore only,
/// so it can be handed to an agent without any chance of injecting a line.
pub fn valid_request_id(value: &Value) -> bool {
    matches!(value, Value::String(s) if !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Text sent to an agent: non-empty, bounded, no control characters.
pub fn valid_text(value: &Value) -> bool {
    matches!(value, Value::String(s) if !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
}

/// Strict validation mirroring the v1 request schemas: unknown fields are refused.
pub fn valid_request(op: &str, request: &Map<String, Value>) -> bool {
    let required: &[&str] = match op {
        "hello" | "status" | "session.list" | "resources" => &[],
        "session.resume" | "session.purge" => &["session", "generation"],
        "snapshot" | "unsubscribe" => &["session"],
        "subscribe" => &["session", "after"],
        "session.create" => &["kind"],
        "input" => &["session", "generation", "request_id", "text"],
        "input.status" => &["session", "request_id"],
        "approval.answer" => &["session", "generation", "approval_id", "answer"],
        "view.attach" => &["session", "kind"],
        _ => &["view"], // view.detach
    };
    for (key, value) in request {
        let ok = match (op, key.as_str()) {
            (_, "op") => true,
            (_, "v") => value.as_i64() == Some(1),
            (_, "id") => valid_str(value, 64),
            ("snapshot" | "subscribe" | "unsubscribe" | "input" | "input.status" | "approval.answer" | "view.attach" | "session.resume" | "session.purge", "session") => valid_str(value, 64),
            ("subscribe" | "view.attach", "after") => in_range(value, -1, i64::MAX),
            ("snapshot" | "subscribe" | "input" | "approval.answer" | "view.attach" | "session.resume" | "session.purge", "generation") => in_range(value, 1, i64::MAX),
            ("subscribe", "snapshot") => value.is_boolean(),
            ("session.create", "kind") => matches!(value.as_str(), Some("synthetic" | "codex")),
            ("session.create", "rate") => in_range(value, 0, 10_000),
            ("session.create", "history") => in_range(value, 0, 100_000),
            ("session.create", "payload") => in_range(value, 1, 4096),
            ("input" | "input.status", "request_id") => valid_request_id(value),
            ("input", "text") => valid_text(value),
            ("approval.answer", "approval_id") => in_range(value, 1, i64::MAX),
            ("approval.answer", "answer") => matches!(value.as_str(), Some("allow" | "deny")),
            ("view.attach", "kind") => matches!(value.as_str(), Some("gui" | "cli")),
            ("view.detach", "view") => valid_str(value, 64),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    required.iter().all(|key| request.contains_key(*key))
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
    let session = text(&request, "session").to_string();
    let generation = int(&request, "generation");
    let result = match op.as_str() {
        "hello" => core::hello(shared),
        "status" => core::status(shared),
        "session.list" => core::session_list(shared),
        "resources" => core::resources(shared),
        "session.resume" => core::session_resume(shared, &session, generation.unwrap_or(0)),
        "session.purge" => core::session_purge(shared, &session, generation.unwrap_or(0)),
        "session.create" => core::session_create(
            shared,
            text(&request, "kind"),
            int(&request, "rate").unwrap_or(0) as u64,
            int(&request, "history").unwrap_or(0) as u64,
            int(&request, "payload").unwrap_or(shared.cfg.payload as i64) as usize,
        ),
        "snapshot" => core::snapshot(shared, &session, generation),
        "input" => core::input(shared, &session, generation.unwrap_or(0), text(&request, "request_id"), text(&request, "text")),
        "input.status" => core::input_status(shared, &session, text(&request, "request_id")),
        "approval.answer" => core::approval_answer(shared, &session, generation.unwrap_or(0), int(&request, "approval_id").unwrap_or(0), text(&request, "answer")),
        "view.attach" => match core::view_attach(shared, Some((sub, &rid)), &session, generation, int(&request, "after").unwrap_or(-1)) {
            Ok(_) => return, // the reply was queued under the session lock
            Err(code) => Err(code),
        },
        "view.detach" => core::view_detach(shared, text(&request, "view")),
        _ => return stream_op(shared, sub, &request, &op, &rid),
    };
    match result {
        Ok(body) => sub.push(encode(reply_fields(&rid, body))),
        Err(code) => fail(sub, code, rid.as_ref()),
    }
}

/// `subscribe` and `unsubscribe` change what this connection receives.
fn stream_op(shared: &Arc<Shared>, sub: &Arc<Sub>, request: &Map<String, Value>, op: &str, rid: &Option<Value>) {
    let reply = |body: Value| sub.push(encode(reply_fields(rid, obj(body))));
    let Some(session) = core::find(shared, text(request, "session")) else { return fail(sub, "unknown_session", rid.as_ref()) };
    let mut guard = session.lock().unwrap();
    if let Some(g) = int(request, "generation") {
        if g != guard.generation as i64 {
            return fail(sub, "stale_generation", rid.as_ref());
        }
    }
    if op == "unsubscribe" {
        guard.subs.retain(|s| !Arc::ptr_eq(s, sub));
        sub.clear_overflow(&guard.id);
        return reply(json!({"session": guard.id, "seq": guard.last_seq()}));
    }
    let after = int(request, "after").unwrap_or(-1);
    let want_snapshot = request.get("snapshot").and_then(Value::as_bool) == Some(true);
    let (oldest, last) = (guard.oldest(), guard.last_seq());
    let id = guard.id.clone();
    let mut body = json!({"session": id, "generation": guard.generation});
    let valid = oldest <= after + 1 && after <= last;
    if want_snapshot || !valid {
        // the latest screen now, live records after it; a stale cursor is a gap
        body["gap"] = json!(!valid);
        body["snapshot"] = Value::Object(guard.snapshot());
        body["from"] = json!(last + 1);
        if want_snapshot {
            body["missed"] = if valid { json!(last - after) } else { Value::Null };
        }
        reply(body);
        sub.clear_overflow(&id);
        guard.add_sub(sub);
    } else {
        body["gap"] = json!(false);
        body["from"] = json!(after + 1);
        reply(body);
        sub.clear_overflow(&id);
        drop(guard);
        core::replay(&session, sub, &id, after);
    }
}

pub fn connection(stream: UnixStream, shared: Arc<Shared>) {
    if !peer_is_owner(&stream) {
        return;
    }
    let sub = Sub::new(shared.cfg.queue_bytes);
    let writer = {
        let sub = sub.clone();
        let mut out = stream.try_clone().expect("clone");
        let _ = out.set_write_timeout(Some(Duration::from_secs(30))); // a stalled peer is cut off
        thread::spawn(move || {
            while let Some(line) = sub.pop_wait() {
                if line.is_empty() {
                    continue;
                }
                if out.write_all(&line).is_err() {
                    break;
                }
            }
            sub.closed.store(true, Ordering::Relaxed);
            let _ = out.shutdown(std::net::Shutdown::Both); // ends the reader too
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
            if sub.closed.load(Ordering::Relaxed) {
                break 'read;
            }
        }
        if pending.len() > MAX_REQUEST {
            too_large(&sub);
            break;
        }
    }
    core::drop_connection(&shared, &sub);
    thread::sleep(Duration::from_millis(20)); // let the writer flush a final error line
    sub.closed.store(true, Ordering::Relaxed);
    let _ = input.shutdown(std::net::Shutdown::Both); // frees a writer stuck on a peer that never reads
    sub.wake.notify_one();
    let _ = writer.join();
}
