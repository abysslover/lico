//! The server: one accept thread, one thread per connection, everything bounded and stoppable.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::script::{Behaviour, Body, Ending, Reply, Rule, Script};

const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 8 * 1024 * 1024;
const REQUEST_DEADLINE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

/// A request the fake received. `Debug` never shows the `Authorization` value; `authorization()` gives it to a test that wants to check it.
#[derive(Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn authorization(&self) -> Option<&str> {
        self.header("authorization")
    }
}

impl std::fmt::Debug for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(&str, &str)> = self.headers.iter().map(|(n, v)| (n.as_str(), if n == "authorization" { "<redacted>" } else { v.as_str() })).collect();
        f.debug_struct("Seen").field("method", &self.method).field("path", &self.path).field("headers", &headers).field("body_bytes", &self.body.len()).finish()
    }
}

struct Shared {
    stop: AtomicBool,
    rules: Mutex<Vec<Rule>>,
    seen: Mutex<Vec<Seen>>,
}

pub struct Fake {
    addr: SocketAddr,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<Vec<JoinHandle<()>>>>,
}

impl Fake {
    /// Starts listening on `127.0.0.1` with an ephemeral port.
    pub fn start(script: Script) -> std::io::Result<Fake> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared { stop: AtomicBool::new(false), rules: Mutex::new(script.rules), seen: Mutex::new(Vec::new()) });
        let s = shared.clone();
        let accept = thread::spawn(move || {
            let mut handlers = Vec::new();
            while !s.stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let s = s.clone();
                        handlers.push(thread::spawn(move || handle(stream, &s)));
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(POLL),
                    Err(_) => thread::sleep(POLL),
                }
            }
            handlers
        });
        Ok(Fake { addr, shared, accept: Some(accept) })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Every request received so far, in order.
    pub fn requests(&self) -> Vec<Seen> {
        self.shared.seen.lock().unwrap().clone()
    }

    /// Stops listening and ends every connection; returns when all threads are done.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept.take() {
            if let Ok(handlers) = h.join() {
                for h in handlers {
                    let _ = h.join();
                }
            }
        }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn stopped(s: &Shared) -> bool {
    s.stop.load(Ordering::Relaxed)
}

/// Sleeps `d` in slices; false when the fake was stopped meanwhile.
fn wait(d: Duration, s: &Shared) -> bool {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if stopped(s) {
            return false;
        }
        thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
    }
    !stopped(s)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn handle(mut stream: TcpStream, s: &Shared) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(POLL));
    let Some(seen) = read_request(&mut stream, s) else { return };
    let behaviour = {
        let mut rules = s.rules.lock().unwrap();
        let found = rules.iter_mut().find(|r| r.method == seen.method && r.path == seen.path && r.remaining != Some(0));
        match found {
            Some(rule) => {
                if let Some(n) = rule.remaining.as_mut() {
                    *n -= 1;
                }
                rule.behaviour.clone()
            }
            None => Behaviour::Answer(Reply::json(404, &format!("{{\"error\":\"no script for {} {}\"}}", seen.method, seen.path))),
        }
    };
    s.seen.lock().unwrap().push(seen);
    match behaviour {
        Behaviour::Close => {}
        Behaviour::Hang => hold(&mut stream, s),
        Behaviour::Answer(reply) => answer(&mut stream, &reply, s),
    }
    let _ = stream.shutdown(Shutdown::Both);
}

/// Keeps the connection open and silent until the client closes it or the fake stops.
fn hold(stream: &mut TcpStream, s: &Shared) {
    let mut one = [0u8; 1];
    while !stopped(s) {
        match stream.read(&mut one) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}

fn read_request(stream: &mut TcpStream, s: &Shared) -> Option<Seen> {
    let started = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        if buf.len() > MAX_HEAD || stopped(s) || started.elapsed() > REQUEST_DEADLINE {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return None,
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_string(), first.next()?.to_string());
    let headers: Vec<(String, String)> = lines.filter_map(|l| l.split_once(':').map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))).collect();
    let length = headers.iter().find(|(n, _)| n == "content-length").and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
    if length > MAX_BODY {
        let _ = stream.write_all(b"HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return None;
    }
    let mut body = buf[head_end..].to_vec();
    while body.len() < length {
        if stopped(s) || started.elapsed() > REQUEST_DEADLINE {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return None,
        }
    }
    body.truncate(length);
    Some(Seen { method, path, headers, body })
}

fn answer(stream: &mut TcpStream, r: &Reply, s: &Shared) {
    if !wait(r.first_byte_delay, s) {
        return;
    }
    if let Body::Raw(bytes) = &r.body {
        let _ = stream.write_all(bytes);
        return;
    }
    let mut head = format!("HTTP/1.1 {} {}\r\n", r.status, reason(r.status));
    for (n, v) in &r.headers {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    match &r.body {
        Body::Whole(b) => head.push_str(&format!("Content-Length: {}\r\n", b.len())),
        Body::Chunked(_) => head.push_str("Transfer-Encoding: chunked\r\n"),
        _ => {}
    }
    head.push_str("Connection: close\r\n\r\n");
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    match &r.body {
        Body::Whole(b) => {
            let n = match r.ending {
                Ending::Complete => b.len(),
                Ending::Cut(n) | Ending::Silent(n) => n.min(b.len()),
            };
            if stream.write_all(&b[..n]).is_err() {
                return;
            }
            if let Ending::Silent(_) = r.ending {
                hold(stream, s);
            }
        }
        Body::Chunked(pieces) | Body::UntilClose(pieces) => {
            let chunked = matches!(r.body, Body::Chunked(_));
            let limit = match r.ending {
                Ending::Complete => pieces.len(),
                Ending::Cut(n) | Ending::Silent(n) => n.min(pieces.len()),
            };
            for (i, p) in pieces[..limit].iter().enumerate() {
                if i > 0 && !wait(r.gap, s) {
                    return;
                }
                let ok = if chunked { stream.write_all(format!("{:x}\r\n", p.len()).as_bytes()).and_then(|_| stream.write_all(p)).and_then(|_| stream.write_all(b"\r\n")) } else { stream.write_all(p) };
                if ok.is_err() {
                    return;
                }
            }
            match r.ending {
                Ending::Complete => {
                    if chunked {
                        let _ = stream.write_all(b"0\r\n\r\n");
                    }
                }
                Ending::Cut(_) => {}
                Ending::Silent(_) => hold(stream, s),
            }
        }
        Body::Raw(_) => {}
    }
}
