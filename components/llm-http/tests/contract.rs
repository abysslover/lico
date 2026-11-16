//! The contract tests of `lico-llm-http` (SPEC.md), against the fake server of `lico-llm-fake` and a few raw sockets of the test's own.

use lico_llm_fake::*;
use lico_llm_http::*;
use std::net::{SocketAddr, TcpListener};
use std::time::{Duration, Instant};

const MS: fn(u64) -> Duration = Duration::from_millis;

fn fast() -> Limits {
    Limits { connect: Duration::from_secs(2), first_byte: Duration::from_secs(3), idle: Duration::from_secs(3), total: Duration::from_secs(10), max_body: 1 << 20 }
}

struct Got {
    head: Result<Head, HttpError>,
    pieces: Vec<(Instant, Vec<u8>)>,
}

impl Got {
    fn body(&self) -> Vec<u8> {
        self.pieces.iter().flat_map(|(_, p)| p.clone()).collect()
    }
}

fn run(addr: SocketAddr, method: Method, path: &str, headers: &[(&str, &str)], body: &[u8], limits: &Limits, cancel: &Cancel) -> Got {
    let mut pieces = Vec::new();
    let req = Request { addr, method, path, headers, body };
    let head = send(&req, limits, cancel, &mut |b| {
        pieces.push((Instant::now(), b.to_vec()));
        Flow::Continue
    });
    Got { head, pieces }
}

fn get(addr: SocketAddr, path: &str) -> Got {
    run(addr, Method::Get, path, &[], b"", &fast(), &Cancel::new())
}

fn post(addr: SocketAddr, path: &str, body: &[u8]) -> Got {
    run(addr, Method::Post, path, &[("Content-Type", "application/json")], body, &fast(), &Cancel::new())
}

fn fake_with(path: &str, b: Behaviour) -> Fake {
    Fake::start(Script::new().on("POST", path, b.clone()).on("GET", path, b)).unwrap()
}

fn pieces(texts: &[&str]) -> Vec<Vec<u8>> {
    texts.iter().map(|t| t.as_bytes().to_vec()).collect()
}

// ---- the request and the reply ------------------------------------------------------------------------------------------------

#[test]
fn a_post_and_a_get_reach_the_server_as_documented_and_the_whole_reply_comes_back() {
    let f = fake_with("/v1/x", Behaviour::Answer(Reply::json(200, "{\"ok\":true}").header("X-Reply", "yes")));
    let got = run(f.addr(), Method::Post, "/v1/x", &[("Content-Type", "application/json"), ("Authorization", "Bearer SECRET-9")], b"{\"a\":1}", &fast(), &Cancel::new());
    let head = got.head.as_ref().unwrap();
    assert_eq!(head.status, 200);
    assert_eq!(head.header("x-reply"), Some("yes"));
    assert_eq!(head.header("Content-Length"), Some("11"), "header names are found without regard to case");
    assert_eq!(got.body(), b"{\"ok\":true}");
    let g = get(f.addr(), "/v1/x");
    assert_eq!(g.head.unwrap().status, 200);
    let seen = f.requests();
    assert_eq!((seen[0].method.as_str(), seen[0].path.as_str()), ("POST", "/v1/x"));
    assert_eq!(seen[0].body, b"{\"a\":1}");
    assert_eq!(seen[0].header("content-length"), Some("7"));
    assert_eq!(seen[0].header("content-type"), Some("application/json"));
    assert_eq!(seen[0].authorization(), Some("Bearer SECRET-9"));
    assert_eq!(seen[0].header("host"), Some(format!("127.0.0.1:{}", f.addr().port()).as_str()));
    assert_eq!(seen[0].header("connection"), Some("close"));
    assert!(seen[0].header("user-agent").is_some());
    assert_eq!((seen[1].method.as_str(), seen[1].body.len()), ("GET", 0));
    assert_eq!(seen[1].header("content-length"), None, "a GET without a body has no length");
}

#[test]
fn a_chunked_body_is_handed_on_as_it_arrives_not_when_it_is_complete() {
    let f = fake_with("/s", Behaviour::Answer(Reply::chunked(200, "text/event-stream", pieces(&["one", "two", "three"])).gap(MS(150))));
    let got = post(f.addr(), "/s", b"{}");
    assert_eq!(got.head.clone().unwrap().status, 200);
    assert_eq!(got.body(), b"onetwothree");
    assert_eq!(got.pieces.len(), 3, "each chunk is its own piece");
    let spread = got.pieces.last().unwrap().0 - got.pieces[0].0;
    assert!(spread >= MS(250), "the first piece came long before the last: {spread:?}");
}

