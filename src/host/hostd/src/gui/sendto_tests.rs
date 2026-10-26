//! Sending text to another session from the window: when the menu offers it, what the panel lists and takes, what the delivery line says, that nothing
//! of the person's text is lost when the host does not carry the send out, and the look of the header on the receiving side. (The host's side is in
//! `tests/test_transfer.py`, the whole way through the window in `tests/test_send_to_session_window.py`; the host here is a few lines that answer.)

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

use cosmic_text::{Attrs, AttrsList, Color, Family};
use serde_json::{json, Value};

use super::super::super::menu::Cmd;
use super::super::super::scroll_oracle::app;
use super::super::super::ui::Row;
use super::super::Step;
use super::*;
use crate::handoff::Scratch;

const DRAFT: &str = "please check the login test";
const HEADER: &str = ">> [from session 'api-server' (s012345) | 14:02 | message | 27 B]";

/// `api` is open with the draft in its box; `docs` (Codex) can receive, `old` is closed.
fn three() -> App {
    let row = |id: &str, name: &str, provider: &str, state: &str, activity: &str| Row { session: id.into(), name: name.into(), provider: provider.into(), state: state.into(), activity: activity.into(), generation: 1, ..Default::default() };
    let mut app = app(1.0, false);
    app.ui.rows = vec![row("session-s0", "api", "claude", "running", "idle"), row("session-s1", "docs", "codex", "running", "idle"), row("session-s2", "old", "claude", "disconnected", "")];
    app.ui.numbered = ["session-s0", "session-s1", "session-s2"].map(String::from).to_vec();
    app.ed.put(DRAFT);
    app
}

/// `api` and `docs` only: the one other session is the one to send to.
fn two() -> App {
    let mut app = three();
    app.ui.rows.truncate(2);
    app.ui.numbered.truncate(2);
    app
}

fn select(app: &mut App, lines: &[&str]) {
    app.view.as_mut().unwrap().lines = lines.iter().enumerate().map(|(i, l)| (i as i64, l.to_string())).collect();
    app.ui.sel = Some((0, lines.len() - 1));
}

/// Forty lines of 2000 bytes: 80 KB, over the 64 KiB of one input.
fn select_huge(app: &mut App) {
    let line = "x".repeat(2000);
    select(app, &[line.as_str(); 40]);
}

fn step(app: &App) -> Option<Step> {
    app.ui.panel.as_ref().map(|p| p.step)
}

fn notice(app: &App) -> String {
    app.ui.panel.as_ref().map(|p| p.notice.clone()).unwrap_or_default()
}

fn outgoing(app: &App) -> &Outgoing {
    app.ui.panel.as_ref().unwrap().send.as_ref().unwrap()
}

fn lines(app: &App) -> Vec<String> {
    app.panel_view().unwrap().1
}

fn at(body: &[String], needle: &str) -> usize {
    body.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle:?} is on no line of {body:#?}"))
}

/// A host that answers each connection with the next reply and keeps what it was asked. One that is not asked within three seconds stops waiting
/// (the test then finds fewer questions than it expected, and says so, instead of hanging).
pub(in super::super) fn fake_host(dir: &Scratch, replies: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    let path = dir.0.join("h.sock");
    let _ = std::fs::remove_file(&path); // a test that asks twice leaves the first one's file behind
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        let mut asked = Vec::new();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
        for reply in replies {
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(_) if std::time::Instant::now() < end => std::thread::sleep(std::time::Duration::from_millis(5)),
                    Err(_) => return asked,
                }
            };
            stream.set_nonblocking(false).unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            asked.push(serde_json::from_str(&line).unwrap());
            (&stream).write_all(format!("{reply}\n").as_bytes()).unwrap();
        }
        asked
    });
    (path.to_string_lossy().into_owned(), handle)
}

fn delivered(to: &str, delivery: &str, size: usize, receiver: &str) -> Value {
    json!({"ok": 1, "to": to, "request_id": "x-1", "kind": "text", "delivery": delivery, "size": size, "receiver": receiver, "hops": 0})
}

