//! Reading the whole text behind a card of the host's own, and answering such a card: which keys are offered, what the view shows, how it pages, and what the
//! window says of the answer. (The host's side is in `tests/test_tool_send.py`, the whole way through the window in `tests/test_card_window.py`; the host here
//! is a few lines that answer.)

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use winit::keyboard::ModifiersState;

use super::super::sendto::tests::fake_host;
use super::super::Step;
use super::*;
use crate::gui::scroll_oracle::app;
use crate::gui::ui::Row;
use crate::handoff::Scratch;

const CARD: &str = "send to 'docs' (s111111) | 22 B, 2 lines | \"first line\"";

/// `api` is open with a card of the host's own (7) that came up just now and has not been drawn yet.
fn fresh_card() -> App {
    let mut app = app(1.0, false);
    app.ui.rows = vec![Row { session: "session-s0".into(), name: "api".into(), provider: "claude".into(), state: "running".into(), activity: "idle".into(), generation: 1, approvals: vec![(7, CARD.into())], asks: vec![7], ..Default::default() }];
    app.ui.numbered = vec!["session-s0".into()];
    app
}

/// The first frame with the card was drawn at the returned instant, and the clock stands `ms` after it.
fn drawn_ms_ago(app: &mut App, ms: u64) -> Instant {
    let first = Instant::now();
    app.ui.clock = Some(first);
    app.card_drawn();
    app.ui.clock = Some(first + Duration::from_millis(ms));
    first
}

/// `fresh_card()` as it is a moment later, when a key means an answer: the dwell has passed (the tests of what a key does start here).
fn with_card() -> App {
    let mut app = fresh_card();
    drawn_ms_ago(&mut app, DWELL.as_millis() as u64);
    app
}

fn with_program_card() -> App {
    let mut app = fresh_card();
    app.ui.rows[0].approvals = vec![(7, "run: ls".into())];
    app.ui.rows[0].asks.clear();
    app
}

fn page(text: &str, offset: usize, next: Option<usize>, total: usize) -> Value {
    json!({"ok": 1, "session": "session-s0", "approval_id": 7, "text": text, "offset": offset, "next": next, "total": total})
}

fn answered(outcome: &str) -> Value {
    json!({"ok": 1, "session": "session-s0", "approval_id": 7, "state": "answered", "outcome": outcome})
}

fn read(offset: usize) -> Value {
    json!({"op": "approval.text", "session": "session-s0", "generation": 1, "approval_id": 7, "offset": offset})
}

fn answer(what: &str, keep: bool) -> Value {
    let mut request = json!({"op": "approval.answer", "session": "session-s0", "generation": 1, "approval_id": 7, "answer": what});
    if keep {
        request["keep"] = json!(true);
    }
    request
}

fn body(app: &App) -> Vec<String> {
    app.panel_view().unwrap().1
}

fn shown(app: &App) -> Vec<String> {
    let all = body(app);
    let first = all.iter().position(|l| l.starts_with("bytes ")).unwrap() + 2;
    all[first..].iter().take_while(|l| !l.is_empty()).cloned().collect()
}

/// Forty lines `l00` to `l39`, the first page of a text of two.
fn forty() -> String {
    (0..40).map(|i| format!("l{i:02}")).collect::<Vec<_>>().join("\n")
}

#[test]
fn a_card_of_the_host_adds_two_keys_to_the_hint_and_a_card_of_the_program_does_not() {
    let app = with_card();
    assert!(app.hint_text(2000, 1.0).starts_with("Y allow  N deny  Shift+S allow for the session  V read it all   "), "{}", app.hint_text(2000, 1.0));
    let mut program = with_program_card();
    let hint = program.hint_text(2000, 1.0);
    assert!(hint.starts_with("Y allow  N deny   ") && !hint.contains("V read"), "{hint}");
    assert!(!program.answer_key("s") && !program.answer_key("v") && program.ui.panel.is_none(), "the program's question has neither a text of ours nor an allowance for the session");
    let mut none = with_card();
    none.ui.rows[0].approvals.clear();
    assert!(!none.answer_key("y") && !none.answer_key("v") && !none.answer_key("s"), "no card, no keys");
    assert!(!with_card().answer_key("x"), "any other key is typing");
}