#[test]
fn a_body_that_ends_with_the_connection_and_one_with_no_bytes_and_one_without_a_body_are_read() {
    let f = Fake::start(
        Script::new()
            .on("POST", "/u", Behaviour::Answer(Reply { body: Body::UntilClose(pieces(&["ab", "cd"])), ..Reply::json(200, "") }))
            .on("POST", "/empty", Behaviour::Answer(Reply::json(200, "")))
            .on("POST", "/nc", Behaviour::Answer(Reply { status: 204, body: Body::Whole(Vec::new()), ..Reply::json(204, "") })),
    )
    .unwrap();
    assert_eq!(post(f.addr(), "/u", b"").body(), b"abcd");
    let e = post(f.addr(), "/empty", b"");
    assert!(e.head.clone().unwrap().status == 200 && e.body().is_empty());
    assert_eq!(post(f.addr(), "/nc", b"").head.unwrap().status, 204);
}

#[test]
fn a_redirect_is_returned_as_it_is_and_never_followed() {
    let f = fake_with("/r", Behaviour::Answer(Reply::json(302, "").header("Location", "http://127.0.0.1:1/elsewhere")));
    let got = post(f.addr(), "/r", b"{}");
    let head = got.head.clone().unwrap();
    assert_eq!((head.status, head.header("location")), (302, Some("http://127.0.0.1:1/elsewhere")));
    assert_eq!(f.requests().len(), 1);
}

#[test]
fn the_body_limit_is_exact_and_a_callback_may_stop_the_read() {
    let f = fake_with("/b", Behaviour::Answer(Reply::json(200, &"x".repeat(1000))));
    let mut limits = fast();
    limits.max_body = 1000;
    assert_eq!(run(f.addr(), Method::Post, "/b", &[], b"", &limits, &Cancel::new()).body().len(), 1000, "the limit itself is allowed");
    limits.max_body = 999;
    assert_eq!(run(f.addr(), Method::Post, "/b", &[], b"", &limits, &Cancel::new()).head.unwrap_err(), HttpError::TooLarge);
    // a body that is chunked or ends with the connection has no declared length: the bytes are counted as they arrive
    for body in [Body::Chunked(pieces(&["x".repeat(600).as_str(), "y".repeat(600).as_str()])), Body::UntilClose(pieces(&["x".repeat(600).as_str(), "y".repeat(600).as_str()]))] {
        let h = fake_with("/cb", Behaviour::Answer(Reply { body, ..Reply::json(200, "") }));
        let mut tight = fast();
        tight.max_body = 1199;
        let got = run(h.addr(), Method::Post, "/cb", &[], b"", &tight, &Cancel::new());
        assert_eq!(got.head.clone().unwrap_err(), HttpError::TooLarge);
        tight.max_body = 1200;
        assert_eq!(run(h.addr(), Method::Post, "/cb", &[], b"", &tight, &Cancel::new()).body().len(), 1200);
    }
    let many: Vec<Vec<u8>> = (0..200).map(|_| b"x".to_vec()).collect();
    let g = fake_with("/many", Behaviour::Answer(Reply::chunked(200, "text/plain", many).gap(MS(10))));
    let mut got = 0;
    let started = Instant::now();
    let req = Request { addr: g.addr(), method: Method::Post, path: "/many", headers: &[], body: b"" };
    let head = send(&req, &fast(), &Cancel::new(), &mut |b| {
        got += b.len();
        if got >= 3 { Flow::Stop } else { Flow::Continue }
    });
    assert_eq!(head.unwrap().status, 200);
    assert!((3..10).contains(&got), "{got}");
    assert!(started.elapsed() < Duration::from_secs(1), "stopping did not wait for the rest: {:?}", started.elapsed());
}

// ---- failures --------------------------------------------------------------------------------------------------------------------

#[test]
fn a_cut_body_is_an_error_after_the_pieces_that_did_arrive() {
    let f = Fake::start(
        Script::new()
            .on("POST", "/cut", Behaviour::Answer(Reply::chunked(200, "text/plain", pieces(&["a", "b", "c"])).ending(Ending::Cut(2))))
            .on("POST", "/short", Behaviour::Answer(Reply::json(200, "0123456789").ending(Ending::Cut(4)))),
    )
    .unwrap();
    let got = post(f.addr(), "/cut", b"");
    assert!(matches!(got.head, Err(HttpError::BadResponse(_))), "{:?}", got.head);
    assert_eq!(got.body(), b"ab", "what arrived was handed on");
    let got = post(f.addr(), "/short", b"");
    assert!(matches!(got.head, Err(HttpError::BadResponse(_))), "{:?}", got.head);
    assert_eq!(got.body(), b"0123");
}

