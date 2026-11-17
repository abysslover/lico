//! `lico-proxy` (SPEC.md): the fetch guard of a browser engine. A small HTTP and `CONNECT` proxy on a loopback port that resolves names itself, refuses every address that is not
//! public (loopback only when the person opted in), connects to the address it checked, fixes ONE destination per connection and ends every connection after it. Std only; threads
//! exist only between `start` and `stop`.

mod http;
mod policy;

pub use policy::{check_host, classify, Class, Host, HostError, Policy, MAX_HOST};

/// Why a destination was refused with a 403 (0.3.0): so that the person is told the truth and not a guess. `Other` is every other refusal (a malformed request, a name that was not found, ...);
/// `None` is for an event that is not a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    None,
    Other,
    /// The address is on this computer or a private network (or any other address that is not public).
    NotPublic,
    /// The guard of a tab for this machine's own services allows loopback only.
    LoopbackOnly,
    /// A public destination on a port that is not 80, 443, 8080 or 8443.
    Port,
    /// A port of this machine that no page may reach (the host's own door, an engine).
    Denied,
}

use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A page opens at most six connections to a host and a few hosts: 16 is room. Each costs one or two threads of `STACK` bytes (about 3 MB of the host's memory at the very most),
/// and only while an engine runs. The next connection gets a 503.
pub const DEFAULT_MAX_CONNECTIONS: usize = 16;
/// A request head (and a response head) over this is refused: a real one is under 8 KiB.
pub const MAX_HEAD: usize = 16 * 1024;
/// A real request has about twenty headers.
pub const MAX_HEADERS: usize = 100;
/// The largest request body (by Content-Length): form posts are small; an upload is not what a browser tab is for.
pub const MAX_BODY: usize = 16 << 20;
/// A connect is immediate on a good network; 10 s covers a slow one.
pub const DEFAULT_CONNECT: Duration = Duration::from_secs(10);
/// A name lookup that takes longer than this is given up on.
pub const DEFAULT_RESOLVE: Duration = Duration::from_secs(5);
/// A connection that says nothing for this long is ended.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(60);
/// No connection lasts longer than this: a page load is seconds, a stream a few minutes.
pub const DEFAULT_TOTAL: Duration = Duration::from_secs(300);
/// The bytes one connection may move in both directions together.
pub const DEFAULT_MAX_BYTES: u64 = 256 << 20;
/// The ports of a PUBLIC destination (http, https and their usual alternatives); a loopback destination (the person's opt-in) may use any port.
pub const PUBLIC_PORTS: [u16; 4] = [80, 443, 8080, 8443];
/// The stack of a connection thread: it copies bytes and holds buffers on the heap.
const STACK: usize = 256 * 1024;
const POLL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug)]
pub struct Limits {
    pub connect: Duration,
    pub resolve: Duration,
    pub idle: Duration,
    pub total: Duration,
    pub max_bytes: u64,
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { connect: DEFAULT_CONNECT, resolve: DEFAULT_RESOLVE, idle: DEFAULT_IDLE, total: DEFAULT_TOTAL, max_bytes: DEFAULT_MAX_BYTES, max_body: MAX_BODY }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Connect,
    Http,
    Refused(u16),
}

/// What the observer is told: the destination host and port, never a path or a query.
#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub host: String,
    pub port: u16,
    pub authenticated: bool,
    pub why: Why,
}

#[derive(Clone)]
pub struct Config {
    /// If set, `Proxy-Authorization: Basic` with the user `lico` and this token is required.
    pub token: Option<String>,
    pub max_connections: usize,
    pub limits: Limits,
    pub policy: Policy,
    pub observe: Option<Arc<dyn Fn(&Event) + Send + Sync>>,
    /// Ports on this machine that no destination may use (0.2.0): the host's own listening ports and the engine's, so that a page can never reach them through the guard. Asked
    /// for a loopback destination only (a public site on the same number is a public site), in every mode.
    pub deny_port: Option<Arc<dyn Fn(u16) -> bool + Send + Sync>>,
}

impl Default for Config {
    fn default() -> Config {
        Config { token: None, max_connections: DEFAULT_MAX_CONNECTIONS, limits: Limits::default(), policy: Policy::default(), observe: None, deny_port: None }
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config").field("token", &self.token.as_ref().map(|_| "<hidden>")).field("max_connections", &self.max_connections).field("limits", &self.limits).field("policy", &self.policy).finish()
    }
}

