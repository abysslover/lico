//! The calls of an agent that act, on the tool connection (S03b): `send` now; `create` and `history` join with S04. The caller is the session of the token and
//! nobody else (no argument names it). The limits of AC-04 are kept here, in memory and nowhere else: calls per minute of one session, and (in `transfer`) the depth.

use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::core::{self, Shared};
use crate::transfer::{self, By, MAX_HOPS};

/// Calls per minute of one session. Every `send` counts, a refused one too; `sessions` does not (`create` and `history` count from S04).
pub const PER_MINUTE: usize = 20;
const MINUTE: Duration = Duration::from_secs(60);
const KEPT: usize = 2 * PER_MINUTE; // a session that goes on asking does not make its list grow

/// When each session called the host in the last minute: a sliding window.
#[derive(Default)]
struct Window {
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

fn window() -> &'static Mutex<Window> {
    static WINDOW: OnceLock<Mutex<Window>> = OnceLock::new();
    WINDOW.get_or_init(Mutex::default)
}

static OPEN: AtomicBool = AtomicBool::new(false);

/// Until the person can be asked (S04), `send` refuses every call. This lets a host that is started for a test send without that question.
pub fn open_without_approval() {
    OPEN.store(true, Ordering::Relaxed);
}

/// The tool `send`: the text of `arguments` to the session `to`, from the session that holds the token.
pub fn send(shared: &Shared, caller: &str, arguments: &Map<String, Value>) -> Result<String, String> {
    send_with(shared, caller, arguments, OPEN.load(Ordering::Relaxed), window(), Instant::now())
}

fn send_with(shared: &Shared, caller: &str, arguments: &Map<String, Value>, open: bool, window: &Mutex<Window>, now: Instant) -> Result<String, String> {
    if let Err(wait) = window.lock().unwrap_or_else(|e| e.into_inner()).take(caller, now) {
        return Err(format!("send_limit: at most {PER_MINUTE} calls a minute from one session; wait {wait} seconds and try again"));
    }
    let to = arguments.get("to").and_then(Value::as_str).filter(|to| !to.is_empty() && to.len() <= 64);
    let text = arguments.get("text").and_then(Value::as_str).filter(|text| !text.is_empty());
    let (Some(to), Some(text)) = (to, text) else { return Err(refusal(shared, caller, "bad_request")) };
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
    if !open {
        return Err("not_allowed: this host cannot ask the person to approve a send yet, so it sends nothing for an agent".into());
    }
    match transfer::send(shared, Some(caller), to, "text", text, By::Session) {
        Ok(done) => Ok(json!({"sent": true, "to": to, "delivery": done["delivery"], "size": done["size"], "receiver": done["receiver"]}).to_string()),
        Err(code) => Err(refusal(shared, caller, code)),
    }
}

/// What the model reads for a refusal: the name of the error, then what to do about it.
fn refusal(shared: &Shared, caller: &str, code: &str) -> String {
    let what = match code {
        "unknown_session" => "there is no session with that id; ask the sessions tool for the ids".to_string(),
        "session_disconnected" => "that session has no running program; the person can resume it".into(),
        "session_exited" => "the program of that session has ended".into(),
        "busy" => "the host could not take the text just now; try again in a moment".into(),
        "too_large" => "the text is too large; split it, or write it to a file and ask the person to hand it over".into(),
        "bad_request" => "send needs `to` (a session id from the sessions tool) and `text` (not empty)".into(),
        "chain_depth" => {
            let hops = core::find(shared, caller).map_or(0, |found| found.lock().unwrap().incoming_hops) + 1;
            format!("this would be step {hops} in a row of sessions that pass text on without a person, and the limit is {MAX_HOPS}; stop here and tell the person")
        }
        _ => "the host refused it".into(),
    };
    format!("{code}: {what}")
}

#[cfg(test)]
mod tests {
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

    fn ask(to: &str, text: &str) -> Map<String, Value> {
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

    fn call(w: &World, from: &str, args: &Map<String, Value>) -> Result<String, String> {
        send_with(&w.shared, from, args, true, &Mutex::default(), Instant::now())
    }

    #[test]
    fn a_send_goes_to_the_other_session_as_the_caller_and_the_reply_is_small() {
        let w = pair();
        let got = call(&w, ONE, &ask(TWO, "hello")).unwrap();
        let reply: Value = serde_json::from_str(&got).unwrap();
        assert_eq!(reply, json!({"sent": true, "to": TWO, "delivery": "inline", "size": 5, "receiver": "idle"}));
        let delivered = w.delivered(TWO).unwrap();
        let (head, body) = delivered.split_once('\n').unwrap();
        assert!(head.starts_with("[from session 'writer' (s012345) | ") && head.ends_with(" | message | 5 B]"), "{head}");
        assert_eq!(body, "hello");
        assert_eq!(w.with(TWO, |s| s.incoming_hops), 1, "an agent's text is one step on a chain");
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
    fn until_the_person_can_be_asked_nothing_is_sent_and_the_call_still_counts() {
        let w = pair();
        let window = Mutex::default();
        let got = send_with(&w.shared, ONE, &ask(TWO, "hello"), false, &window, Instant::now());
        let text = got.unwrap_err();
        assert!(text.starts_with("not_allowed: ") && text.contains("approve"), "{text}");
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
            let text = send_with(&w.shared, ONE, case.as_object().unwrap(), true, &window, Instant::now()).unwrap_err();
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
        let text = call(&w, TWO, &ask(ONE, "fifth")).unwrap_err();
        assert!(text.starts_with("chain_depth: ") && text.contains("5") && text.contains("4") && text.contains("person"), "{text}");
        assert!(w.delivered(ONE).is_none(), "nothing reached it");
        w.with(TWO, |s| s.incoming_hops = 0); // a person wrote to it
        assert!(call(&w, TWO, &ask(ONE, "answer")).is_ok());
    }

    #[test]
    fn the_twenty_first_call_in_a_minute_is_refused_with_the_seconds_to_wait_whatever_it_was() {
        let w = pair();
        let (window, now) = (Mutex::default(), Instant::now());
        for k in 0..PER_MINUTE {
            let args = if k % 4 == 0 { ask(TWO, "ok") } else { Map::new() }; // refused calls count the same
            let _ = send_with(&w.shared, ONE, &args, true, &window, now);
        }
        let text = send_with(&w.shared, ONE, &ask(TWO, "one too many"), true, &window, now).unwrap_err();
        assert!(text.starts_with("send_limit: ") && text.contains("20") && text.contains("wait 60 seconds"), "{text}");
        let text = send_with(&w.shared, ONE, &Map::new(), true, &window, now).unwrap_err();
        assert!(text.starts_with("send_limit: "), "a call that would have been refused for another reason is refused for this one: {text}");
        let mut seen = 0;
        while w.delivered(TWO).is_some() {
            seen += 1;
        }
        assert_eq!(seen, 5, "only the five that were good arrived");
        assert!(send_with(&w.shared, TWO, &ask(ONE, "mine"), true, &window, now).is_ok(), "the other session's window is its own");
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
