//! `lico-ws` (SPEC.md): a WebSocket connection between two ends on this machine. The client role always; the server role (reading an upgrade request and answering it)
//! only behind the feature `server`, for the fake CDP server and the tests. Loopback addresses only, no TLS, no proxy, no name lookup, no extensions, no subprotocols,
//! no thread. Std only.

mod frame;
mod sha1;
#[cfg(test)]
mod tests;

pub use frame::{encode_frame, Decoder, Incoming, Op, Role};

use std::collections::VecDeque;
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The longest head of an upgrade request or answer: a real one is under 1 KiB; 16 KiB leaves room for a long path and a token and stops a peer that never ends a head.
pub const MAX_HEAD: usize = 16 * 1024;
/// The most header lines in a head: a real one has about ten; a peer that sends hundreds is refused.
pub const MAX_HEADERS: usize = 64;
/// The default cap of one message (and of one frame): 4 MiB, from the first measurement of the engine (M17-P03): its largest screencast frame was 69,612 bytes; a 1920x1080 frame
/// is 8.3 MB as raw pixels and a compressed one is a fraction of it (ASSUMED: under 2 MB for a busy page), so 4 MiB leaves room and keeps a hostile peer from making the host buffer
/// more. A caller that knows better passes its own `Limits`.
pub const DEFAULT_MAX_MESSAGE: usize = 4 * 1024 * 1024;
/// The most a caller can ask for as the cap of a message: a bug elsewhere cannot make this crate buffer gigabytes.
pub const HARD_MAX_MESSAGE: usize = 64 * 1024 * 1024;
/// The most a control frame (ping, pong, close) may carry (RFC 6455 section 5.5).
pub const MAX_CONTROL_PAYLOAD: usize = 125;
/// A loopback connect is immediate; 3 s is for a machine under load.
pub const DEFAULT_CONNECT: Duration = Duration::from_secs(3);
/// The upgrade answer of a local program comes at once; 3 s is for a program that is starting.
pub const DEFAULT_HANDSHAKE: Duration = Duration::from_secs(3);
/// The longest a read or a write may wait for the peer by default; a caller that polls sets a shorter one with `set_read_timeout`.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct Limits {
    pub connect: Duration,
    pub handshake: Duration,
    pub idle: Duration,
    pub max_message: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { connect: DEFAULT_CONNECT, handshake: DEFAULT_HANDSHAKE, idle: DEFAULT_IDLE, max_message: DEFAULT_MAX_MESSAGE }
    }
}

/// What `connect` is given. `Debug` shows the names of the headers and never a value (a header may carry a token).
pub struct Request<'a> {
    pub addr: SocketAddr,
    pub path: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
}

impl fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(n, _)| *n).collect();
        f.debug_struct("Request").field("addr", &self.addr).field("path", &self.path).field("header_names", &names).finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Close { code: Option<u16>, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NotLoopback,
    Connect,
    Refused(u16),
    Handshake(&'static str),
    Timeout,
    Cancelled,
    Protocol(&'static str),
    TooBig,
    Closed,
    Io(ErrorKind),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotLoopback => write!(f, "the address is not a loopback address"),
            Error::Connect => write!(f, "could not connect"),
            Error::Refused(status) => write!(f, "the server refused the upgrade with status {status}"),
            Error::Handshake(why) => write!(f, "the upgrade failed: {why}"),
            Error::Timeout => write!(f, "timed out"),
            Error::Cancelled => write!(f, "cancelled"),
            Error::Protocol(why) => write!(f, "protocol error: {why}"),
            Error::TooBig => write!(f, "a message is over the cap"),
            Error::Closed => write!(f, "the connection is closed"),
            Error::Io(kind) => write!(f, "I/O error: {kind}"),
        }
    }
}

impl std::error::Error for Error {}

/// The accept key of a client key (RFC 6455 section 4.2.2).
pub fn accept_key(key: &str) -> String {
    sha1::base64(&sha1::sha1(format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes()))
}

/// Not a secret: a mask and a key only keep a proxy from mistaking the bytes for a request, and there is no proxy here. A counter, the clock and the process's random keys.
fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.finish()
}

