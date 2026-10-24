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
const MAX_BODY: usize = 4096;
const MAX_CONNECTIONS: usize = 32;
const RATE_PER_SECOND: f64 = 200.0;
const BURST: f64 = 50.0;
const FAILED_AUTH_PER_SECOND: f64 = 20.0;
const READ_DEADLINE: Duration = Duration::from_secs(10);

struct Bucket {
    tokens: f64,
    at: Instant,
}

struct Request {
    method: String,
    path: String,
    query: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn status_of(code: &str) -> u16 {
    match code {
        "bad_request" | "version_unsupported" => 400,
        "unknown_op" | "unknown_session" | "unknown_view" | "unknown_request" | "unknown_approval" => 404,
        "stale_generation" | "session_exited" => 409,
        "too_large" => 413,
        _ => 503,
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
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

/// Read the request head and a bounded body; Err carries the status to answer with.
fn read_request(stream: &mut TcpStream) -> Result<Request, u16> {
    let started = Instant::now(); // one deadline for the whole request, not one per read
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
    if length > MAX_BODY {
        return Err(413);
    }
    let mut body = data[head_end + 4..].to_vec();
    while body.len() < length {
        if started.elapsed() > READ_DEADLINE {
            return Err(400);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(400),
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(length);
    Ok(Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), headers, body })
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

/// Map an authenticated request to a host operation; Err is a v1 error code.
fn route(shared: &Arc<Shared>, request: &Request) -> Result<Map<String, Value>, (u16, &'static str)> {
    let parts: Vec<&str> = request.path.trim_start_matches('/').split('/').collect();
    let method = request.method.as_str();
    let bad = |code: &'static str| (status_of(code), code);
    let allowed = |ok: bool| if ok { Ok(()) } else { Err((405u16, "bad_request")) };
    let result = match parts.as_slice() {
        ["v1", "status"] => {
            allowed(method == "GET")?;
            core::status(shared)
        }
        ["v1", "resources"] => {
            allowed(method == "GET")?;
            core::resources(shared)
        }
        ["v1", "sessions"] => match method {
            "GET" => core::session_list(shared),
            "POST" => {
                let map = body_map(request).map_err(bad)?;
                if !valid_request("session.create", &map.clone().into_iter().chain([("op".to_string(), json!("session.create"))]).collect()) {
                    return Err(bad("bad_request"));
                }
                let kind = crate::ipc::text(&map, "kind").to_string();
                core::session_create(shared, &kind, int(&map, "rate").unwrap_or(0) as u64, int(&map, "history").unwrap_or(0) as u64,
                                     int(&map, "payload").unwrap_or(shared.cfg.payload as i64) as usize)
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
        ["v1", "sessions", id, "input"] if id_ok(id) => {
            allowed(method == "POST")?;
            let mut map = body_map(request).map_err(bad)?;
            map.insert("op".into(), json!("input"));
            map.insert("session".into(), json!(id));
            if !valid_request("input", &map) {
                return Err(bad("bad_request"));
            }
            core::input(shared, id, int(&map, "generation").unwrap_or(0), crate::ipc::text(&map, "request_id"), crate::ipc::text(&map, "text"))
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
            core::approval_answer(shared, crate::ipc::text(&map, "session"), int(&map, "generation").unwrap_or(0), approval, crate::ipc::text(&map, "answer"))
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

fn take(bucket: &Mutex<Bucket>, per_second: f64, burst: f64) -> bool {
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
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(status) => {
            let code = if status == 413 { "too_large" } else { "bad_request" };
            error(&mut stream, status, code);
            // read what the client still sends so it can see the answer before the connection closes
            let _ = stream.shutdown(Shutdown::Write);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut sink = [0u8; 4096];
            let mut drained = 0;
            while drained < 65536 && stream.read(&mut sink).map_or(false, |n| n > 0) {
                drained += 4096;
            }
            return;
        }
    };
    // Host and Origin are checked first: they defeat DNS rebinding and cross-site requests
    let host_ok = matches!(header(&request, "host"), Some(h) if h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    let origin_ok = match header(&request, "origin") {
        None => true,
        Some(o) => o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}"),
    };
    if !host_ok || !origin_ok {
        return error(&mut stream, 403, "bad_request");
    }
    let presented = header(&request, "authorization").and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if !constant_time_eq(presented.as_bytes(), token.as_bytes()) {
        // failed attempts have their own small budget, so guessing is slow and cannot starve real callers
        if !take(&buckets.1, FAILED_AUTH_PER_SECOND, FAILED_AUTH_PER_SECOND) {
            return respond(&mut stream, 429, obj(json!({"error": "busy"})), "Retry-After: 1\r\n");
        }
        return respond(&mut stream, 401, obj(json!({"error": "bad_request"})), "WWW-Authenticate: Bearer\r\n");
    }
    if !take(&buckets.0, RATE_PER_SECOND, BURST) {
        return respond(&mut stream, 429, obj(json!({"error": "busy"})), "Retry-After: 1\r\n");
    }
    match route(&shared, &request) {
        Ok(body) => respond(&mut stream, 200, body, ""),
        Err((405, _)) => respond(&mut stream, 405, obj(json!({"error": "bad_request"})), ""),
        Err((status, code)) => error(&mut stream, status, code),
    }
}

pub fn serve(shared: Arc<Shared>, port: u16, token: String) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind the loopback listener");
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