#[test]
fn v_opens_the_text_with_what_the_card_said_and_where_the_page_stands_and_esc_gives_the_box_back() {
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("first line\nsecond line", 0, None, 22)]);
    let mut app = with_card();
    app.socket = path;
    assert!(app.answer_key("v"));
    assert_eq!(host.join().unwrap(), [read(0)]);
    assert_eq!(app.ui.panel.as_ref().map(|p| p.step), Some(Step::Card));
    let (title, lines) = app.panel_view().unwrap();
    assert_eq!(title, "Request 7");
    assert_eq!(lines[0], CARD);
    assert_eq!(lines[1], "bytes 0 to 22 of 22 (page 1 of 1)");
    assert_eq!(shown(&app), ["first line", "second line"]);
    assert!(lines.iter().any(|l| l.starts_with("Y allow  N deny  Shift+S allow for the session")) && lines.last().map(String::as_str) == Some("Esc  back out"), "{lines:?}");
    app.panel_close();
    assert!(app.ui.panel.is_none());
}

#[test]
fn the_text_of_an_agent_is_shown_not_obeyed_and_wrapped_to_the_view_without_losing_a_character() {
    let hostile = format!("tab\there\u{1b}[31m red\u{202e}x\n{}\n{}\n\n  indented", "x".repeat(200), "\u{d55c}".repeat(60));
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page(&hostile, 0, None, hostile.len())]);
    let mut app = with_card();
    app.socket = path;
    app.answer_key("v");
    host.join().unwrap();
    app.card_move(Move::End);
    let lines = app.ui.panel.as_ref().unwrap().card.as_ref().unwrap().lines.clone();
    assert!(lines.iter().all(|l| !l.chars().any(|c| c.is_control() || c == '\u{202e}')), "{lines:?}");
    assert!(lines.iter().all(|l| l.chars().map(|c| if crate::gui::editor::wide(c) { 2 } else { 1 }).sum::<usize>() <= WIDE), "{lines:?}");
    assert_eq!(lines[0], "tab    here[31m redx", "a tab is spaces; the escape is dropped, so the rest of it is plain text");
    assert_eq!(lines.iter().map(|l| l.matches('x').count()).sum::<usize>(), 201);
    assert_eq!(lines.iter().map(|l| l.matches('\u{d55c}').count()).sum::<usize>(), 60);
    assert!(lines.contains(&String::new()) && lines.last().map(String::as_str) == Some("  indented"), "blank lines and indentation stay: {lines:?}");
}

#[test]
fn the_view_moves_by_line_and_page_and_reads_the_next_page_when_this_one_is_passed_and_the_earlier_one_when_it_is_left() {
    let dir = Scratch::new("cardview");
    let (first, second) = (forty(), (0..10).map(|i| format!("m{i:02}")).collect::<Vec<_>>().join("\n"));
    let n0 = first.len() + 1; // where the second page starts
    let total = n0 + second.len();
    let (head, tail) = (format!("{first}\n"), second.as_str());
    let replies = vec![page(&head, 0, Some(n0), total), page(tail, n0, None, total), page(&head, 0, Some(n0), total), page(tail, n0, None, total), page(&head, 0, Some(n0), total)];
    let (path, host) = fake_host(&dir, replies);
    let mut app = with_card();
    app.socket = path;
    app.answer_key("v");
    assert_eq!((shown(&app).first().cloned(), shown(&app).len()), (Some("l00".into()), SHOWN));
    assert_eq!(body(&app)[1], format!("bytes 0 to {n0} of {total} (page 1 of 2)"), "a page that is followed by another is not the last");
    app.card_move(Move::Line(1));
    assert_eq!(shown(&app).first().map(String::as_str), Some("l01"));
    app.card_move(Move::Line(-1));
    app.card_move(Move::Line(-1));
    assert_eq!(shown(&app).first().map(String::as_str), Some("l00"), "the top of the first page is the top");
    app.card_move(Move::Page(1));
    assert_eq!(shown(&app).first().map(String::as_str), Some("l14"));
    app.card_move(Move::Page(1));
    assert_eq!(shown(&app).first().map(String::as_str), Some("l26"), "the last full view of the page, not past it");
    app.card_move(Move::Line(1)); // past the end of this page: the next one is read
    assert_eq!((shown(&app).first().cloned(), shown(&app).len()), (Some("m00".into()), 10));
    assert!(body(&app)[1].starts_with(&format!("bytes {n0} to {total} of {total}")) && body(&app)[1].ends_with("(page 2 of 2)"), "{}", body(&app)[1]);
    app.card_move(Move::Line(-1)); // and before the start of this one, the earlier one is read again, at its end
    assert_eq!(shown(&app).first().map(String::as_str), Some("l26"));
    app.card_move(Move::End);
    assert_eq!(shown(&app).first().map(String::as_str), Some("m00"));
    app.card_move(Move::Home);
    assert_eq!(shown(&app).first().map(String::as_str), Some("l00"));
    assert_eq!(host.join().unwrap(), [read(0), read(n0), read(0), read(n0), read(0)]);
}

