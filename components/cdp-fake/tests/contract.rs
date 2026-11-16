//! The contract tests of `lico-cdp-fake` (SPEC.md): through the client role of `lico-ws` and a few raw sockets of the test's own.

use lico_cdp_fake::*;
use lico_ws::{connect, Conn, Error, Message, Request};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

fn fast() -> lico_ws::Limits {
    lico_ws::Limits { connect: Duration::from_secs(2), handshake: Duration::from_secs(2), idle: Duration::from_secs(3), max_message: 16 << 20 }
}

fn open(addr: SocketAddr, token: Option<&str>) -> Result<Conn, Error> {
    let bearer = token.map(|t| format!("Bearer {t}"));
    let headers: Vec<(&str, &str)> = bearer.iter().map(|b| ("Authorization", b.as_str())).collect();
    connect(&Request { addr, path: "/devtools/browser/abc", headers: &headers }, &fast())
}

fn call(c: &mut Conn, id: u64, method: &str, params: Value, session: Option<&str>) -> Value {
    let mut m = json!({"id": id, "method": method, "params": params});
    if let Some(s) = session {
        m["sessionId"] = json!(s);
    }
    c.send_text(&m.to_string()).unwrap();
    next_json(c)
}

fn next_json(c: &mut Conn) -> Value {
    match c.recv().unwrap() {
        Message::Text(t) => serde_json::from_str(&t).unwrap(),
        other => panic!("{other:?}"),
    }
}

fn http_get(addr: SocketAddr, path: &str, authorization: Option<&str>) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let auth = authorization.map(|a| format!("Authorization: {a}\r\n")).unwrap_or_default();
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n{auth}\r\n").as_bytes()).unwrap();
    let mut text = String::new();
    let _ = s.read_to_string(&mut text);
    let status = text.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, text.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

#[test]
fn it_listens_on_loopback_with_an_ephemeral_port_and_stops_quickly_whatever_the_connections_do() {
    let fake = Fake::start(Script::new().on("Slow.never", Answer::silent())).unwrap();
    assert!(fake.addr().ip().is_loopback() && fake.addr().port() != 0);
    let mut hung = open(fake.addr(), None).unwrap();
    hung.send_text(&json!({"id": 1, "method": "Slow.never"}).to_string()).unwrap();
    let _idle = open(fake.addr(), None).unwrap();
    let t = Instant::now();
    fake.stop();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    assert!(matches!(hung.recv(), Err(Error::Closed) | Err(Error::Io(_))), "the connections are ended");
}

#[test]
fn a_call_gets_its_scripted_result_with_its_id_and_session_and_is_recorded() {
    let fake = Fake::start(Script::new().on("Page.navigate", Answer::result(json!({"frameId": "F1"})))).unwrap();
    let mut c = open(fake.addr(), None).unwrap();
    let reply = call(&mut c, 7, "Page.navigate", json!({"url": "http://127.0.0.1:1/"}), Some("S9"));
    assert_eq!(reply, json!({"id": 7, "result": {"frameId": "F1"}, "sessionId": "S9"}));
    let reply = call(&mut c, 8, "Page.navigate", json!({}), None);
    assert_eq!(reply, json!({"id": 8, "result": {"frameId": "F1"}}), "no session in, none out");
    assert_eq!(fake.calls(), vec![
        Call { method: "Page.navigate".into(), params: json!({"url": "http://127.0.0.1:1/"}), session_id: Some("S9".into()) },
        Call { method: "Page.navigate".into(), params: json!({}), session_id: None },
    ]);
}

#[test]
fn a_method_without_a_rule_is_the_cdp_error_32601_and_is_still_recorded() {
    let fake = Fake::start(Script::new()).unwrap();
    let mut c = open(fake.addr(), None).unwrap();
    let reply = call(&mut c, 1, "Nope.nothing", json!({"a": 1}), None);
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["error"]["code"], -32601);
    assert!(reply["error"]["message"].as_str().unwrap().contains("Nope.nothing"));
    assert_eq!(fake.calls().len(), 1);
}