#[test]
fn the_menu_offers_a_send_only_with_another_running_session_and_something_to_send_and_says_why_not() {
    let entry = |app: &App| app.menu_entries(1).into_iter().find(|e| e.label == "Send to session...").map(|e| (e.key, e.reason));
    let mut app = three();
    assert_eq!(entry(&app), Some(("Ctrl+Shift+S", None)));
    app.ed.clear();
    assert_eq!(entry(&app).unwrap().1, Some("nothing to send: type in the input box or select lines"));
    select(&mut app, &["one", "two"]);
    assert_eq!(entry(&app).unwrap().1, None, "selected lines are enough without a draft");
    select_huge(&mut app);
    assert_eq!(entry(&app).unwrap().1, Some("the selected lines are over 64 KiB: select fewer"));
    app.ed.put(DRAFT);
    assert_eq!(entry(&app).unwrap().1, None, "the draft can be sent whatever the selection is");
    app.panel_open(Step::Keys);
    assert_eq!(entry(&app).unwrap().1, Some("close the open panel first"));
    app.panel_close();
    app.ui.rows[1].state = "disconnected".into();
    assert_eq!(entry(&app).unwrap().1, Some("no other session is running"));
    app.ui.rows.truncate(1);
    app.ui.numbered.truncate(1);
    assert_eq!(entry(&app).unwrap().1, Some("needs two sessions"));
    app.session.clear();
    assert_eq!(entry(&app).unwrap().1, Some("no session is open"));
}

#[test]
fn the_hand_over_entry_is_offered_for_a_claude_session_with_another_running_one_whatever_the_box_holds_and_says_why_not_otherwise() {
    let entry = |app: &App| app.menu_entries(1).into_iter().find(|e| e.label == "Hand over conversation...").map(|e| (e.key, e.reason));
    let mut app = three();
    app.ed.clear(); // the conversation is the source: no draft and no selection are needed
    assert_eq!(entry(&app), Some(("", None)));
    app.panel_open(Step::Keys);
    assert_eq!(entry(&app).unwrap().1, Some("close the open panel first"));
    app.panel_close();
    app.ui.rows[1].state = "disconnected".into();
    assert_eq!(entry(&app).unwrap().1, Some("no other session is running"));
    app.ui.rows[1].state = "running".into();
    app.ui.rows[0].provider = "codex".into();
    assert_eq!(entry(&app).unwrap().1, Some("only a Claude session has a record to hand over"));
    app.run_cmd(Cmd::HandOver);
    assert!(app.ui.panel.is_none() && app.ui.flash.as_deref() == Some("Hand over conversation: only a Claude session has a record to hand over"), "a dimmed entry opens nothing");
    app.ui.rows[0].provider = "claude".into();
    app.ui.rows.truncate(1);
    app.ui.numbered.truncate(1);
    assert_eq!(entry(&app).unwrap().1, Some("needs two sessions"));
    app.session.clear();
    assert_eq!(entry(&app).unwrap().1, Some("no session is open"));
}

#[test]
fn the_hand_over_panel_has_the_conversation_chosen_says_how_it_goes_and_what_it_may_hold_and_the_other_sources_stay_there() {
    let mut app = three();
    app.run_cmd(Cmd::HandOver);
    assert_eq!((step(&app), outgoing(&app).source), (Some(Step::Send), Source::Conversation));
    let body = lines(&app);
    let whole = &body[at(&body, "Whole conversation")];
    assert!(whole.contains("[x]") && whole.contains(" C ") && whole.contains("the host reads the program's record"), "{whole}");
    assert!(body[at(&body, "Draft text")].contains("[ ]") && body[at(&body, "Selected lines")].contains("[ ]"), "{body:?}");
    let delivery = at(&body, "Delivery");
    assert!(body[delivery].contains("as a file. The host reads the program's record when you press Enter. Nothing is cut up to 32 MiB."), "{body:?}");
    assert!(body[delivery + 1].contains("may hold what you or a tool typed") && body[delivery + 2].contains("keys, passwords, other people's data"), "{body:?}");
    let go = |app: &App| lines(app).into_iter().find(|l| l.starts_with("Enter  ")).unwrap();
    assert_eq!(go(&app), "Enter  Hand over (pick a session first)");
    app.send_key('2');
    assert_eq!(go(&app), "Enter  Hand over to 'docs'");
    assert!(lines(&app).iter().any(|l| l.contains("Saved in the handoff folder of 'docs'")), "{:?}", lines(&app));
    app.send_key('d');
    let body = lines(&app);
    assert!(!body.iter().any(|l| l.contains("may hold what you or a tool typed")) && go(&app) == "Enter  Send to 'docs'", "{body:?}");
    app.send_key('c');
    assert_eq!(outgoing(&app).source, Source::Conversation);
    let mut by_click = three();
    by_click.run_cmd(Cmd::HandOver);
    by_click.send_key('l');
    let body = lines(&by_click);
    assert!(by_click.send_click(at(&body, "Whole conversation")) && outgoing(&by_click).source == Source::Conversation, "a click on the line does what C does");
    // the chord from a session that is not Claude: the line says so and the source cannot be chosen
    let mut codex = three();
    codex.ui.rows[0].provider = "codex".into();
    codex.send_ask();
    assert!(lines(&codex)[at(&lines(&codex), "Whole conversation")].contains("only a Claude session has a record"), "{:?}", lines(&codex));
    codex.send_key('c');
    assert!(outgoing(&codex).source == Source::Draft && notice(&codex).starts_with("Only a Claude session has a record"), "{}", notice(&codex));
}

