//! Enter in the box of a disconnected session: the window asks the host to resume it, holds the text, and sends it once as an ordinary input with the new
//! generation. The host here is the other end of a socket pair: the tests read what the window wrote and answer by handing it a line. (The window and a host
//! with each simulator are in `tests/test_reconnect_window.py`; what the host answers is pinned in the files of the host.)

use std::io::Read;
use std::os::unix::net::UnixStream;

use serde_json::{json, Value};

use super::super::super::scroll_oracle::app;
use super::super::Row;
use super::*;

const ENDED_WORDS: &str = "That session has ended and cannot be reopened. Ctrl+N starts a new one. The text is back in the box.";
const RESUMED: &str = r#"{"ok":1,"id":"reconnect-s-old","session":"s-old","generation":4}"#;

fn row(id: &str, state: &str, generation: i64) -> Row {
    Row { session: id.into(), name: id.into(), provider: "claude".into(), state: state.into(), generation, ..Default::default() }
}

/// `s-old` (generation 3, disconnected) is open; `s-live` runs and `s-gone` has ended. The window's connection is a socket pair; the test is the host.
fn window() -> (App, UnixStream) {
    let mut app = app(1.0, false);
    app.ui.rows = vec![row("s-live", "running", 1), row("s-old", "disconnected", 3), row("s-gone", "exited", 2)];
    app.ui.numbered = ["s-live", "s-old", "s-gone"].map(String::from).to_vec();
    app.session = "s-old".into();
    app.generation = 3;
    let (ours, host) = UnixStream::pair().unwrap();
    host.set_nonblocking(true).unwrap();
    app.sock = ours;
    (app, host)
}

/// Every line the window wrote since the last call.
fn asked(host: &mut UnixStream) -> Vec<Value> {
    let (mut text, mut buffer) = (String::new(), [0u8; 65536]);
    while let Ok(n @ 1..) = host.read(&mut buffer) {
        text.push_str(std::str::from_utf8(&buffer[..n]).unwrap());
    }
    text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn ops(lines: &[Value]) -> Vec<&str> {
    lines.iter().map(|l| l["op"].as_str().unwrap()).collect()
}

fn note(app: &App) -> Option<String> {
    app.ed.dump(&app.session)["note"].as_str().map(String::from)
}

/// The person opens another session the way the window does: the box is parked with the session it was typed in, and the other one's draft comes back.
fn open(app: &mut App, session: &str, generation: i64) {
    app.ed.park(&app.session.clone(), "");
    app.session = session.into();
    app.generation = generation;
    app.ed.resume(session);
}

fn type_and_enter(app: &mut App, text: &str) {
    app.ed.set(text.to_string());
    app.send_draft();
}

#[test]
fn enter_in_a_disconnected_session_asks_the_host_to_resume_it_holds_the_text_and_sends_no_input_yet() {
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "what did you find?");
    assert_eq!(asked(&mut host), [json!({"op": "session.resume", "session": "s-old", "generation": 3, "id": "reconnect-s-old"})]);
    assert_eq!(app.ed.text(), "", "the text left the box");
    assert_eq!(note(&app).as_deref(), Some("Reconnecting the session..."));
    assert_eq!(app.pending_inputs.get("reconnect-s-old").map(|p| p.1.as_str()), Some("what did you find?"));
}

#[test]
fn the_answer_sends_the_held_text_once_with_the_generation_of_the_answer_and_a_repeat_of_it_sends_nothing() {
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "what did you find?\nsecond \"line\"");
    asked(&mut host);
    app.on_sock_line(RESUMED);
    assert_eq!(note(&app), None, "\"Reconnecting...\" does not stay on once the session is back");
    let lines = asked(&mut host);
    let inputs: Vec<&Value> = lines.iter().filter(|l| l["op"] == "input").collect();
    assert_eq!(inputs.len(), 1, "{lines:?}");
    let sent = inputs[0];
    assert_eq!((sent["session"].as_str(), sent["generation"].as_i64(), sent["text"].as_str()), (Some("s-old"), Some(4), Some("what did you find?\nsecond \"line\"")));
    let id = sent["request_id"].as_str().unwrap();
    assert!(id.starts_with("gui-") && id.ends_with('r') && sent["id"] == sent["request_id"], "{sent}");
    assert!(ops(&lines).iter().all(|op| ["input", "status"].contains(op)), "{lines:?}");
    assert!(!app.pending_inputs.contains_key("reconnect-s-old") && app.pending_inputs.contains_key(id));
    app.on_sock_line(RESUMED);
    assert!(asked(&mut host).is_empty(), "the text went with the first answer");
    app.on_sock_line(&format!(r#"{{"ok":1,"id":"{id}","session":"s-old","request_id":"{id}","state":"accepted","duplicate":false}}"#));
    assert!(app.pending_inputs.is_empty() && app.ed.text() == "");
}

#[test]
fn a_second_enter_while_the_session_is_reconnecting_is_ignored_and_the_new_text_stays_in_the_box() {
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "first");
    asked(&mut host);
    type_and_enter(&mut app, "second");
    assert!(asked(&mut host).is_empty(), "no second resume and no input");
    assert_eq!((app.ed.text(), note(&app).as_deref()), ("second", Some("Reconnecting the session...")));
    assert_eq!(app.pending_inputs.len(), 1);
    assert_eq!(app.pending_inputs["reconnect-s-old"].1, "first", "held once");
    app.resume_session();
    assert!(asked(&mut host).is_empty() && app.ui.flash.as_deref() == Some("Resume session: the session is being reconnected"), "Ctrl+R does not start a second resume");
}