fn new_key() -> String {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&random_u64().to_le_bytes());
    bytes[8..].copy_from_slice(&random_u64().to_le_bytes());
    sha1::base64(&bytes)
}

fn new_mask() -> [u8; 4] {
    loop {
        let m = (random_u64() as u32).to_le_bytes();
        if m != [0; 4] {
            return m;
        }
    }
}

/// Ends a blocking `recv` of the connection it came from, from any thread; the connection is dead afterwards.
pub struct Canceller {
    flag: Arc<AtomicBool>,
    stream: TcpStream,
}

impl Canceller {
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

pub struct Conn {
    stream: TcpStream,
    role: Role,
    dec: Decoder,
    pending: VecDeque<Result<Incoming, Error>>,
    idle: Duration,
    read_timeout: Option<Duration>,
    peer_closed: bool,
    sent_close: bool,
    cancelled: Arc<AtomicBool>,
}

impl fmt::Debug for Conn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Conn").field("role", &self.role).finish()
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

impl Conn {
    fn new(stream: TcpStream, role: Role, limits: &Limits, leftover: &[u8]) -> Conn {
        let _ = stream.set_nodelay(true);
        let mut c = Conn {
            stream,
            role,
            dec: Decoder::new(role, limits.max_message.min(HARD_MAX_MESSAGE)),
            pending: VecDeque::new(),
            idle: limits.idle,
            read_timeout: None,
            peer_closed: false,
            sent_close: false,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        if !leftover.is_empty() {
            c.pending.extend(c.dec.feed(leftover));
        }
        c
    }

    pub fn send_text(&mut self, text: &str) -> Result<(), Error> {
        self.send(Op::Text, text.as_bytes())
    }

    pub fn send_binary(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.send(Op::Binary, bytes)
    }

    pub fn send_ping(&mut self, payload: &[u8]) -> Result<(), Error> {
        if payload.len() > MAX_CONTROL_PAYLOAD {
            return Err(Error::Protocol("a control frame over 125 bytes"));
        }
        self.send(Op::Ping, payload)
    }

    fn send(&mut self, op: Op, payload: &[u8]) -> Result<(), Error> {
        if self.sent_close || self.cancelled.load(Ordering::SeqCst) {
            return Err(Error::Closed);
        }
        let mask = (self.role == Role::Client).then(new_mask);
        let bytes = encode_frame(true, op, payload, mask);
        let _ = self.stream.set_write_timeout(Some(self.idle));
        self.stream.write_all(&bytes).map_err(|e| match e.kind() {
            ErrorKind::WouldBlock | ErrorKind::TimedOut => Error::Timeout,
            ErrorKind::BrokenPipe | ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::NotConnected => Error::Closed,
            kind => Error::Io(kind),
        })
    }

    /// Sends a close frame with the code; nothing can be sent after it, and `recv` goes on until the peer answers or ends.
    pub fn close(&mut self, code: u16) -> Result<(), Error> {
        let r = self.send(Op::Close, &code.to_be_bytes());
        self.sent_close = true;
        r
    }

    /// How long one `recv` waits for the peer: `None` is the `idle` of the limits.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.read_timeout = timeout;
    }

    pub fn canceller(&self) -> Canceller {
        Canceller { flag: self.cancelled.clone(), stream: self.stream.try_clone().expect("a duplicate of a socket") }
    }