#[test]
fn an_error_answer_is_a_cdp_error_and_events_follow_the_reply_in_order() {
    let fake = Fake::start(
        Script::new()
            .on("Bad.thing", Answer::error(-32000, "not allowed"))
            .on("Page.startScreencast", Answer::result(json!({})).event("Page.frameStartedLoading", json!({"frameId": "F"})).event_in("S1", "Page.screencastFrame", screencast_frame(1, "AAAA", 640, 480))),
    )
    .unwrap();
    let mut c = open(fake.addr(), None).unwrap();
    assert_eq!(call(&mut c, 1, "Bad.thing", json!({}), None), json!({"id": 1, "error": {"code": -32000, "message": "not allowed"}}));
    assert_eq!(call(&mut c, 2, "Page.startScreencast", json!({}), None), json!({"id": 2, "result": {}}));
    assert_eq!(next_json(&mut c), json!({"method": "Page.frameStartedLoading", "params": {"frameId": "F"}}));
    let frame = next_json(&mut c);
    assert_eq!((frame["method"].clone(), frame["sessionId"].clone()), (json!("Page.screencastFrame"), json!("S1")));
    assert_eq!(frame["params"]["data"], "AAAA");
    assert_eq!(frame["params"]["metadata"]["deviceWidth"], 640);
    assert_eq!(frame["params"]["sessionId"], 1, "the id to acknowledge");
}

#[test]
fn a_silent_answer_never_replies_a_close_answer_ends_the_connection_without_a_close_frame_and_after_delays_the_reply() {
    let fake = Fake::start(Script::new().on("A.silent", Answer::silent()).on("A.close", Answer::close()).on("A.slow", Answer::result(json!(1)).after(250))).unwrap();
    let mut c = open(fake.addr(), None).unwrap();
    c.send_text(&json!({"id": 1, "method": "A.silent"}).to_string()).unwrap();
    c.set_read_timeout(Some(Duration::from_millis(300)));
    assert_eq!(c.recv(), Err(Error::Timeout));
    let t = Instant::now();
    c.set_read_timeout(Some(Duration::from_secs(3)));
    c.send_text(&json!({"id": 2, "method": "A.slow"}).to_string()).unwrap();
    assert_eq!(next_json(&mut c)["id"], 2);
    assert!(t.elapsed() >= Duration::from_millis(240), "{:?}", t.elapsed());
    c.send_text(&json!({"id": 3, "method": "A.close"}).to_string()).unwrap();
    assert_eq!(c.recv(), Err(Error::Closed), "no close frame: the end of the stream");
    assert_eq!(fake.calls().len(), 3);
}

#[test]
fn a_rule_that_is_used_up_hands_over_to_the_next_one() {
    let fake = Fake::start(Script::new().times(2, "M.x", Answer::result(json!("first"))).on("M.x", Answer::result(json!("then")))).unwrap();
    let mut c = open(fake.addr(), None).unwrap();
    let got: Vec<Value> = (1..=4).map(|i| call(&mut c, i, "M.x", json!({}), None)["result"].clone()).collect();
    assert_eq!(got, [json!("first"), json!("first"), json!("then"), json!("then")]);
}