pub trait Resolver: Send + Sync {
    fn resolve(&self, host: &str, timeout: Duration) -> std::io::Result<Vec<IpAddr>>;
}

pub trait Connector: Send + Sync {
    fn connect(&self, ip: IpAddr, port: u16, timeout: Duration) -> std::io::Result<TcpStream>;
}

/// Runs `f` on a thread of its own and waits for its answer at most `timeout`; `None` when it is not there in time (the thread ends when `f` does).
pub fn with_deadline<T: Send + 'static>(timeout: Duration, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new().name("lico-proxy-lookup".into()).stack_size(STACK).spawn(move || {
        let _ = tx.send(f());
    }).ok()?;
    rx.recv_timeout(timeout).ok()
}

/// The system's name lookup behind a deadline. Thin: it cannot be tested without a real lookup, which no test may make (an address literal needs none).
pub struct StdResolver;

impl Resolver for StdResolver {
    fn resolve(&self, host: &str, timeout: Duration) -> std::io::Result<Vec<IpAddr>> {
        let host = host.to_string();
        match with_deadline(timeout, move || (host.as_str(), 0u16).to_socket_addrs().map(|i| i.map(|a| a.ip()).collect::<Vec<_>>())) {
            Some(r) => r,
            None => Err(std::io::Error::new(ErrorKind::TimedOut, "the lookup took too long")),
        }
    }
}

/// A plain TCP connect to the address that was checked. Thin, like `StdResolver`.
pub struct StdConnector;

impl Connector for StdConnector {
    fn connect(&self, ip: IpAddr, port: u16, timeout: Duration) -> std::io::Result<TcpStream> {
        let s = TcpStream::connect_timeout(&SocketAddr::new(ip, port), timeout)?;
        let _ = s.set_nodelay(true);
        Ok(s)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub accepted: u64,
    pub refused: u64,
    pub active: usize,
    pub bytes: u64,
}

struct Shared {
    cfg: Config,
    resolver: Arc<dyn Resolver>,
    connector: Arc<dyn Connector>,
    stop: AtomicBool,
    active: AtomicUsize,
    accepted: AtomicU64,
    refused: AtomicU64,
    bytes: AtomicU64,
    next_id: AtomicU64,
    streams: Mutex<HashMap<u64, TcpStream>>,
}

pub struct Proxy {
    addr: SocketAddr,
    shared: Arc<Shared>,
    acceptor: Option<thread::JoinHandle<()>>,
}

impl Proxy {
    /// Binds `127.0.0.1:0` (never another address) and starts the acceptor. Nothing runs before this and nothing after `stop`.
    pub fn start(cfg: Config, resolver: Arc<dyn Resolver>, connector: Arc<dyn Connector>) -> std::io::Result<Proxy> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            cfg,
            resolver,
            connector,
            stop: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            accepted: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            next_id: AtomicU64::new(1),
            streams: Mutex::new(HashMap::new()),
        });
        let sh = shared.clone();
        let acceptor = thread::Builder::new().name("lico-proxy".into()).spawn(move || accept_loop(listener, sh))?;
        Ok(Proxy { addr, shared, acceptor: Some(acceptor) })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn stats(&self) -> Stats {
        let s = &self.shared;
        Stats { accepted: s.accepted.load(Ordering::SeqCst), refused: s.refused.load(Ordering::SeqCst), active: s.active.load(Ordering::SeqCst), bytes: s.bytes.load(Ordering::SeqCst) }
    }

    /// Ends the listener and every connection; returns within about 2 s (a connection that waits for a lookup that does not return ends when the lookup does or at its deadline).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        // the accept blocks (no timer): a connection to ourselves wakes it, and it sees the stop
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(500));
        for s in self.shared.streams.lock().unwrap().values() {
            let _ = s.shutdown(Shutdown::Both);
        }
        if let Some(h) = self.acceptor.take() {
            let _ = h.join();
        }
        let until = Instant::now() + Duration::from_millis(1500);
        while self.shared.active.load(Ordering::SeqCst) > 0 && Instant::now() < until {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn notify(sh: &Shared, kind: EventKind, host: &str, port: u16, authenticated: bool, why: Why) {
    if let Some(f) = &sh.cfg.observe {
        f(&Event { kind, host: host.to_string(), port, authenticated, why });
    }
}

fn reason(code: u16) -> &'static str {
    match code {
        400 => "Bad Request",
        403 => "Forbidden",
        405 => "Method Not Allowed",
        407 => "Proxy Authentication Required",
        408 => "Request Timeout",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Bad Gateway",
    }
}

