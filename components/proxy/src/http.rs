//! The pure part of the HTTP handling: a proxy request is parsed under strict rules and rewritten to origin form; a response head is rewritten to end the connection. Anything
//! ambiguous is a refusal with the status that says why. No socket here.

use crate::{HostError, MAX_HEADERS};

/// A refusal: the status to answer with and a reason for the tests and the observer (never shown to the page).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Refusal(pub u16, pub &'static str);

pub(crate) const BAD: u16 = 400;

fn bad(why: &'static str) -> Refusal {
    Refusal(BAD, why)
}

#[derive(Debug)]
pub(crate) struct Request {
    pub method: String,
    pub connect: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub content_length: usize,
}

fn tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// `host:port` or `[v6]:port`; the port is needed when `default_port` is `None`.
pub(crate) fn authority(s: &str, default_port: Option<u16>) -> Result<(String, u16), Refusal> {
    let (host, port): (&str, Option<&str>) = if let Some(inner) = s.strip_prefix('[') {
        let close = inner.find(']').ok_or(bad("an unbalanced bracket"))?;
        match &inner[close + 1..] {
            "" => (&inner[..close], None),
            r if r.starts_with(':') => (&inner[..close], Some(&r[1..])),
            _ => return Err(bad("text after a bracketed address")),
        }
    } else {
        match s.rfind(':') {
            Some(i) if !s[..i].contains(':') => (&s[..i], Some(&s[i + 1..])),
            Some(_) => return Err(bad("an address with colons that is not in brackets")),
            None => (s, None),
        }
    };
    let port = match (port, default_port) {
        (Some(p), _) => {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad("a port that is not digits"));
            }
            match p.parse::<u32>() {
                Ok(n) if (1..=65535).contains(&n) => n as u16,
                _ => return Err(bad("a port out of range")),
            }
        }
        (None, Some(d)) => d,
        (None, None) => return Err(bad("no port")),
    };
    Ok((host.to_string(), port))
}

/// Parses a request head (everything up to and including the empty line). `Refusal(431, ..)` for a head with too many headers.
pub(crate) fn parse(head: &[u8]) -> Result<Request, Refusal> {
    if head.contains(&0) {
        return Err(bad("a NUL in the head"));
    }
    let text = std::str::from_utf8(head).map_err(|_| bad("a head that is not text"))?;
    if text.bytes().any(|b| b == b'\r') && text.matches('\r').count() != text.matches("\r\n").count() || text.matches('\n').count() != text.matches("\r\n").count() {
        return Err(bad("a bare line feed or carriage return"));
    }
    let body = text.strip_suffix("\r\n\r\n").ok_or(bad("no end of head"))?;
    let mut lines = body.split("\r\n");
    let first = lines.next().unwrap_or("");
    let parts: Vec<&str> = first.split(' ').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(bad("a request line that is not three parts"));
    }
    let (method, target, version) = (parts[0], parts[1], parts[2]);
    if !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(bad("a method that is not upper case letters"));
    }
    if !matches!(method, "GET" | "HEAD" | "POST" | "PUT" | "DELETE" | "OPTIONS" | "PATCH" | "CONNECT") {
        return Err(Refusal(405, "a method that is not allowed"));
    }
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(bad("a version that is not HTTP/1.0 or 1.1"));
    }
    if target.bytes().any(|b| b <= 0x20 || b >= 0x7f) {
        return Err(bad("a target with a control, space or non-ASCII character"));
    }
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length: Option<usize> = None;
    let mut has_te = false;
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(bad("a folded header"));
        }
        let (name, value) = line.split_once(':').ok_or(bad("a header without a colon"))?;
        if name.is_empty() || !name.bytes().all(tchar) {
            return Err(bad("a header name that is not a token"));
        }
        let value = value.trim_matches(|c| c == ' ' || c == '\t');
        if value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
            return Err(bad("a control character in a header"));
        }
        if headers.len() >= MAX_HEADERS {
            return Err(Refusal(431, "more headers than the cap"));
        }
        let lower = name.to_ascii_lowercase();
        if lower == "content-length" {
            if content_length.is_some() {
                return Err(bad("two Content-Length headers"));
            }
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad("a Content-Length that is not digits"));
            }
            content_length = Some(value.parse::<usize>().map_err(|_| bad("a Content-Length out of range"))?);
        }
        if lower == "transfer-encoding" {
            has_te = true;
        }
        headers.push((name.to_string(), value.to_string()));
    }
    if has_te {
        return Err(if content_length.is_some() { bad("Transfer-Encoding together with Content-Length") } else { Refusal(501, "Transfer-Encoding is not supported") });
    }
    if method == "CONNECT" {
        let (host, port) = authority(target, None)?;
        return Ok(Request { method: method.into(), connect: true, host, port, path: String::new(), headers, content_length: 0 });
    }
    let lower_target = target.to_ascii_lowercase();
    if lower_target.starts_with("https://") {
        return Err(bad("https in absolute form (a client tunnels https with CONNECT)"));
    }
    if !lower_target.starts_with("http://") {
        return Err(bad("a target that is not an absolute http address"));
    }
    let rest = &target[7..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let auth = &rest[..end];
    if auth.contains('@') {
        return Err(bad("a user name or password in the address"));
    }
    let (host, port) = authority(auth, Some(80))?;
    let tail = &rest[end..];
    let tail = tail.split('#').next().unwrap_or("");
    let path = if tail.is_empty() { "/".to_string() } else if tail.starts_with('?') { format!("/{tail}") } else { tail.to_string() };
    Ok(Request { method: method.into(), connect: false, host, port, path, headers, content_length: content_length.unwrap_or(0) })
}

