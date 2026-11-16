//! The call: check the request, connect, write, read the head, read the body by its framing, every wait bounded.

use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::{Cancel, Flow, Head, HttpError, Limits, Method, Phase, Request, MAX_HEAD_BYTES};

const POLL: Duration = Duration::from_millis(250);
const READ: usize = 8192;
const MAX_LINE: usize = 1024;

fn bad(s: &str) -> HttpError {
    HttpError::BadResponse(s.to_string())
}

fn token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// The request line and headers as bytes, or why the request is refused. Messages name a header, never show its value.
fn build(req: &Request) -> Result<Vec<u8>, HttpError> {
    let path = req.path.as_bytes();
    if path.first() != Some(&b'/') || path.iter().any(|b| !b.is_ascii_graphic()) {
        return Err(HttpError::BadRequest("the path must start with `/` and have only visible ASCII characters".into()));
    }
    let mut out = Vec::with_capacity(256 + req.body.len());
    out.extend_from_slice(if req.method == Method::Get { b"GET " } else { b"POST " });
    out.extend_from_slice(path);
    out.extend_from_slice(format!(" HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUser-Agent: lico\r\nAccept: */*\r\nConnection: close\r\n", req.addr.port()).as_bytes());
    for (name, value) in req.headers {
        if name.is_empty() || !name.bytes().all(token_char) {
            return Err(HttpError::BadRequest("a header name has a character that is not allowed".into()));
        }
        if ["host", "content-length", "connection", "transfer-encoding"].contains(&name.to_ascii_lowercase().as_str()) {
            return Err(HttpError::BadRequest(format!("the header {name} is set by the client")));
        }
        if value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
            return Err(HttpError::BadRequest(format!("the value of the header {name} has a control character")));
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    if !req.body.is_empty() || req.method == Method::Post {
        out.extend_from_slice(format!("Content-Length: {}\r\n", req.body.len()).as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(req.body);
    Ok(out)
}

/// How the body is framed.
enum Framing {
    None,
    Length(usize),
    Chunked,
    UntilClose,
}

fn parse_head(raw: &[u8]) -> Result<Head, HttpError> {
    let text = std::str::from_utf8(raw).map_err(|_| bad("the head is not text"))?;
    let mut lines = text.trim_end_matches("\r\n").split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let mut parts = status_line.splitn(3, ' ');
    let (version, code) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !version.starts_with("HTTP/1.") || code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad("the status line is not HTTP"));
    }
    let status: u16 = code.parse().map_err(|_| bad("the status is not a number"))?;
    if !(100..=599).contains(&status) {
        return Err(bad("the status is out of range"));
    }
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { return Err(bad("a header line has no colon")) };
        if name.is_empty() || !name.bytes().all(token_char) {
            return Err(bad("a header name is not allowed"));
        }
        headers.push((name.to_ascii_lowercase(), value.trim_matches(|c| c == ' ' || c == '\t').to_string()));
    }
    Ok(Head { status, headers })
}

fn framing_of(head: &Head, max_body: usize) -> Result<Framing, HttpError> {
    if head.status < 200 || head.status == 204 || head.status == 304 {
        return Ok(Framing::None);
    }
    if let Some((_, te)) = head.headers.iter().find(|(n, _)| n == "transfer-encoding") {
        return if te.to_ascii_lowercase().contains("chunked") { Ok(Framing::Chunked) } else { Err(bad("an unsupported transfer encoding")) };
    }
    let lengths: Vec<&str> = head.headers.iter().filter(|(n, _)| n == "content-length").map(|(_, v)| v.as_str()).collect();
    if let Some(first) = lengths.first() {
        if lengths.iter().any(|l| l != first) {
            return Err(bad("two different Content-Length values"));
        }
        if first.is_empty() || !first.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad("the Content-Length is not a number"));
        }
        let n: usize = first.parse().map_err(|_| bad("the Content-Length is too large"))?;
        if n > max_body {
            return Err(HttpError::TooLarge);
        }
        return Ok(Framing::Length(n));
    }
    Ok(Framing::UntilClose)
}