#[test]
fn a_conversation_the_host_hands_over_is_asked_for_without_text_leaves_the_draft_and_a_refusal_keeps_the_panel_and_points_to_selected_lines() {
    let dir = Scratch::new("sendto");
    let done = json!({"ok": 1, "to": "session-s1", "request_id": "x-2", "kind": "conversation", "delivery": "file", "size": 123_456, "receiver": "idle", "hops": 0});
    let (path, host) = fake_host(&dir, vec![done]);
    let mut app = two();
    app.socket = path;
    app.run_cmd(Cmd::HandOver);
    app.send_go();
    assert_eq!(host.join().unwrap(), [json!({"op": "transfer.send", "from": "session-s0", "to": "session-s1", "kind": "conversation"})], "no text, and the open session is the one whose conversation it is");
    assert!(app.ui.panel.is_none() && app.ed.text() == DRAFT, "the draft was not what was handed over");
    assert_eq!(app.ui.flash.as_deref(), Some("Handed over the conversation to 'docs' (as a file, 120.6 KiB); it was idle."));
    for (error, said) in [
        ("record_not_found", "the host did not find the program's record of this session; select the lines you want and send them with Ctrl+Shift+S"),
        ("record_ambiguous", "more than one record has the id of this session; select the lines you want and send them with Ctrl+Shift+S"),
        ("record_format", "the record is not in the layout this Lico reads (Claude Code 2.1.310); select the lines you want and send them with Ctrl+Shift+S"),
        ("too_large", "the conversation is over 32 MiB; select the lines you want and send them with Ctrl+Shift+S"),
        ("no_conversation_yet", "this session has no conversation yet: its program has not answered once"),
        ("bad_request", "the host cannot read the record of this session"),
        ("session_disconnected", "that session is closed (Ctrl+R in it brings it back)"),
    ] {
        let (path, host) = fake_host(&dir, vec![json!({"ok": 0, "error": error})]);
        let mut app = two();
        app.socket = path;
        app.run_cmd(Cmd::HandOver);
        app.send_go();
        host.join().unwrap();
        assert!(step(&app) == Some(Step::Send) && notice(&app) == format!("Not handed over: {said}. Nothing was sent."), "{error}: {}", notice(&app));
        assert_eq!((outgoing(&app).to.as_deref(), outgoing(&app).source), (Some("session-s1"), Source::Conversation));
        app.panel_close();
        assert_eq!(app.ed.text(), DRAFT, "{error}");
    }
}

#[test]
fn the_chord_opens_the_panel_or_says_why_it_cannot() {
    let mut app = three();
    app.ed.clear();
    app.send_ask();
    assert!(app.ui.panel.is_none() && app.ui.flash.as_deref() == Some("Send to session: nothing to send: type in the input box or select lines"), "{:?}", app.ui.flash);
    app.ed.put(DRAFT);
    app.send_ask();
    assert_eq!(step(&app), Some(Step::Send));
    assert_eq!(app.ed.text(), "", "the panel has the box; the draft is its to give back");
    app.panel_close();
    assert_eq!(app.ed.text(), DRAFT);
}