#[test]
fn a_server_that_is_gone_and_a_port_with_nobody_are_told_apart() {
    let f = fake_with("/gone", Behaviour::Close);
    assert!(matches!(post(f.addr(), "/gone", b"").head, Err(HttpError::BadResponse(_))), "the connection closed before any answer");
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    assert_eq!(post(port, "/x", b"").head.unwrap_err(), HttpError::Connect(std::io::ErrorKind::ConnectionRefused));
    let v6: SocketAddr = "[::1]:9".parse().unwrap();
    assert!(matches!(post(v6, "/x", b"").head, Err(HttpError::Connect(_))), "the IPv6 loopback is loopback: it is tried and refused, not rejected");
}

#[test]
fn only_loopback_addresses_are_tried_and_nothing_is_sent_to_any_other() {
    let started = Instant::now();
    for a in ["192.0.2.1:80", "10.0.0.1:80", "169.254.1.1:80", "0.0.0.0:80", "8.8.8.8:443", "[2001:db8::1]:80", "[::]:80", "[fe80::1]:80"] {
        let addr: SocketAddr = a.parse().unwrap();
        assert_eq!(post(addr, "/x", b"").head.unwrap_err(), HttpError::NotLoopback, "{a}");
    }
    for a in ["127.0.0.1:1", "127.9.9.9:1", "[::1]:1"] {
        let addr: SocketAddr = a.parse().unwrap();
        assert_ne!(post(addr, "/x", b"").head.unwrap_err(), HttpError::NotLoopback, "{a}");
    }
    assert!(started.elapsed() < Duration::from_secs(2), "a refused address costs no connect: {:?}", started.elapsed());
}

#[test]
fn the_deadlines_name_the_phase_that_ran_out() {
    let late = fake_with("/late", Behaviour::Answer(Reply::json(200, "x").first_byte_after(MS(800))));
    let mut limits = fast();
    limits.first_byte = MS(250);
    let started = Instant::now();
    assert_eq!(run(late.addr(), Method::Post, "/late", &[], b"", &limits, &Cancel::new()).head.unwrap_err(), HttpError::Timeout(Phase::FirstByte));
    assert!(started.elapsed() < MS(700), "{:?}", started.elapsed());
    let quiet = fake_with("/quiet", Behaviour::Answer(Reply::chunked(200, "text/plain", pieces(&["a", "b"])).ending(Ending::Silent(1))));
    limits.idle = MS(300);
    let got = run(quiet.addr(), Method::Post, "/quiet", &[], b"", &limits, &Cancel::new());
    assert_eq!(got.head.clone().unwrap_err(), HttpError::Timeout(Phase::Idle));
    assert_eq!(got.body(), b"a");
    let slow = fake_with("/slow", Behaviour::Answer(Reply::chunked(200, "text/plain", pieces(&["a"; 50])).gap(MS(60))));
    let mut limits = fast();
    limits.total = MS(400);
    let started = Instant::now();
    assert_eq!(run(slow.addr(), Method::Post, "/slow", &[], b"", &limits, &Cancel::new()).head.unwrap_err(), HttpError::Timeout(Phase::Total));
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    let hang = fake_with("/hang", Behaviour::Hang);
    let mut limits = fast();
    limits.first_byte = MS(300);
    assert_eq!(run(hang.addr(), Method::Post, "/hang", &[], b"", &limits, &Cancel::new()).head.unwrap_err(), HttpError::Timeout(Phase::FirstByte));
}