    /// The next message. Pings are answered here and pongs dropped; a close frame is answered once and handed on as `Message::Close`, after which the connection is `Closed`.
    pub fn recv(&mut self) -> Result<Message, Error> {
        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }
            match self.pending.pop_front() {
                Some(Ok(Incoming::Text(t))) => return Ok(Message::Text(t)),
                Some(Ok(Incoming::Binary(b))) => return Ok(Message::Binary(b)),
                Some(Ok(Incoming::Ping(p))) => {
                    let _ = self.send(Op::Pong, &p);
                    continue;
                }
                Some(Ok(Incoming::Pong(_))) => continue,
                Some(Ok(Incoming::Close { code, reason })) => {
                    if !self.sent_close {
                        let _ = self.send(Op::Close, &code.unwrap_or(1000).to_be_bytes());
                        self.sent_close = true;
                    }
                    self.peer_closed = true;
                    return Ok(Message::Close { code, reason });
                }
                Some(Err(e)) => {
                    self.peer_closed = true;
                    return Err(e);
                }
                None => {}
            }
            if self.peer_closed {
                return Err(Error::Closed);
            }
            let _ = self.stream.set_read_timeout(Some(self.read_timeout.unwrap_or(self.idle)));
            let mut buf = [0u8; 16 * 1024];
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    self.peer_closed = true;
                    if self.cancelled.load(Ordering::SeqCst) {
                        return Err(Error::Cancelled);
                    }
                    return Err(Error::Closed);
                }
                Ok(n) => self.pending.extend(self.dec.feed(&buf[..n])),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err(if self.cancelled.load(Ordering::SeqCst) { Error::Cancelled } else { Error::Timeout });
                }
                Err(e) => {
                    self.peer_closed = true;
                    if self.cancelled.load(Ordering::SeqCst) {
                        return Err(Error::Cancelled);
                    }
                    return Err(match e.kind() {
                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe | ErrorKind::NotConnected => Error::Closed,
                        kind => Error::Io(kind),
                    });
                }
            }
        }
    }
}

// ---- the head of an upgrade request or answer ------------------------------------------------------------------------------------------------------

struct RawHead {
    first_line: String,
    headers: Vec<(String, String)>, // names in lower case
    leftover: Vec<u8>,
}

fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b > 0x20 && b < 0x7f && b != b':')
}

fn clean_value(s: &str) -> bool {
    s.bytes().all(|b| b == b'\t' || (b >= 0x20 && b != 0x7f))
}

/// Reads until the empty line that ends a head, within `deadline`; what follows the head is kept.
fn read_head(stream: &mut TcpStream, deadline: Instant) -> Result<RawHead, Error> {
    let mut data: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if data.len() > MAX_HEAD {
            return Err(Error::Handshake("the head is too long"));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::Timeout);
        }
        let _ = stream.set_read_timeout(Some(left));
        match stream.read(&mut chunk) {
            Ok(0) => return Err(Error::Closed),
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Err(Error::Timeout),
            Err(e) => return Err(Error::Io(e.kind())),
        }
    };
    if end > MAX_HEAD {
        return Err(Error::Handshake("the head is too long"));
    }
    let leftover = data[end + 4..].to_vec();
    let text = std::str::from_utf8(&data[..end]).map_err(|_| Error::Handshake("the head is not text"))?;
    let mut lines = text.split("\r\n");
    let first_line = lines.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(Error::Handshake("a header line without a colon"))?;
        if !is_token(name) {
            return Err(Error::Handshake("a header name that is not one"));
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
        if headers.len() > MAX_HEADERS {
            return Err(Error::Handshake("too many headers"));
        }
    }
    Ok(RawHead { first_line, headers, leftover })
}

fn find<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn has_token(value: Option<&str>, token: &str) -> bool {
    value.is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
}