#[test]
fn y_n_and_s_in_the_view_answer_with_the_text_that_was_read_and_say_what_came_of_it() {
    let cases: [(char, &str, bool, Value, &str); 7] = [
        ('Y', "allow", false, answered("done"), "Allowed: the text was sent."),
        ('N', "deny", false, answered("declined"), "Declined: nothing was sent."),
        ('y', "allow", false, answered("done"), "Allowed: the text was sent."),
        ('n', "deny", false, answered("declined"), "Declined: nothing was sent."),
        ('S', "allow", true, answered("done"), "Allowed for the session: the text was sent, and later sends to that session are not asked."),
        ('y', "allow", false, json!({"ok": 1, "session": "session-s0", "approval_id": 7, "state": "answered", "outcome": "not_done", "reason": "session_exited"}), "Allowed, but not sent: that session has ended."),
        ('y', "allow", false, json!({"ok": 1, "session": "session-s0", "approval_id": 7, "state": "answered", "outcome": "not_done", "reason": "chain_depth"}), "Allowed, but not sent: the chain of agents is too long; a person has to start it again."),
    ];
    for (key, what, keep, reply, said) in cases {
        let dir = Scratch::new("cardview");
        let (path, host) = fake_host(&dir, vec![page("first line\nsecond line", 0, None, 22), reply]);
        let mut app = with_card();
        app.socket = path;
        app.answer_key("v");
        if key == 'S' {
            app.ui.mods = ModifiersState::SHIFT;
        }
        app.card_key(key);
        assert_eq!(host.join().unwrap(), [read(0), answer(what, keep)], "{key}");
        assert!(app.ui.panel.is_none() && app.ui.rows[0].approvals.is_empty() && app.ui.flash.as_deref() == Some(said), "{key}: {:?}", app.ui.flash);
    }
}

#[test]
fn a_card_answered_from_the_list_without_the_view_is_answered_the_same_way_with_the_same_words() {
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![answered("done")]);
    let mut app = with_card();
    app.socket = path;
    app.ui.mods = ModifiersState::SHIFT;
    assert!(app.answer_key("S"));
    assert_eq!(host.join().unwrap(), [answer("allow", true)]);
    assert!(app.ui.rows[0].approvals.is_empty() && app.ui.flash.as_deref().is_some_and(|f| f.starts_with("Allowed for the session: ")), "{:?}", app.ui.flash);
    // the menu entries and the old key path reach the same function
    let (path, host) = fake_host(&dir, vec![answered("declined")]);
    let mut app = with_card();
    app.socket = path;
    app.answer_approval(false);
    assert_eq!(host.join().unwrap(), [answer("deny", false)]);
    assert_eq!(app.ui.flash.as_deref(), Some("Declined: nothing was sent."));
}

#[test]
fn a_request_that_is_gone_is_said_to_be_gone_and_one_that_waits_for_its_session_stays() {
    let gone = [("unknown_approval", "Request 7 is not waiting any more."), ("expired", "Request 7 ran out of time: nothing was sent.")];
    for (error, said) in gone {
        let dir = Scratch::new("cardview");
        let (path, host) = fake_host(&dir, vec![json!({"ok": 0, "error": error})]);
        let mut app = with_card();
        app.socket = path;
        assert!(app.answer_key("v"));
        host.join().unwrap();
        assert!(app.ui.panel.is_none() && app.ui.rows[0].approvals.is_empty() && app.ui.flash.as_deref() == Some(said), "{error}: {:?}", app.ui.flash);
    }
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![json!({"ok": 0, "error": "session_disconnected"})]);
    let mut app = with_card();
    app.socket = path;
    assert!(app.answer_key("y"));
    host.join().unwrap();
    assert_eq!(app.ui.rows[0].approvals.len(), 1, "it was not answered, so it is still asked");
    assert_eq!(app.ui.flash.as_deref(), Some("Not answered: that session is closed (Ctrl+R in it brings it back); the request waits."));
    let mut app = with_card(); // no host at all
    assert!(app.answer_key("y"));
    assert!(app.ui.rows[0].approvals.len() == 1 && app.ui.flash.as_deref() == Some("Not answered: the host did not answer; the request waits."), "{:?}", app.ui.flash);
}

