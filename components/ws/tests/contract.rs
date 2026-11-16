//! The contract tests of `lico-ws` (SPEC.md): the pure parts with the RFC 6455 examples, then the client against the server role of the same crate over loopback,
//! and a few raw sockets of the test's own for what a server may do wrong.

use lico_ws::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

fn fast() -> Limits {
    Limits { connect: Duration::from_secs(2), handshake: Duration::from_secs(2), idle: Duration::from_secs(3), max_message: 1 << 20 }
}

fn feed_all(d: &mut Decoder, bytes: &[u8]) -> Vec<Result<Incoming, Error>> {
    d.feed(bytes)
}

// ---- the constants and the defaults -------------------------------------------------------------------------------------------------------------------

#[test]
fn the_limits_are_named_constants_with_the_defaults_they_give() {
    let d = Limits::default();
    assert_eq!((d.connect, d.handshake, d.idle, d.max_message), (DEFAULT_CONNECT, DEFAULT_HANDSHAKE, DEFAULT_IDLE, DEFAULT_MAX_MESSAGE));
    assert_eq!((MAX_HEAD, MAX_HEADERS, MAX_CONTROL_PAYLOAD), (16 * 1024, 64, 125));
    assert_eq!(DEFAULT_MAX_MESSAGE, 4 * 1024 * 1024, "the figure of the first measurement: the largest frame was 69,612 bytes");
    assert!(DEFAULT_MAX_MESSAGE <= HARD_MAX_MESSAGE && HARD_MAX_MESSAGE <= 64 << 20);
    assert!(DEFAULT_CONNECT <= Duration::from_secs(10) && DEFAULT_HANDSHAKE <= Duration::from_secs(10) && DEFAULT_IDLE <= Duration::from_secs(120));
}

// ---- the pure parts: the RFC 6455 examples ------------------------------------------------------------------------------------------------------------

#[test]
fn the_accept_key_is_the_one_of_the_rfc() {
    assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
}