#[test]
fn the_panel_lists_the_other_sessions_by_their_numbers_and_a_number_or_a_click_picks_one_that_can_receive() {
    let mut app = three();
    app.send_ask();
    let (title, body) = app.panel_view().unwrap();
    assert_eq!(title, "Send to session");
    let (docs, old) = (at(&body, "docs"), at(&body, " old "));
    assert!(body[docs].contains(" 2 ") && body[docs].contains("codex") && body[docs].contains("idle"), "{}", body[docs]);
    assert!(body[old].contains(" 3 ") && body[old].contains("claude") && body[old].contains("closed"), "{}", body[old]);
    assert!(!body.iter().any(|l| l.contains("api")), "the open session is not offered: {body:?}");
    assert_eq!(outgoing(&app).to, None, "with two to choose from, nobody is chosen for the person");
    app.send_go();
    assert!(notice(&app).starts_with("Pick the session to send to") && step(&app) == Some(Step::Send), "{}", notice(&app));
    app.send_key('1');
    assert!(notice(&app).contains("open session") && outgoing(&app).to.is_none(), "{}", notice(&app));
    app.send_key('3');
    assert!(notice(&app).contains("'old' is closed") && outgoing(&app).to.is_none(), "{}", notice(&app));
    app.send_key('9');
    assert!(notice(&app).contains("no session 9") && outgoing(&app).to.is_none(), "{}", notice(&app));
    app.send_key('2');
    assert_eq!(outgoing(&app).to.as_deref(), Some("session-s1"));
    assert_eq!(notice(&app), "", "a good choice takes the complaint away");
    let body = lines(&app);
    assert!(body[at(&body, "docs")].contains("[x]") && body[at(&body, " old ")].contains("[ ]"), "{body:?}");
    assert!(body.iter().any(|l| l.contains("'docs'") && l.contains("came from 'api'")), "the delivery line names both: {body:?}");
    // a click on a line does what its key does; a click on any other line does nothing
    let mut by_click = three();
    by_click.send_ask();
    let body = lines(&by_click);
    assert!(!by_click.send_click(at(&body, "Delivery")), "a line that is only words takes no click");
    assert!(by_click.send_click(at(&body, " old ")) && outgoing(&by_click).to.is_none() && notice(&by_click).contains("'old' is closed"));
    assert!(by_click.send_click(at(&body, "docs")) && outgoing(&by_click).to.as_deref() == Some("session-s1"));
    assert!(!by_click.send_click(999));
    // with one other session it is chosen already
    let alone = {
        let mut a = two();
        a.send_ask();
        a
    };
    assert_eq!(outgoing(&alone).to.as_deref(), Some("session-s1"));
}

#[test]
fn the_selected_lines_are_the_source_when_there_are_some_and_each_source_is_taken_when_the_panel_opens_and_an_empty_or_a_too_large_one_cannot_be_chosen() {
    let mut app = two();
    select(&mut app, &["alpha", "beta"]);
    app.send_ask();
    assert_eq!(outgoing(&app).source, Source::Lines);
    let body = lines(&app);
    assert!(body[at(&body, "Selected lines")].contains("[x]") && body[at(&body, "Selected lines")].contains("(2 lines, 10 B)"), "{body:?}");
    assert!(body[at(&body, "Draft text")].contains("[ ]") && body[at(&body, "Draft text")].contains(&format!("({} B)", DRAFT.len())), "{body:?}");
    app.send_key('d');
    assert_eq!(outgoing(&app).source, Source::Draft);
    app.send_key('l');
    assert_eq!(outgoing(&app).source, Source::Lines);
    app.ui.sel = None; // what was selected when the panel opened is what is sent, whatever happens to the selection meanwhile
    assert_eq!(outgoing(&app).lines, "alpha\nbeta");
    // nothing selected: the draft; the lines cannot be chosen
    let mut app = two();
    app.send_ask();
    assert_eq!(outgoing(&app).source, Source::Draft);
    app.send_key('l');
    assert!(outgoing(&app).source == Source::Draft && notice(&app).starts_with("No lines were selected"), "{}", notice(&app));
    // an empty box: the lines; the draft cannot be chosen
    let mut app = two();
    app.ed.clear();
    select(&mut app, &["one"]);
    app.send_ask();
    assert_eq!(outgoing(&app).source, Source::Lines);
    assert!(lines(&app)[at(&lines(&app), "Selected lines")].contains("(1 line, 3 B)"), "one line is not 1 lines");
    app.send_key('d');
    assert!(outgoing(&app).source == Source::Lines && notice(&app).starts_with("The input box was empty"), "{}", notice(&app));
    // lines over the limit of one input: the draft is the source, and the lines say why they cannot be chosen
    let mut app = two();
    select_huge(&mut app);
    app.send_ask();
    assert_eq!(outgoing(&app).source, Source::Draft);
    assert!(lines(&app)[at(&lines(&app), "Selected lines")].contains("over the limit of 64 KiB"), "{:?}", lines(&app));
    app.send_key('l');
    assert!(outgoing(&app).source == Source::Draft && notice(&app).contains("64 KiB"), "{}", notice(&app));
}