#[test]
fn a_request_that_was_answered_elsewhere_while_it_is_read_is_not_answered_again_and_the_next_one_is_not_answered_in_its_place() {
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("first line\nsecond line", 0, None, 22)]);
    let mut app = with_card();
    app.socket = path;
    app.answer_key("v");
    app.ui.rows[0].approvals = vec![(8, "send to 'docs' (s111111) | 5 B | \"other\"".into())]; // a status came: 7 was answered on the command line, 8 is next
    app.ui.rows[0].asks = vec![8];
    app.card_key('y');
    // no second connection: a host that was asked for 7 or 8 here would have been asked for a text that was not read
    assert_eq!(host.join().unwrap(), [read(0)]);
    assert!(app.ui.panel.is_none() && app.ui.flash.as_deref() == Some("Request 7 is not waiting any more."), "{:?}", app.ui.flash);
    assert_eq!(app.ui.rows[0].approvals.iter().map(|a| a.0).collect::<Vec<_>>(), [8], "the other request is still asked, and was not touched");
}

#[test]
fn the_letter_in_the_view_answers_the_request_that_was_read_even_when_another_one_comes_first() {
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("first line\nsecond line", 0, None, 22), answered("done")]);
    let mut app = with_card();
    app.socket = path;
    app.answer_key("v");
    app.ui.rows[0].approvals.insert(0, (3, "run: ls".into())); // the program's own question is first in the list now
    app.card_key('y');
    let asked = host.join().unwrap();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert_eq!(asked[1], answer("allow", false), "it is request 7 that was read, and the one that is first is not touched");
    assert_eq!(app.ui.rows[0].approvals.iter().map(|a| a.0).collect::<Vec<_>>(), [3]);
}

#[test]
fn end_reads_on_through_every_page_to_the_last_and_home_and_the_line_above_the_top_come_back_through_them() {
    let dir = Scratch::new("cardview");
    let lines = |p: &str, n: usize| (0..n).map(|i| format!("{p}{i:02}")).collect::<Vec<_>>().join("\n");
    let (a, b, c) = (format!("{}\n", lines("a", 20)), format!("{}\n", lines("b", 20)), lines("c", 10));
    let replies = vec![page(&a, 0, Some(100), 300), page(&b, 100, Some(200), 300), page(&c, 200, None, 300), page(&b, 100, Some(200), 300), page(&a, 0, Some(100), 300)];
    let (path, host) = fake_host(&dir, replies);
    let mut app = with_card();
    app.socket = path;
    app.answer_key("v");
    app.card_move(Move::End);
    assert_eq!((shown(&app).first().cloned(), body(&app)[1].ends_with("(page 3 of 3)")), (Some("c00".into()), true), "{:?}", body(&app)[1]);
    app.card_move(Move::Line(-1)); // above the top of the last page: the one before it, at its end
    assert_eq!(shown(&app).first().map(String::as_str), Some("b06"));
    assert!(body(&app)[1].starts_with("bytes 100 to ") && body(&app)[1].contains("(page 2 of "), "{}", body(&app)[1]);
    app.card_move(Move::Home);
    assert_eq!((shown(&app).first().map(String::as_str), body(&app)[1].contains("(page 1 of ")), (Some("a00"), true));
    assert_eq!(host.join().unwrap(), [read(0), read(100), read(200), read(100), read(0)]);
}

/// The window drops some of what it is given on its own (`wrapped`), so a text that was not cleaned first would be read as one thing and sent as another:
/// the escape of a colour code goes and its `[31m` stays. What the host has cleaned is shown whole.
#[test]
fn a_text_the_host_has_cleaned_is_wrapped_by_the_window_without_losing_or_adding_a_character() {
    let letters = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let raw = "\u{1b}[31mred\u{1b}[0m \u{202e}evil\u{200b}\r\nsecond\u{e0041} line";
    assert!(letters(&wrapped(raw).concat()).contains("[31m"), "the window alone shows the rest of a sequence as text");
    let long = format!("{}\u{1b}[1m{}\t{}", "x".repeat(200), "\u{d55c}".repeat(120), "y".repeat(97));
    for text in [raw, "\u{2764}\u{fe0f}\u{fe0f}\u{fe0f} a\u{fe0f} \u{1f468}\u{200d}\u{1f469}", "a\u{85}b\u{2028}c\u{2029}d\u{200e}e\u{200f}f\u{61c}g", "\u{1b}]0;title\u{7}tab\there", long.as_str(), ""] {
        let clean = crate::leads::agent_text(text).text;
        assert!(clean.chars().all(|c| !hidden(c) && (!c.is_control() || c == '\n' || c == '\t')), "the window has nothing of its own to drop: {text:?}");
        assert_eq!(letters(&wrapped(&clean).concat()), letters(&clean), "{text:?}");
    }
}