#[test]
fn the_text_goes_to_the_session_it_was_typed_in_with_that_sessions_new_generation_when_another_is_open_by_then() {
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "for the old one");
    asked(&mut host);
    open(&mut app, "s-live", 1);
    app.ed.put("a draft of the live one");
    app.on_sock_line(RESUMED);
    let lines = asked(&mut host);
    let input = lines.iter().find(|l| l["op"] == "input").expect("the input");
    assert_eq!((input["session"].as_str(), input["generation"].as_i64(), input["text"].as_str()), (Some("s-old"), Some(4), Some("for the old one")));
    assert_eq!(app.ed.text(), "a draft of the live one", "the box of the open session is not touched");
    // and when the host refuses it there, it waits in the draft of the old session, with no line under the other box
    let id = input["request_id"].as_str().unwrap();
    app.on_sock_line(&format!(r#"{{"ok":0,"id":"{id}","error":"session_disconnected"}}"#));
    assert_eq!((app.ed.text(), note(&app)), ("a draft of the live one", None));
    assert_eq!(app.ed.dump("s-old")["parked"], 1);
}

#[test]
fn enter_in_an_ended_session_sends_nothing_and_says_it_cannot_be_reopened_and_the_text_stays() {
    let (mut app, mut host) = window();
    app.session = "s-gone".into();
    type_and_enter(&mut app, "hello?");
    assert!(asked(&mut host).is_empty() && app.pending_inputs.is_empty());
    assert_eq!((app.ed.text(), note(&app).as_deref()), ("hello?", Some(ENDED_WORDS)));
}

#[test]
fn enter_in_an_ended_session_with_an_empty_or_too_long_box_is_the_ordinary_path_and_does_not_say_the_session_ended() {
    for (text, said) in [("", None), ("  \n ", None), (&"x".repeat(MAX_TEXT + 1)[..], Some("Not sent: "))] {
        let (mut app, mut host) = window();
        app.session = "s-gone".into();
        type_and_enter(&mut app, text);
        assert!(asked(&mut host).is_empty() && app.pending_inputs.is_empty(), "{} bytes", text.len());
        match said {
            None => assert_eq!(note(&app), None, "an empty box is not an Enter worth an answer"),
            Some(start) => assert!(note(&app).unwrap().starts_with(start) && note(&app).as_deref() != Some(ENDED_WORDS), "{:?}", note(&app)),
        }
    }
}

#[test]
fn enter_in_a_running_session_is_the_ordinary_send_and_an_empty_or_too_long_box_never_asks_for_a_resume() {
    let (mut app, mut host) = window();
    app.session = "s-live".into();
    app.generation = 1;
    type_and_enter(&mut app, "ordinary");
    let lines = asked(&mut host);
    assert_eq!(ops(&lines), ["input"]);
    let id = lines[0]["request_id"].as_str().unwrap();
    assert!(id.starts_with("gui-") && !id.ends_with('r') && lines[0]["generation"] == 1, "{id}");
    app.session = "s-old".into();
    for text in ["", "  \n ", &"x".repeat(MAX_TEXT + 1)] {
        type_and_enter(&mut app, text);
        assert!(asked(&mut host).is_empty() && app.pending_inputs.len() == 1, "{} bytes", text.len());
    }
    assert!(note(&app).unwrap().starts_with("Not sent: ") && app.ed.text().len() == MAX_TEXT + 1, "over the limit it says so and keeps the text: {:?}", note(&app));
}

#[test]
fn a_refused_resume_gives_the_text_back_with_a_sentence_and_sends_no_input() {
    let table = [
        ("bad_request", "That session never got a conversation, so there is nothing to reconnect. Ctrl+N starts a new one. The text is back in the box."),
        ("stale_generation", "Not sent: the session changed meanwhile; look at it and send again. The text is back in the box."),
        ("busy", "Not sent: the session could not be reconnected: the host could not take it just now. The text is back in the box."),
        ("folder_gone", "Not sent: the session could not be reconnected: the folder of the session is not usable any more. The text is back in the box."),
        ("some_new_code", "Not sent: the session could not be reconnected: the host answered some_new_code. The text is back in the box."),
    ];
    for (code, said) in table {
        let (mut app, mut host) = window();
        type_and_enter(&mut app, "keep me");
        asked(&mut host);
        app.on_sock_line(&format!(r#"{{"ok":0,"id":"reconnect-s-old","error":"{code}"}}"#));
        let lines = asked(&mut host);
        assert!(!ops(&lines).contains(&"input") && !ops(&lines).contains(&"session.resume"), "{code}: {lines:?}");
        assert_eq!((app.ed.text(), note(&app).as_deref()), ("keep me", Some(said)), "{code}");
        assert!(app.pending_inputs.is_empty(), "{code}");
        app.send_draft(); // the person presses Enter again: a new resume is asked for
        assert_eq!(ops(&asked(&mut host)), ["session.resume"], "{code}: the same text is asked for again");
    }
    // another session is open when the refusal comes: the text waits in the draft of its own session
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "keep me");
    asked(&mut host);
    open(&mut app, "s-live", 1);
    app.on_sock_line(r#"{"ok":0,"id":"reconnect-s-old","error":"bad_request"}"#);
    assert_eq!((app.ed.text(), note(&app), app.ed.dump("s-old")["parked"].as_i64()), ("", None, Some(1)));
}

#[test]
fn a_session_that_ends_again_at_once_refuses_the_text_and_it_says_so_and_is_not_reconnected_a_second_time() {
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "do you remember?");
    asked(&mut host);
    app.on_sock_line(RESUMED);
    let id = asked(&mut host).iter().find(|l| l["op"] == "input").unwrap()["request_id"].as_str().unwrap().to_string();
    app.on_sock_line(&format!(r#"{{"ok":0,"id":"{id}","error":"session_disconnected"}}"#));
    assert_eq!(app.ed.text(), "do you remember?");
    assert_eq!(note(&app).as_deref(), Some("The session was reconnected but ended again at once; the provider could not continue that conversation. The text is back in the box."));
    assert!(asked(&mut host).iter().all(|l| l["op"] != "session.resume" && l["op"] != "input"), "no second automatic resume");
    // a text refused as disconnected on the ordinary path is another thing: the row said it was running
    app.session = "s-live".into();
    app.generation = 1;
    type_and_enter(&mut app, "ordinary");
    let id = asked(&mut host).iter().find(|l| l["op"] == "input").unwrap()["request_id"].as_str().unwrap().to_string();
    app.on_sock_line(&format!(r#"{{"ok":0,"id":"{id}","error":"session_disconnected"}}"#));
    let said = note(&app).unwrap();
    assert!(said.contains("press Enter again to reconnect it and send") && !said.contains("ended again at once"), "{said}");
}

#[test]
fn the_permanent_line_names_what_enter_does_in_a_disconnected_session_shorter_when_the_window_is_narrow() {
    let (mut app, _host) = window();
    let wide = app.hint_text(1200, 1.0);
    assert!(wide.starts_with("Disconnected: Enter reconnects and sends, Ctrl+R reconnects   Ctrl+N new session"), "{wide}");
    let narrow = app.hint_text(300, 1.0);
    assert_eq!(narrow, "Disconnected: Enter reconnects", "{narrow}");
    app.session = "s-live".into();
    assert!(!app.hint_text(1200, 1.0).contains("Disconnected"), "a running session has the ordinary line");
    app.session = "s-gone".into();
    assert!(!app.hint_text(1200, 1.0).contains("Disconnected"));
}

#[test]
fn a_replaced_host_gives_the_waiting_text_back_and_lets_go_of_what_was_in_flight() {
    let (mut app, mut host) = window();
    app.session = "s-live".into();
    app.generation = 1;
    type_and_enter(&mut app, "in flight");
    asked(&mut host);
    app.session = "s-old".into();
    type_and_enter(&mut app, "waiting");
    asked(&mut host);
    app.release_held();
    assert!(app.pending_inputs.is_empty());
    assert_eq!(app.ed.text(), "waiting");
    assert_eq!(note(&app).as_deref(), Some("Not sent: the host was replaced while the session was reconnecting. The text is back in the box."));
}