#[test]
fn a_cancel_ends_the_call_within_about_a_second_in_mid_stream_and_before_it_starts() {
    let f = fake_with("/c", Behaviour::Answer(Reply::chunked(200, "text/plain", pieces(&["a", "b"])).ending(Ending::Silent(1))));
    let cancel = Cancel::new();
    let c = cancel.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(MS(300));
        c.cancel();
    });
    let started = Instant::now();
    let got = run(f.addr(), Method::Post, "/c", &[], b"", &fast(), &cancel);
    t.join().unwrap();
    assert_eq!(got.head.clone().unwrap_err(), HttpError::Cancelled);
    assert!(started.elapsed() < MS(1300), "{:?}", started.elapsed());
    assert_eq!(got.body(), b"a");
    assert!(cancel.is_cancelled());
    let before = Cancel::new();
    before.cancel();
    std::thread::sleep(MS(300));
    let n = f.requests().len();
    assert_eq!(run(f.addr(), Method::Post, "/c", &[], b"", &fast(), &before).head.unwrap_err(), HttpError::Cancelled);
    std::thread::sleep(MS(300));
    assert_eq!(f.requests().len(), n, "nothing was sent");
}

#[test]
fn garbage_from_a_server_is_a_bad_response_and_never_a_panic_or_a_hang() {
    let raws: Vec<&[u8]> = vec![
        b"",
        b"not http at all\r\n\r\n",
        b"HTTP/1.1 abc OK\r\n\r\n",
        b"HTTP/1.1 99 Low\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: -5\r\n\r\nxx",
        b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999999999999999\r\n\r\nxx",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nZZ\r\nxx\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFFFF\r\nx",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabcXX0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nab",
        b"HTTP/1.1 200 OK\r\nBad Header Line\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\nabc",
        b"HTTP/1.1 600 Too High\r\n\r\n",
        b"HTTP/1.1 099 Too Low\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabcX\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n+3\r\nabc\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nabc",
    ];
    for (i, raw) in raws.iter().enumerate() {
        let f = fake_with("/g", Behaviour::Answer(Reply { body: Body::Raw(raw.to_vec()), ..Reply::json(200, "") }));
        let started = Instant::now();
        let got = post(f.addr(), "/g", b"");
        assert!(matches!(got.head, Err(HttpError::BadResponse(_))), "case {i}: {:?} {:?}", got.head, String::from_utf8_lossy(raw));
        assert!(started.elapsed() < Duration::from_secs(2), "case {i} hung");
    }
    // a head that never ends is cut at the head limit, not read forever
    let huge = format!("HTTP/1.1 200 OK\r\nX-Pad: {}\r\n\r\n", "p".repeat(MAX_HEAD_BYTES + 100));
    let f = fake_with("/g", Behaviour::Answer(Reply { body: Body::Raw(huge.into_bytes()), ..Reply::json(200, "") }));
    assert!(matches!(post(f.addr(), "/g", b"").head, Err(HttpError::BadResponse(_))));
    // a head that has no end in sight is refused for its length, not waited for until the connection ends
    let endless = format!("HTTP/1.1 200 OK\r\n{}", "X-Pad: pppppppppppppppppppppppppppppppppppppp\r\n".repeat(1000));
    let f = fake_with("/g", Behaviour::Answer(Reply { body: Body::Raw(endless.into_bytes()), ..Reply::json(200, "") }));
    assert!(matches!(post(f.addr(), "/g", b"").head, Err(HttpError::BadResponse(m)) if m.contains("too long")));
    // statuses without a body have none, whatever follows the head
    for status in ["204 No Content", "304 Not Modified"] {
        let f = fake_with("/g", Behaviour::Answer(Reply { body: Body::Raw(format!("HTTP/1.1 {status}\r\n\r\nabc").into_bytes()), ..Reply::json(200, "") }));
        let got = post(f.addr(), "/g", b"");
        assert!(got.head.clone().is_ok() && got.body().is_empty(), "{status}: {:?} {:?}", got.head, got.body());
    }
    // and ordinary things that look odd are fine: trailers after the last chunk, a reason phrase with spaces, upper-case header names
    let ok = b"HTTP/1.1 200 Everything Is Fine\r\nCONTENT-TYPE: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n3;ext=1\r\nabc\r\n0\r\nX-Trailer: 1\r\n\r\n";
    let f = fake_with("/g", Behaviour::Answer(Reply { body: Body::Raw(ok.to_vec()), ..Reply::json(200, "") }));
    let got = post(f.addr(), "/g", b"");
    assert_eq!(got.body(), b"abc");
    assert_eq!(got.head.clone().unwrap().header("content-type"), Some("text/plain"));
}

// ---- wrong use ---------------------------------------------------------------------------------------------------------------------