#[test]
fn frames_are_encoded_as_in_the_rfc() {
    assert_eq!(encode_frame(true, Op::Text, b"Hello", None), [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);
    assert_eq!(encode_frame(true, Op::Text, b"Hello", Some([0x37, 0xfa, 0x21, 0x3d])), [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]);
    assert_eq!(encode_frame(false, Op::Text, b"Hel", None), [0x01, 0x03, 0x48, 0x65, 0x6c]);
    assert_eq!(encode_frame(true, Op::Continuation, b"lo", None), [0x80, 0x02, 0x6c, 0x6f]);
    assert_eq!(encode_frame(true, Op::Ping, b"Hello", None), [0x89, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);
    assert_eq!(encode_frame(true, Op::Pong, b"Hello", Some([0x37, 0xfa, 0x21, 0x3d])), [0x8a, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]);
    let f = encode_frame(true, Op::Binary, &[7; 256], None);
    assert_eq!(&f[..4], [0x82, 126, 0x01, 0x00]);
    assert_eq!(f.len(), 4 + 256);
    let f = encode_frame(true, Op::Binary, &vec![7; 65536], None);
    assert_eq!(&f[..10], [0x82, 127, 0, 0, 0, 0, 0, 1, 0, 0]);
    assert_eq!(f.len(), 10 + 65536);
    let f = encode_frame(true, Op::Binary, &[1; 125], None);
    assert_eq!(f[1], 125, "125 is the last length of one byte");
    assert_eq!(encode_frame(true, Op::Binary, &[1; 126], None)[1], 126);
}

#[test]
fn a_client_reads_unmasked_frames_whole_or_one_byte_at_a_time_and_joins_the_fragments_of_a_message() {
    let mut bytes = Vec::new();
    bytes.extend(encode_frame(true, Op::Text, "안녕 hello".as_bytes(), None));
    bytes.extend(encode_frame(false, Op::Text, b"Hel", None));
    bytes.extend(encode_frame(true, Op::Ping, b"p", None));
    bytes.extend(encode_frame(true, Op::Continuation, b"lo", None));
    bytes.extend(encode_frame(true, Op::Binary, &[9; 300], None));
    let want = vec![Incoming::Text("안녕 hello".into()), Incoming::Ping(b"p".to_vec()), Incoming::Text("Hello".into()), Incoming::Binary(vec![9; 300])];
    let whole: Vec<_> = feed_all(&mut Decoder::new(Role::Client, 1 << 20), &bytes).into_iter().map(Result::unwrap).collect();
    assert_eq!(whole, want, "a ping between fragments is handed on at once, the message follows");
    let mut d = Decoder::new(Role::Client, 1 << 20);
    let mut got = Vec::new();
    for b in &bytes {
        got.extend(d.feed(std::slice::from_ref(b)).into_iter().map(Result::unwrap));
    }
    assert_eq!(got, want);
}

#[test]
fn a_server_reads_masked_frames_and_unmasks_them() {
    let masked = encode_frame(true, Op::Text, b"Hello", Some([0x37, 0xfa, 0x21, 0x3d]));
    let got = Decoder::new(Role::Server, 1024).feed(&masked);
    assert_eq!(got, vec![Ok(Incoming::Text("Hello".into()))]);
    let got = Decoder::new(Role::Server, 1024).feed(&encode_frame(true, Op::Text, b"Hello", None));
    assert_eq!(got, vec![Err(Error::Protocol("an unmasked frame from a client"))]);
    let got = Decoder::new(Role::Client, 1024).feed(&masked);
    assert_eq!(got, vec![Err(Error::Protocol("a masked frame from a server"))]);
}

fn protocol(bytes: &[u8]) -> Error {
    match Decoder::new(Role::Client, 1 << 16).feed(bytes).pop() {
        Some(Err(e)) => e,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn what_the_rfc_forbids_is_a_protocol_error() {
    assert!(matches!(protocol(&[0xc1, 0x00]), Error::Protocol(_)), "a reserved bit");
    assert!(matches!(protocol(&[0x83, 0x00]), Error::Protocol(_)), "an unknown data opcode");
    assert!(matches!(protocol(&[0x8b, 0x00]), Error::Protocol(_)), "an unknown control opcode");
    assert!(matches!(protocol(&[0x80, 0x00]), Error::Protocol(_)), "a continuation with nothing to continue");
    assert!(matches!(protocol(&[0x09, 0x00]), Error::Protocol(_)), "a fragmented control frame");
    let mut long_ping = vec![0x89, 126, 0, 126];
    long_ping.extend([0u8; 126]);
    assert!(matches!(protocol(&long_ping), Error::Protocol(_)), "a control frame over 125 bytes");
    let mut two_starts = encode_frame(false, Op::Text, b"a", None);
    two_starts.extend(encode_frame(true, Op::Text, b"b", None));
    assert!(matches!(protocol(&two_starts), Error::Protocol(_)), "a new message while one is open");
    assert!(matches!(protocol(&[0x81, 127, 0x80, 0, 0, 0, 0, 0, 0, 1]), Error::Protocol(_)), "a 64-bit length with the top bit set");
    assert!(matches!(protocol(&encode_frame(true, Op::Text, &[0xff, 0xfe], None)), Error::Protocol(_)), "text that is not UTF-8");
    let mut split_utf8 = encode_frame(false, Op::Text, &[0xe2, 0x82], None);
    split_utf8.extend(encode_frame(true, Op::Continuation, &[0xac], None));
    let ok = Decoder::new(Role::Client, 1024).feed(&split_utf8);
    assert_eq!(ok, vec![Ok(Incoming::Text("\u{20ac}".into()))], "a character split between fragments is whole in the message");
    let mut split_bad = encode_frame(false, Op::Text, &[0xe2, 0x82], None);
    split_bad.extend(encode_frame(true, Op::Continuation, &[0x41], None));
    assert!(matches!(protocol(&split_bad), Error::Protocol(_)));
}

#[test]
fn a_frame_or_a_message_over_the_cap_is_too_big_before_it_is_buffered() {
    let mut d = Decoder::new(Role::Client, 1000);
    assert_eq!(d.feed(&[0x82, 126, 0x04, 0x00]), vec![Err(Error::TooBig)], "the header alone says 1024 bytes: refused without waiting for them");
    let mut d = Decoder::new(Role::Client, 1000);
    let mut bytes = encode_frame(false, Op::Binary, &[1; 600], None);
    bytes.extend(encode_frame(true, Op::Continuation, &[1; 600], None));
    assert_eq!(d.feed(&bytes), vec![Err(Error::TooBig)], "two fragments of 600 are over 1000");
    let mut d = Decoder::new(Role::Client, 1000);
    assert_eq!(d.feed(&encode_frame(true, Op::Binary, &[1; 1000], None)).len(), 1, "exactly the cap is allowed");
    let mut d = Decoder::new(Role::Client, 1000);
    assert_eq!(d.feed(&encode_frame(true, Op::Binary, &[1; 1001], None)), vec![Err(Error::TooBig)], "one byte over the cap");
}

#[test]
fn a_decoder_that_failed_stays_failed() {
    let mut d = Decoder::new(Role::Client, 1000);
    assert!(matches!(d.feed(&[0xc1, 0x00])[0], Err(Error::Protocol(_))));
    assert_eq!(d.feed(&encode_frame(true, Op::Text, b"fine", None)), vec![Err(Error::Closed)]);
}

#[test]
fn a_close_frame_carries_a_code_and_a_reason_or_nothing_and_a_bad_one_is_refused() {
    let mut payload = 1000u16.to_be_bytes().to_vec();
    payload.extend(b"bye");
    assert_eq!(Decoder::new(Role::Client, 1024).feed(&encode_frame(true, Op::Close, &payload, None)), vec![Ok(Incoming::Close { code: Some(1000), reason: "bye".into() })]);
    assert_eq!(Decoder::new(Role::Client, 1024).feed(&encode_frame(true, Op::Close, b"", None)), vec![Ok(Incoming::Close { code: None, reason: String::new() })]);
    assert!(matches!(protocol(&encode_frame(true, Op::Close, &[0x03], None)), Error::Protocol(_)), "one byte is not a code");
    assert!(matches!(protocol(&encode_frame(true, Op::Close, &1005u16.to_be_bytes(), None)), Error::Protocol(_)), "1005 is never on the wire");
    assert!(matches!(protocol(&encode_frame(true, Op::Close, &[0x03, 0xe8, 0xff], None)), Error::Protocol(_)), "a reason that is not UTF-8");
}

// ---- the client against the server role, over loopback -------------------------------------------------------------------------------------------------

/// A server in a thread: the closure gets the connection after the upgrade (the head is checked by `check`).
fn serve<F>(check: fn(&Head) -> Result<(), u16>, work: F) -> (SocketAddr, thread::JoinHandle<()>)
where
    F: FnOnce(Conn) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let limits = fast();
        let head = match read_request(&mut stream, &limits) {
            Ok(h) => h,
            Err(_) => return,
        };
        if let Err(status) = check(&head) {
            let _ = stream.write_all(format!("HTTP/1.1 {status} No\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
            return;
        }
        if let Ok(conn) = accept(stream, &head, &limits) {
            work(conn);
        }
    });
    (addr, handle)
}

fn open(addr: SocketAddr) -> Conn {
    connect(&Request { addr, path: "/devtools/browser", headers: &[] }, &fast()).unwrap()
}

fn any(_: &Head) -> Result<(), u16> {
    Ok(())
}

#[test]
fn text_and_binary_go_both_ways_and_the_head_the_server_sees_is_the_one_the_client_made() {
    let (addr, server) = serve(any, |mut c| {
        assert_eq!(c.recv().unwrap(), Message::Text("안녕".into()));
        assert_eq!(c.recv().unwrap(), Message::Binary(vec![0, 1, 2]));
        c.send_text("echo: 안녕").unwrap();
        c.send_binary(&[9; 70_000]).unwrap();
    });
    let mut c = connect(&Request { addr, path: "/devtools/page/1?x=y", headers: &[("Authorization", "Bearer SECRET-TOKEN")] }, &fast()).unwrap();
    c.send_text("안녕").unwrap();
    c.send_binary(&[0, 1, 2]).unwrap();
    assert_eq!(c.recv().unwrap(), Message::Text("echo: 안녕".into()));
    assert_eq!(c.recv().unwrap(), Message::Binary(vec![9; 70_000]));
    server.join().unwrap();
}

#[test]
fn the_server_role_reads_a_head_with_lower_case_names_and_no_origin_is_ever_sent() {
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let head = read_request(&mut s, &fast()).unwrap();
        tx.send(head.clone()).unwrap();
        let _ = accept(s, &head, &fast());
    });
    let _c = connect(&Request { addr, path: "/p", headers: &[("X-Thing", "1")] }, &fast()).unwrap();
    let head = rx.recv().unwrap();
    assert_eq!((head.method.as_str(), head.path.as_str(), head.is_upgrade()), ("GET", "/p", true));
    assert_eq!(head.header("x-thing"), Some("1"));
    assert_eq!(head.header("X-Thing"), Some("1"), "lookup ignores case");
    assert_eq!(head.header("host"), Some(format!("127.0.0.1:{}", addr.port()).as_str()));
    assert!(head.header("origin").is_none() && head.header("sec-websocket-extensions").is_none() && head.header("sec-websocket-protocol").is_none());
    assert_eq!(head.header("sec-websocket-version"), Some("13"));
    h.join().unwrap();
}

#[test]
fn a_plain_request_is_read_as_a_head_that_is_not_an_upgrade() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let head = read_request(&mut s, &fast()).unwrap();
        assert!(!head.is_upgrade() && head.path == "/json/version");
    });
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(b"GET /json/version HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    h.join().unwrap();
}

