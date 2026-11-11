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

pub fn peer_is_owner(stream: &UnixStream) -> bool {
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

/// The longest text one `input` may carry, in bytes: the one limit for the window, the CLI and HTTP.
pub const MAX_INPUT_TEXT: usize = 64 * 1024;

/// Text sent to an agent: non-empty, bounded, newline and tab allowed, no other control character.
pub fn valid_text(value: &Value) -> bool {
    matches!(value, Value::String(s) if !s.is_empty() && s.len() <= MAX_INPUT_TEXT && !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t'))
}

/// An account label: 1 to 40 characters, none a control character or one that draws nothing or reorders text, and not only characters that show nothing (the same rule as a name).
pub fn valid_label(value: &Value) -> bool {
    matches!(value, Value::String(s) if !s.is_empty() && s.chars().count() <= 40 && !s.chars().any(crate::folders::refused_char) && !crate::folders::shows_nothing(s))
}

/// Strict validation mirroring the v1 request schemas: unknown fields are refused.
pub fn valid_request(op: &str, request: &Map<String, Value>) -> bool {
    let required: &[&str] = match op {
        "hello" | "status" | "session.list" | "resources" | "account.list" | "host.stop" | "session.options" | "picture.slot" | "real_screen.stop" => &[],
        "session.resume" | "session.purge" | "session.close" => &["session", "generation"],
        "session.rename" => &["session", "name"],
        "session.rebind" => &["session", "generation", "account"],
        "account.create" => &["provider", "label"],
        "account.default" => &["account"],
        "account.credential" => &["account", "credential_ref"],
        "account.affected" => &["account"],
        "history" => &["session", "before", "limit"],
        "account.remove" => &["account", "sessions"],
        "account.login.start" | "account.login.cancel" | "account.login.status" => &["account"],
        "account.login.input" => &["account", "code"],
        "snapshot" | "unsubscribe" | "display.ensure" | "display.status" | "display.stop" => &["session"],
        "subscribe" => &["session", "after"],
        "session.create" => &["kind"],
        "input" => &["session", "generation", "request_id", "text"],
        "transfer.send" => &["to", "kind"], // and `text`, or for a conversation `from` instead: see `transfer_shape`
        "input.status" => &["session", "request_id"],
        "approval.answer" => &["session", "generation", "approval_id", "answer"],
        "approval.text" => &["session", "generation", "approval_id"],
        "view.attach" => &["session", "kind"],
        "clipboard.write" => &["kind"], // and `text`, or for a picture `path` instead: see `clipboard_shape`
        "clipboard.done" => &["req", "ok"],
        "settings.set" => &["clipboard_write"],
        _ => &["view"], // view.detach
    };
    for (key, value) in request {
        let ok = match (op, key.as_str()) {
            (_, "op") => true,
            (_, "v") => value.as_i64() == Some(1),
            (_, "id") => valid_str(value, 64),
            ("snapshot" | "subscribe" | "unsubscribe" | "input" | "input.status" | "approval.answer" | "approval.text" | "view.attach" | "session.resume" | "session.purge" | "session.close" | "session.rebind" | "session.rename" | "history" | "display.ensure" | "display.status" | "display.stop" | "real_screen.stop", "session") => valid_str(value, 64),
            ("subscribe" | "view.attach", "after") => in_range(value, -1, i64::MAX),
            ("snapshot" | "subscribe" | "input" | "approval.answer" | "approval.text" | "view.attach" | "session.resume" | "session.purge" | "session.close" | "session.rebind", "generation") => in_range(value, 1, i64::MAX),
            ("subscribe", "snapshot") => value.is_boolean(),
            ("session.create", "mode") => matches!(value.as_str(), Some("auto" | "default")),
            ("session.create", "kind") => matches!(value.as_str(), Some("synthetic" | "codex" | "claude")),
            ("session.create", "account") | ("session.rebind", "account") | ("account.default", "account") | ("account.credential", "account") | ("account.credential", "credential_ref") | ("account.affected", "account") | ("account.remove", "account") => valid_str(value, 64),
            ("account.login.start", "account") | ("account.login.cancel", "account") | ("account.login.status", "account") | ("account.login.input", "account") => valid_str(value, 64),
            ("account.login.start", "method") => matches!(value.as_str(), Some("browser" | "device")),
            ("host.stop", "force") => value.is_boolean(),
            ("host.stop", "build") => valid_str(value, 32),
            ("account.login.status", "check") => value.is_boolean(),
            ("account.login.input", "code") => matches!(value, Value::String(s) if !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)),
            ("account.remove", "sessions") => matches!(value.as_str(), Some("detach" | "end")),
            ("account.create", "provider") => matches!(value.as_str(), Some("codex" | "claude")),
            ("account.create", "label") => valid_label(value),
            ("history", "before") => in_range(value, 0, i64::MAX),
            ("history", "limit") => in_range(value, 1, 200),
            ("session.create", "cwd") => matches!(value, Value::String(s) if crate::folders::valid_path_shape(s)),
            ("session.create" | "session.rename", "name") => matches!(value, Value::String(s) if crate::folders::valid_name(s)),
            ("session.create", "model") => matches!(value, Value::String(s) if crate::folders::valid_model(s)),
            ("session.create", "effort") => matches!(value, Value::String(s) if crate::folders::valid_effort(s)),
            ("session.create", "rate") => in_range(value, 0, 10_000),
            ("session.create", "history") => in_range(value, 0, 100_000),
            ("session.create", "payload") => in_range(value, 1, 4096),
            ("input" | "input.status", "request_id") => valid_request_id(value),
            ("input", "attachment") => matches!(value, Value::String(s) if crate::pasted::valid_name(s)), // the NAME of a file in the pasted folder; the host builds the path
            ("input" | "transfer.send", "text") => valid_text(value),
            ("transfer.send", "from" | "to") => valid_str(value, 64),
            ("transfer.send", "kind") => matches!(value.as_str(), Some("text" | "lines" | "conversation")),
            ("approval.answer" | "approval.text", "approval_id") => in_range(value, 1, i64::MAX),
            ("approval.text", "offset") => in_range(value, 0, i64::MAX),
            ("approval.text", "limit") => in_range(value, 4, crate::pending::PAGE as i64),
            ("approval.answer", "answer") => matches!(value.as_str(), Some("allow" | "deny")),
            ("approval.answer", "keep") => value.is_boolean(),
            ("view.attach", "kind") => matches!(value.as_str(), Some("gui" | "cli")),
            ("view.detach", "view") => valid_str(value, 64),
            ("clipboard.write", "kind") => matches!(value.as_str(), Some("text" | "image")),
            ("clipboard.write", "text") => matches!(value, Value::String(s) if !s.is_empty() && s.len() <= MAX_INPUT_TEXT), // any character: a clipboard holds what it holds
            ("clipboard.write", "path") => matches!(value, Value::String(s) if s.starts_with('/') && crate::folders::valid_path_shape(s)),
            ("clipboard.write", "timeout_ms") => in_range(value, 1, crate::clipwrite::TIMEOUT_CEILING_MS),
            ("clipboard.done", "req") => valid_request_id(value),
            ("clipboard.done", "ok") => value.is_boolean(),
            ("clipboard.done", "kind") => matches!(value.as_str(), Some("text" | "image")),
            ("clipboard.done", "error") => valid_str(value, 32),
            ("clipboard.done", "bytes") => in_range(value, 0, i64::MAX),
            ("clipboard.done", "width" | "height") => in_range(value, 1, 8000),
            ("settings.set", "clipboard_write") => value.is_boolean(),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    required.iter().all(|key| request.contains_key(*key)) && (op != "transfer.send" || transfer_shape(request)) && (op != "clipboard.write" || clipboard_shape(request))
}

/// A text write has `text` and no `path`; a picture write has `path` and no `text`.
fn clipboard_shape(request: &Map<String, Value>) -> bool {
    if request.get("kind").and_then(Value::as_str) == Some("image") {
        request.contains_key("path") && !request.contains_key("text")
    } else {
        request.contains_key("text") && !request.contains_key("path")
    }
}

/// A conversation is the whole conversation of the session `from`: it has no text. Text and lines are text, and `from` is the person's choice.
fn transfer_shape(request: &Map<String, Value>) -> bool {
    if request.get("kind").and_then(Value::as_str) == Some("conversation") {
        request.contains_key("from") && !request.contains_key("text")
    } else {
        request.contains_key("text")
    }
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
        "session.close" => crate::lifecycle::session_close(shared, &session, generation.unwrap_or(0)),
        "session.create" => core::session_create(
            shared,
            text(&request, "kind"),
            int(&request, "rate").unwrap_or(0) as u64,
            int(&request, "history").unwrap_or(0) as u64,
            int(&request, "payload").unwrap_or(shared.cfg.payload as i64) as usize,
            request.get("account").and_then(Value::as_str),
            text(&request, "mode"),
            crate::folders::Settings::of_request(&request),
        ),
        "session.rename" => crate::folders::rename(shared, &session, text(&request, "name")),
        "session.options" => crate::folders::options(shared),
        "session.rebind" => core::session_rebind(shared, &session, generation.unwrap_or(0), text(&request, "account")),
        "account.create" => core::account_create(shared, text(&request, "provider"), text(&request, "label")),
        "account.list" => core::account_list(shared),
        "account.default" => core::account_default(shared, text(&request, "account")),
        "history" => core::history(shared, &session, int(&request, "before").unwrap_or(0), int(&request, "limit").unwrap_or(100)),
        "account.affected" => core::account_affected(shared, text(&request, "account")),
        "account.login.start" => crate::login::start(shared, text(&request, "account"), if text(&request, "method").is_empty() { "browser" } else { text(&request, "method") }),
        "account.login.input" => crate::login::input(shared, text(&request, "account"), text(&request, "code")),
        "account.login.status" => crate::login::status(shared, text(&request, "account"), request.get("check").and_then(Value::as_bool).unwrap_or(false)),
        "host.stop" => core::host_stop(shared, request.get("force").and_then(Value::as_bool).unwrap_or(false), request.get("build").and_then(Value::as_str)),
        "account.login.cancel" => crate::login::cancel(shared, text(&request, "account")),
        "account.remove" => core::account_remove(shared, text(&request, "account"), text(&request, "sessions")),
        "account.credential" => core::account_credential(shared, text(&request, "account"), text(&request, "credential_ref")),
        "snapshot" => core::snapshot(shared, &session, generation),
        "input" => crate::pasted::with_cleanup(shared, request.get("attachment").and_then(Value::as_str), |attachment| core::input(shared, &session, Some(generation.unwrap_or(0)), text(&request, "request_id"), text(&request, "text"), crate::session::Origin::Person, attachment)),
        "picture.slot" => crate::pasted::slot(shared),
        "transfer.send" if text(&request, "kind") == "conversation" => crate::transfer::hand_over(shared, text(&request, "from"), text(&request, "to")),
        "transfer.send" => crate::transfer::send(shared, request.get("from").and_then(Value::as_str), text(&request, "to"), text(&request, "kind"), text(&request, "text"), crate::transfer::By::Person),
        "input.status" => core::input_status(shared, &session, text(&request, "request_id")),
        "approval.text" => crate::pending::text_page(shared, &session, generation.unwrap_or(0), int(&request, "approval_id").unwrap_or(0), int(&request, "offset").unwrap_or(0) as usize, int(&request, "limit").unwrap_or(crate::pending::PAGE as i64) as usize),
        "approval.answer" => core::approval_answer(shared, &session, generation.unwrap_or(0), int(&request, "approval_id").unwrap_or(0), text(&request, "answer"), request.get("keep").and_then(Value::as_bool).unwrap_or(false)),
        "view.attach" => match core::view_attach(shared, Some((sub, &rid)), &session, generation, int(&request, "after").unwrap_or(-1)) {
            Ok(body) => {
                if text(&request, "kind") == "gui" {
                    crate::clipwrite::mark_gui(shared, body.get("view").and_then(Value::as_str).unwrap_or_default());
                }
                return; // the reply was queued under the session lock
            }
            Err(code) => Err(code),
        },
        "clipboard.write" => crate::clipwrite::write(shared, &request),
        "clipboard.done" => crate::clipwrite::done(shared, sub, &request),
        "settings.set" => crate::clipboard::settings::set_clipboard_write(&shared.cfg.handoff_dir, request.get("clipboard_write").and_then(Value::as_bool).unwrap_or(false)),
        "display.ensure" => crate::vdisplay::ensure(shared, &session),
        "display.status" => crate::vdisplay::status_op(shared, &session),
        "display.stop" => crate::vdisplay::stop_op(shared, &session),
        "real_screen.stop" => crate::computer::real::stop_op(shared, request.get("session").and_then(Value::as_str)),
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
