//! The contract tests of `lico-cdp` (SPEC.md): against the fake endpoint, and against a raw server of the test's own for what a wrong endpoint may do.

use lico_cdp::*;
use lico_cdp_fake::{screencast_frame, Answer, Fake, Script};
use serde_json::{json, Value};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const SEC: Duration = Duration::from_secs(2);

fn limits() -> Limits {
    Limits { connect: SEC, handshake: SEC, idle: Duration::from_secs(3), max_message: 1 << 20 }
}

fn client(fake: &Fake) -> Client {
    Client::connect(fake.addr(), "/devtools/browser/x", None, &limits()).unwrap()
}

/// A raw server: the closure gets the server end of the WebSocket after the upgrade.
fn raw<F: FnOnce(lico_ws::Conn) + Send + 'static>(work: F) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let head = lico_ws::read_request(&mut s, &limits_ws()).unwrap();
        work(lico_ws::accept(s, &head, &limits_ws()).unwrap());
    });
    (addr, h)
}

fn limits_ws() -> lico_ws::Limits {
    lico_ws::Limits { connect: SEC, handshake: SEC, idle: Duration::from_secs(3), max_message: 1 << 20 }
}

fn text(c: &mut lico_ws::Conn) -> Value {
    match c.recv().unwrap() {
        lico_ws::Message::Text(t) => serde_json::from_str(&t).unwrap(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_constants_are_named_with_the_values_the_spec_gives() {
    assert_eq!(MAX_QUEUED_EVENTS, 64);
    assert_eq!(DEFAULT_CALL_TIMEOUT, Duration::from_secs(30));
}

#[test]
fn a_call_gets_its_result_with_the_session_it_was_made_for() {
    let fake = Fake::start(Script::new().on("Page.navigate", Answer::result(json!({"frameId": "F1"})))).unwrap();
    let mut c = client(&fake);
    assert_eq!(c.call("Page.navigate", json!({"url": "http://127.0.0.1:1/"}), Some("S1"), SEC).unwrap(), json!({"frameId": "F1"}));
    let calls = fake.calls();
    assert_eq!((calls[0].method.as_str(), calls[0].session_id.as_deref()), ("Page.navigate", Some("S1")));
    assert_eq!(calls[0].params, json!({"url": "http://127.0.0.1:1/"}));
}

#[test]
fn ids_start_at_one_and_rise_and_params_are_always_an_object() {
    let (addr, h) = raw(|mut c| {
        for want in 1..=3u64 {
            let m = text(&mut c);
            assert_eq!(m["id"], want);
            assert!(m["params"].is_object(), "{m}");
            assert!(m.get("sessionId").is_none());
            c.send_text(&json!({"id": want, "result": {}}).to_string()).unwrap();
        }
    });
    let mut c = Client::connect(addr, "/", None, &limits()).unwrap();
    c.call("A.b", json!({}), None, SEC).unwrap();
    c.call("A.b", Value::Null, None, SEC).unwrap();
    c.call("A.b", json!({"x": 1}), None, SEC).unwrap();
    h.join().unwrap();
}

#[test]
fn params_that_are_not_an_object_are_refused_before_anything_is_sent() {
    let fake = Fake::start(Script::new()).unwrap();
    let mut c = client(&fake);
    assert_eq!(c.call("A.b", json!([1]), None, SEC), Err(Error::Protocol("params is not an object")));
    assert_eq!(c.call("A.b", json!("x"), None, SEC), Err(Error::Protocol("params is not an object")));
    assert!(fake.calls().is_empty());
}

#[test]
fn an_error_reply_is_remote_with_its_code_and_message_and_the_connection_goes_on() {
    let fake = Fake::start(Script::new().on("Bad.thing", Answer::error(-32000, "not allowed")).on("Ok.thing", Answer::result(json!({"a": 1})))).unwrap();
    let mut c = client(&fake);
    assert_eq!(c.call("Bad.thing", json!({}), None, SEC), Err(Error::Remote { code: -32000, message: "not allowed".into() }));
    assert_eq!(c.call("Ok.thing", json!({}), None, SEC).unwrap(), json!({"a": 1}));
    let e = c.call("Nope.x", json!({}), None, SEC).unwrap_err();
    assert!(matches!(e, Error::Remote { code: -32601, .. }), "{e:?}");
}

#[test]
fn events_that_arrive_during_a_call_are_kept_in_order_and_given_by_next_event() {
    let fake = Fake::start(
        Script::new().on("Page.startScreencast", Answer::result(json!({})).event("Page.frameStartedLoading", json!({"frameId": "F"})).event_in("S1", "Page.screencastFrame", screencast_frame(1, "QUJD", 640, 480))),
    )
    .unwrap();
    let mut c = client(&fake);
    c.call("Page.startScreencast", json!({}), None, SEC).unwrap();
    let a = c.next_event(SEC).unwrap().unwrap();
    assert_eq!((a.method.as_str(), a.session_id.clone(), a.params["frameId"].clone()), ("Page.frameStartedLoading", None, json!("F")));
    let b = c.next_event(SEC).unwrap().unwrap();
    assert_eq!((b.method.as_str(), b.session_id.as_deref(), b.params["data"].clone(), b.params["sessionId"].clone()), ("Page.screencastFrame", Some("S1"), json!("QUJD"), json!(1)));
    let t = Instant::now();
    assert_eq!(c.next_event(Duration::from_millis(150)).unwrap(), None, "nothing more: a timeout is not an error");
    assert!(t.elapsed() < Duration::from_secs(1));
}

#[test]
fn an_event_that_comes_before_the_reply_is_kept_and_does_not_confuse_the_call() {
    let (addr, h) = raw(|mut c| {
        let m = text(&mut c);
        c.send_text(&json!({"method": "Early.event", "params": {"n": 1}}).to_string()).unwrap();
        c.send_text(&json!({"id": m["id"], "result": {"ok": true}}).to_string()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let mut c = Client::connect(addr, "/", None, &limits()).unwrap();
    assert_eq!(c.call("A.b", json!({}), None, SEC).unwrap(), json!({"ok": true}));
    assert_eq!(c.next_event(SEC).unwrap().unwrap().params, json!({"n": 1}));
    h.join().unwrap();
}

#[test]
fn the_event_queue_holds_64_and_drops_the_oldest_and_counts_them() {
    // the events come while a call waits (before its reply): that is when a queue can fill, since a reader that reads takes them one by one
    let (addr, h) = raw(|mut c| {
        let m = text(&mut c);
        for n in 0..70 {
            c.send_text(&json!({"method": "E.n", "params": {"n": n}}).to_string()).unwrap();
        }
        c.send_text(&json!({"id": m["id"], "result": {}}).to_string()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let mut c = Client::connect(addr, "/", None, &limits()).unwrap();
    c.call("Flood.go", json!({}), None, SEC).unwrap();
    let mut got = Vec::new();
    while let Some(e) = c.next_event(Duration::from_millis(100)).unwrap() {
        got.push(e.params["n"].as_u64().unwrap());
    }
    assert_eq!(got, (6..70).collect::<Vec<u64>>(), "the 64 newest, in order");
    assert_eq!(c.dropped(), 6);
    h.join().unwrap();
}

#[test]
fn a_reply_nobody_waits_for_is_ignored_and_counted_and_a_late_reply_is_not_given_to_the_next_call() {
    let (addr, h) = raw(|mut c| {
        let first = text(&mut c);
        thread::sleep(Duration::from_millis(400)); // the client gave up on the first call after 150 ms
        c.send_text(&json!({"id": first["id"], "result": "late"}).to_string()).unwrap();
        c.send_text(&json!({"id": 999, "result": "nobody"}).to_string()).unwrap();
        let second = text(&mut c);
        c.send_text(&json!({"id": second["id"], "result": "mine"}).to_string()).unwrap();
        thread::sleep(Duration::from_millis(300));
    });
    let mut c = Client::connect(addr, "/", None, &limits()).unwrap();
    assert_eq!(c.call("A.slow", json!({}), None, Duration::from_millis(150)), Err(Error::Timeout));
    assert_eq!(c.call("A.next", json!({}), None, SEC).unwrap(), json!("mine"));
    assert_eq!(c.stray(), 2);
    h.join().unwrap();
}

#[test]
fn a_silent_endpoint_is_a_timeout_and_the_connection_is_still_good_after_it() {
    let fake = Fake::start(Script::new().times(1, "S.x", Answer::silent()).on("S.x", Answer::result(json!(2)))).unwrap();
    let mut c = client(&fake);
    let t = Instant::now();
    assert_eq!(c.call("S.x", json!({}), None, Duration::from_millis(200)), Err(Error::Timeout));
    assert!(t.elapsed() < Duration::from_secs(1));
    assert_eq!(c.call("S.x", json!({}), None, SEC).unwrap(), json!(2));
}

#[test]
fn an_endpoint_that_ends_is_closed_and_stays_closed() {
    let fake = Fake::start(Script::new().on("End.now", Answer::close())).unwrap();
    let mut c = client(&fake);
    assert_eq!(c.call("End.now", json!({}), None, SEC), Err(Error::Closed));
    assert_eq!(c.call("A.b", json!({}), None, SEC), Err(Error::Closed));
    assert_eq!(c.next_event(Duration::from_millis(50)), Err(Error::Closed));
}

#[test]
fn what_is_not_a_cdp_message_is_a_protocol_error_and_ends_the_connection_at_once() {
    for bad in ["not json", "[1,2]", "42", "{\"neither\": true}", "{\"id\": \"x\", \"result\": 1}"] {
        let (release, wait) = mpsc::channel::<()>();
        let (addr, h) = raw(move |mut c| {
            let _ = text(&mut c);
            let _ = c.send_text(bad);
            let _ = wait.recv_timeout(Duration::from_secs(5)); // the socket stays open: the client must end the connection by itself
        });
        let mut c = Client::connect(addr, "/", None, &limits()).unwrap();
        assert!(matches!(c.call("A.b", json!({}), None, SEC), Err(Error::Protocol(_))), "{bad}");
        let t = Instant::now();
        assert_eq!(c.call("A.b", json!({}), None, SEC), Err(Error::Closed), "{bad}: the connection is over");
        assert_eq!(c.next_event(SEC), Err(Error::Closed));
        assert!(t.elapsed() < Duration::from_millis(500), "{bad}: refused without asking the socket: {:?}", t.elapsed());
        release.send(()).unwrap();
        h.join().unwrap();
    }
    let (release, wait) = mpsc::channel::<()>();
    let (addr, h) = raw(move |mut c| {
        let _ = text(&mut c);
        let _ = c.send_binary(&[1, 2, 3]);
        let _ = wait.recv_timeout(Duration::from_secs(5));
    });
    let mut c = Client::connect(addr, "/", None, &limits()).unwrap();
    assert!(matches!(c.call("A.b", json!({}), None, SEC), Err(Error::Protocol(_))), "a binary message");
    assert_eq!(c.call("A.b", json!({}), None, SEC), Err(Error::Closed));
    release.send(()).unwrap();
    h.join().unwrap();
}

#[test]
fn a_message_over_the_cap_is_too_big_and_ends_the_connection() {
    let fake = Fake::start(Script::new().on("Big.reply", Answer::result(json!({"pad": "a".repeat(5000)})))).unwrap();
    let small = Limits { max_message: 1000, ..limits() };
    let mut c = Client::connect(fake.addr(), "/", None, &small).unwrap();
    assert_eq!(c.call("Big.reply", json!({}), None, SEC), Err(Error::TooBig));
    assert_eq!(c.call("Big.reply", json!({}), None, SEC), Err(Error::Closed));
}

#[test]
fn a_canceller_ends_a_waiting_call_from_another_thread() {
    let fake = Fake::start(Script::new().on("Wait.for.ever", Answer::silent())).unwrap();
    let mut c = client(&fake);
    let cancel = c.canceller();
    let h = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        cancel.cancel();
    });
    let t = Instant::now();
    assert_eq!(c.call("Wait.for.ever", json!({}), None, Duration::from_secs(10)), Err(Error::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(2));
    h.join().unwrap();
}

#[test]
fn the_token_goes_into_one_authorization_header_and_nowhere_else() {
    let fake = Fake::start(Script::new().token("s3cret-token-value").on("A.b", Answer::result(json!(1)))).unwrap();
    let e = Client::connect(fake.addr(), "/", None, &limits()).err().unwrap();
    assert_eq!(e, Error::Ws(lico_ws::Error::Refused(401)));
    let e = Client::connect(fake.addr(), "/", Some("wrong-token"), &limits()).err().unwrap();
    assert!(!format!("{e:?}{e}").contains("wrong-token"));
    let mut c = Client::connect(fake.addr(), "/", Some("s3cret-token-value"), &limits()).unwrap();
    assert_eq!(c.call("A.b", json!({}), None, SEC).unwrap(), json!(1));
    let shown = format!("{c:?} {:?}", fake.heads());
    assert!(!shown.contains("s3cret-token-value"), "{shown}");
    assert_eq!(fake.heads()[2].authorization(), Some("Bearer s3cret-token-value"));
    assert!(Client::connect(fake.addr(), "/", Some("bad\r\nInjected: 1"), &limits()).is_err(), "a token that is not one clean line");
}

#[test]
fn an_address_that_is_not_loopback_is_refused_before_any_connection() {
    let t = Instant::now();
    let e = Client::connect("192.0.2.1:9".parse().unwrap(), "/", None, &limits()).err().unwrap();
    assert_eq!(e, Error::Ws(lico_ws::Error::NotLoopback));
    assert!(t.elapsed() < Duration::from_millis(500));
}

// ---- the parameters of the commands -----------------------------------------------------------------------------------------------------------------

#[test]
fn the_builders_make_the_parameters_the_commands_take() {
    assert_eq!(cmd::navigate("http://127.0.0.1:1/a?b=c"), json!({"url": "http://127.0.0.1:1/a?b=c"}));
    assert_eq!(cmd::set_viewport(1200, 800, 1.0), json!({"width": 1200, "height": 800, "deviceScaleFactor": 1.0, "mobile": false}));
    assert_eq!(cmd::start_screencast(ImageFormat::Jpeg, 80, 1200, 800, 1), json!({"format": "jpeg", "quality": 80, "maxWidth": 1200, "maxHeight": 800, "everyNthFrame": 1}));
    assert_eq!(cmd::start_screencast(ImageFormat::Png, 100, 640, 480, 2)["format"], "png");
    assert_eq!(cmd::screencast_ack(7), json!({"sessionId": 7}));
    assert_eq!(cmd::capture_screenshot(ImageFormat::Png), json!({"format": "png"}));
    assert_eq!(cmd::mouse(MouseKind::Pressed, 10.5, 20.0, Button::Left, 1, 0.0), json!({"type": "mousePressed", "x": 10.5, "y": 20.0, "button": "left", "clickCount": 1, "deltaX": 0.0, "deltaY": 0.0}));
    assert_eq!(cmd::mouse(MouseKind::Wheel, 5.0, 6.0, Button::None, 0, 120.0)["deltaY"], 120.0);
    assert_eq!(cmd::mouse(MouseKind::Moved, 1.0, 2.0, Button::None, 0, 0.0)["type"], "mouseMoved");
    assert_eq!(cmd::mouse(MouseKind::Released, 1.0, 2.0, Button::Right, 1, 0.0)["button"], "right");
    assert_eq!(cmd::key(KeyKind::Down, "a", "KeyA", "a"), json!({"type": "keyDown", "key": "a", "code": "KeyA", "text": "a"}));
    assert_eq!(cmd::key(KeyKind::Up, "Enter", "Enter", ""), json!({"type": "keyUp", "key": "Enter", "code": "Enter"}), "no text, no text key");
    assert_eq!(cmd::key(KeyKind::Char, "한", "", "한")["type"], "char");
    assert_eq!(cmd::insert_text("안녕"), json!({"text": "안녕"}));
    assert_eq!(cmd::create_target("about:blank"), json!({"url": "about:blank"}));
    assert_eq!(cmd::attach("T1"), json!({"targetId": "T1", "flatten": true}));
    assert_eq!((method::NAVIGATE, method::START_SCREENCAST, method::SCREENCAST_ACK, method::MOUSE, method::KEY, method::INSERT_TEXT), ("Page.navigate", "Page.startScreencast", "Page.screencastFrameAck", "Input.dispatchMouseEvent", "Input.dispatchKeyEvent", "Input.insertText"));
}

#[test]
fn a_screencast_conversation_works_end_to_end_with_the_fake() {
    let fake = Fake::start(
        Script::new()
            .on(method::START_SCREENCAST, Answer::result(json!({})).event_in("S1", method::SCREENCAST_FRAME, screencast_frame(1, "QUJD", 640, 480)))
            .on(method::SCREENCAST_ACK, Answer::result(json!({}))),
    )
    .unwrap();
    let mut c = client(&fake);
    c.call(method::START_SCREENCAST, cmd::start_screencast(ImageFormat::Jpeg, 80, 640, 480, 1), Some("S1"), SEC).unwrap();
    let frame = c.next_event(SEC).unwrap().unwrap();
    assert_eq!(frame.method, method::SCREENCAST_FRAME);
    let id = frame.params["sessionId"].as_u64().unwrap();
    c.call(method::SCREENCAST_ACK, cmd::screencast_ack(id), Some("S1"), SEC).unwrap();
    assert_eq!(fake.calls()[1].params, json!({"sessionId": 1}));
}