#[test]
fn a_head_that_is_too_long_or_has_too_many_headers_or_early_bytes_is_refused() {
    for request in [
        format!("GET /{} HTTP/1.1\r\nHost: x\r\n\r\n", "a".repeat(MAX_HEAD)),
        format!("GET / HTTP/1.1\r\n{}\r\n", (0..=MAX_HEADERS).map(|i| format!("H{i}: v\r\n")).collect::<String>()),
        "GET / HTTP/1.1\r\nHost: x\r\n\r\nEARLY".to_string(),
        "NOT AN HTTP REQUEST\r\n\r\n".to_string(),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s, &fast())
        });
        let mut s = TcpStream::connect(addr).unwrap();
        let _ = s.write_all(request.as_bytes());
        assert!(matches!(h.join().unwrap(), Err(Error::Handshake(_))), "{}", &request[..20.min(request.len())]);
    }
}

#[test]
fn a_ping_is_answered_inside_recv_and_a_pong_is_dropped() {
    let (addr, server) = serve(any, |mut c| {
        c.send_ping(b"are you there").unwrap();
        c.send_text("after the ping").unwrap();
        assert_eq!(c.recv().unwrap(), Message::Text("after the pong".into()), "the pong of the client was answered inside its recv and did not show");
    });
    let mut c = open(addr);
    assert_eq!(c.recv().unwrap(), Message::Text("after the ping".into()));
    c.send_text("after the pong").unwrap();
    server.join().unwrap();
}

