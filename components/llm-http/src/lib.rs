//! A small loopback-only HTTP/1.1 client for Local LLM servers (see `SPEC.md`): one request, the reply body handed on as it arrives, bounded in time and bytes,
//! cancellable. No TLS, no proxy, no name lookup, no redirects, no keep-alive. Only `std`.

mod client;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The largest reply head that is read.
pub const MAX_HEAD_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// One request. `addr` must be a loopback address; `path` starts with `/`; `headers` are extra headers (not `Host`, `Content-Length`, `Connection`, `Transfer-Encoding`).
pub struct Request<'a> {
    pub addr: SocketAddr,
    pub method: Method,
    pub path: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    pub body: &'a [u8],
}

/// Header names whose value is a secret: shown as `<redacted>` by `Debug`.
fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "authorization" || n == "proxy-authorization" || n == "cookie" || n.contains("key") || n.contains("token")
}

impl std::fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(&str, &str)> = self.headers.iter().map(|(n, v)| (*n, if secret_name(n) { "<redacted>" } else { *v })).collect();
        f.debug_struct("Request").field("addr", &self.addr).field("method", &self.method).field("path", &self.path).field("headers", &headers).field("body_bytes", &self.body.len()).finish()
    }
}

/// The time and size bounds of one call.
#[derive(Clone, Debug)]
pub struct Limits {
    /// To make the connection.
    pub connect: Duration,
    /// From the end of the request to the first byte of the reply.
    pub first_byte: Duration,
    /// Between two reads of the reply after the first.
    pub idle: Duration,
    /// The whole call.
    pub total: Duration,
    /// The most bytes of reply body.
    pub max_body: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { connect: Duration::from_secs(2), first_byte: Duration::from_secs(120), idle: Duration::from_secs(60), total: Duration::from_secs(600), max_body: 8 * 1024 * 1024 }
    }
}

/// A flag to end a call from another thread.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The status and headers of a reply; header names are in lower case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// What the body callback answers: go on, or stop reading (the call then returns the head).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

/// The part of a call whose time ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Connect,
    FirstByte,
    Idle,
    Total,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    /// The address is not a loopback address; nothing was done.
    NotLoopback,
    /// The request itself is refused (a header or a path that could break the message); nothing was sent.
    BadRequest(String),
    Connect(std::io::ErrorKind),
    Timeout(Phase),
    Cancelled,
    TooLarge,
    /// The server's answer is not HTTP, or was cut, or contradicts itself.
    BadResponse(String),
    Io(std::io::ErrorKind),
}

/// Sends the request and calls `on_body` with each piece of the reply body, in order, as it arrives. Returns the head when the body is complete, or when `on_body`
/// answered `Stop`. When the reply is cut or fails, the pieces that arrived have already been handed on.
pub fn send(request: &Request, limits: &Limits, cancel: &Cancel, on_body: &mut dyn FnMut(&[u8]) -> Flow) -> Result<Head, HttpError> {
    client::run(request, limits, cancel, on_body)
}