/// Answers with a status and closes: no body, so nothing of the proxy's insides reaches the page.
fn refuse(sh: &Shared, client: &mut TcpStream, code: u16, host: &str, port: u16, authenticated: bool) {
    refuse_why(sh, client, code, host, port, authenticated, Why::Other)
}

fn refuse_why(sh: &Shared, client: &mut TcpStream, code: u16, host: &str, port: u16, authenticated: bool, why: Why) {
    let challenge = if code == 407 { "Proxy-Authenticate: Basic realm=\"lico\"\r\n" } else { "" };
    let _ = client.write_all(format!("HTTP/1.1 {code} {}\r\nConnection: close\r\nContent-Length: 0\r\n{challenge}\r\n", reason(code)).as_bytes());
    let _ = client.shutdown(Shutdown::Both);
    sh.refused.fetch_add(1, Ordering::SeqCst);
    notify(sh, EventKind::Refused(code), host, port, authenticated, why);
}

fn accept_loop(listener: TcpListener, sh: Arc<Shared>) {
    while !sh.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if sh.stop.load(Ordering::SeqCst) {
                    break; // the connection that `stop` made to wake this accept
                }
                sh.accepted.fetch_add(1, Ordering::SeqCst);
                if sh.active.load(Ordering::SeqCst) >= sh.cfg.max_connections {
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
                    refuse(&sh, &mut stream, 503, "", 0, false);
                    continue;
                }
                sh.active.fetch_add(1, Ordering::SeqCst);
                let id = sh.next_id.fetch_add(1, Ordering::SeqCst);
                if let Ok(copy) = stream.try_clone() {
                    sh.streams.lock().unwrap().insert(id, copy);
                }
                let sh2 = sh.clone();
                let spawned = thread::Builder::new().name("lico-proxy-conn".into()).stack_size(STACK).spawn(move || {
                    serve(stream, &sh2);
                    sh2.streams.lock().unwrap().remove(&id);
                    sh2.active.fetch_sub(1, Ordering::SeqCst);
                });
                if spawned.is_err() {
                    sh.streams.lock().unwrap().remove(&id);
                    sh.active.fetch_sub(1, Ordering::SeqCst);
                }
            }
            Err(_) => thread::sleep(POLL),
        }
    }
}

// ---- one connection --------------------------------------------------------------------------------------------------------------------------------

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let s = s.trim_end_matches('=');
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0u32);
    for c in s.bytes() {
        acc = acc << 6 | val(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Compares in time that does not depend on where the first difference is.
fn same(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() != b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(1);
    }
    diff == 0
}

fn authorized(sh: &Shared, header: Option<&str>) -> bool {
    let Some(token) = &sh.cfg.token else { return false };
    let Some(h) = header else { return false };
    let Some(encoded) = h.strip_prefix("Basic ").or_else(|| h.strip_prefix("basic ")) else { return false };
    let Some(decoded) = base64_decode(encoded.trim()) else { return false };
    same(&decoded, format!("lico:{token}").as_bytes())
}

enum Head {
    Done(Vec<u8>, Vec<u8>),
    Refused(u16),
    Closed,
}

/// Reads until the empty line; the head and what followed it. A bare line feed is refused at once, a head over the cap is a 431, a stall is a 408.
fn read_head(client: &mut TcpStream, sh: &Shared, deadline: Instant) -> Head {
    let mut data: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            if at + 4 > MAX_HEAD {
                return Head::Refused(431);
            }
            let rest = data.split_off(at + 4);
            return Head::Done(data, rest);
        }
        let bare = data.iter().enumerate().any(|(i, b)| *b == b'\n' && (i == 0 || data[i - 1] != b'\r'));
        if bare {
            return Head::Refused(400);
        }
        if data.len() > MAX_HEAD {
            return Head::Refused(431);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || sh.stop.load(Ordering::SeqCst) {
            return Head::Refused(408);
        }
        let _ = client.set_read_timeout(Some(left.min(sh.cfg.limits.idle)));
        match client.read(&mut chunk) {
            Ok(0) => return Head::Closed,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return if data.is_empty() { Head::Closed } else { Head::Refused(408) },
            Err(_) => return Head::Closed,
        }
    }
}