/// Connects, upgrades and checks the answer. Nothing is attempted for an address that is not a loopback address or for a path or a header that is not one clean line.
pub fn connect(req: &Request<'_>, limits: &Limits) -> Result<Conn, Error> {
    if !req.addr.ip().is_loopback() {
        return Err(Error::NotLoopback);
    }
    if req.path.is_empty() || !req.path.starts_with('/') || req.path.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(Error::Handshake("the path is not one"));
    }
    for (name, value) in req.headers {
        let lower = name.to_ascii_lowercase();
        if !is_token(name) || !clean_value(value) {
            return Err(Error::Handshake("a header that is not one clean line"));
        }
        if matches!(lower.as_str(), "host" | "upgrade" | "connection" | "origin" | "content-length") || lower.starts_with("sec-websocket-") {
            return Err(Error::Handshake("a header that the upgrade sets itself"));
        }
    }
    let mut stream = TcpStream::connect_timeout(&req.addr, limits.connect).map_err(|e| match e.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => Error::Timeout,
        _ => Error::Connect,
    })?;
    let _ = stream.set_write_timeout(Some(limits.handshake));
    let key = new_key();
    let mut head = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n", req.path, req.addr);
    for (name, value) in req.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(|e| Error::Io(e.kind()))?;
    let answer = read_head(&mut stream, Instant::now() + limits.handshake)?;
    let status: u16 = answer.first_line.strip_prefix("HTTP/1.1 ").and_then(|r| r.get(..3)).and_then(|s| s.parse().ok()).ok_or(Error::Handshake("the answer is not an HTTP/1.1 status line"))?;
    if status != 101 {
        return Err(Error::Refused(status));
    }
    if !find(&answer.headers, "upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket")) || !has_token(find(&answer.headers, "connection"), "upgrade") {
        return Err(Error::Handshake("the answer does not upgrade to websocket"));
    }
    if find(&answer.headers, "sec-websocket-accept") != Some(accept_key(&key).as_str()) {
        return Err(Error::Handshake("the accept key is wrong"));
    }
    if find(&answer.headers, "sec-websocket-extensions").is_some() || find(&answer.headers, "sec-websocket-protocol").is_some() {
        return Err(Error::Handshake("an extension or a subprotocol that nobody asked for"));
    }
    Ok(Conn::new(stream, Role::Client, limits, &answer.leftover))
}

// ---- the server role (feature `server`) -------------------------------------------------------------------------------------------------------------

/// The head of a request read by the server role: names in lower case.
#[cfg(feature = "server")]
#[derive(Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

/// The headers whose values are secrets: `Debug` of a `Head` shows their names and not their values.
#[cfg(feature = "server")]
const SECRET_HEADERS: [&str; 3] = ["authorization", "proxy-authorization", "cookie"];

#[cfg(feature = "server")]
impl fmt::Debug for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown: Vec<String> = self.headers.iter().map(|(n, v)| if SECRET_HEADERS.contains(&n.as_str()) { format!("{n}: <hidden>") } else { format!("{n}: {v}") }).collect();
        f.debug_struct("Head").field("method", &self.method).field("path", &self.path).field("headers", &shown).finish()
    }
}

#[cfg(feature = "server")]
impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        find(&self.headers, name)
    }

    pub fn is_upgrade(&self) -> bool {
        self.method == "GET" && self.header("upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket")) && has_token(self.header("connection"), "upgrade")
    }
}

/// Reads one request head within the handshake deadline. A head with bytes after it (a peer that talks before it is answered) is refused.
#[cfg(feature = "server")]
pub fn read_request(stream: &mut TcpStream, limits: &Limits) -> Result<Head, Error> {
    let raw = read_head(stream, Instant::now() + limits.handshake)?;
    if !raw.leftover.is_empty() {
        return Err(Error::Handshake("bytes before the request was answered"));
    }
    let mut parts = raw.first_line.split(' ');
    let (method, path, version) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !is_token(method) || !path.starts_with('/') || !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return Err(Error::Handshake("not an HTTP request line"));
    }
    Ok(Head { method: method.to_string(), path: path.to_string(), headers: raw.headers })
}

/// Answers an upgrade request with 101 (the head must be one: version 13 and a key).
#[cfg(feature = "server")]
pub fn accept(mut stream: TcpStream, head: &Head, limits: &Limits) -> Result<Conn, Error> {
    if !head.is_upgrade() || head.header("sec-websocket-version") != Some("13") {
        return Err(Error::Handshake("not an upgrade request for version 13"));
    }
    let key = head.header("sec-websocket-key").filter(|k| k.len() == 24 && k.ends_with("==")).ok_or(Error::Handshake("no usable key"))?;
    let answer = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", accept_key(key));
    let _ = stream.set_write_timeout(Some(limits.handshake));
    stream.write_all(answer.as_bytes()).map_err(|e| Error::Io(e.kind()))?;
    Ok(Conn::new(stream, Role::Server, limits, &[]))
}
