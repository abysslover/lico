//! The probe's HTTP client: as little as it takes to ask a listening socket of this machine one question. Every call is bounded in time (a deadline of the whole run and its own
//! budget) and in bytes. First contact sends nothing: a server that speaks first (SSH, SMTP, a database) is not HTTP and is left alone. A request is `GET <path> HTTP/1.0` with
//! `Host`, `Accept`, `User-Agent` and `Connection: close`: no body, no credential, no cookie. A redirect is an answer like any other and is never followed.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use super::classify::Response;

pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
pub const QUIET_WAIT: Duration = Duration::from_millis(150);
pub const REQUEST_TIMEOUT: Duration = Duration::from_millis(1500);
/// The most of an answer's body that is read; the head may take another 16 KiB.
pub const BODY_CAP: usize = 64 * 1024;
const HEAD_CAP: usize = 16 * 1024;

pub enum Contact {
    /// The server sent something before it was asked: not HTTP, and it is sent nothing.
    Speaks,
    /// It accepted and closed without a word.
    Closed,
    Failed(&'static str),
    /// Nothing came for 150 ms: the connection, ready for the first question.
    Quiet(TcpStream),
}

pub enum Fault {
    NotHttp,
    Timeout,
    Io,
}

fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// Connects and listens for a moment without sending anything.
pub fn first_contact(addr: SocketAddr, deadline: Instant) -> Contact {
    let budget = left(deadline);
    if budget.is_zero() {
        return Contact::Failed("budget");
    }
    let stream = match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT.min(budget)) {
        Ok(s) => s,
        Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::WouldBlock => return Contact::Failed("timeout"),
        Err(_) => return Contact::Failed("refused"),
    };
    let _ = stream.set_read_timeout(Some(QUIET_WAIT.min(left(deadline).max(Duration::from_millis(1)))));
    let mut one = [0u8; 1];
    match (&stream).read(&mut one) {
        Ok(0) => Contact::Closed,
        Ok(_) => Contact::Speaks,
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Contact::Quiet(stream),
        Err(_) => Contact::Closed,
    }
}

/// A fresh connection to a server that was found quiet.
pub fn connect(addr: SocketAddr, deadline: Instant) -> Result<TcpStream, Fault> {
    let budget = left(deadline);
    if budget.is_zero() {
        return Err(Fault::Timeout);
    }
    TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT.min(budget)).map_err(|e| if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) { Fault::Timeout } else { Fault::Io })
}

/// The request text: the whole of what the probe ever sends.
pub fn request_text(path: &str, host: &str) -> String {
    format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nAccept: application/json\r\nUser-Agent: lico-discover\r\nConnection: close\r\n\r\n")
}

/// Asks one question on `stream` and reads the answer up to the bound.
pub fn get(mut stream: TcpStream, addr: SocketAddr, path: &str, deadline: Instant) -> Result<Response, Fault> {
    let end = (Instant::now() + REQUEST_TIMEOUT).min(deadline);
    let _ = stream.set_write_timeout(Some(left(end).max(Duration::from_millis(1))));
    stream.write_all(request_text(path, &addr.to_string()).as_bytes()).map_err(|_| Fault::Io)?;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut stopped_by_cap = false;
    loop {
        if buf.len() >= 5 && !buf.starts_with(b"HTTP/") {
            return Err(Fault::NotHttp);
        }
        let wait = left(end);
        if wait.is_zero() {
            break;
        }
        let _ = stream.set_read_timeout(Some(wait));
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() >= HEAD_CAP + BODY_CAP {
                    stopped_by_cap = true;
                    break;
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
            Err(_) => break,
        }
    }
    parse(&buf, stopped_by_cap)
}