struct Tunnel {
    bytes: AtomicU64,
    cap: u64,
    deadline: Instant,
    done: AtomicBool,
}

fn pump(mut from: TcpStream, mut to: TcpStream, t: &Tunnel, sh: &Shared, idle: Duration) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let left = t.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || t.done.load(Ordering::SeqCst) || sh.stop.load(Ordering::SeqCst) {
            break;
        }
        let _ = from.set_read_timeout(Some(left.min(idle)));
        match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let total = t.bytes.fetch_add(n as u64, Ordering::SeqCst) + n as u64;
                sh.bytes.fetch_add(n as u64, Ordering::SeqCst);
                if total > t.cap || to.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                if Instant::now() >= t.deadline || left <= idle {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    t.done.store(true, Ordering::SeqCst);
    let _ = from.shutdown(Shutdown::Both);
    let _ = to.shutdown(Shutdown::Both);
}

/// Copies both ways until one side ends or a cap is reached, then ends both.
fn tunnel(client: TcpStream, upstream: TcpStream, sh: &Shared, deadline: Instant) {
    let t = Tunnel { bytes: AtomicU64::new(0), cap: sh.cfg.limits.max_bytes, deadline, done: AtomicBool::new(false) };
    let idle = sh.cfg.limits.idle;
    let (Ok(c2), Ok(u2)) = (client.try_clone(), upstream.try_clone()) else { return };
    thread::scope(|scope| {
        let _ = thread::Builder::new().name("lico-proxy-back".into()).stack_size(STACK).spawn_scoped(scope, || pump(u2, c2, &t, sh, idle));
        pump(client, upstream, &t, sh, idle);
    });
}

fn serve(mut client: TcpStream, sh: &Arc<Shared>) {
    let limits = sh.cfg.limits.clone();
    let deadline = Instant::now() + limits.total;
    let _ = client.set_write_timeout(Some(limits.idle));
    let _ = client.set_nodelay(true);
    let (head, rest) = match read_head(&mut client, sh, deadline) {
        Head::Done(h, r) => (h, r),
        Head::Refused(code) => return refuse(sh, &mut client, code, "", 0, false),
        Head::Closed => {
            let _ = client.shutdown(Shutdown::Both);
            return;
        }
    };
    let req = match http::parse(&head) {
        Ok(r) => r,
        Err(http::Refusal(code, _)) => return refuse(sh, &mut client, code, "", 0, false),
    };
    let authenticated = authorized(sh, http::proxy_authorization(&req));
    if sh.cfg.token.is_some() && !authenticated {
        return refuse(sh, &mut client, 407, &req.host, req.port, false);
    }
    if req.connect && !rest.is_empty() || !req.connect && rest.len() > req.content_length {
        return refuse(sh, &mut client, 400, &req.host, req.port, authenticated); // bytes after the request: a second request, whatever it names
    }
    if req.content_length > limits.max_body {
        return refuse(sh, &mut client, 413, &req.host, req.port, authenticated);
    }
    let ips: Vec<IpAddr> = match check_host(&req.host) {
        Err(e) => return refuse(sh, &mut client, http::host_refusal(e).0, &req.host, req.port, authenticated),
        Ok(Host::Ip(ip)) => vec![ip],
        Ok(Host::Name(name)) => {
            let (resolver, timeout) = (sh.resolver.clone(), limits.resolve);
            let n2 = name.clone();
            match with_deadline(timeout, move || resolver.resolve(&n2, timeout)) {
                None => return refuse(sh, &mut client, 504, &req.host, req.port, authenticated),
                Some(Err(_)) => return refuse(sh, &mut client, 502, &req.host, req.port, authenticated),
                Some(Ok(v)) if v.is_empty() => return refuse(sh, &mut client, 502, &req.host, req.port, authenticated),
                Some(Ok(v)) => v,
            }
        }
    };
    if ips.iter().any(|ip| !sh.cfg.policy.allows(classify(*ip))) {
        return refuse_why(sh, &mut client, 403, &req.host, req.port, authenticated, if sh.cfg.policy.only_loopback { Why::LoopbackOnly } else { Why::NotPublic }); // ANY answer that is not allowed refuses the name
    }
    let all_loopback = ips.iter().all(|ip| classify(*ip) == Class::Loopback);
    if all_loopback && sh.cfg.deny_port.as_ref().is_some_and(|deny| deny(req.port)) {
        return refuse_why(sh, &mut client, 403, &req.host, req.port, authenticated, Why::Denied); // a port of this machine's own that no page may reach
    }
    if !all_loopback && !PUBLIC_PORTS.contains(&req.port) {
        return refuse_why(sh, &mut client, 403, &req.host, req.port, authenticated, Why::Port);
    }
    let upstream = match sh.connector.connect(ips[0], req.port, limits.connect) {
        Ok(s) => s,
        Err(_) => return refuse(sh, &mut client, 502, &req.host, req.port, authenticated),
    };
    if let Ok(copy) = upstream.try_clone() {
        // the upstream socket is ended with the connection when the proxy stops
        let id = sh.next_id.fetch_add(1, Ordering::SeqCst);
        sh.streams.lock().unwrap().insert(id, copy);
    }
    if req.connect {
        notify(sh, EventKind::Connect, &req.host, req.port, authenticated, Why::None);
        if client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").is_err() {
            return;
        }
        return tunnel(client, upstream, sh, deadline);
    }
    notify(sh, EventKind::Http, &req.host, req.port, authenticated, Why::None);
    forward(client, upstream, &req, rest, sh, deadline);
}

