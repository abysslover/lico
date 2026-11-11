//! The optional loopback HTTP listener: the same operations as the local socket
//! behind a control credential, exact Host and Origin handling, bounded requests,
//! a connection cap and a throttle. It binds 127.0.0.1 only.

use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::{self, Shared};
use crate::ipc::{int, valid_request};
use crate::session::{encode, obj};

const MAX_HEAD: usize = 8192;
const MAX_BODY: usize = crate::core::MAX_REQUEST;
const MAX_CONNECTIONS: usize = 32;
const RATE_PER_SECOND: f64 = 200.0;
const BURST: f64 = 50.0;
const FAILED_AUTH_PER_SECOND: f64 = 20.0;
const READ_DEADLINE: Duration = Duration::from_secs(10);

pub(crate) struct Bucket {
    pub(crate) tokens: f64,
    pub(crate) at: Instant,
}

pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

pub(crate) fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn status_of(code: &str) -> u16 {
    match code {
        "bad_request" | "version_unsupported" | "option_unavailable" => 400,
        "folder_not_absolute" | "folder_missing" | "folder_not_directory" | "folder_unreadable" | "folder_protected" => 400,
        "file_not_regular" | "file_unreadable" => 400,
        "worktree_needs_folder" | "not_git_repo" | "invalid_branch" => 400,
        "unknown_op" | "unknown_session" | "unknown_account" | "unknown_view" | "unknown_request" | "unknown_approval" | "file_missing" => 404,
        "stale_generation" | "session_exited" | "branch_exists" | "worktree_dirty" => 409,
        "too_large" => 413,
        _ => 503,
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        _ => "Service Unavailable",
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: Map<String, Value>, extra: &str) {
    let mut all = Map::new();
    all.insert("ok".into(), json!(if status == 200 { 1 } else { 0 }));
    for (key, value) in body {
        all.insert(key, value);
    }
    let text = String::from_utf8(encode(all).to_vec()).unwrap_or_default();
    let big = text.len() >= 65_536;
    write_response(stream, status, &text, extra);
    drop(text);
    if big {
        crate::memory::trim_heap();
    }
}

/// One response whose body is already JSON text (`respond` wraps a v1 body in it; the tool connection speaks JSON-RPC).
pub(crate) fn write_response(stream: &mut TcpStream, status: u16, text: &str, extra: &str) {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n{extra}\r\n",
        reason(status),
        text.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(text.as_bytes());
}

fn error(stream: &mut TcpStream, status: u16, code: &str) {
    respond(stream, status, obj(json!({"error": code})), "");
}

/// A request whose head has been read: the body is not, and is only read once the head has passed (`read_body`).
pub(crate) struct Head {
    pub(crate) request: Request,
    length: usize,   // the declared length of the body
    started: Instant, // one deadline for the whole request, not one per read
}

/// Read the request head. Err carries the status to answer with.
pub(crate) fn read_head(stream: &mut TcpStream) -> Result<Head, u16> {
    let started = Instant::now();
    let mut data: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 2048];
    let head_end = loop {
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if data.len() > MAX_HEAD || started.elapsed() > READ_DEADLINE {
            return Err(400);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(400),
            Ok(n) => data.extend_from_slice(&chunk[..n]),
        }
    };
    let head = std::str::from_utf8(&data[..head_end]).map_err(|_| 400u16)?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next().ok_or(400u16)?.split(' ');
    let (method, target, version) = (first.next().ok_or(400u16)?, first.next().ok_or(400u16)?, first.next().ok_or(400u16)?);
    if version != "HTTP/1.1" || !target.starts_with('/') {
        return Err(400);
    }
    let mut headers = Vec::new();
    for line in lines {
        let (key, value) = line.split_once(':').ok_or(400u16)?;
        headers.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        if headers.len() > 32 {
            return Err(400);
        }
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let length: usize = match headers.iter().find(|(k, _)| k == "content-length") {
        Some((_, v)) => v.parse().map_err(|_| 400u16)?,
        None => 0,
    };
    let body = data[head_end + 4..].to_vec(); // what arrived with the head
    Ok(Head { request: Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), headers, body }, length, started })
}