const MAKE: &str = "create session 'reviewer' in '/work/api' | mode: asks for approval | model: haiku, effort: low | starts at once on the account's allowance | 22 B, 2 lines | 3 hidden characters removed | \"first line\"";

/// `api` is open with a request of its agent for a new session (7).
fn with_make_card() -> App {
    let mut app = with_card();
    app.ui.rows[0].approvals = vec![(7, MAKE.into())];
    app.ui.rows[0].makes = vec![7];
    app
}

#[test]
fn a_request_for_a_session_has_no_yes_for_the_session_in_the_hint_the_view_or_the_keys_and_the_view_shows_what_the_card_says_whole() {
    let mut app = with_make_card();
    app.ui.mods = ModifiersState::SHIFT; // so that it is the card, not the missing Shift, that refuses
    let hint = app.hint_text(2000, 1.0);
    assert!(hint.starts_with("Y allow  N deny  V read it all   ") && !hint.contains("for the session"), "{hint}");
    assert!(!app.answer_key("s") && !app.answer_key("S") && app.ui.flash.is_none() && app.ui.panel.is_none(), "S is typing here, not an answer");
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("first line\nsecond line", 0, None, 22)]);
    app.socket = path;
    assert!(app.answer_key("v"));
    let lines = body(&app);
    assert_eq!(lines[0], MAKE, "the view starts with the card, with what the cleaning took out and that the session starts at once");
    assert!(lines.contains(&"Y allow  N deny   Up Down PgUp PgDn Home End  move".to_string()), "{lines:?}");
    app.card_key('s');
    app.card_key('S');
    assert_eq!(host.join().unwrap(), [read(0)], "nothing was asked of the host for S");
    assert!(app.ui.panel.as_ref().is_some_and(|p| p.notice.is_empty()) && app.ui.rows[0].approvals.len() == 1, "the request still waits, and a call that could not be made would have said so");
}

#[test]
fn the_window_says_that_a_session_was_made_or_was_not_and_never_that_text_was_sent() {
    let not_done = json!({"ok": 1, "session": "session-s0", "approval_id": 7, "state": "answered", "outcome": "not_done", "reason": "send_limit"});
    let cases = [
        ("y", "allow", answered("done"), "Allowed: the session was made and is working on its first prompt."),
        ("n", "deny", answered("declined"), "Declined: no session was made."),
        ("y", "allow", not_done, "Allowed, but no session was made: too many sessions that agents made are running; one has to be closed first."),
        ("y", "allow", json!({"ok": 0, "error": "expired"}), "Request 7 ran out of time: nothing was made."),
    ];
    for (key, what, reply, said) in cases {
        let dir = Scratch::new("cardview");
        let (path, host) = fake_host(&dir, vec![reply]);
        let mut app = with_make_card();
        app.socket = path;
        assert!(app.answer_key(key));
        assert_eq!(host.join().unwrap(), [answer(what, false)], "{key}");
        assert!(app.ui.rows[0].approvals.is_empty() && app.ui.rows[0].makes.is_empty() && app.ui.flash.as_deref() == Some(said), "{key}: {:?}", app.ui.flash);
    }
}

#[test]
fn the_row_knows_which_of_its_requests_ask_for_a_session() {
    let entry = json!({"session": "s1", "provider": "claude", "approvals": [{"approval_id": 3, "prompt": "run: ls"}, {"approval_id": 4, "prompt": "send", "tool": "send"}, {"approval_id": 5, "prompt": "create", "tool": "create"}]});
    let row = Row::of_status(&entry).unwrap();
    assert_eq!((row.asks, row.makes), (vec![4, 5], vec![5]));
}

#[test]
fn a_yes_for_the_session_is_never_sent_with_a_request_for_a_session_whoever_asks_for_it() {
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![answered("done")]);
    let mut app = with_make_card();
    app.socket = path;
    app.answer_id(7, true, true);
    assert_eq!(host.join().unwrap(), [answer("allow", false)], "the host refuses `keep` for a session to be made; the person would be told it was not answered");
}

const READ: &str = "read the history of 'docs' (s111111) | last 100 records of the host's record | it may hold what that session saw or was typed";

/// `api` is open with a request of its agent to read the history of another session (7).
fn with_read_card() -> App {
    let mut app = with_card();
    app.ui.rows[0].approvals = vec![(7, READ.into())];
    app.ui.rows[0].reads = vec![7];
    app
}