enum Chunk {
    Size(Vec<u8>),
    Data(usize),
    End(u8),
    Trailer(Vec<u8>),
}

/// The body reader: bytes in, pieces out through the callback; `Ok(true)` when the body is complete.
struct Body<'a> {
    framing: Framing,
    chunk: Chunk,
    remaining: usize,
    delivered: usize,
    max: usize,
    on_body: &'a mut dyn FnMut(&[u8]) -> Flow,
    stopped: bool,
}

impl Body<'_> {
    fn give(&mut self, bytes: &[u8]) -> Result<(), HttpError> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.delivered += bytes.len();
        if self.delivered > self.max {
            return Err(HttpError::TooLarge);
        }
        if (self.on_body)(bytes) == Flow::Stop {
            self.stopped = true;
        }
        Ok(())
    }

    fn feed(&mut self, mut data: &[u8]) -> Result<bool, HttpError> {
        loop {
            if self.stopped {
                return Ok(true);
            }
            match self.framing {
                Framing::None => return Ok(true),
                Framing::UntilClose => {
                    self.give(data)?;
                    return Ok(self.stopped);
                }
                Framing::Length(_) => {
                    let take = data.len().min(self.remaining);
                    self.give(&data[..take])?;
                    self.remaining -= take;
                    return Ok(self.remaining == 0 || self.stopped);
                }
                Framing::Chunked => {}
            }
            if data.is_empty() {
                return Ok(false);
            }
            match &mut self.chunk {
                Chunk::Size(line) => {
                    let b = data[0];
                    data = &data[1..];
                    line.push(b);
                    if line.len() > MAX_LINE {
                        return Err(bad("a chunk size line is too long"));
                    }
                    if line.ends_with(b"\r\n") {
                        let text = std::str::from_utf8(&line[..line.len() - 2]).map_err(|_| bad("a chunk size is not text"))?;
                        let digits = text.split(';').next().unwrap_or("").trim();
                        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                            return Err(bad("a chunk size is not hexadecimal"));
                        }
                        let size = u64::from_str_radix(digits, 16).map_err(|_| bad("a chunk size is too large"))?;
                        let size = usize::try_from(size).map_err(|_| bad("a chunk size is too large"))?;
                        self.chunk = if size == 0 { Chunk::Trailer(Vec::new()) } else { Chunk::Data(size) };
                    }
                }
                Chunk::Data(left) => {
                    let take = data.len().min(*left);
                    *left -= take;
                    let done = *left == 0;
                    let piece = &data[..take];
                    data = &data[take..];
                    if done {
                        self.chunk = Chunk::End(0);
                    }
                    self.give(piece)?;
                }
                Chunk::End(seen) => {
                    let b = data[0];
                    data = &data[1..];
                    match (*seen, b) {
                        (0, b'\r') => *seen = 1,
                        (1, b'\n') => self.chunk = Chunk::Size(Vec::new()),
                        _ => return Err(bad("a chunk does not end with a line end")),
                    }
                }
                Chunk::Trailer(line) => {
                    let b = data[0];
                    data = &data[1..];
                    line.push(b);
                    if line.len() > MAX_HEAD_BYTES {
                        return Err(bad("the trailer is too long"));
                    }
                    if line.as_slice() == b"\r\n" {
                        return Ok(true);
                    }
                    if line.ends_with(b"\r\n") {
                        line.clear();
                    }
                }
            }
        }
    }

    /// The connection ended: is that the end of the body?
    fn ended(&self) -> Result<(), HttpError> {
        match self.framing {
            Framing::None | Framing::UntilClose => Ok(()),
            Framing::Length(_) if self.remaining == 0 => Ok(()),
            Framing::Length(_) => Err(bad("the body is shorter than its Content-Length")),
            Framing::Chunked => Err(bad("the body was cut before its last chunk")),
        }
    }
}

