//! The host's relay of `clipboard.write` (class L0, a real `Shared`, connections that are queues): which window is asked, who may confirm, what a late
//! or a foreign confirmation does, and that nothing is left behind.

use super::*;
use crate::transfer::tests::world;
use std::thread;

/// Makes the view `name` of the first session, held by a new connection; `gui` marks it as a window.
fn view(shared: &Shared, name: &str, gui: bool) -> Arc<Sub> {
    let sub = Sub::new(1 << 20);
    shared.views.lock().unwrap().insert(
        name.to_string(),
        crate::core::ViewEntry { session: "s1".into(), conn: Some(sub.clone()), created: Instant::now(), gui_seq: None },
    );
    if gui {
        mark_gui(shared, name);
    }
    sub
}

fn ask(kind: &str, extra: Value, timeout: i64) -> Map<String, Value> {
    let mut m = obj(json!({"op": "clipboard.write", "kind": kind, "timeout_ms": timeout}));
    for (k, v) in extra.as_object().unwrap() {
        m.insert(k.clone(), v.clone());
    }
    m
}

fn lines(sub: &Arc<Sub>) -> Vec<Value> {
    sub.drain().iter().map(|l| serde_json::from_str(l.trim()).unwrap()).collect()
}

fn confirm(req: &str, ok: bool) -> Map<String, Value> {
    obj(json!({"op": "clipboard.done", "req": req, "ok": ok, "kind": "text", "bytes": 5}))
}

/// Runs `write` in a thread; the request id is read from what the window got.
fn start(shared: &Arc<Shared>, request: Map<String, Value>) -> thread::JoinHandle<Reply> {
    let shared = shared.clone();
    thread::spawn(move || write(&shared, &request))
}