pub(crate) fn host_refusal(e: HostError) -> Refusal {
    bad(match e {
        HostError::Empty => "no host",
        HostError::TooLong => "a host that is too long",
        HostError::NonAscii => "a host that is not ASCII",
        HostError::Numeric => "a host of numbers that the system would read as an address",
        HostError::Invalid => "a host with characters a name does not have",
    })
}

const HOP_BY_HOP: [&str; 10] = ["host", "connection", "proxy-connection", "keep-alive", "te", "trailer", "upgrade", "proxy-authorization", "proxy-authenticate", "expect"];

/// The request as the origin gets it: origin form, the host of the destination, the end-to-end headers, `Connection: close`.
pub(crate) fn origin_request(r: &Request) -> Vec<u8> {
    let host = if r.host.contains(':') { format!("[{}]", r.host) } else { r.host.clone() };
    let host_header = if r.port == 80 { host } else { format!("{host}:{}", r.port) };
    let mut out = format!("{} {} HTTP/1.1\r\nHost: {host_header}\r\n", r.method, r.path);
    for (n, v) in &r.headers {
        if !HOP_BY_HOP.contains(&n.to_ascii_lowercase().as_str()) {
            out.push_str(&format!("{n}: {v}\r\n"));
        }
    }
    out.push_str("Connection: close\r\n\r\n");
    out.into_bytes()
}

/// The Proxy-Authorization header of a request, if any.
pub(crate) fn proxy_authorization(r: &Request) -> Option<&str> {
    r.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("proxy-authorization")).map(|(_, v)| v.as_str())
}

/// A response head (up to and including the empty line) rewritten so that no keep-alive survives; `None` when it is not an HTTP/1.x head or is ambiguous.
pub(crate) fn response_head(head: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(head).ok()?;
    if text.contains('\0') || text.matches('\n').count() != text.matches("\r\n").count() {
        return None;
    }
    let body = text.strip_suffix("\r\n\r\n")?;
    let mut lines = body.split("\r\n");
    let status = lines.next()?;
    let code: u16 = status.strip_prefix("HTTP/1.")?.get(2..5)?.parse().ok()?;
    if !(200..600).contains(&code) || !(status.as_bytes().get(8) == Some(&b' ')) {
        return None;
    }
    let (mut out, mut has_te, mut has_cl) = (format!("{status}\r\n"), false, false);
    for line in lines {
        let (name, _) = line.split_once(':')?;
        if name.is_empty() || !name.bytes().all(tchar) {
            return None;
        }
        let lower = name.to_ascii_lowercase();
        has_te |= lower == "transfer-encoding";
        has_cl |= lower == "content-length";
        if !["connection", "keep-alive", "proxy-connection", "proxy-authenticate"].contains(&lower.as_str()) {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    if has_te && has_cl {
        return None;
    }
    out.push_str("Connection: close\r\n\r\n");
    Some(out.into_bytes())
}