pub(crate) fn run(req: &Request, limits: &Limits, cancel: &Cancel, on_head: &mut dyn FnMut(&Head) -> Flow, on_body: &mut dyn FnMut(&[u8]) -> Flow) -> Result<Head, HttpError> {
    if !req.addr.ip().is_loopback() {
        return Err(HttpError::NotLoopback);
    }
    let mut message = build(req)?;
    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(&req.addr, limits.connect).map_err(|e| match e.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => HttpError::Timeout(Phase::Connect),
        k => HttpError::Connect(k),
    })?;
    let _ = stream.set_nodelay(true);
    let left = limits.total.saturating_sub(started.elapsed()).max(Duration::from_millis(1));
    let _ = stream.set_write_timeout(Some(left.min(Duration::from_secs(10))));
    let sent = stream.write_all(&message);
    message.iter_mut().for_each(|b| *b = 0);
    std::hint::black_box(&message);
    sent.map_err(|e| HttpError::Io(e.kind()))?;
    let sent_at = Instant::now();
    let mut last_read = sent_at;
    let mut got_any = false;
    let mut head_bytes: Vec<u8> = Vec::new();
    let mut head: Option<Head> = None;
    let mut body: Option<Body> = None;
    let mut buf = [0u8; READ];
    loop {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        let now = Instant::now();
        if now.duration_since(started) >= limits.total {
            return Err(HttpError::Timeout(Phase::Total));
        }
        if !got_any && now.duration_since(sent_at) >= limits.first_byte {
            return Err(HttpError::Timeout(Phase::FirstByte));
        }
        if got_any && now.duration_since(last_read) >= limits.idle {
            return Err(HttpError::Timeout(Phase::Idle));
        }
        let wait = POLL.min(limits.total.saturating_sub(now.duration_since(started))).max(Duration::from_millis(1));
        let _ = stream.set_read_timeout(Some(wait));
        match stream.read(&mut buf) {
            Ok(0) => {
                return match (&head, &body) {
                    (Some(h), Some(b)) => b.ended().map(|()| h.clone()),
                    _ => Err(bad("the connection closed before the head was complete")),
                };
            }
            Ok(n) => {
                got_any = true;
                last_read = Instant::now();
                let mut data = &buf[..n];
                if head.is_none() {
                    head_bytes.extend_from_slice(data);
                    let Some(end) = head_bytes.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4) else {
                        if head_bytes.len() > MAX_HEAD_BYTES {
                            return Err(bad("the head is too long"));
                        }
                        continue;
                    };
                    if end > MAX_HEAD_BYTES {
                        return Err(bad("the head is too long"));
                    }
                    let parsed = parse_head(&head_bytes[..end])?;
                    let framing = framing_of(&parsed, limits.max_body)?;
                    if on_head(&parsed) == Flow::Stop {
                        return Ok(parsed);
                    }
                    let remaining = if let Framing::Length(n) = framing { n } else { 0 };
                    body = Some(Body { framing, chunk: Chunk::Size(Vec::new()), remaining, delivered: 0, max: limits.max_body, on_body: &mut *on_body, stopped: false });
                    head = Some(parsed);
                    let rest = head_bytes.split_off(end);
                    head_bytes = Vec::new();
                    let b = body.as_mut().unwrap();
                    if matches!(b.framing, Framing::None) || (matches!(b.framing, Framing::Length(0))) {
                        return Ok(head.unwrap());
                    }
                    if b.feed(&rest)? {
                        return Ok(head.unwrap());
                    }
                    continue;
                }
                if let Some(b) = body.as_mut() {
                    if b.feed(std::mem::take(&mut data))? {
                        return Ok(head.unwrap());
                    }
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(e) => return Err(HttpError::Io(e.kind())),
        }
    }
}