/// Read the rest of a bounded body. Err carries the status to answer with.
pub(crate) fn read_body(stream: &mut TcpStream, head: Head, max: usize) -> Result<Request, u16> {
    let Head { mut request, length, started } = head;
    if length > max {
        return Err(413);
    }
    let mut chunk = [0u8; 2048];
    while request.body.len() < length {
        if started.elapsed() > READ_DEADLINE {
            return Err(400);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(400),
            Ok(n) => request.body.extend_from_slice(&chunk[..n]),
        }
    }
    request.body.truncate(length);
    Ok(request)
}

/// Read what the client still sends, so that it can see the answer before the connection closes (a close with unread data resets it).
pub(crate) fn drain(stream: &mut TcpStream) {
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let mut sink = [0u8; 4096];
    let mut drained = 0;
    while drained < 65536 && stream.read(&mut sink).map_or(false, |n| n > 0) {
        drained += 4096;
    }
}

fn id_ok(text: &str) -> bool {
    !text.is_empty() && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn body_map(request: &Request) -> Result<Map<String, Value>, &'static str> {
    match serde_json::from_slice::<Value>(&request.body) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err("bad_request"),
    }
}

fn decode_query_component(raw: &str) -> Result<String, &'static str> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err("bad_request");
                }
                let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).map_err(|_| "bad_request")?;
                let byte = u8::from_str_radix(hex, 16).map_err(|_| "bad_request")?;
                out.push(byte);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "bad_request")
}

fn query_map(query: &str) -> Result<Map<String, Value>, &'static str> {
    let mut out = Map::new();
    if query.is_empty() {
        return Ok(out);
    }
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').ok_or("bad_request")?;
        let key = decode_query_component(k)?;
        let val = decode_query_component(v)?;
        if key.is_empty() || out.contains_key(&key) {
            return Err("bad_request");
        }
        out.insert(key, Value::String(val));
    }
    Ok(out)
}