#[test]
fn the_row_knows_which_of_its_requests_read_a_history() {
    let entry = json!({"session": "s1", "provider": "claude", "approvals": [{"approval_id": 3, "prompt": "run: ls"}, {"approval_id": 4, "prompt": "send", "tool": "send"},
                                                                         {"approval_id": 5, "prompt": "create", "tool": "create"}, {"approval_id": 6, "prompt": "history", "tool": "history"}]});
    let row = Row::of_status(&entry).unwrap();
    assert_eq!((row.asks, row.makes, row.reads), (vec![4, 5, 6], vec![5], vec![6]));
}

#[test]
fn a_request_to_read_a_history_can_be_allowed_for_the_session_and_the_window_says_that_records_were_handed_over_and_never_that_text_was_sent() {
    let mut app = with_read_card();
    let hint = app.hint_text(2000, 1.0);
    assert!(hint.starts_with("Y allow  N deny  Shift+S allow for the session  V read it all   "), "{hint}");
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("last 100 records", 0, None, 16)]);
    app.socket = path;
    assert!(app.answer_key("v"));
    assert_eq!(body(&app)[0], READ, "the view starts with the card");
    assert!(body(&app).contains(&"Y allow  N deny  Shift+S allow for the session   Up Down PgUp PgDn Home End  move".to_string()), "{:?}", body(&app));
    assert_eq!(host.join().unwrap(), [read(0)]);
    let not_done = json!({"ok": 1, "session": "session-s0", "approval_id": 7, "state": "answered", "outcome": "not_done", "reason": "unknown_session"});
    let cases = [
        ("y", false, answered("done"), "Allowed: the records were handed over."),
        ("S", true, answered("done"), "Allowed for the session: the records were handed over, and later reads of that history are not asked."),
        ("n", false, answered("declined"), "Declined: nothing was read."),
        ("y", false, not_done.clone(), "Allowed, but nothing was read: the host does not know that session any more."),
        ("S", true, not_done, "Allowed for the session, but nothing was read: the host does not know that session any more."),
        ("y", false, json!({"ok": 0, "error": "expired"}), "Request 7 ran out of time: nothing was read."),
    ];
    for (key, keep, reply, said) in cases {
        let (path, host) = fake_host(&dir, vec![reply]);
        let mut app = with_read_card();
        app.socket = path;
        app.ui.mods = ModifiersState::SHIFT;
        assert!(app.answer_key(key));
        let what = if key == "n" { "deny" } else { "allow" };
        assert_eq!(host.join().unwrap(), [answer(what, keep)], "{key}");
        assert!(app.ui.rows[0].approvals.is_empty() && app.ui.rows[0].reads.is_empty() && app.ui.flash.as_deref() == Some(said), "{key}: {:?}", app.ui.flash);
    }
}

#[test]
fn shift_s_in_the_view_of_a_request_to_read_a_history_allows_it_for_the_session() {
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("last 100 records", 0, None, 16), answered("done")]);
    let mut app = with_read_card();
    app.socket = path;
    app.answer_key("v");
    app.ui.mods = ModifiersState::SHIFT;
    app.card_key('S');
    assert_eq!(host.join().unwrap(), [read(0), answer("allow", true)]);
    assert!(app.ui.panel.is_none() && app.ui.rows[0].reads.is_empty() && app.ui.flash.as_deref().is_some_and(|f| f.starts_with("Allowed for the session: the records were handed over")), "{:?}", app.ui.flash);
}

/// Every bare key that answers or opens a card of the host's own, with what the host is asked for it and says back.
fn bare_keys() -> Vec<(&'static str, Value, Value)> {
    vec![
        ("y", answer("allow", false), answered("done")),
        ("Y", answer("allow", false), answered("done")),
        ("n", answer("deny", false), answered("declined")),
        ("N", answer("deny", false), answered("declined")),
        ("S", answer("allow", true), answered("done")),
        ("v", read(0), page("first line", 0, None, 10)),
        ("V", read(0), page("first line", 0, None, 10)),
    ]
}

