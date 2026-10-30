//! The calls of an agent that act, on the tool connection: `send` (S03b, with the person's approval from S04); `create` (S04c) is in `create` and `history` (S07) in `history`. The caller is the
//! session of the token and nobody else (no argument names it). The limits of AC-04 are kept here, in memory and nowhere else: calls per minute of one session, and (in
//! `transfer`) the depth. What the agent asks for does not happen at the call: the call leaves a card on the agent's row (`pending`) and happens when the person says yes.

use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::codex::sanitize;
use crate::core::{self, Shared};
use crate::leads;
use crate::pending::{self, Asked, Request};
use crate::transfer::{self, By, MAX_HOPS};

/// Calls per minute of one session. Every `send`, `create`, `history`, `capture`, `clipboard_read` and `clipboard_write` counts, a refused one too; `sessions` and `read_handoff` do not.
pub const PER_MINUTE: usize = 20;
const MINUTE: Duration = Duration::from_secs(60);
const KEPT: usize = 2 * PER_MINUTE; // a session that goes on asking does not make its list grow
pub(crate) const CARD_CHARS: usize = 200; // what the window shows of a card (`Session::add_card` cuts at the same number)
const LABEL_CHARS: usize = 60;

/// When each session called the host in the last minute: a sliding window.
#[derive(Default)]
pub(crate) struct Window {
    calls: HashMap<String, VecDeque<Instant>>,
}

impl Window {
    /// Count a call of `session` made at `now`. Err: the seconds to wait before a call would be let through (the refused call counts, so asking again sooner only lengthens it).
    fn take(&mut self, session: &str, now: Instant) -> Result<(), u64> {
        let gone = |at: &Instant| now.saturating_duration_since(*at) >= MINUTE;
        self.calls.retain(|_, at| at.back().is_some_and(|last| !gone(last)));
        let at = self.calls.entry(session.to_string()).or_default();
        while at.front().is_some_and(gone) {
            at.pop_front();
        }
        let refused = at.len() >= PER_MINUTE;
        if at.len() < KEPT {
            at.push_back(now);
        }
        if !refused {
            return Ok(());
        }
        // the next call is let through when enough of the earlier ones (this refused one among them) have left for fewer than PER_MINUTE to be in the minute
        let leaves = at[at.len() - PER_MINUTE] + MINUTE;
        Err(leaves.saturating_duration_since(now).as_millis().div_ceil(1000).max(1) as u64)
    }
}

pub(crate) fn window() -> &'static Mutex<Window> {
    static WINDOW: OnceLock<Mutex<Window>> = OnceLock::new();
    WINDOW.get_or_init(Mutex::default)
}

/// The tool `send`: the text of `arguments` to the session `to`, from the session that holds the token. The text is not sent at the call, unless the person has
/// allowed this session to send to that one for as long as it lives (`keep`): the call asks the person and says so.
pub fn send(shared: &Shared, caller: &str, arguments: &Map<String, Value>) -> Result<String, String> {
    pending::sweep(shared, caller);
    send_with(shared, caller, arguments, window(), Instant::now(), pending::held_text())
}

/// Count a call of `caller` in the minute's window. Err: the refusal the model reads. `send`, `create` and `history` are counted together, one window for a session.
pub(crate) fn counted(window: &Mutex<Window>, caller: &str, now: Instant) -> Result<(), String> {
    window.lock().unwrap_or_else(|e| e.into_inner()).take(caller, now).map_err(|wait| format!("send_limit: at most {PER_MINUTE} calls a minute from one session; wait {wait} seconds and try again"))
}