/// Map an authenticated request to a host operation; Err is a v1 error code.
fn route(shared: &Arc<Shared>, request: &Request) -> Result<Map<String, Value>, (u16, &'static str)> {
    let parts: Vec<&str> = request.path.trim_start_matches('/').split('/').collect();
    let method = request.method.as_str();
    let bad = |code: &'static str| (status_of(code), code);
    let allowed = |ok: bool| if ok { Ok(()) } else { Err((405u16, "bad_request")) };
    let result = match parts.as_slice() {
        ["v1"] => {
            allowed(method == "POST")?;
            let map = body_map(request).map_err(bad)?;
            if let Some(v) = map.get("v").and_then(Value::as_i64) {
                if v != 1 {
                    return Err(bad("version_unsupported"));
                }
            }
            let Some(op) = map.get("op").and_then(Value::as_str) else {
                return Err(bad("bad_request"));
            };
            if !core::OPS.contains(&op) {
                return Err(bad("unknown_op"));
            }
            if !valid_request(op, &map) {
                return Err(bad("bad_request"));
            }
            let rid = map.get("id").cloned();
            let mut body = crate::ipc::dispatch_request(shared, op, &map, true).unwrap_or(Err("bad_request")).map_err(bad)?;
            if let Some(id) = rid {
                body.insert("id".into(), id);
            }
            Ok(body)
        }
        ["v1", "capabilities"] => {
            allowed(method == "GET")?;
            crate::admin::capabilities(shared)
        }
        ["v1", "status"] => {
            allowed(method == "GET")?;
            core::status(shared)
        }
        ["v1", "resources"] => {
            allowed(method == "GET")?;
            core::resources(shared)
        }
        ["v1", "accounts"] => match method {
            "GET" => core::account_list(shared),
            "POST" => {
                let mut map = body_map(request).map_err(bad)?;
                map.insert("op".into(), json!("account.create"));
                if !valid_request("account.create", &map) {
                    return Err(bad("bad_request"));
                }
                core::account_create(shared, crate::ipc::text(&map, "provider"), crate::ipc::text(&map, "label"))
            }
            _ => return Err((405, "bad_request")),
        },
        ["v1", "settings"] => match method {
            "GET" => Ok(obj(json!({"settings": crate::clipboard::settings::status_json(&shared.cfg.handoff_dir)}))),
            "POST" => {
                let mut map = body_map(request).map_err(bad)?;
                map.insert("op".into(), json!("settings.set"));
                if !valid_request("settings.set", &map) {
                    return Err(bad("bad_request"));
                }
                crate::clipboard::settings::update_settings(
                    &shared.cfg.handoff_dir,
                    crate::clipboard::settings::Patch {
                        clipboard_write: map.get("clipboard_write").and_then(Value::as_bool),
                        notifications: map.get("notifications").and_then(Value::as_bool),
                        group_by: map.get("group_by").and_then(Value::as_str),
                        font_px: int(&map, "font_px"),
                    },
                )
            }
            _ => return Err((405, "bad_request")),
        },
        ["v1", "sessions"] => match method {
            "GET" => core::session_list(shared),
            "POST" => {
                let map = body_map(request).map_err(bad)?;
                if !valid_request("session.create", &map.clone().into_iter().chain([("op".to_string(), json!("session.create"))]).collect()) {
                    return Err(bad("bad_request"));
                }
                let kind = crate::ipc::text(&map, "kind").to_string();
                core::session_create(shared, &kind, int(&map, "rate").unwrap_or(0) as u64, int(&map, "history").unwrap_or(0) as u64,
                                     int(&map, "payload").unwrap_or(shared.cfg.payload as i64) as usize,
                                     map.get("account").and_then(|a| a.as_str()), crate::ipc::text(&map, "mode"),
                                     crate::folders::Settings::of_request(&map))
            }
            _ => return Err((405, "bad_request")),
        },
        ["v1", "sessions", id] if id_ok(id) => {
            allowed(method == "GET")?;
            core::session_one(shared, id)
        }
        ["v1", "sessions", id, "snapshot"] if id_ok(id) => {
            allowed(method == "GET")?;
            core::snapshot(shared, id, None)
        }
        ["v1", "sessions", id, "events"] if id_ok(id) => {
            allowed(method == "GET")?;
            let after = match request.query.as_str() {
                "" => -1,
                q => q.strip_prefix("after=").and_then(|n| n.parse::<i64>().ok()).filter(|n| *n >= -1).ok_or_else(|| bad("bad_request"))?,
            };
            core::events(shared, id, after)
        }
        ["v1", "sessions", id, "history"] if id_ok(id) => {
            allowed(method == "GET")?;
            let qmap = query_map(&request.query).map_err(bad)?;
            if qmap.keys().any(|k| k != "before" && k != "limit") {
                return Err(bad("bad_request"));
            }
            let before = qmap.get("before").and_then(Value::as_str).and_then(|s| s.parse::<i64>().ok()).ok_or_else(|| bad("bad_request"))?;
            let limit = match qmap.get("limit") {
                Some(v) => v.as_str().and_then(|s| s.parse::<i64>().ok()).ok_or_else(|| bad("bad_request"))?,
                None => 100,
            };
            let req: Map<String, Value> = json!({"op": "history", "session": id, "before": before, "limit": limit}).as_object().cloned().unwrap_or_default();
            if !valid_request("history", &req) {
                return Err(bad("bad_request"));
            }
            core::history(shared, id, before, limit)
        }
        ["v1", "search"] => {
            allowed(method == "GET")?;
            let qmap = query_map(&request.query).map_err(bad)?;
            if qmap.keys().any(|k| k != "q" && k != "session" && k != "limit") {
                return Err(bad("bad_request"));
            }
            let query = qmap.get("q").and_then(Value::as_str).ok_or_else(|| bad("bad_request"))?;
            let mut req = Map::new();
            req.insert("op".into(), json!("session.search"));
            req.insert("query".into(), json!(query));
            if let Some(s) = qmap.get("session").and_then(Value::as_str) {
                req.insert("session".into(), json!(s));
            }
            let limit = match qmap.get("limit") {
                Some(v) => {
                    let parsed = v.as_str().and_then(|s| s.parse::<i64>().ok()).ok_or_else(|| bad("bad_request"))?;
                    req.insert("limit".into(), json!(parsed));
                    parsed
                }
                None => crate::search::DEFAULT_LIMIT as i64,
            };
            if !valid_request("session.search", &req) {
                return Err(bad("bad_request"));
            }
            crate::search::search_op(shared, query, req.get("session").and_then(Value::as_str), limit as usize)
        }
        ["v1", "files"] => {
            allowed(method == "GET")?;
            let qmap = query_map(&request.query).map_err(bad)?;
            if qmap.len() != 1 || !qmap.contains_key("cwd") {
                return Err(bad("bad_request"));
            }
            let cwd = qmap.get("cwd").and_then(Value::as_str).ok_or_else(|| bad("bad_request"))?;
            let req: Map<String, Value> = json!({"op": "folder.list", "cwd": cwd}).as_object().cloned().unwrap_or_default();
            if !valid_request("folder.list", &req) {
                return Err(bad("bad_request"));
            }
            crate::fsview::folder_list(shared, cwd)
        }
        ["v1", "files", "read"] => {
            allowed(method == "GET")?;
            let qmap = query_map(&request.query).map_err(bad)?;
            if qmap.len() != 1 || !qmap.contains_key("path") {
                return Err(bad("bad_request"));
            }
            let path = qmap.get("path").and_then(Value::as_str).ok_or_else(|| bad("bad_request"))?;
            let req: Map<String, Value> = json!({"op": "folder.read", "path": path}).as_object().cloned().unwrap_or_default();
            if !valid_request("folder.read", &req) {
                return Err(bad("bad_request"));
            }
            crate::fsview::folder_read(shared, path)
        }
        ["v1", "sessions", id, "input"] if id_ok(id) => {
            allowed(method == "POST")?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("input"));
            map.insert("session".into(), json!(id));
            if !valid_request("input", &map) || map.contains_key("attachment") { // a picture goes by the socket only (S05a): this route never reads a file
                return Err(bad("bad_request"));
            }
            core::input(shared, id, Some(int(&map, "generation").unwrap_or(0)), crate::ipc::text(&map, "request_id"), crate::ipc::text(&map, "text"), crate::session::Origin::Person, None)
        }
        ["v1", "sessions", id, "mode"] if id_ok(id) => {
            allowed(method == "POST")?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("session.mode"));
            map.insert("session".into(), json!(id));
            if !valid_request("session.mode", &map) {
                return Err(bad("bad_request"));
            }
            crate::admin::session_mode(shared, id, int(&map, "generation").unwrap_or(0), crate::ipc::text(&map, "mode"))
        }
        ["v1", "sessions", id, "rebind"] if id_ok(id) => {
            allowed(method == "POST")?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("session.rebind"));
            map.insert("session".into(), json!(id));
            if !valid_request("session.rebind", &map) {
                return Err(bad("bad_request"));
            }
            core::session_rebind(shared, id, int(&map, "generation").unwrap_or(0), crate::ipc::text(&map, "account"))
        }
        ["v1", "accounts", id, "rename"] if id_ok(id) => {
            allowed(method == "POST")?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("account.rename"));
            map.insert("account".into(), json!(id));
            if !valid_request("account.rename", &map) {
                return Err(bad("bad_request"));
            }
            crate::admin::account_rename(shared, id, crate::ipc::text(&map, "label"))
        }
        ["v1", "approvals", number, "response"] => {
            allowed(method == "POST")?;
            let approval: i64 = number.parse().map_err(|_| bad("bad_request"))?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("approval.answer"));
            map.insert("approval_id".into(), json!(approval));
            if !valid_request("approval.answer", &map) {
                return Err(bad("bad_request"));
            }
            core::approval_answer(shared, crate::ipc::text(&map, "session"), int(&map, "generation").unwrap_or(0), approval, crate::ipc::text(&map, "answer"), map.get("keep").and_then(Value::as_bool).unwrap_or(false))
        }
        ["v1", "views"] => {
            allowed(method == "POST")?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("view.attach"));
            if !valid_request("view.attach", &map) {
                return Err(bad("bad_request"));
            }
            core::view_attach(shared, None, crate::ipc::text(&map, "session"), int(&map, "generation"), int(&map, "after").unwrap_or(-1))
        }
        ["v1", "views", view] if id_ok(view) => {
            allowed(method == "DELETE")?;
            core::view_detach(shared, view)
        }
        _ => return Err((404, "unknown_op")),
    };
    result.map_err(|code| (status_of(code), code))
}