#[test]
fn the_delivery_line_says_inline_up_to_4096_bytes_and_a_file_above_and_that_nothing_is_cut() {
    let cases = [
        (4096, "inline (4.0 KiB is at most 4 KiB)", "The text goes into 'docs' as a message that says it came from 'api'."),
        (4097, "as a file (4.0 KiB is more than 4 KiB). Nothing is cut.", "Saved in the handoff folder of 'docs'; it gets the path and the first lines."),
        (20_000, "as a file (19.5 KiB is more than 4 KiB). Nothing is cut.", "Saved in the handoff folder of 'docs'; it gets the path and the first lines."),
    ];
    for (len, first, second) in cases {
        let mut app = two();
        app.ed.clear();
        app.ed.put(&"x".repeat(len));
        app.send_ask();
        let body = lines(&app);
        let i = at(&body, "Delivery");
        assert!(body[i].contains(first), "{len}: {body:?}");
        assert!(body[i + 1].contains(second), "{len}: {body:?}");
    }
    let mut app = two();
    select(&mut app, &["a", "b", "c"]);
    app.send_ask();
    assert!(lines(&app).iter().any(|l| l.contains("as a selection that says it came from 'api'")), "{:?}", lines(&app));
    let mut app = three();
    app.send_ask();
    let body = lines(&app);
    assert!(body[at(&body, "Delivery")].contains("inline") && body[at(&body, "Delivery") + 1].contains("Pick a session"), "without a receiver the line still says how: {body:?}");
}

#[test]
fn a_send_the_host_carries_out_clears_the_draft_it_sent_and_only_that_and_says_what_the_receiver_was_doing() {
    let dir = Scratch::new("sendto");
    let (path, host) = fake_host(&dir, vec![delivered("session-s1", "inline", DRAFT.len(), "busy")]);
    let mut app = two();
    app.socket = path;
    app.send_ask();
    app.send_go();
    assert_eq!(host.join().unwrap(), [json!({"op": "transfer.send", "from": "session-s0", "to": "session-s1", "kind": "text", "text": DRAFT})]);
    assert!(app.ui.panel.is_none() && app.ed.text() == "", "delivered: the box is empty");
    assert_eq!(app.ui.flash.as_deref(), Some("Sent to 'docs' (inline, 27 B); it was busy."));
    // lines: the draft stays where it is
    let (path, host) = fake_host(&dir, vec![delivered("session-s1", "file", 6000, "waiting_for_approval")]);
    let mut app = two();
    app.socket = path;
    select(&mut app, &["alpha", "beta"]);
    app.send_ask();
    app.send_go();
    assert_eq!(host.join().unwrap(), [json!({"op": "transfer.send", "from": "session-s0", "to": "session-s1", "kind": "lines", "text": "alpha\nbeta"})]);
    assert!(app.ui.panel.is_none() && app.ed.text() == DRAFT, "the draft was not what was sent");
    assert_eq!(app.ui.flash.as_deref(), Some("Sent to 'docs' (as a file, 5.9 KiB); it was waiting for approval."));
}