fn send_with(shared: &Shared, caller: &str, arguments: &Map<String, Value>, window: &Mutex<Window>, now: Instant, held: &Arc<AtomicUsize>) -> Result<String, String> {
    counted(window, caller, now)?;
    let to = arguments.get("to").and_then(Value::as_str).filter(|to| !to.is_empty() && to.len() <= 64);
    let text = arguments.get("text").and_then(Value::as_str).filter(|text| !text.is_empty());
    let (Some(to), Some(text)) = (to, text) else { return Err(refusal(shared, caller, "bad_request")) };
    // the text is cleaned here, once: the card, the pages of it, the window, the command, the record and the receiver all get this string
    let cleaned = leads::agent_text(text);
    let (text, removed) = (cleaned.text.as_str(), cleaned.removed);
    if text.is_empty() || crate::folders::shows_nothing(text) {
        return Err(refusal(shared, caller, "bad_request"));
    }
    if to == caller {
        return Err("bad_request: a session cannot send to itself".into());
    }
    // the tool connection is for Claude sessions (SF-05): an agent does not send to any other kind (a person's own send from the window can)
    if core::find(shared, to).is_some_and(|found| {
        let s = found.lock().unwrap();
        !s.purged && s.provider != "claude"
    }) {
        return Err("not_allowed: agents send to Claude Code sessions only; that session is not one".into());
    }
    // what is already known to be refused is refused now, so that the person is not asked for a send that could not happen
    let label = receiver_label(shared, caller, to).map_err(|code| refusal(shared, caller, code))?;
    if pending::kept(shared, caller, "send", to) {
        return match transfer::send(shared, Some(caller), to, "text", text, By::Session) {
            Ok(done) => Ok(json!({"sent": true, "to": to, "delivery": done["delivery"], "size": done["size"], "receiver": done["receiver"]}).to_string()),
            Err(code) => Err(refusal(shared, caller, code)),
        };
    }
    let (card, who) = (send_card(&label, to, text, removed), format!("session '{label}' ({})", transfer::id7(to)));
    let asked = pending::ask(shared, caller, &Request { tool: "send", target: to, who: &who, card: &card, text }, pending::now(), held)?;
    let (id, again) = match asked {
        Asked::New(id) => (id, false),
        Asked::Again(id) => (id, true),
    };
    let mut reply = json!({
        "status": "pending", "approval_id": id,
        "message": format!("Nothing was sent yet. The person has been asked to approve this send to {who}. Lico will tell you here when they decide; you can end your turn. A restart of Lico drops the request."),
    });
    if again {
        reply["already_pending"] = json!(true);
    }
    Ok(reply.to_string())
}

/// Whether `to` can take a send from `caller` at all, and what it is called: the answers `transfer::send` would give at the question, so that they come at the call.
fn receiver_label(shared: &Shared, caller: &str, to: &str) -> Result<String, &'static str> {
    if core::find(shared, caller).map_or(0, |found| found.lock().unwrap().incoming_hops) + 1 > MAX_HOPS {
        return Err("chain_depth");
    }
    let found = core::find(shared, to).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    match s.state {
        _ if s.purged => return Err("unknown_session"),
        crate::session::State::Disconnected => return Err("session_disconnected"),
        crate::session::State::Exited => return Err("session_exited"),
        crate::session::State::Running => {}
    }
    Ok(transfer::label(&s.id, s.settings.name.as_deref(), s.settings.cwd.as_deref()).chars().take(LABEL_CHARS).collect())
}

/// The line on the card of a send: to whom, how much, and the start of the text, in one line of at most `CARD_CHARS`.
fn send_card(label: &str, to: &str, text: &str, removed: usize) -> String {
    card_line(&format!("send to '{label}' ({})", transfer::id7(to)), text, removed)
}

/// What a card says after its head: how much text there is (the cleaned text, which is what is sent), what the cleaning took out, and the opening quote of the start of it.
pub(crate) fn card_tail(text: &str, removed: usize) -> String {
    let lines = transfer::line_count(text);
    let size = if lines > 1 { format!("{}, {}", transfer::size_text(text.len()), transfer::lines_word(lines)) } else { transfer::size_text(text.len()) };
    let removed = match removed {
        0 => String::new(),
        1 => " | 1 hidden character removed".to_string(),
        n => format!(" | {n} hidden characters removed"),
    };
    format!(" | {size}{removed} | \"")
}

/// A card's line: `head`, the tail (`card_tail`) and the start of the text, in one line of at most `CARD_CHARS`.
pub(crate) fn card_line(head: &str, text: &str, removed: usize) -> String {
    let head = format!("{head}{}", card_tail(text, removed));
    let first = text.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let first = sanitize(first).replace('"', "'");
    let room = CARD_CHARS.saturating_sub(head.chars().count() + 2);
    let cut = first.chars().count() > room;
    let shown: String = first.chars().take(room).collect();
    format!("{head}{shown}{}\"", if cut { "\u{2026}" } else { "" })
}