fn forward(mut client: TcpStream, mut upstream: TcpStream, req: &http::Request, mut body: Vec<u8>, sh: &Shared, deadline: Instant) {
    let limits = &sh.cfg.limits;
    let _ = upstream.set_write_timeout(Some(limits.idle));
    if upstream.write_all(&http::origin_request(req)).is_err() {
        return refuse(sh, &mut client, 502, &req.host, req.port, true);
    }
    let mut buf = [0u8; 8192];
    // the rest of the body, as announced
    while body.len() < req.content_length {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        let _ = client.set_read_timeout(Some(left.min(limits.idle)));
        match client.read(&mut buf) {
            Ok(0) | Err(_) => {
                let _ = upstream.shutdown(Shutdown::Both);
                return;
            }
            Ok(n) => {
                if body.len() + n > req.content_length {
                    return refuse(sh, &mut client, 400, &req.host, req.port, true);
                }
                body.extend_from_slice(&buf[..n]);
            }
        }
    }
    if !body.is_empty() && upstream.write_all(&body).is_err() {
        return refuse(sh, &mut client, 502, &req.host, req.port, true);
    }
    // the answer: its head is rewritten, its body relayed as it is
    let mut data: Vec<u8> = Vec::new();
    let end = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if data.len() > MAX_HEAD {
            return refuse(sh, &mut client, 502, &req.host, req.port, true);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return refuse(sh, &mut client, 504, &req.host, req.port, true);
        }
        let _ = upstream.set_read_timeout(Some(left.min(limits.idle)));
        match upstream.read(&mut buf) {
            Ok(0) | Err(_) => return refuse(sh, &mut client, 502, &req.host, req.port, true),
            Ok(n) => data.extend_from_slice(&buf[..n]),
        }
    };
    if end > MAX_HEAD {
        return refuse(sh, &mut client, 502, &req.host, req.port, true);
    }
    let Some(head) = http::response_head(&data[..end]) else { return refuse(sh, &mut client, 502, &req.host, req.port, true) };
    if client.write_all(&head).is_err() || client.write_all(&data[end..]).is_err() {
        return;
    }
    let mut moved = (head.len() + data.len() - end) as u64;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || moved > limits.max_bytes {
            break;
        }
        let _ = upstream.set_read_timeout(Some(left.min(limits.idle)));
        match upstream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                moved += n as u64;
                sh.bytes.fetch_add(n as u64, Ordering::SeqCst);
                if client.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = client.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
}