#[test]
fn a_send_the_host_does_not_carry_out_keeps_the_panel_the_choices_and_the_draft_and_says_why() {
    let dir = Scratch::new("sendto");
    for (error, said) in [("session_disconnected", "that session is closed (Ctrl+R in it brings it back)"), ("unknown_session", "the host does not know that session any more")] {
        let (path, host) = fake_host(&dir, vec![json!({"ok": 0, "error": error})]);
        let mut app = two();
        app.socket = path;
        app.send_ask();
        app.send_go();
        host.join().unwrap();
        assert!(step(&app) == Some(Step::Send) && notice(&app) == format!("Not sent: {said}. The text is kept."), "{}", notice(&app));
        assert_eq!((outgoing(&app).to.as_deref(), outgoing(&app).source), (Some("session-s1"), Source::Draft));
        app.panel_close();
        assert_eq!(app.ed.text(), DRAFT, "{error}");
        std::fs::remove_file(dir.0.join("h.sock")).unwrap();
    }
    let mut app = two(); // no host at all: nothing left the window
    app.send_ask();
    app.send_go();
    assert!(step(&app) == Some(Step::Send) && notice(&app) == "Not sent: the host did not answer. The text is kept.", "{}", notice(&app));
    app.panel_close();
    assert_eq!(app.ed.text(), DRAFT);
    assert!(failure("host_timeout").contains("may have been delivered") && failure("too_large").contains("too large") && failure("odd").contains("odd"));
}

#[test]
fn the_header_of_another_sessions_text_is_dimmed_with_the_name_lit_and_only_when_it_is_the_whole_record() {
    let name = |text: &str| match header_look(text) {
        Some(Look::Session(r)) => Some(text[r].to_string()),
        _ => None,
    };
    assert_eq!(name(HEADER).as_deref(), Some("api-server"));
    assert_eq!(name(">> [from session 'My Docs 한글' (sabcdef) | 09:05 | selection | 3.1 KiB, 24 lines]").as_deref(), Some("My Docs 한글"));
    assert_eq!(name(">> [from session 'x' (s012345) | 23:59 | selection | 1.0 MiB, 1 line]").as_deref(), Some("x"));
    assert_eq!(header_look(">> [from the person | 14:02 | message | 5 B]"), Some(Look::Person));
    assert_eq!(name(">> [from session 'api-server' (s012345) | 14:02 | conversation | 120.6 KiB, 2400 lines]").as_deref(), Some("api-server"), "the notice of a whole conversation has the look too");
    for not in [
        ">> [from session 'api-server' (s012345) | 14:02 | conversation | 120.6 KiB, 2400 lines] x",
        ">> [from session 'api-server' (s012345) | 14:02 | Conversation | 5 B]",
        "[agent] >> [from session 'api-server' (s012345) | 14:02 | message | 27 B]",  // how the host records an agent line that imitates it
        "[from session 'api-server' (s012345) | 14:02 | message | 27 B]",             // no `>> `: not a record of text that was sent in
        " >> [from session 'api-server' (s012345) | 14:02 | message | 27 B]",
        ">>  [from session 'api-server' (s012345) | 14:02 | message | 27 B]",
        "$ >> [from session 'api-server' (s012345) | 14:02 | message | 27 B]",
        ">> [from session 'api-server' (s012345) | 14:02 | message | 27 B] and then more",
        ">> [from session 'api-server' (s012345) | 14:02 | message | 27 B]]",
        ">> [from session '' (s012345) | 14:02 | message | 27 B]",
        ">> [from session 'a(b' (s012345) | 14:02 | message | 27 B]",
        ">> [from session 'a|b' (s012345) | 14:02 | message | 27 B]",
        ">> [from session 'api-server' (s01234) | 14:02 | message | 27 B]",
        ">> [from session 'api-server' (S012345) | 14:02 | message | 27 B]",
        ">> [from session 'api-server' (s01234g) | 14:02 | message | 27 B]",
        ">> [from session 'api-server' (s012345) | 1402 | message | 27 B]",
        ">> [from session 'api-server' (s012345) | 14:02 | order | 27 B]",
        ">> [from session 'api-server' (s012345) | 14:02 | message]",
        ">> [from session 'api-server' (s012345) | 14:02 | message | 27 B | more]",
        ">> [from session 'api-server' (s012345) | 14:02 | message | the password is hunter2]",
        ">> [from session 'api-server' (s012345) | 14:02 | message | ]",
        ">> [from the person | 14:02 | message | 5 B] x",
        ">> [from someone | 14:02 | message | 5 B]",
        ">> hello",
        "",
    ] {
        assert_eq!(header_look(not), None, "{not:?}");
    }
}