#[test]
fn a_bare_key_answers_a_card_of_the_host_only_after_the_card_has_been_on_screen_for_the_dwell() {
    for ms in [0, 1, 400, 799] {
        for (key, _, _) in bare_keys() {
            let mut app = fresh_card(); // no host at all: a key that was taken would say that the host did not answer
            drawn_ms_ago(&mut app, ms);
            assert!(!app.card_ready(), "{ms} ms");
            assert!(!app.answer_key(key), "{key} at {ms} ms is not an answer: it goes to the input box");
            assert!(app.ui.rows[0].approvals.len() == 1 && app.ui.flash.is_none() && app.ui.panel.is_none(), "{key} at {ms} ms changed something: {:?}", app.ui.flash);
        }
    }
    for (key, asked, reply) in bare_keys() {
        let dir = Scratch::new("cardview");
        let (path, host) = fake_host(&dir, vec![reply]);
        let mut app = fresh_card();
        app.socket = path;
        if key == "S" {
            app.ui.mods = ModifiersState::SHIFT; // a capital S is the answer only with Shift held (see the test of Caps Lock)
        }
        drawn_ms_ago(&mut app, 800);
        assert!(app.card_ready() && app.answer_key(key), "{key} at 800 ms");
        assert_eq!(host.join().unwrap(), [asked], "{key}");
    }
}

#[test]
fn the_dwell_counts_from_the_first_frame_that_drew_the_card_and_from_no_other_event() {
    let mut app = fresh_card(); // the status that brought the card came long ago, but no frame has drawn it
    app.ui.clock = Some(Instant::now() + Duration::from_secs(3600));
    assert!(!app.card_ready() && !app.answer_key("y") && app.ui.rows[0].approvals.len() == 1, "a card that was not drawn is not read");
    let first = drawn_ms_ago(&mut app, 500);
    app.ui.clock = Some(first + Duration::from_millis(600));
    app.card_drawn(); // a later frame with the same card does not start the count again
    app.ui.clock = Some(first + Duration::from_millis(800));
    assert!(app.card_ready());
    app.ui.rows[0].approvals.clear(); // a frame without the card forgets it
    app.card_drawn();
    assert!(app.ui.card_seen.is_none() && !app.card_ready());
}

#[test]
fn another_card_and_another_session_start_the_dwell_again() {
    let mut app = with_card();
    assert!(app.card_ready());
    app.ui.rows[0].approvals = vec![(8, CARD.into())]; // the next request of the same session
    app.ui.rows[0].asks = vec![8];
    assert!(!app.card_ready(), "the card that was seen was another one");
    let first = Instant::now();
    app.ui.clock = Some(first);
    app.card_drawn();
    app.ui.clock = Some(first + Duration::from_millis(799));
    assert!(!app.card_ready() && !app.answer_key("y"));
    app.ui.clock = Some(first + Duration::from_millis(800));
    assert!(app.card_ready());
    // the person goes to another session with a card of its own and comes back: each time the card is new to the window
    let mut other = app.ui.rows[0].clone();
    (other.session, other.name) = ("session-s1".into(), "docs".into());
    app.ui.rows.push(other);
    app.session = "session-s1".into();
    assert!(!app.card_ready(), "the card that was seen was in another session, whatever its number");
    app.card_forget();
    assert!(!app.card_ready(), "selected just now");
    app.card_drawn();
    app.ui.clock = Some(first + Duration::from_millis(1599));
    assert!(!app.card_ready());
    app.ui.clock = Some(first + Duration::from_millis(1600));
    assert!(app.card_ready());
    app.session = "session-s0".into();
    app.card_forget();
    app.card_drawn();
    assert!(!app.card_ready() && !app.answer_key("y"), "back in the first session the count starts at the first frame again");
}

#[test]
fn only_a_capital_s_allows_for_the_session_and_a_small_one_is_typing_in_the_list_and_in_the_view() {
    for small in ["s", "\u{17f}"] {
        let mut app = with_card();
        assert!(!app.answer_key(small) && app.ui.flash.is_none() && app.ui.rows[0].approvals.len() == 1, "{small:?} is typing");
    }
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![page("first line", 0, None, 10), answered("done")]);
    let mut app = with_card();
    app.socket = path;
    assert!(app.answer_key("V"));
    app.card_key('s');
    assert!(app.ui.panel.is_some() && app.ui.rows[0].approvals.len() == 1, "a small s in the view keeps nothing");
    app.ui.mods = ModifiersState::SHIFT;
    app.card_key('S');
    assert_eq!(host.join().unwrap(), [read(0), answer("allow", true)]);
    assert!(app.ui.rows[0].approvals.is_empty());
}