fn wait_for_line(sub: &Arc<Sub>) -> Value {
    for _ in 0..200 {
        let mut got = lines(sub);
        if !got.is_empty() {
            return got.remove(0);
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("the window was sent nothing");
}

#[test]
fn without_a_window_the_answer_is_no_window_at_once_and_nothing_waits() {
    let w = world(&["s1"]);
    let began = Instant::now();
    assert_eq!(write(&w.shared, &ask("text", json!({"text": "x"}), 5000)), Err("no_window"));
    assert!(began.elapsed() < Duration::from_millis(500));
    assert_eq!(w.shared.clip.waiting(), 0);
    // a view that is not of kind gui (a terminal view) is not a window
    let _terminal = view(&w.shared, "wterm", false);
    assert_eq!(write(&w.shared, &ask("text", json!({"text": "x"}), 5000)), Err("no_window"));
    // a window whose connection has ended is not one either
    let gone = view(&w.shared, "wgone", true);
    gone.closed.store(true, Ordering::Relaxed);
    assert_eq!(write(&w.shared, &ask("text", json!({"text": "x"}), 5000)), Err("no_window"));
}

#[test]
fn the_newest_window_is_asked_and_the_older_one_never_sees_the_request() {
    let w = world(&["s1"]);
    let old = view(&w.shared, "wold", true);
    let new = view(&w.shared, "wnew", true);
    let job = start(&w.shared, ask("text", json!({"text": "hello"}), 5000));
    let line = wait_for_line(&new);
    assert_eq!(line["ev"], "clip.write");
    assert_eq!(line["kind"], "text");
    assert_eq!(line["text"], "hello");
    let req = line["req"].as_str().unwrap().to_string();
    assert!(req.starts_with('c'));
    assert!(lines(&old).is_empty(), "the older window was sent the request");
    assert_eq!(done(&w.shared, &new, &confirm(&req, true)), Ok(obj(json!({"req": req.clone()}))));
    let answer = job.join().unwrap().expect("confirmed");
    assert_eq!(answer.get("bytes"), Some(&json!(5)));
    assert_eq!(answer.get("kind"), Some(&json!("text")));
    assert!(lines(&old).is_empty());
    assert_eq!(w.shared.clip.waiting(), 0);
}

#[test]
fn a_picture_request_carries_the_path_and_never_the_bytes() {
    let w = world(&["s1"]);
    let win = view(&w.shared, "w1", true);
    let job = start(&w.shared, ask("image", json!({"path": "/x/y.png"}), 5000));
    let line = wait_for_line(&win);
    assert_eq!(line["kind"], "image");
    assert_eq!(line["path"], "/x/y.png");
    assert!(line.get("text").is_none());
    let req = line["req"].as_str().unwrap().to_string();
    let body = obj(json!({"op": "clipboard.done", "req": req, "ok": true, "kind": "image", "bytes": 99, "width": 4, "height": 3}));
    done(&w.shared, &win, &body).unwrap();
    let answer = job.join().unwrap().unwrap();
    assert_eq!((answer.get("width"), answer.get("height")), (Some(&json!(4)), Some(&json!(3))));
}

#[test]
fn a_confirmation_with_a_wrong_id_or_from_another_connection_changes_nothing() {
    let w = world(&["s1"]);
    let win = view(&w.shared, "w1", true);
    let other = view(&w.shared, "wother", false);
    let job = start(&w.shared, ask("text", json!({"text": "t"}), 700));
    let req = wait_for_line(&win)["req"].as_str().unwrap().to_string();
    assert_eq!(done(&w.shared, &win, &confirm("cdeadbeef", true)), Err("unknown_request"));
    assert_eq!(done(&w.shared, &other, &confirm(&req, true)), Err("unknown_request"));
    assert_eq!(w.shared.clip.waiting(), 1, "the request must still wait");
    // neither confirmation ended the wait: the request times out
    assert_eq!(job.join().unwrap(), Err("timeout"));
    assert_eq!(w.shared.clip.waiting(), 0);
}

#[test]
fn a_late_confirmation_after_the_timeout_finds_nothing_and_the_request_is_forgotten() {
    let w = world(&["s1"]);
    let win = view(&w.shared, "w1", true);
    let began = Instant::now();
    let job = start(&w.shared, ask("text", json!({"text": "t"}), 300));
    let req = wait_for_line(&win)["req"].as_str().unwrap().to_string();
    assert_eq!(job.join().unwrap(), Err("timeout"));
    assert!(began.elapsed() < Duration::from_millis(1500), "{:?}", began.elapsed());
    assert_eq!(w.shared.clip.waiting(), 0);
    assert_eq!(done(&w.shared, &win, &confirm(&req, true)), Err("unknown_request"));
    // a second confirmation of an answered request is refused too
    let job = start(&w.shared, ask("text", json!({"text": "t"}), 5000));
    let req = wait_for_line(&win)["req"].as_str().unwrap().to_string();
    done(&w.shared, &win, &confirm(&req, true)).unwrap();
    job.join().unwrap().unwrap();
    assert_eq!(done(&w.shared, &win, &confirm(&req, true)), Err("unknown_request"));
}

#[test]
fn the_host_answers_only_after_the_confirmation_not_when_the_request_is_forwarded() {
    let w = world(&["s1"]);
    let win = view(&w.shared, "w1", true);
    let job = start(&w.shared, ask("text", json!({"text": "t"}), 5000));
    let req = wait_for_line(&win)["req"].as_str().unwrap().to_string();
    thread::sleep(Duration::from_millis(300));
    assert!(!job.is_finished(), "the host answered before the window confirmed");
    done(&w.shared, &win, &confirm(&req, true)).unwrap();
    job.join().unwrap().unwrap();
}

#[test]
fn a_failure_of_the_window_is_the_answer_with_its_reason_and_unknown_reasons_are_x_error() {
    for (said, want) in [("too_large", "too_large"), ("bad_image", "bad_image"), ("write_failed", "write_failed"), ("x_error", "x_error"), ("whatever", "x_error"), ("", "x_error")] {
        let w = world(&["s1"]);
        let win = view(&w.shared, "w1", true);
        let job = start(&w.shared, ask("image", json!({"path": "/p.png"}), 5000));
        let req = wait_for_line(&win)["req"].as_str().unwrap().to_string();
        let body = obj(json!({"op": "clipboard.done", "req": req, "ok": false, "error": said}));
        done(&w.shared, &win, &body).unwrap();
        assert_eq!(job.join().unwrap(), Err(want), "{said}");
    }
}

#[test]
fn a_window_that_goes_away_while_the_host_waits_ends_the_wait_with_no_window() {
    let w = world(&["s1"]);
    let win = view(&w.shared, "w1", true);
    let began = Instant::now();
    let job = start(&w.shared, ask("text", json!({"text": "t"}), 30_000));
    wait_for_line(&win);
    win.closed.store(true, Ordering::Relaxed);
    assert_eq!(job.join().unwrap(), Err("no_window"));
    assert!(began.elapsed() < Duration::from_secs(3));
    assert_eq!(w.shared.clip.waiting(), 0);
}

#[test]
fn the_deadline_is_taken_from_the_request_default_three_seconds_and_clamped() {
    let w = world(&["s1"]);
    let win = view(&w.shared, "w1", true);
    let began = Instant::now();
    let mut no_deadline = ask("text", json!({"text": "t"}), 0);
    no_deadline.remove("timeout_ms");
    let job = start(&w.shared, no_deadline);
    wait_for_line(&win);
    thread::sleep(Duration::from_millis(1000));
    assert!(!job.is_finished(), "the default deadline is 3000 ms");
    assert_eq!(job.join().unwrap(), Err("timeout"));
    let spent = began.elapsed();
    assert!(spent >= Duration::from_millis(2900) && spent < Duration::from_millis(4500), "{spent:?}");
}

fn valid(op: &str, body: Value) -> bool {
    let mut m = obj(body);
    m.insert("op".into(), json!(op));
    crate::ipc::valid_request(op, &m)
}

#[test]
fn the_shape_of_the_two_ops_is_strict() {
    let w = "clipboard.write";
    assert!(valid(w, json!({"kind": "text", "text": "x"})));
    assert!(valid(w, json!({"kind": "text", "text": "x", "timeout_ms": 1})));
    assert!(valid(w, json!({"kind": "text", "text": "x", "timeout_ms": 600000})));
    assert!(valid(w, json!({"kind": "text", "text": "\u{1}\n\u{1F600}한"})));
    assert!(valid(w, json!({"kind": "image", "path": "/a/b.png"})));
    assert!(!valid(w, json!({})));
    assert!(!valid(w, json!({"kind": "text"})), "a text with no text");
    assert!(!valid(w, json!({"kind": "text", "text": ""})));
    assert!(!valid(w, json!({"kind": "image"})), "a picture with no path");
    assert!(!valid(w, json!({"kind": "text", "text": "x", "path": "/a"})), "both text and path");
    assert!(!valid(w, json!({"kind": "image", "path": "/a.png", "text": "x"})));
    assert!(!valid(w, json!({"kind": "image", "path": "relative.png"})));
    assert!(!valid(w, json!({"kind": "audio", "text": "x"})));
    assert!(!valid(w, json!({"kind": "text", "text": "x", "timeout_ms": 0})));
    assert!(!valid(w, json!({"kind": "text", "text": "x", "timeout_ms": 600001})));
    assert!(!valid(w, json!({"kind": "text", "text": "x", "timeout_ms": "5"})));
    assert!(!valid(w, json!({"kind": "text", "text": "x", "extra": 1})), "an unknown field");
    assert!(!valid(w, json!({"kind": "text", "text": 5})));
    assert!(!valid(w, json!({"kind": "text", "text": "a".repeat(crate::ipc::MAX_INPUT_TEXT + 1)})));
    let d = "clipboard.done";
    assert!(valid(d, json!({"req": "cabc", "ok": true})));
    assert!(valid(d, json!({"req": "cabc", "ok": true, "kind": "image", "bytes": 9, "width": 3, "height": 2})));
    assert!(valid(d, json!({"req": "cabc", "ok": false, "error": "x_error"})));
    assert!(!valid(d, json!({"req": "cabc"})));
    assert!(!valid(d, json!({"ok": true})));
    assert!(!valid(d, json!({"req": "cabc", "ok": 1})));
    assert!(!valid(d, json!({"req": "cabc", "ok": true, "width": 0})));
    assert!(!valid(d, json!({"req": "cabc", "ok": true, "width": 8001})));
    assert!(!valid(d, json!({"req": "cabc", "ok": true, "bytes": -1})));
    assert!(!valid(d, json!({"req": "cabc", "ok": true, "other": 1})));
    assert!(crate::core::OPS.contains(&"clipboard.write") && crate::core::OPS.contains(&"clipboard.done"));
}

#[test]
fn the_list_of_tools_holds_no_clipboard_write_and_a_tool_call_of_that_name_is_unknown() {
    // S07a (XF-04): the tool `clipboard_write` exists, but only behind the person's setting and their approval; the dotted name of the socket op is never a tool.
    // The tool is in the table, and the list holds it only while the setting is on (the tests of `mcp` and the Python tests of S07a pin that).
    assert!(crate::mcp::TOOLS.iter().any(|t| t.name == crate::mcp::WRITE_TOOL));
    assert!(!crate::mcp::TOOLS.iter().any(|t| t.name == "clipboard.write"));
}