/// What the person's yes does for a request of `caller`. Err: the refusal the host gives now, in the words the model reads.
pub(crate) fn perform(shared: &Arc<Shared>, caller: &str, id: i64, tool: &str, target: &str, who: &str, text: &str) -> Result<String, String> {
    match tool {
        "create" => crate::create::perform(shared, caller, target, text),
        "history" => crate::history::perform(shared, caller, target, text),
        "capture" | "clipboard_read" | "clipboard_write" => crate::screentools::perform(shared, caller, id, tool, target, who, text),
        "send" => match transfer::send(shared, Some(caller), target, "text", text, By::Session) {
            Ok(done) => Ok(format!("{} delivered to {who} ({}); the receiver was {}", transfer::size_text(text.len()), done["delivery"].as_str().unwrap_or("inline"), done["receiver"].as_str().unwrap_or("idle"))),
            Err(code) => Err(refusal(shared, caller, code)),
        },
        _ => Err(refusal(shared, caller, "refused")),
    }
}

/// What the model reads for a refusal: the name of the error, then what to do about it.
pub(crate) fn refusal(shared: &Shared, caller: &str, code: &str) -> String {
    let what = match code {
        "unknown_session" => "there is no session with that id; ask the sessions tool for the ids".to_string(),
        "session_disconnected" => "that session has no running program; the person can resume it".into(),
        "session_exited" => "the program of that session has ended".into(),
        "busy" => "the host could not take the text just now; try again in a moment".into(),
        "too_large" => "the text is too large; split it, or write it to a file and ask the person to hand it over".into(),
        "bad_request" => "send needs `to` (a session id from the sessions tool) and `text` (not empty, and not only characters that draw nothing)".into(),
        "chain_depth" => {
            let hops = core::find(shared, caller).map_or(0, |found| found.lock().unwrap().incoming_hops) + 1;
            format!("this would be step {hops} in a row of sessions that pass text on without a person, and the limit is {MAX_HOPS}; stop here and tell the person")
        }
        _ => "the host refused it".into(),
    };
    format!("{code}: {what}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::session::State;
    use crate::transfer::tests::{world, World, ONE, TWO};
    use serde_json::json;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    fn held(w: &Window, session: &str) -> usize {
        w.calls.get(session).map_or(0, VecDeque::len)
    }

    /// 20 calls spread over the first 20 s, then the 21st at t=20 s: refused, and it counts.
    fn spread(base: Instant) -> Window {
        let mut w = Window::default();
        for k in 0..20 {
            assert_eq!(w.take("a", at(base, k * 1000)), Ok(()), "call {k}");
        }
        assert!(w.take("a", at(base, 20_000)).is_err());
        w
    }

    #[test]
    fn twenty_calls_in_a_minute_are_let_through_and_the_next_ones_are_refused_with_the_seconds_to_wait_and_a_refused_call_counts() {
        let (mut w, base) = (Window::default(), Instant::now());
        for k in 0..PER_MINUTE {
            assert_eq!(w.take("a", base), Ok(()), "call {k}");
        }
        assert_eq!(w.take("a", base), Err(60), "the 21st");
        assert_eq!(w.take("a", base), Err(60), "and the 22nd: the refused ones are counted, not forgiven");
        assert_eq!(w.take("a", at(base, 59_000)), Err(1), "one second before the end: every call so far is from the first instant");
        let mut fresh = Window::default();
        for _ in 0..PER_MINUTE {
            fresh.take("a", base).unwrap();
        }
        assert_eq!(fresh.take("a", at(base, 59_900)), Err(1), "a tenth of a second short of the end: wait one second, never zero");
        assert_eq!(fresh.take("b", base), Ok(()), "another session has a window of its own");
    }

    #[test]
    fn the_window_slides_a_call_leaves_it_a_minute_after_it_was_made() {
        let base = Instant::now();
        let mut w = spread(base);
        assert_eq!(w.take("a", at(base, 21_000)), Err(41), "the refused calls count: the third call of the first minute has to leave, at 62 s");
        let mut w = spread(base);
        assert!(w.take("a", at(base, 60_000)).is_err(), "at 60 s only the first call has left: 19 of the minute and the refused one make 20");
        let mut w = spread(base);
        assert_eq!(w.take("a", at(base, 61_000)), Ok(()), "at 61 s the first two have left: 18 and the refused one, and this is the 20th");
    }

    #[test]
    fn a_session_that_goes_on_asking_does_not_make_its_list_grow_and_an_idle_one_is_forgotten() {
        let (mut w, base) = (Window::default(), Instant::now());
        for _ in 0..10_000 {
            let _ = w.take("a", base);
        }
        assert!(held(&w, "a") <= 2 * PER_MINUTE, "{}", held(&w, "a"));
        assert!(w.take("a", at(base, 1000)).is_err());
        assert_eq!(w.take("b", at(base, 61_000)), Ok(()));
        assert_eq!(held(&w, "a"), 0, "a minute without a call and the session is not in the table");
        assert_eq!(held(&w, "b"), 1);
    }

    pub(crate) fn ask(to: &str, text: &str) -> Map<String, Value> {
        json!({"to": to, "text": text}).as_object().unwrap().clone()
    }

    /// Two Claude sessions: ONE is called `writer`.
    fn pair() -> World {
        let w = world(&[ONE, TWO]);
        for id in [ONE, TWO] {
            w.with(id, |s| s.provider = "claude");
        }
        w.with(ONE, |s| s.settings.name = Some("writer".into()));
        w
    }

    /// A call at a time and with a window of the caller's choosing, and a count of the held bytes of its own.
    pub(crate) fn call_in(w: &World, from: &str, args: &Map<String, Value>, window: &Mutex<Window>, now: Instant, held: &Arc<AtomicUsize>) -> Result<String, String> {
        send_with(&w.shared, from, args, window, now, held)
    }

    pub(crate) fn call_counted(w: &World, from: &str, args: &Map<String, Value>, held: &Arc<AtomicUsize>) -> Result<String, String> {
        call_in(w, from, args, &Mutex::default(), Instant::now(), held)
    }

    /// A call, and the person's yes to the card it leaves when it has to wait for one: the reply is that of the call when nothing waited, else that of the answer.
    fn call(w: &World, from: &str, args: &Map<String, Value>) -> Result<String, String> {
        let got = call_counted(w, from, args, &Arc::default())?;
        let reply: Value = serde_json::from_str(&got).unwrap();
        if reply["status"] != "pending" {
            return Ok(got);
        }
        let generation = w.with(from, |s| s.generation as i64);
        let done = core::approval_answer(&w.shared, from, generation, reply["approval_id"].as_i64().unwrap(), "allow", false).unwrap();
        Ok(Value::Object(done).to_string())
    }

    #[test]
    fn a_send_goes_to_the_other_session_as_the_caller_when_the_person_says_yes_and_the_replies_are_small() {
        let w = pair();
        let got = call_counted(&w, ONE, &ask(TWO, "hello"), &Arc::default()).unwrap();
        let reply: Value = serde_json::from_str(&got).unwrap();
        let keys: Vec<&str> = reply.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!((reply["status"].as_str(), keys), (Some("pending"), vec!["approval_id", "message", "status"]));
        assert!(reply["message"].as_str().unwrap().len() < 300 && w.delivered(TWO).is_none());
        let generation = w.with(ONE, |s| s.generation as i64);
        core::approval_answer(&w.shared, ONE, generation, 1, "allow", true).unwrap();
        let delivered = w.delivered(TWO).unwrap();
        let (head, body) = delivered.split_once('\n').unwrap();
        assert!(head.starts_with("[from session 'writer' (s012345) | ") && head.ends_with(" | message | 5 B]"), "{head}");
        assert_eq!(body, "hello");
        assert_eq!(w.with(TWO, |s| s.incoming_hops), 1, "an agent's text is one step on a chain");
        let again = call_counted(&w, ONE, &ask(TWO, "hello"), &Arc::default()).unwrap();
        let reply: Value = serde_json::from_str(&again).unwrap();
        assert_eq!(reply, json!({"sent": true, "to": TWO, "delivery": "inline", "size": 5, "receiver": "idle"}), "once allowed for the session the reply is the plain one");
    }

    /// The cards on the row of `id`, as the window gets them.
    fn cards_of(w: &World, id: &str) -> Vec<String> {
        let all = core::status(&w.shared).unwrap();
        let row = all["sessions"].as_array().unwrap().iter().find(|s| s["session"] == id).unwrap().clone();
        row["approvals"].as_array().map_or_else(Vec::new, |list| list.iter().map(|a| a["prompt"].as_str().unwrap().to_string()).collect())
    }

    fn card_of(w: &World, id: &str) -> String {
        cards_of(w, id).remove(0)
    }

    const DIRTY: &str = "\u{1b}[31mred\u{1b}[0m\u{202e}evil\u{200b}\r\nsecond\u{e0041} line"; // 9 characters of colour, 3 that draw nothing, and a CR LF
    const CLEAN: &str = "redevil\nsecond line";

    #[test]
    fn an_agents_text_is_cleaned_once_at_the_call_and_the_card_the_pages_and_the_receiver_all_get_the_same_string() {
        let w = pair();
        let got: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, DIRTY), &Arc::default()).unwrap()).unwrap();
        let card = card_of(&w, ONE);
        assert!(card.contains(" | 19 B, 2 lines | 12 hidden characters removed | \"redevil"), "{card}");
        assert!(!card.chars().any(|c| c.is_control() || c == '\u{202e}'), "{card:?}");
        let generation = w.with(ONE, |s| s.generation as i64);
        let page = pending::text_page(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), 0, pending::PAGE).unwrap();
        assert_eq!((page["text"].as_str(), page["total"].as_u64()), (Some(CLEAN), Some(19)), "the person reads what the receiver will get");
        core::approval_answer(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), "allow", false).unwrap();
        let delivered = w.delivered(TWO).unwrap();
        let (head, body) = delivered.split_once('\n').unwrap();
        assert!(head.ends_with(" | message | 19 B]"), "{head}");
        assert_eq!(body, "redevil\nsecond line");
    }

    #[test]
    fn what_is_allowed_for_the_session_is_cleaned_the_same_way_and_the_size_is_that_of_the_clean_text() {
        let w = pair();
        let id = call_counted(&w, ONE, &ask(TWO, "first"), &Arc::default()).unwrap();
        let generation = w.with(ONE, |s| s.generation as i64);
        core::approval_answer(&w.shared, ONE, generation, serde_json::from_str::<Value>(&id).unwrap()["approval_id"].as_i64().unwrap(), "allow", true).unwrap();
        assert!(w.delivered(TWO).is_some());
        let reply: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, DIRTY), &Arc::default()).unwrap()).unwrap();
        assert_eq!((reply["sent"].as_bool(), reply["size"].as_u64()), (Some(true), Some(19)), "{reply}");
        assert_eq!(w.delivered(TWO).unwrap().split_once('\n').unwrap().1, CLEAN);
        let long = format!("{}{}", "a".repeat(4000), "\u{1b}[31m".repeat(40)); // 4200 bytes as written, 4000 as sent: it goes inline
        let reply: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, &long), &Arc::default()).unwrap()).unwrap();
        assert_eq!((reply["delivery"].as_str(), reply["size"].as_u64()), (Some("inline"), Some(4000)), "{reply}");
        assert!(w.files(TWO).is_empty());
    }

    #[test]
    fn the_card_says_how_many_characters_went_in_one_form_for_one_and_none_for_a_text_that_was_clean() {
        let w = pair();
        for (text, note) in [("clean", None), ("one\u{200b}", Some(" | 1 hidden character removed | ")), ("two\u{200b}\u{1b}[0m", Some(" | 5 hidden characters removed | "))] {
            let got: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, text), &Arc::default()).unwrap()).unwrap();
            let card = card_of(&w, ONE);
            assert_eq!(card.contains("hidden character"), note.is_some(), "{card}");
            if let Some(note) = note {
                assert!(card.contains(note), "{card}");
            }
            let generation = w.with(ONE, |s| s.generation as i64);
            core::approval_answer(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), "deny", false).unwrap();
        }
    }

    #[test]
    fn a_text_of_nothing_that_draws_is_refused_as_an_empty_one_and_counts_as_a_call() {
        let w = pair();
        let window = Mutex::default();
        // empty once cleaned, and what is left of it shows nothing: a blank braille cell, a mark with nothing to sit on, a space, a line break; the last two
        // show something until they are cleaned (the colour code, the title and the invisible space are taken out first), so the check is of the cleaned text
        let texts = ["\u{200b}", "\u{1b}[0m\u{e0041}\u{202e}", "\u{1b}]0;title\u{7}", "\u{2800}", "\u{0301}", " ", "\n", "\u{1b}[31m\u{2800}", "\u{1b}]0;t\u{7}\u{200b}\u{2800}"];
        for text in texts {
            let text = call_in(&w, ONE, &ask(TWO, text), &window, Instant::now(), &Arc::default()).unwrap_err();
            assert!(text.starts_with("bad_request: ") && text.contains("draw nothing"), "{text}");
        }
        assert_eq!(held(&window.lock().unwrap(), ONE), texts.len());
        assert!(cards_of(&w, ONE).is_empty() && w.delivered(TWO).is_none());
    }

    #[test]
    fn a_word_and_a_heart_with_its_selector_show_something_and_get_a_card() {
        let w = pair();
        for (text, shown) in [("word", "word"), ("\u{2764}\u{fe0f}", "\u{2764}")] {
            let got: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, text), &Arc::default()).unwrap()).unwrap();
            assert!(card_of(&w, ONE).contains(shown), "{text:?}");
            let generation = w.with(ONE, |s| s.generation as i64);
            core::approval_answer(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), "deny", false).unwrap();
        }
    }

    #[test]
    fn what_the_caller_says_about_itself_is_ignored_and_the_text_stays_a_message() {
        let w = pair();
        let mut forged = ask(TWO, "hi");
        forged.insert("from".into(), json!(TWO));
        forged.insert("kind".into(), json!("lines"));
        forged.insert("hops".into(), json!(0));
        forged.insert("by".into(), json!("person"));
        call(&w, ONE, &forged).unwrap();
        let delivered = w.delivered(TWO).unwrap();
        assert!(delivered.starts_with("[from session 'writer' (s012345) | ") && delivered.contains(" | message | "), "{delivered}");
        assert_eq!(w.with(TWO, |s| s.incoming_hops), 1);
    }

    #[test]
    fn a_send_that_waits_for_the_person_sends_nothing_and_counts_as_a_call() {
        let w = pair();
        let window = Mutex::default();
        let got = call_in(&w, ONE, &ask(TWO, "hello"), &window, Instant::now(), &Arc::default()).unwrap();
        assert!(got.contains("\"pending\""), "{got}");
        assert!(w.delivered(TWO).is_none() && w.files(TWO).is_empty());
        assert_eq!(held(&window.lock().unwrap(), ONE), 1);
    }

    #[test]
    fn a_call_without_a_receiver_or_a_text_or_with_the_wrong_kind_of_value_is_refused_and_counted() {
        let w = pair();
        let window = Mutex::default();
        let long = "s".repeat(65);
        let cases: Vec<Value> = vec![json!({}), json!({"to": TWO}), json!({"text": "x"}), json!({"to": TWO, "text": ""}), json!({"to": "", "text": "x"}), json!({"to": 5, "text": "x"}), json!({"to": TWO, "text": 5}),
                                     json!({"to": TWO, "text": ["x"]}), json!({"to": long, "text": "x"}), json!({"to": ONE, "text": "to myself"})];
        for (k, case) in cases.iter().enumerate() {
            let text = call_in(&w, ONE, case.as_object().unwrap(), &window, Instant::now(), &Arc::default()).unwrap_err();
            assert!(text.starts_with("bad_request: "), "{case}: {text}");
            assert_eq!(held(&window.lock().unwrap(), ONE), k + 1, "{case}");
        }
        assert!(w.delivered(TWO).is_none() && w.delivered(ONE).is_none());
    }

    #[test]
    fn a_receiver_that_cannot_take_it_is_refused_in_words_that_say_what_to_do() {
        let w = pair();
        let gone = "s0000000000000000";
        let refused = |to: &str| call(&w, ONE, &ask(to, "x")).unwrap_err();
        let text = refused(gone);
        assert!(text.starts_with("unknown_session: ") && text.contains("sessions"), "{text}");
        w.with(TWO, |s| s.state = State::Disconnected);
        let text = refused(TWO);
        assert!(text.starts_with("session_disconnected: ") && text.contains("resume"), "{text}");
        w.with(TWO, |s| s.state = State::Exited);
        assert!(refused(TWO).starts_with("session_exited: "));
        w.with(TWO, |s| s.state = State::Running);
        w.with(TWO, |s| s.purged = true);
        assert!(refused(TWO).starts_with("unknown_session: "));
        assert!(w.delivered(TWO).is_none());
    }

    #[test]
    fn the_tool_connection_is_for_claude_sessions_so_an_agent_sends_to_no_other_kind() {
        let w = pair();
        for kind in ["codex", ""] {
            w.with(TWO, |s| s.provider = kind);
            let text = call(&w, ONE, &ask(TWO, "x")).unwrap_err();
            assert!(text.starts_with("not_allowed: ") && text.contains("Claude"), "{kind:?}: {text}");
            assert!(w.delivered(TWO).is_none());
        }
        w.with(TWO, |s| s.provider = "claude");
        assert!(call(&w, ONE, &ask(TWO, "x")).is_ok());
    }

    #[test]
    fn a_chain_of_agents_stops_at_the_fifth_step_and_the_words_name_both_numbers() {
        let w = pair();
        w.with(ONE, |s| s.incoming_hops = 3);
        assert!(call(&w, ONE, &ask(TWO, "fourth")).is_ok());
        assert_eq!(w.with(TWO, |s| s.incoming_hops), 4);
        assert!(w.delivered(ONE).unwrap().contains("approved and done"), "the notice of the fourth, to the one who asked");
        let text = call(&w, TWO, &ask(ONE, "fifth")).unwrap_err();
        assert!(text.starts_with("chain_depth: ") && text.contains("5") && text.contains("4") && text.contains("person"), "{text}");
        assert!(w.delivered(ONE).is_none() && w.with(ONE, |s| s.approvals.is_empty()), "nothing reached it and no question was asked");
        w.with(TWO, |s| s.incoming_hops = 0); // a person wrote to it
        assert!(call(&w, TWO, &ask(ONE, "answer")).is_ok());
    }

    #[test]
    fn the_twenty_first_call_in_a_minute_is_refused_with_the_seconds_to_wait_whatever_it_was() {
        let w = pair();
        let (window, now) = (Mutex::default(), Instant::now());
        for k in 0..PER_MINUTE {
            let args = if k % 4 == 0 { ask(TWO, &format!("ok {k}")) } else { Map::new() }; // refused calls count the same
            let _ = call_in(&w, ONE, &args, &window, now, &Arc::default());
        }
        let text = call_in(&w, ONE, &ask(TWO, "one too many"), &window, now, &Arc::default()).unwrap_err();
        assert!(text.starts_with("send_limit: ") && text.contains("20") && text.contains("wait 60 seconds"), "{text}");
        let text = call_in(&w, ONE, &Map::new(), &window, now, &Arc::default()).unwrap_err();
        assert!(text.starts_with("send_limit: "), "a call that would have been refused for another reason is refused for this one: {text}");
        assert_eq!(w.with(ONE, |s| s.approvals.len()), 5, "only the five that were good were put to the person");
        assert!(w.delivered(TWO).is_none());
        assert!(call_in(&w, TWO, &ask(ONE, "mine"), &window, now, &Arc::default()).is_ok(), "the other session's window is its own");
    }

    #[test]
    fn every_refusal_starts_with_its_name_and_one_that_is_not_known_still_says_so() {
        let w = pair();
        for code in ["unknown_session", "session_disconnected", "session_exited", "busy", "too_large", "bad_request", "queue_full"] {
            let text = refusal(&w.shared, ONE, code);
            assert!(text.starts_with(&format!("{code}: ")) && text.len() > code.len() + 10, "{text}");
        }
    }
}