#[test]
fn a_required_token_is_checked_before_the_upgrade_and_before_a_plain_answer_and_never_shown() {
    let fake = Fake::start(Script::new().token("s3cret-token-value").http("/json/version", 200, r#"{"Browser":"Fake/1"}"#)).unwrap();
    assert_eq!(open(fake.addr(), None).err(), Some(Error::Refused(401)));
    assert_eq!(open(fake.addr(), Some("wrong")).err(), Some(Error::Refused(401)));
    assert!(open(fake.addr(), Some("s3cret-token-value")).is_ok());
    assert_eq!(http_get(fake.addr(), "/json/version", None).0, 401);
    assert_eq!(http_get(fake.addr(), "/json/version", Some("Bearer wrong")).0, 401);
    assert_eq!(http_get(fake.addr(), "/json/version", Some("Bearer s3cret-token-value")), (200, r#"{"Browser":"Fake/1"}"#.to_string()));
    let seen = fake.heads();
    assert_eq!(seen.len(), 6, "every head that arrived is recorded, refused or not");
    assert_eq!(seen[2].authorization(), Some("Bearer s3cret-token-value"));
    let shown = format!("{seen:?}");
    assert!(!shown.contains("s3cret-token-value") && !shown.contains("wrong"), "{shown}");
    assert_eq!(seen[0].path, "/devtools/browser/abc");
}

#[test]
fn plain_http_rules_answer_by_path_and_the_rest_is_a_404() {
    let started = Instant::now();
    let fake = Fake::start(Script::new().http("/json/version", 200, r#"{"webSocketDebuggerUrl":"ws://127.0.0.1:1/x"}"#).http("/json/new", 405, "no")).unwrap();
    assert_eq!(http_get(fake.addr(), "/json/version", None), (200, r#"{"webSocketDebuggerUrl":"ws://127.0.0.1:1/x"}"#.to_string()));
    assert_eq!(http_get(fake.addr(), "/json/version?x=1", None).0, 200, "the query is not part of the match");
    assert_eq!(http_get(fake.addr(), "/json/new", None).0, 405);
    assert_eq!(http_get(fake.addr(), "/missing", None).0, 404);
    assert!(started.elapsed() < Duration::from_secs(2), "each answer ends its connection at once: {:?}", started.elapsed());
}

#[test]
fn a_message_that_is_not_a_json_object_or_is_too_big_ends_the_connection() {
    let fake = Fake::start(Script::new().max_message(1000)).unwrap();
    for bad in ["not json", "[1,2]", "{\"id\": \"x\"}", "{\"method\": 5}"] {
        let mut c = open(fake.addr(), None).unwrap();
        c.send_text(bad).unwrap();
        assert!(matches!(c.recv(), Ok(Message::Close { code: Some(1007), .. })), "{bad}: closed with the code of invalid data");
    }
    let mut c = open(fake.addr(), None).unwrap();
    c.send_binary(&[1, 2, 3]).unwrap();
    assert!(matches!(c.recv(), Ok(Message::Close { code: Some(1003), .. })), "a binary message: the code of unsupported data");
    let mut c = open(fake.addr(), None).unwrap();
    c.send_text(&format!("{{\"id\":1,\"method\":\"X.y\",\"params\":{{\"pad\":\"{}\"}}}}", "a".repeat(2000))).unwrap();
    assert!(matches!(c.recv(), Ok(Message::Close { .. }) | Err(Error::Closed)), "over the cap");
    assert!(fake.calls().is_empty(), "none of them was a call");
}

#[test]
fn a_connection_that_says_nothing_ends_after_the_idle_time() {
    let fake = Fake::start(Script::new().idle(Duration::from_millis(300))).unwrap();
    let mut c = open(fake.addr(), None).unwrap();
    let t = Instant::now();
    assert!(matches!(c.recv(), Ok(Message::Close { .. }) | Err(Error::Closed)));
    assert!(t.elapsed() < Duration::from_secs(2));
    assert_eq!(CONNECTION_IDLE, Duration::from_secs(5));
    assert_eq!((MAX_MESSAGE, lico_ws::MAX_HEAD), (8 * 1024 * 1024, 16 * 1024));
}

#[test]
fn many_connections_at_once_are_all_answered_and_all_recorded() {
    let fake = Fake::start(Script::new().on("Echo.n", Answer::result(json!("ok")))).unwrap();
    let addr = fake.addr();
    let handles: Vec<_> = (0..6)
        .map(|i| {
            std::thread::spawn(move || {
                let mut c = open(addr, None).unwrap();
                for k in 0..5 {
                    assert_eq!(call(&mut c, k, "Echo.n", json!({"i": i, "k": k}), None)["result"], "ok");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(fake.calls().len(), 30);
}

#[test]
fn the_screencast_frame_has_the_chrome_shape() {
    let p = screencast_frame(3, "QUJD", 1200, 800);
    assert_eq!(p, json!({"data": "QUJD", "sessionId": 3, "metadata": {"offsetTop": 0, "pageScaleFactor": 1, "deviceWidth": 1200, "deviceHeight": 800, "scrollOffsetX": 0, "scrollOffsetY": 0, "timestamp": 0}}));
}