#[test]
fn the_close_handshake_ends_both_ends_and_a_send_after_it_is_closed() {
    let (addr, server) = serve(any, |mut c| {
        assert_eq!(c.recv().unwrap(), Message::Close { code: Some(1000), reason: String::new() });
        assert_eq!(c.recv(), Err(Error::Closed));
    });
    let mut c = open(addr);
    c.close(1000).unwrap();
    assert_eq!(c.send_text("late"), Err(Error::Closed));
    server.join().unwrap();
}

#[test]
fn a_close_from_the_server_is_a_message_then_closed() {
    let (addr, server) = serve(any, |mut c| {
        c.close(1001).unwrap();
    });
    let mut c = open(addr);
    assert_eq!(c.recv().unwrap(), Message::Close { code: Some(1001), reason: String::new() });
    assert_eq!(c.recv(), Err(Error::Closed));
    server.join().unwrap();
}

#[test]
fn a_connection_that_just_ends_is_closed_and_dropping_a_connection_ends_the_peer() {
    let (addr, server) = serve(any, |c| drop(c));
    let mut c = open(addr);
    assert_eq!(c.recv(), Err(Error::Closed));
    server.join().unwrap();
    let (addr, server) = serve(any, |mut c| {
        assert_eq!(c.recv(), Err(Error::Closed), "the client dropped: end of stream");
    });
    drop(open(addr));
    server.join().unwrap();
}

