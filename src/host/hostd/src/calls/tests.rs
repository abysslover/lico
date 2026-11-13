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
fn agents_send_to_claude_and_codex_provider_sessions_and_refuse_non_provider_ones() {
    let w = pair();
    w.with(TWO, |s| s.provider = "");
    let text = call(&w, ONE, &ask(TWO, "x")).unwrap_err();
    assert!(text.starts_with("not_allowed: ") && text.contains("provider sessions only"), "{text}");
    assert!(w.delivered(TWO).is_none());
    for kind in ["codex", "claude"] {
        w.with(TWO, |s| s.provider = kind);
        assert!(call(&w, ONE, &ask(TWO, "x")).is_ok(), "{kind}");
    }
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

#[test]
fn empty_created_session_arms_peer_turn_and_relays_reply_back_to_caller_while_create_prompt_does_not() {
    let w = pair();
    w.with(ONE, |s| {
        s.text_mode = true;
        s.settings.name = Some("Claude-Left".into());
    });
    w.with(TWO, |s| {
        s.text_mode = true;
        s.made_by = Some(ONE.to_string());
        s.finished_seq = -1; // empty ready tab created without an initial prompt
        s.settings.name = Some("Claude-Right - made by Claude-Left".into());
    });

    // 1. `with_create_prompt` (founding prompt of `create`) does NOT arm `peer_turn` or open a Call Room.
    with_create_prompt(|| transfer::send(&w.shared, Some(ONE), TWO, "text", "initial prompt", By::Session)).unwrap();
    assert!(w.delivered(TWO).is_some());
    assert!(w.with(TWO, |s| s.peer_turn.active.is_none()), "founding create prompt does not arm peer_turn");
    assert_eq!(crate::call_room::op_list(&w.shared, false).unwrap()["calls"].as_array().unwrap().len(), 0);

    // 2. Sending via `mcp__lico__send` to an empty created session (`made_by.is_some()` & `finished_seq < 0`)
    //    DOES arm `peer_turn`, record `[peer turn started]`, and relay `받았음` back to `Claude-Left`!
    let got: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, "hi. 수신 확인용 인사입니다."), &Arc::default()).unwrap()).unwrap();
    let generation = w.with(ONE, |s| s.generation as i64);
    core::approval_answer(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), "allow", true).unwrap();
    let _approval_notice = w.delivered(ONE).unwrap();
    assert!(w.delivered(TWO).is_some());

    let active = w.with(TWO, |s| s.peer_turn.active.clone());
    assert!(active.as_ref().is_some_and(|(caller, cid, r, _)| caller == ONE && cid.is_some() && *r == 1), "{active:?}");
    let has_peer_marker = w.with(TWO, |s| {
        s.snapshot()["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[1].as_str() == Some("[peer turn started]"))
    });
    assert!(has_peer_marker, "peer turn marker recorded on receiver");

    // Simulate `Claude-Right` writing `받았음` and finishing its turn.
    let done = w.with(TWO, |s| {
        s.peer_turn.push_line("받았음");
        s.peer_turn.finish(TWO, transfer::session_label(s))
    });
    deliver_peer_reply(&w.shared, done);

    let relayed = w.delivered(ONE).expect("peer reply must arrive in Claude-Left");
    assert!(relayed.starts_with("[peer reply from 'Claude-Right - made by Claude-Left' ("), "{relayed}");
    assert!(relayed.ends_with("\n받았음"), "{relayed}");
    let calls = crate::call_room::op_list(&w.shared, false).unwrap();
    let cid = calls["calls"][0]["call_id"].as_str().unwrap().to_string();
    let detail = crate::call_room::op_get(&w.shared, &cid).unwrap();
    assert_eq!(detail["call"]["entries"].as_array().unwrap().len(), 2);
    assert_eq!(detail["call"]["entries"][1]["text"], "받았음");
}

#[test]
fn explicit_peer_send_while_caller_is_busy_defers_transcript_card_below_active_turn_and_uses_peer_reply_header() {
    let w = pair();
    w.with(ONE, |s| {
        s.text_mode = true;
        s.settings.name = Some("Claude-Left".into());
    });
    w.with(TWO, |s| {
        s.text_mode = true;
        s.made_by = Some(ONE.to_string());
        s.finished_seq = -1;
        s.settings.name = Some("Claude-Right - made by Claude-Left".into());
    });

    // `Claude-Left` sends `hi` to `Claude-Right`; owner approves (`approval 1`), starting Turn L1 on `Claude-Left`.
    let got: Value = serde_json::from_str(&call_counted(&w, ONE, &ask(TWO, "hi"), &Arc::default()).unwrap()).unwrap();
    let generation = w.with(ONE, |s| s.generation as i64);
    core::approval_answer(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), "allow", true).unwrap();
    let notice = w.delivered(ONE).unwrap();
    assert!(notice.contains("its `[peer reply from ...]` will arrive in a separate turn once it finishes"), "{notice}");
    assert!(w.delivered(TWO).is_some());

    // Simulate `Claude-Left` still thinking in Turn L1 (`s.busy = true`, `s.peer_turn.active = true` on `TWO`).
    w.with(ONE, |s| s.set_busy(true));
    w.with(TWO, |s| s.set_busy(true));

    // `Claude-Right` answers via explicit `mcp__lico__send` ("받았음 [call done]") while `Claude-Left` is busy!
    let reply_call = call_counted(&w, TWO, &ask(ONE, "받았음 [call done]"), &Arc::default()).unwrap();
    let reply_json: Value = serde_json::from_str(&reply_call).unwrap();
    assert_eq!(reply_json["sent"], json!(true), "{reply_json}");

    // Nothing delivered or echoed into `Claude-Left`'s transcript yet while Turn L1 is busy!
    assert!(w.delivered(ONE).is_none(), "peer reply must wait for Turn L1 to finish");
    let before_finish_lines = w.with(ONE, |s| s.snapshot()["lines"].as_array().unwrap().clone());
    assert!(
        !before_finish_lines.iter().any(|r| r[1].as_str().unwrap_or("").contains("받았음")),
        "peer reply must not record above Turn L1's output: {before_finish_lines:?}"
    );

    // Turn L1 emits its output and finishes (`set_busy(false)` + `flush_deferred`).
    w.with(ONE, |s| {
        s.append("Claude-Right에 인사가 전달되었습니다.".as_bytes().to_vec());
        s.append(b"[turn completed]".to_vec());
        s.set_busy(false);
        flush_deferred(s);
    });

    // Now the deferred `[peer reply from ...]` arrives cleanly BELOW Turn L1's output!
    let flushed = w.delivered(ONE).expect("deferred peer reply flushed after Turn L1");
    assert!(flushed.starts_with("[peer reply from 'Claude-Right - made by Claude-Left' ("), "{flushed}");
    assert!(flushed.contains(" | call c") && flushed.ends_with("]\n받았음"), "{flushed}");

    let after_lines: Vec<String> = w.with(ONE, |s| {
        s.snapshot()["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r[1].as_str().unwrap().to_string())
            .collect()
    });
    let turn1_pos = after_lines.iter().position(|l| l == "Claude-Right에 인사가 전달되었습니다.").unwrap();
    let reply_pos = after_lines.iter().position(|l| l == ">> 받았음").unwrap();
    assert!(turn1_pos < reply_pos, "Turn L1 must appear ABOVE `>> 받았음`: {after_lines:?}");
}