pub(crate) fn take(bucket: &Mutex<Bucket>, per_second: f64, burst: f64) -> bool {
    let mut b = bucket.lock().unwrap();
    let now = Instant::now();
    b.tokens = (b.tokens + now.duration_since(b.at).as_secs_f64() * per_second).min(burst);
    b.at = now;
    if b.tokens < 1.0 {
        return false;
    }
    b.tokens -= 1.0;
    true
}

fn serve_one(mut stream: TcpStream, shared: Arc<Shared>, port: u16, token: Arc<String>, buckets: Arc<(Mutex<Bucket>, Mutex<Bucket>)>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let head = match read_head(&mut stream) {
        Ok(head) => head,
        Err(status) => {
            error(&mut stream, status, "bad_request");
            return drain(&mut stream);
        }
    };
    // Host and Origin are checked first: they defeat DNS rebinding and cross-site requests. Then the credential. All of it on the head: the body (up to
    // 256 KiB, and the client may never send it) is read only for a request that has passed, so a caller without the token cannot hold a connection with one.
    let request = &head.request;
    let host_ok = matches!(header(request, "host"), Some(h) if h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    let origin_ok = match header(request, "origin") {
        None => true,
        Some(o) => o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}"),
    };
    if !host_ok || !origin_ok {
        error(&mut stream, 403, "bad_request");
        return drain(&mut stream);
    }
    let presented = header(request, "authorization").and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if !constant_time_eq(presented.as_bytes(), token.as_bytes()) {
        // failed attempts have their own small budget, so guessing is slow and cannot starve real callers
        if !take(&buckets.1, FAILED_AUTH_PER_SECOND, FAILED_AUTH_PER_SECOND) {
            respond(&mut stream, 429, obj(json!({"error": "busy"})), "Retry-After: 1\r\n");
        } else {
            respond(&mut stream, 401, obj(json!({"error": "bad_request"})), "WWW-Authenticate: Bearer\r\n");
        }
        return drain(&mut stream);
    }
    if !take(&buckets.0, RATE_PER_SECOND, BURST) {
        respond(&mut stream, 429, obj(json!({"error": "busy"})), "Retry-After: 1\r\n");
        return drain(&mut stream);
    }
    let request = match read_body(&mut stream, head, MAX_BODY) {
        Ok(request) => request,
        Err(status) => {
            error(&mut stream, status, if status == 413 { "too_large" } else { "bad_request" });
            return drain(&mut stream);
        }
    };
    match route(&shared, &request) {
        Ok(body) => respond(&mut stream, 200, body, ""),
        Err((405, _)) => respond(&mut stream, 405, obj(json!({"error": "bad_request"})), ""),
        Err((status, code)) => error(&mut stream, status, code),
    }
}

/// Take the loopback port; port 0 lets the system choose, and the listener says which it chose.
pub fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

pub fn serve(listener: TcpListener, shared: Arc<Shared>, token: String) {
    let port = listener.local_addr().map_or(0, |address| address.port());
    let token = Arc::new(token);
    let active = Arc::new(AtomicUsize::new(0));
    let buckets = Arc::new((Mutex::new(Bucket { tokens: BURST, at: Instant::now() }), Mutex::new(Bucket { tokens: FAILED_AUTH_PER_SECOND, at: Instant::now() })));
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            thread::sleep(Duration::from_millis(50)); // out of descriptors or a failed accept: do not spin
            continue;
        };
        if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
            let mut stream = stream;
            error(&mut stream, 503, "busy");
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        let (shared, token, buckets, active) = (shared.clone(), token.clone(), buckets.clone(), active.clone());
        thread::spawn(move || {
            serve_one(stream, shared, port, token, buckets);
            active.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