#[test]
fn a_message_over_the_cap_is_too_big() {
    let (addr, server) = serve(any, |mut c| {
        let _ = c.send_binary(&vec![1; 100_000]);
    });
    let limits = Limits { max_message: 1000, ..fast() };
    let mut c = connect(&Request { addr, path: "/", headers: &[] }, &limits).unwrap();
    assert_eq!(c.recv(), Err(Error::TooBig));
    server.join().unwrap();
}

#[test]
fn recv_times_out_and_the_read_timeout_can_be_set_for_polling() {
    let (addr, server) = serve(any, |mut c| {
        thread::sleep(Duration::from_millis(600));
        let _ = c.send_text("late");
    });
    let mut c = open(addr);
    c.set_read_timeout(Some(Duration::from_millis(100)));
    let t = Instant::now();
    assert_eq!(c.recv(), Err(Error::Timeout));
    assert!(t.elapsed() < Duration::from_millis(500));
    c.set_read_timeout(Some(Duration::from_secs(3)));
    assert_eq!(c.recv().unwrap(), Message::Text("late".into()), "the connection is still good after a timeout");
    server.join().unwrap();
}

#[test]
fn a_canceller_ends_a_blocking_recv_from_another_thread() {
    let (addr, server) = serve(any, |mut c| {
        let _ = c.recv();
    });
    let mut c = open(addr);
    let cancel = c.canceller();
    let t = Instant::now();
    let h = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        cancel.cancel();
    });
    assert_eq!(c.recv(), Err(Error::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(2));
    h.join().unwrap();
    drop(c);
    server.join().unwrap();
}

// ---- what a wrong server may do ------------------------------------------------------------------------------------------------------------------

/// A raw server: reads the request head, then writes what the test says (the accept key of the request is passed in).
fn raw(reply: fn(&str) -> Vec<u8>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if s.read(&mut b).unwrap_or(0) == 0 {
                return;
            }
            head.push(b[0]);
        }
        let text = String::from_utf8_lossy(&head).to_string();
        let key = text.lines().find_map(|l| l.strip_prefix("Sec-WebSocket-Key: ")).unwrap_or("").to_string();
        let _ = s.write_all(&reply(&key));
        thread::sleep(Duration::from_millis(300));
    });
    addr
}

fn try_connect(addr: SocketAddr) -> Result<Conn, Error> {
    connect(&Request { addr, path: "/", headers: &[("Authorization", "Bearer SECRET-TOKEN")] }, &fast())
}

fn good(key: &str, extra: &str) -> String {
    format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n{extra}\r\n", accept_key(key))
}