/// The status and the body of an answer that has a complete head; without one it is not an answer (nothing came: a timeout; something else came: not HTTP).
pub fn parse(buf: &[u8], stopped_by_cap: bool) -> Result<Response, Fault> {
    if buf.is_empty() {
        return Err(Fault::Timeout);
    }
    if buf.len() < 5 || !buf.starts_with(b"HTTP/") {
        return Err(if buf.len() < 5 && b"HTTP/".starts_with(buf) { Fault::Timeout } else { Fault::NotHttp });
    }
    let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or(Fault::Timeout)?;
    let status = std::str::from_utf8(&buf[..head_end.min(64)]).ok().and_then(|h| h.split_whitespace().nth(1)).and_then(|s| s.parse::<u16>().ok()).ok_or(Fault::NotHttp)?;
    let body = &buf[head_end + 4..];
    let truncated = stopped_by_cap || body.len() > BODY_CAP;
    Ok(Response { status, body: body[..body.len().min(BODY_CAP)].to_vec(), truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn serve(reply: &'static [u8], speak_first: bool) -> (SocketAddr, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            if speak_first {
                s.write_all(reply).unwrap();
            }
            s.set_read_timeout(Some(Duration::from_millis(600))).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                if got.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            if !speak_first && !got.is_empty() {
                let _ = s.write_all(reply);
            }
            got
        });
        (addr, handle)
    }

    #[test]
    fn the_request_is_a_http_1_0_get_with_nothing_else() {
        let text = request_text("/v1/models", "127.0.0.1:80");
        assert_eq!(text, "GET /v1/models HTTP/1.0\r\nHost: 127.0.0.1:80\r\nAccept: application/json\r\nUser-Agent: lico-discover\r\nConnection: close\r\n\r\n");
        assert!(!text.to_lowercase().contains("authorization") && !text.to_lowercase().contains("cookie") && !text.contains("Content-Length"));
    }

    #[test]
    fn a_server_that_speaks_first_is_met_with_silence_and_a_quiet_one_gets_the_question_and_answers() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (addr, banner) = serve(b"SSH-2.0-test\r\n", true);
        assert!(matches!(first_contact(addr, deadline), Contact::Speaks));
        assert!(banner.join().unwrap().is_empty(), "it was sent nothing");
        let (addr, quiet) = serve(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nhi", false);
        let Contact::Quiet(stream) = first_contact(addr, deadline) else { panic!("a quiet server") };
        let answer = get(stream, addr, "/v1/models", deadline).ok().unwrap();
        assert_eq!((answer.status, answer.body.as_slice(), answer.truncated), (200, &b"hi"[..], false));
        let seen = String::from_utf8(quiet.join().unwrap()).unwrap();
        assert!(seen.starts_with("GET /v1/models HTTP/1.0\r\n"), "{seen}");
    }

    #[test]
    fn a_refused_port_a_closed_connection_and_a_server_that_never_answers_end_inside_their_bounds() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap(); // the listener is gone
        assert!(matches!(first_contact(dead, deadline), Contact::Failed("refused")));
        let closer = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = closer.local_addr().unwrap();
        let drop_it = thread::spawn(move || drop(closer.accept().unwrap()));
        assert!(matches!(first_contact(addr, deadline), Contact::Closed));
        let _ = drop_it.join();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hold = thread::spawn(move || {
            let (_s, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(2500));
        });
        let Contact::Quiet(stream) = first_contact(addr, deadline) else { panic!("quiet") };
        let t0 = Instant::now();
        assert!(matches!(get(stream, addr, "/x", deadline), Err(Fault::Timeout)));
        assert!(t0.elapsed() < Duration::from_millis(2200), "bounded by the request timeout, not by the server");
        let _ = hold.join();
        assert!(matches!(first_contact(addr, Instant::now()), Contact::Failed("budget")), "a spent run budget connects to nothing");
    }

    #[test]
    fn the_answer_is_parsed_by_its_head_and_the_body_is_cut_at_the_bound() {
        let r = parse(b"HTTP/1.1 302 Found\r\nLocation: http://elsewhere/\r\n\r\n", false).ok().unwrap();
        assert_eq!((r.status, r.body.len()), (302, 0), "a redirect is only an answer");
        let big = [b"HTTP/1.0 200 OK\r\n\r\n".to_vec(), vec![b'x'; BODY_CAP + 500]].concat();
        let cut = parse(&big, false).ok().unwrap();
        assert_eq!((cut.body.len(), cut.truncated), (BODY_CAP, true));
        assert!(matches!(parse(b"SSH-2.0-x\r\n", false), Err(Fault::NotHttp)));
        assert!(matches!(parse(b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n", false), Err(Fault::Timeout)), "no complete head: not an answer yet");
        assert!(matches!(parse(b"", false), Err(Fault::Timeout)));
    }
}