#[test]
fn a_capital_s_that_comes_without_shift_is_typing_and_shift_s_answers_in_the_list_and_in_the_view() {
    // with Caps Lock on every letter arrives as a capital and Shift is not held: a person writing a sentence in capitals must not answer with the first S
    let held = [
        (ModifiersState::empty(), "S", false),
        (ModifiersState::SHIFT, "S", true),
        (ModifiersState::SHIFT, "s", false),
        (ModifiersState::empty(), "s", false),
        (ModifiersState::empty(), "Y", true), // Y and N stay as they were: a capital Y is an answer whatever is held
        (ModifiersState::empty(), "N", true),
    ];
    // no host answers: a key that was taken as an answer says so ("Not answered ..."), and one that was typing says nothing
    for (mods, key, answers) in held {
        let mut app = with_card();
        app.ui.mods = mods;
        assert_eq!(app.answer_key(key), answers, "{key} with {mods:?} in the list");
        assert_eq!(app.ui.flash.as_deref().is_some_and(|f| f.starts_with("Not answered")), answers, "{key} with {mods:?} in the list: {:?}", app.ui.flash);
        // and in the view, where the card is read first (the host answers that one read and then is gone)
        let dir = Scratch::new("cardview");
        let (path, host) = fake_host(&dir, vec![page("first line", 0, None, 10)]);
        let mut app = with_card();
        app.socket = path;
        assert!(app.answer_key("V"));
        host.join().unwrap();
        app.ui.mods = mods;
        app.card_key(key.chars().next().unwrap());
        let said = app.ui.panel.as_ref().map(|p| p.notice.clone()).unwrap_or_default();
        assert_eq!(said.starts_with("Not answered"), answers, "{key} with {mods:?} in the view: {said:?}");
    }
}

#[test]
fn the_question_of_the_program_keeps_its_keys_and_is_answered_at_once() {
    for key in ["y", "Y", "n", "N"] {
        let mut app = with_program_card(); // never drawn: there is no dwell for a question that is not ours
        assert!(app.answer_key(key) && app.ui.rows[0].approvals.is_empty(), "{key}");
    }
    let mut app = with_program_card();
    assert!(!app.answer_key("S") && !app.answer_key("V") && app.ui.rows[0].approvals.len() == 1, "the program's question has neither");
    assert!(!app.card_ready(), "the dwell is for the host's own cards");
}

#[test]
fn an_answer_the_host_did_not_confirm_is_not_called_unanswered_and_the_list_is_asked_for_at_once() {
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    const WORDS: &str = "The host has not said yet whether it took your answer; the list shows it when it has.";
    // the host takes the connection, reads the answer and says nothing that is a reply: the window cannot know whether the answer was taken
    let silent = || json!({"nothing": "that answers"});
    let dir = Scratch::new("cardview");
    let (path, host) = fake_host(&dir, vec![silent()]);
    let mut app = with_card();
    app.socket = path;
    let (sock, mut peer) = UnixStream::pair().unwrap();
    app.sock = sock;
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert!(app.answer_key("y"));
    assert_eq!(host.join().unwrap(), [answer("allow", false)]);
    assert_eq!(app.ui.flash.as_deref(), Some(WORDS), "it does not say that the request waits, nor that the answer failed");
    assert_eq!(app.ui.rows[0].approvals.len(), 1, "the row keeps the request until the list says otherwise");
    let mut asked = [0u8; 64];
    let n = peer.read(&mut asked).unwrap();
    assert_eq!(&asked[..n], b"{\"op\":\"status\"}\n", "the status is asked for at once, so the list shows what the host did");
    // the host had taken the answer: a second Y gets the words of a request that is gone
    let (path, host) = fake_host(&dir, vec![json!({"ok": 0, "error": "unknown_approval"})]);
    app.socket = path;
    assert!(app.answer_key("y"));
    assert_eq!(host.join().unwrap(), [answer("allow", false)]);
    assert!(app.ui.rows[0].approvals.is_empty() && app.ui.flash.as_deref() == Some("Request 7 is not waiting any more."), "{:?}", app.ui.flash);
    // from the view the words are in the panel, which stays open
    let (path, host) = fake_host(&dir, vec![page("first line", 0, None, 10), silent()]);
    let mut app = with_card();
    app.socket = path;
    let (sock, mut peer) = UnixStream::pair().unwrap();
    app.sock = sock;
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert!(app.answer_key("V"));
    app.card_key('n');
    assert_eq!(host.join().unwrap(), [read(0), answer("deny", false)]);
    assert!(app.ui.panel.as_ref().is_some_and(|p| p.notice == WORDS) && app.ui.rows[0].approvals.len() == 1, "{:?}", app.ui.panel.as_ref().map(|p| &p.notice));
    let n = peer.read(&mut asked).unwrap();
    assert_eq!(&asked[..n], b"{\"op\":\"status\"}\n");
}