#[test]
fn a_refusal_carries_its_status_and_the_token_is_nowhere_in_the_error() {
    let addr = raw(|_| b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".to_vec());
    let e = try_connect(addr).err().unwrap();
    assert_eq!(e, Error::Refused(401));
    assert!(!format!("{e:?}{e}").contains("SECRET-TOKEN"));
    let addr = raw(|_| b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec());
    assert_eq!(try_connect(addr).err().unwrap(), Error::Refused(200));
}

#[test]
fn a_bad_upgrade_answer_is_a_handshake_error() {
    let addr = raw(|key| good(key, "").replace(&accept_key(key), "AAAAAAAAAAAAAAAAAAAAAAAAAAA=").into_bytes());
    assert!(matches!(try_connect(addr), Err(Error::Handshake(_))), "a wrong accept key");
    let addr = raw(|key| good(key, "Sec-WebSocket-Extensions: permessage-deflate\r\n").into_bytes());
    assert!(matches!(try_connect(addr), Err(Error::Handshake(_))), "an extension nobody asked for");
    let addr = raw(|key| good(key, "Sec-WebSocket-Protocol: chat\r\n").into_bytes());
    assert!(matches!(try_connect(addr), Err(Error::Handshake(_))), "a subprotocol nobody asked for");
    let addr = raw(|key| good(key, "").replace("Upgrade: websocket\r\n", "").into_bytes());
    assert!(matches!(try_connect(addr), Err(Error::Handshake(_))), "no Upgrade header");
    let addr = raw(|_| b"HTTP/1.1 101 Switching Protocols\r\n\r\n".to_vec());
    assert!(matches!(try_connect(addr), Err(Error::Handshake(_))), "no accept key");
    let addr = raw(|_| b"garbage".to_vec());
    assert!(try_connect(addr).is_err());
}

#[test]
fn a_masked_frame_from_the_server_ends_the_connection() {
    let addr = raw(|key| {
        let mut s = good(key, "").into_bytes();
        s.extend([0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]); // the masked "Hello" of the RFC, from a server
        s
    });
    let mut c = try_connect(addr).unwrap();
    assert!(matches!(c.recv(), Err(Error::Protocol(_))));
}

#[test]
fn frames_that_follow_the_upgrade_answer_in_the_same_packet_are_not_lost() {
    let addr = raw(|key| {
        let mut s = good(key, "").into_bytes();
        s.extend([0x01, 0x02, b'h', b'i']); // a text frame without FIN, in the same packet as the upgrade answer
        s
    });
    // An open message waits for its continuation: that it is open (and no error) shows that the bytes were read from what followed the head.
    let mut c = try_connect(addr).unwrap();
    c.set_read_timeout(Some(Duration::from_millis(100)));
    assert_eq!(c.recv(), Err(Error::Timeout), "an open message waits for its continuation: the leftover was handed to the decoder");
}

#[test]
fn a_server_that_never_answers_the_upgrade_times_out_at_the_handshake_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let _hold = thread::spawn(move || {
        let (_s, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_secs(2));
    });
    let limits = Limits { handshake: Duration::from_millis(300), ..fast() };
    let t = Instant::now();
    assert_eq!(connect(&Request { addr, path: "/", headers: &[] }, &limits).err().unwrap(), Error::Timeout);
    assert!(t.elapsed() < Duration::from_secs(1));
}

#[test]
fn nothing_is_attempted_for_an_address_that_is_not_loopback_or_a_header_that_is_not_one_line() {
    let t = Instant::now();
    let far: SocketAddr = "192.0.2.1:9".parse().unwrap();
    assert_eq!(connect(&Request { addr: far, path: "/", headers: &[] }, &fast()).err().unwrap(), Error::NotLoopback);
    let v6: SocketAddr = "[2001:db8::1]:9".parse().unwrap();
    assert_eq!(connect(&Request { addr: v6, path: "/", headers: &[] }, &fast()).err().unwrap(), Error::NotLoopback);
    assert!(t.elapsed() < Duration::from_millis(500), "refused before any connection attempt");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    for (path, name, value) in [("/a\r\nX: y", "A", "b"), ("/", "A\r\nB", "c"), ("/", "A", "b\r\nInjected: 1"), ("/ a", "A", "b"), ("/", "", "b")] {
        let r = connect(&Request { addr, path, headers: &[(name, value)] }, &fast());
        assert!(matches!(r, Err(Error::Handshake(_))), "{path:?} {name:?} {value:?}");
    }
    assert!(listener.accept().is_err(), "no connection was made for any of them");
}

#[test]
fn the_request_never_shows_a_header_value_in_debug() {
    let r = Request { addr: "127.0.0.1:1".parse().unwrap(), path: "/p", headers: &[("Authorization", "Bearer SECRET-TOKEN")] };
    let shown = format!("{r:?}");
    assert!(shown.contains("Authorization") && !shown.contains("SECRET-TOKEN"), "{shown}");
}