#[test]
fn the_look_changes_colours_only_and_keeps_the_font_of_every_character_of_a_name_that_needs_another_font() {
    let plain = || {
        let mut list = AttrsList::new(&Attrs::new().family(Family::Monospace));
        let hangul = HEADER.replace("api-server", "한글api");
        let start = hangul.find('한').unwrap();
        list.add_span(start..start + "한글".len(), &Attrs::new().family(Family::Name("Some Hangul Mono")));
        (hangul, list)
    };
    let (text, list) = plain();
    let tinted = tint(list.clone(), &text);
    let start = text.find('한').unwrap();
    let (first, name_end) = (text.find('[').unwrap(), start + "한글api".len());
    assert_eq!(tinted.get_span(first).color_opt, Some(DIM), "the record is dimmed");
    assert_eq!(tinted.get_span(name_end).color_opt, Some(DIM), "the quote after the name is not lit");
    assert_eq!(tinted.get_span(start - 1).color_opt, Some(DIM), "nor the quote before it");
    for at in [start, start + 3, start + 6, name_end - 1] {
        assert_eq!(tinted.get_span(at).color_opt, Some(NAME), "the name is lit at {at}");
    }
    assert_eq!(tinted.get_span(start).family, Family::Name("Some Hangul Mono"), "a character that needed another font still has it");
    assert_eq!(tinted.get_span(start + 6).family, Family::Monospace);
    assert_eq!(tinted.get_span(first).family, Family::Monospace);
    assert!(DIM != NAME && NAME != Color::rgb(0xE0, 0xE0, 0xE0), "the three are told apart");
    // anything else is left exactly as it was, and so is the person's header apart from the dimming
    for other in ["plain text", "[agent] >> [from session 'api-server' (s012345) | 14:02 | message | 27 B]", ">> a message"] {
        assert!(tint(list.clone(), other) == list, "{other}");
    }
    let person = tint(AttrsList::new(&Attrs::new().family(Family::Monospace)), ">> [from the person | 14:02 | message | 5 B]");
    assert_eq!((person.get_span(4).color_opt, person.spans().len()), (Some(DIM), 0));
}

#[test]
fn the_window_waits_for_a_record_to_be_read_longer_than_for_an_ordinary_reply() {
    assert!(RECORD_WAIT > 3 * super::super::CALL_SECONDS, "a record of up to 1 GiB is read before the host answers: the window must not give up at the time of an ordinary call");
}

/// What the hand-over panel says after each code the host (or the window, when it heard nothing) can answer: whether the panel may say that nothing was sent.
/// A new code in `failure` has to be put here, and so has to choose.
const HAND_OVER_REFUSALS: [(&str, bool); 18] = [
    ("unknown_session", true), ("session_disconnected", true), ("session_exited", true), ("busy", true), ("queue_full", true), ("chain_depth", true), ("too_large", true),
    ("no_conversation_yet", true), ("record_not_found", true), ("record_ambiguous", true), ("record_format", true), ("bad_request", true), ("send_limit", true),
    ("unknown_account", true), ("option_unavailable", true), ("folder_gone", true),
    ("host_unreachable", true), // the window found no host or could not write the request: the host had nothing to act on
    ("host_timeout", false),    // the request was written and no answer came: the host may have stored the file and told the other session
];

fn hand_over_panel_after(code: &str) -> String {
    let dir = Scratch::new("sendto");
    let mut app = two();
    app.run_cmd(Cmd::HandOver);
    let reply = match code {
        "host_unreachable" => None, // no host at all
        "host_timeout" => Some(json!({"nothing": "that answers"})), // the host reads the request and lets go of the connection without a reply
        other => Some(json!({"ok": 0, "error": other})),
    };
    let host = reply.map(|reply| {
        let (path, host) = fake_host(&dir, vec![reply]);
        app.socket = path;
        host
    });
    app.send_go();
    if let Some(host) = host {
        host.join().unwrap();
    }
    assert_eq!(step(&app), Some(Step::Send), "{code}: the panel stays");
    notice(&app)
}