#[test]
fn a_request_that_could_smuggle_a_header_or_break_the_line_is_refused_before_anything_is_sent() {
    let f = fake_with("/ok", Behaviour::Answer(Reply::json(200, "ok")));
    let cases: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("/ok", vec![("X-A", "b\r\nX-Evil: 1")]),
        ("/ok", vec![("X-A", "b\nc")]),
        ("/ok", vec![("X A", "b")]),
        ("/ok", vec![("", "b")]),
        ("/ok", vec![("X-A\r\nX-B", "b")]),
        ("/ok", vec![("Host", "evil")]),
        ("/ok", vec![("content-length", "5")]),
        ("/ok", vec![("Connection", "keep-alive")]),
        ("/ok", vec![("Transfer-Encoding", "chunked")]),
        ("ok", vec![]),
        ("/o k", vec![]),
        ("/ok\r\nX: 1", vec![]),
        ("/ok\u{0}", vec![]),
        ("", vec![]),
    ];
    for (path, headers) in cases {
        let got = run(f.addr(), Method::Post, path, &headers, b"{}", &fast(), &Cancel::new());
        assert!(matches!(got.head, Err(HttpError::BadRequest(_))), "{path:?} {headers:?}: {:?}", got.head);
    }
    assert_eq!(f.requests().len(), 0, "nothing reached the server");
}

#[test]
fn a_key_never_appears_in_a_debug_output_or_an_error() {
    let headers = [("Authorization", "Bearer SECRET-77"), ("X-Api-Key", "SECRET-88")];
    let req = Request { addr: "127.0.0.1:1".parse().unwrap(), method: Method::Post, path: "/x", headers: &headers, body: b"{}" };
    let shown = format!("{req:?}");
    assert!(!shown.contains("SECRET"), "{shown}");
    assert!(shown.contains("Authorization") && shown.contains("/x"), "the rest is shown: {shown}");
    let err = send(&req, &fast(), &Cancel::new(), &mut |_| Flow::Continue).unwrap_err();
    assert!(!format!("{err:?}").contains("SECRET"));
    let f = fake_with("/ok", Behaviour::Answer(Reply::json(200, "ok").ending(Ending::Cut(1))));
    let req = Request { addr: f.addr(), method: Method::Post, path: "/ok", headers: &headers, body: b"{}" };
    let err = send(&req, &fast(), &Cancel::new(), &mut |_| Flow::Continue).unwrap_err();
    assert!(!format!("{err:?}").contains("SECRET"), "{err:?}");
    assert_eq!(f.requests()[0].authorization(), Some("Bearer SECRET-77"), "but the server did get it");
}

// ---- the public names and what the crate does not have -----------------------------------------------------------------------------

/// SPEC.md, API: the names `src/lib.rs` makes public are these and no others.
#[test]
fn the_public_names_are_exactly_these_and_the_limits_have_their_values() {
    let source = include_str!("../src/lib.rs");
    let mut found: Vec<String> = Vec::new();
    for line in source.lines() {
        if line.starts_with("pub ") && !line.starts_with("pub use ") {
            let rest = line.trim_start_matches("pub ").trim_start_matches("const ").trim_start_matches("fn ").trim_start_matches("struct ").trim_start_matches("enum ");
            found.push(rest.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap().to_string());
        }
    }
    found.sort();
    let mut expected: Vec<String> = ["MAX_HEAD_BYTES", "Method", "Request", "Limits", "Cancel", "Head", "Flow", "Phase", "HttpError", "send"].iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(MAX_HEAD_BYTES, 16 * 1024);
    let d = Limits::default();
    assert_eq!((d.connect, d.first_byte, d.idle, d.total, d.max_body), (Duration::from_secs(2), Duration::from_secs(120), Duration::from_secs(60), Duration::from_secs(600), 8 * 1024 * 1024));
}

#[test]
fn the_crate_has_no_dependency_and_no_tls_proxy_name_lookup_or_environment() {
    let manifest = include_str!("../Cargo.toml");
    let deps: Vec<&str> = manifest.split("[dependencies]").nth(1).unwrap().lines().skip(1).take_while(|l| !l.starts_with('[')).filter(|l| !l.trim().is_empty()).collect();
    assert!(deps.is_empty(), "{deps:?}");
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let code = std::fs::read_to_string(&path).unwrap();
        for (n, line) in code.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for word in ["ToSocketAddrs", "std::env", "unsafe", "https:", "_proxy", "PROXY", "resolve", "lookup", "set_var", "std::process", "std::fs", "native_tls", "rustls", "openssl"] {
                assert!(!line.contains(word), "{}:{}: `{word}` in {line}", path.display(), n + 1);
            }
        }
    }
}