#[test]
fn what_a_client_sends_is_masked_with_a_fresh_mask_each_time() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).unwrap();
            head.push(b[0]);
        }
        let key = String::from_utf8_lossy(&head).lines().find_map(|l| l.strip_prefix("Sec-WebSocket-Key: ").map(str::to_string)).unwrap();
        s.write_all(good(&key, "").as_bytes()).unwrap();
        let mut masks = Vec::new();
        for _ in 0..8 {
            let mut frame = [0u8; 10]; // 2 bytes of head, 4 of mask, 4 of "same"
            s.read_exact(&mut frame).unwrap();
            assert_eq!(frame[0], 0x81);
            assert_eq!(frame[1], 0x80 | 4, "the mask bit is set");
            let mask = [frame[2], frame[3], frame[4], frame[5]];
            let text: Vec<u8> = frame[6..].iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
            assert_eq!(text, b"same");
            masks.push(mask);
        }
        assert!(masks.iter().any(|m| *m != masks[0]), "eight frames did not all carry one mask: {masks:?}");
    });
    let mut c = open(addr);
    for _ in 0..8 {
        c.send_text("same").unwrap();
    }
    h.join().unwrap();
}

#[test]
fn a_head_that_never_ends_is_stopped_at_the_cap_and_not_at_the_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let t = Instant::now();
        let r = read_request(&mut s, &Limits { handshake: Duration::from_secs(5), ..fast() });
        (r, t.elapsed())
    });
    let mut s = TcpStream::connect(addr).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nX: ").unwrap();
    s.write_all(&vec![b'a'; MAX_HEAD + 4096]).unwrap();
    let (r, took) = h.join().unwrap();
    assert!(matches!(r, Err(Error::Handshake(_))), "{r:?}");
    assert!(took < Duration::from_secs(2), "refused when the cap was passed: {took:?}");
}

#[test]
fn a_ping_from_the_server_is_answered_with_a_pong_of_the_same_bytes_while_the_client_waits_in_recv() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).unwrap();
            head.push(b[0]);
        }
        let key = String::from_utf8_lossy(&head).lines().find_map(|l| l.strip_prefix("Sec-WebSocket-Key: ").map(str::to_string)).unwrap();
        s.write_all(good(&key, "").as_bytes()).unwrap();
        s.write_all(&encode_frame(true, Op::Ping, b"are you there", None)).unwrap();
        let mut frame = [0u8; 2 + 4 + 13];
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        s.read_exact(&mut frame).unwrap();
        assert_eq!(frame[0], 0x8a, "a pong");
        assert_eq!(frame[1], 0x80 | 13);
        let mask = [frame[2], frame[3], frame[4], frame[5]];
        let text: Vec<u8> = frame[6..].iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
        assert_eq!(text, b"are you there");
        thread::sleep(Duration::from_millis(800)); // keep the socket open while the client's recv waits
    });
    let mut c = open(addr);
    c.set_read_timeout(Some(Duration::from_millis(500)));
    assert_eq!(c.recv(), Err(Error::Timeout), "the ping is no message");
    h.join().unwrap();
}

#[test]
fn the_head_the_server_role_read_never_shows_a_secret_header_in_debug() {
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let head = read_request(&mut s, &fast()).unwrap();
        tx.send(head.clone()).unwrap();
        let _ = accept(s, &head, &fast());
    });
    let _c = connect(&Request { addr, path: "/p", headers: &[("Authorization", "Bearer SECRET-TOKEN"), ("Cookie", "k=SECRET-COOKIE"), ("X-Plain", "visible")] }, &fast()).unwrap();
    let head = rx.recv().unwrap();
    assert_eq!(head.header("authorization"), Some("Bearer SECRET-TOKEN"), "the value is there for whoever checks it");
    let shown = format!("{head:?}");
    assert!(!shown.contains("SECRET-TOKEN") && !shown.contains("SECRET-COOKIE") && shown.contains("visible") && shown.contains("authorization"), "{shown}");
    h.join().unwrap();
}