#[test]
fn a_hand_over_the_host_may_have_carried_out_does_not_say_that_nothing_was_sent() {
    for (code, nothing_sent) in HAND_OVER_REFUSALS {
        let said = hand_over_panel_after(code);
        assert_eq!(said.contains("Nothing was sent."), nothing_sent, "{code}: {said}");
        assert_eq!(said.starts_with("Not handed over: "), nothing_sent, "{code}: {said}");
    }
    assert_eq!(
        hand_over_panel_after("host_timeout"),
        "The host has not said yet whether it handed the conversation over; look at the other session before you hand it over again."
    );
    // a draft is a text: it keeps the words of a text, and the draft is kept
    let dir = Scratch::new("sendto");
    let (path, host) = fake_host(&dir, vec![json!({"nothing": "that answers"})]);
    let mut app = two();
    app.socket = path;
    app.send_ask();
    app.send_go();
    host.join().unwrap();
    assert_eq!(notice(&app), "Not sent: the host did not answer in time; the text may have been delivered, so look at the other session before sending it again. The text is kept.");
}

#[test]
fn every_code_that_failure_words_is_in_the_table_of_the_hand_over() {
    let source = include_str!("sendto.rs");
    let start = source.find("pub fn failure(").unwrap();
    let words = &source[start..source[start..].find("/// The same for a whole conversation").unwrap() + start];
    let mut found = Vec::new();
    for line in words.lines().map(str::trim).filter(|l| l.starts_with('"') && l.contains(" => ")) {
        for code in line.split(" => ").next().unwrap().split(" | ") {
            found.push(code.trim_matches('"').to_string());
        }
    }
    assert!(found.len() >= 17, "the codes were not found in the source: {found:?}");
    for code in &found {
        assert!(HAND_OVER_REFUSALS.iter().any(|(c, _)| c == code), "{code} has to say in HAND_OVER_REFUSALS whether the panel may call it 'Nothing was sent.'");
    }
}

#[test]
fn a_refused_input_says_in_one_sentence_what_the_host_answered_and_always_that_the_text_is_back_in_the_box() {
    let table = [
        ("unknown_session", "does not know that session"),
        ("session_disconnected", "press Enter again to reconnect it and send"),
        ("session_exited", "has ended"),
        ("stale_generation", "the session changed meanwhile; look at it and send again"),
        ("busy", "could not take it just now"),
        ("too_large", "too large"),
        ("bad_request", "does not accept that text"),
        ("version_unsupported", "restart the window"),
        ("unknown_op", "restart the window"),
    ];
    for (code, contains) in table {
        let said = input_refusal(code);
        assert!(said.starts_with("Not sent: ") && said.ends_with(" The text is back in the box.") && said.contains(contains), "{code}: {said}");
        assert!(!said.contains(code), "{code} is said in words, not as the word: {said}");
        assert_eq!(said.matches("Not sent").count() + said.matches("back in the box").count(), 2, "{code}: one sentence: {said}");
    }
}

#[test]
fn a_code_the_table_does_not_know_is_shown_as_the_word_the_host_used() {
    assert_eq!(input_refusal("some_new_code"), "Not sent: the host answered some_new_code. The text is back in the box.");
}

#[test]
fn the_window_puts_the_sentence_of_the_table_under_the_box_and_the_text_back_when_the_host_refuses_an_input() {
    for (code, contains) in [("stale_generation", "the session changed meanwhile"), ("busy", "could not take it just now"), ("some_new_code", "some_new_code")] {
        let mut app = two();
        app.session = "session-s0".into();
        app.pending_inputs.insert("gui-1-1".into(), ("session-s0".into(), "the text".into()));
        app.ed.clear();
        assert!(app.on_input_reply(&format!("{{\"ok\":0,\"id\":\"gui-1-1\",\"error\":\"{code}\"}}")));
        assert_eq!(app.ed.text(), "the text", "{code}");
        let note = app.ed.dump("session-s0")["note"].as_str().unwrap().to_string();
        assert!(note.starts_with("Not sent: ") && note.contains(contains) && note.ends_with(" The text is back in the box."), "{code}: {note}");
    }
}
