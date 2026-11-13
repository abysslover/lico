use super::*;
use crate::session::Session;
use crate::transfer::tests::{world, World, ONE, THREE, TWO};
use std::sync::mpsc::{sync_channel, Receiver};
use std::time::Duration;

const NEW: &str = "s4000000000000000";
const OTHER_NEW: &str = "s4000000000000001";
const EXTRA: [&str; 8] = [
    "s3000000000000001",
    "s3000000000000002",
    "s3000000000000003",
    "s3000000000000004",
    "s3000000000000005",
    "s3000000000000006",
    "s3000000000000007",
    "s3000000000000008",
];
const FOLDER: &str = "/work/api";
const DEFAULT_MODEL: &str = "model: the program's default";

/// Claude sessions: `writer` (ONE, in a folder), `reader` and `third` (scratch), and eight more that the caps count.
fn room() -> World {
    let ids: Vec<&str> = [ONE, TWO, THREE].into_iter().chain(EXTRA).collect();
    let w = world(&ids);
    for id in &ids {
        w.with(id, |s| s.provider = "claude");
    }
    w.with(ONE, |s| {
        s.settings.name = Some("writer".into());
        s.settings.cwd = Some(FOLDER.into());
    });
    w.with(TWO, |s| s.settings.name = Some("reader".into()));
    w
}

fn arguments(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn make_in(w: &World, from: &str, value: Value, window: &Mutex<Window>, held: &Arc<AtomicUsize>) -> Result<String, String> {
    call(&w.shared, from, &arguments(value), window, Instant::now(), held)
}

fn make(w: &World, from: &str, value: Value) -> Result<String, String> {
    make_in(w, from, value, &Mutex::default(), &Arc::default())
}

/// The reply of a call that waits for the person.
fn waits(w: &World, from: &str, value: Value) -> Value {
    let got: Value = serde_json::from_str(&make(w, from, value).unwrap()).unwrap();
    assert_eq!(got["status"], "pending", "{got}");
    got
}

fn cards(w: &World, id: &str) -> Value {
    let all = core::status(&w.shared).unwrap();
    all["sessions"].as_array().unwrap().iter().find(|s| s["session"] == id).unwrap().get("approvals").cloned().unwrap_or(Value::Null)
}

fn mark(w: &World, id: &str, by: &str) {
    w.with(id, |s| s.made_by = Some(by.into()));
}

fn count(w: &World) -> usize {
    w.shared.sessions.read().unwrap().len()
}

#[test]
fn a_create_waits_for_the_person_and_makes_nothing_and_its_card_says_the_folder_the_mode_that_it_starts_at_once_and_the_start_of_the_prompt() {
    let w = room();
    let before = count(&w);
    let prompt = "review the login tests\nthen report";
    let got = waits(&w, ONE, json!({"prompt": prompt, "name": "reviewer"}));
    assert_eq!(got["approval_id"], 1);
    assert!(got["message"].as_str().unwrap().starts_with("Nothing was made yet. The person has been asked to approve the creation of a new session 'reviewer' in '/work/api'."), "{got}");
    assert!(got.get("already_pending").is_none() && got.get("session").is_none());
    let card = format!("create session 'reviewer' in '/work/api' | mode: auto (bypass) | {DEFAULT_MODEL} | {STARTS} | {} B, 2 lines | \"review the login tests\"", prompt.len());
    assert_eq!(cards(&w, ONE), json!([{"approval_id": 1, "prompt": card, "tool": "create"}]));
    assert_eq!(count(&w), before, "no session was made");
    let again = waits(&w, ONE, json!({"prompt": prompt, "name": "reviewer"}));
    assert_eq!((again["approval_id"].clone(), again["already_pending"].clone()), (json!(1), json!(true)), "the same request is the same card");
    waits(&w, TWO, json!({"prompt": "hello"}));
    assert_eq!(cards(&w, TWO), json!([{"approval_id": 1, "prompt": format!("create session in a scratch folder | mode: auto (bypass) | {DEFAULT_MODEL} | {STARTS} | 5 B | \"hello\""), "tool": "create"}]));
    waits(&w, THREE, json!({"prompt": "hello", "name": ""}));
    assert_eq!(cards(&w, THREE)[0]["prompt"], cards(&w, TWO)[0]["prompt"], "an empty name is none");
    waits(&w, EXTRA[0], json!({"prompt": "hello", "name": null}));
    assert_eq!(cards(&w, EXTRA[0])[0]["prompt"], cards(&w, TWO)[0]["prompt"], "a name that is null is none");
}

#[test]
fn a_folder_a_mode_an_account_a_model_or_a_from_in_the_arguments_is_not_heard() {
    let w = room();
    waits(&w, ONE, json!({"prompt": "p", "cwd": "/etc", "mode": "default", "account": "other", "model": "m", "effort": "high", "from": TWO}));
    assert_eq!(cards(&w, ONE)[0]["prompt"], format!("create session in '/work/api' | mode: auto (bypass) | {DEFAULT_MODEL} | {STARTS} | 1 B | \"p\""));
    assert_eq!(cards(&w, TWO), Value::Null, "what the caller said about itself was not heard");
}

#[test]
fn a_long_folder_is_cut_from_its_start_so_that_the_end_shows_and_the_card_fits() {
    let w = room();
    let folder = format!("/{}/project", "d".repeat(100));
    w.with(ONE, |s| s.settings.cwd = Some(folder.clone()));
    waits(&w, ONE, json!({"prompt": "x".repeat(500), "name": "n".repeat(NAME_CHARS)}));
    let card = cards(&w, ONE)[0]["prompt"].as_str().unwrap().to_string();
    assert!(card.chars().count() <= calls::CARD_CHARS, "{}", card.chars().count());
    assert!(card.contains(&format!("'{}' in '\u{2026}ddd", "n".repeat(NAME_CHARS))) && card.ends_with(&format!("/project' | mode: auto (bypass) | {DEFAULT_MODEL} | {STARTS}")), "the size and the prompt gave way, the host's words did not: {card}");
}

/// The caller's choices and the agent's name and prompt, as a `create` card is made of them.
fn card_for(name: &str, folder: Option<&str>, model: Option<&str>, effort: Option<&str>, prompt: &str, removed: usize) -> String {
    let settings = Settings { cwd: folder.map(String::from), name: None, model: model.map(String::from), effort: effort.map(String::from), ..Settings::default() };
    card_of(name, folder, &chosen_words(&settings), prompt, removed)
}

#[test]
fn the_host_words_of_the_card_stay_whole_and_in_order_whatever_the_agent_names_the_session_and_whatever_the_person_chose_within_the_bounds() {
    let long_folder = format!("/{}/project", "d".repeat(100));
    let (id26, id40) = ("claude-sonnet-4-5-20250929", "m".repeat(40));
    let mut cards = 0;
    for name in ["".to_string(), "n".repeat(12), "n".repeat(NAME_CHARS)] {
        for folder in [None, Some("/w/api"), Some(long_folder.as_str())] {
            for model in [None, Some("opus"), Some(id26), Some(id40.as_str())] {
                for effort in [None, Some("high"), Some("medium"), Some("xhigh"), Some("12345678")] {
                    for prompt in ["p".to_string(), "x".repeat(500)] {
                        let settings = Settings { cwd: None, name: None, model: model.map(String::from), effort: effort.map(String::from), ..Settings::default() };
                        let words = format!("mode: auto (bypass) | {} | {STARTS}", chosen_words(&settings));
                        let card = card_for(&name, folder, model, effort, &prompt, 0);
                        assert!(card.chars().count() <= calls::CARD_CHARS && card.starts_with("create session") && card.contains(&words), "{name:?} {folder:?} {model:?} {effort:?}: {card}");
                        cards += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cards, 3 * 3 * 4 * 5 * 2);
}

#[test]
fn the_card_of_a_call_is_made_of_the_name_the_folder_and_the_choices_of_the_caller_as_they_are_when_it_asks() {
    let w = room();
    w.with(ONE, |s| (s.settings.model, s.settings.effort, s.settings.cwd) = (Some("claude-sonnet-4-5-20250929".into()), Some("high".into()), Some(format!("/{}/project", "d".repeat(100)))));
    waits(&w, ONE, json!({"prompt": "p", "name": "n".repeat(NAME_CHARS)}));
    let wanted = card_for(&"n".repeat(NAME_CHARS), Some(&format!("/{}/project", "d".repeat(100))), Some("claude-sonnet-4-5-20250929"), Some("high"), "p", 0);
    assert_eq!(cards(&w, ONE)[0]["prompt"], wanted);
    assert!(wanted.contains("\u{2026}' in '") && wanted.ends_with(STARTS), "{wanted}");
}

#[test]
fn what_gives_way_on_a_card_that_is_too_long_is_the_start_of_the_prompt_then_the_size_then_the_name_then_the_folder() {
    let long_folder = format!("/{}/project", "d".repeat(100));
    let (id26, id40, name) = ("claude-sonnet-4-5-20250929", "m".repeat(40), "n".repeat(NAME_CHARS));
    let fits = card_for("writer", Some("/w/api"), Some("opus"), Some("high"), "fix the tests", 0);
    assert_eq!(fits, format!("create session 'writer' in '/w/api' | mode: auto (bypass) | model: opus, effort: high | {STARTS} | 13 B | \"fix the tests\""));
    let cut = card_for(&"n".repeat(12), Some("/w/api"), Some(id26), Some("high"), &"x".repeat(500), 0);
    assert!(cut.contains(" | 500 B | \"x") && cut.ends_with("\u{2026}\"") && cut.chars().count() <= calls::CARD_CHARS, "{cut}");
    let hidden = card_for(&"n".repeat(12), Some("/w/api"), Some(id26), Some("high"), &"x".repeat(500), 100);
    assert!(hidden.ends_with(STARTS) && hidden.chars().count() < 170, "{hidden}");
    let no_size = card_for(&name, Some(long_folder.as_str()), None, None, "p", 0);
    assert!(no_size.contains(&format!("'{name}' in '\u{2026}")) && no_size.ends_with(STARTS) && !no_size.contains(" B"), "{no_size}");
    let cut_name = card_for(&name, Some(long_folder.as_str()), Some(id26), Some("high"), "p", 0);
    let (named, rest) = cut_name.split_once("' in '").unwrap();
    assert!(named.starts_with("create session 'nnnn") && named.ends_with('\u{2026}') && named.matches('n').count() < NAME_CHARS && rest.starts_with('\u{2026}') && cut_name.ends_with(STARTS), "{cut_name}");
    assert_eq!(cut_name.chars().count(), calls::CARD_CHARS, "the name takes what is left");
    let gone = card_for(&name, Some(long_folder.as_str()), Some(id40.as_str()), Some("123456789012"), "p", 0);
    assert!(gone.starts_with("create session in '\u{2026}") && gone.ends_with(STARTS) && !gone.contains("nnn"), "{gone}");
    let bare_len = |m: usize| format!("create session in a scratch folder | mode: auto (bypass) | model: {}, effort: high | {STARTS}", "m".repeat(m)).chars().count();
    let m = 1 + 196 - bare_len(1);
    let one_left = card_for(&name, None, Some(&"m".repeat(m - 1)), Some("high"), "p", 0);
    assert!(one_left.starts_with("create session 'n\u{2026}' in a scratch folder | ") && one_left.chars().count() == calls::CARD_CHARS && one_left.ends_with(STARTS), "{one_left}");
    let none_left = card_for(&name, None, Some(&"m".repeat(m)), Some("high"), "p", 0);
    assert!(none_left.starts_with("create session in a scratch folder | ") && none_left.chars().count() == 196 && none_left.ends_with(STARTS), "{none_left}");
    let beyond = card_for(&name, Some(long_folder.as_str()), Some(&"m".repeat(64)), Some(&"e".repeat(16)), "p", 0);
    assert!(beyond.starts_with("create session in '\u{2026}") && !beyond.contains("nnn") && beyond.contains("project' | mode: auto (bypass) | model: "), "{beyond}");
    assert_eq!(beyond.split('\'').nth(1).unwrap().chars().count(), 16, "{beyond}");
}

#[test]
fn invalid_kind_scope_prompt_or_name_is_refused_before_the_person_is_asked() {
    let w = room();
    let cases = [
        json!({"kind": "bash"}),
        json!({"kind": 5}),
        json!({"scope": "/etc"}),
        json!({"scope": 5}),
        json!({"prompt": 5}),
        json!({"prompt": "p", "name": 5}),
        json!({"prompt": "p", "name": "a\nb"}),
        json!({"prompt": "p", "name": "a\tb"}),
        json!({"prompt": "p", "name": "x".repeat(NAME_CHARS + 1)}),
        json!({"prompt": "p", "name": "\u{200b}\u{200b}"}),
        json!({"prompt": "\u{200b}\u{1b}[0m"}),
    ];
    for bad in cases {
        let got = make(&w, ONE, bad.clone());
        assert!(got.as_ref().is_err_and(|e| e.starts_with("bad_request: create ")), "{bad}: {got:?}");
    }
    assert_eq!(cards(&w, ONE), Value::Null);
    assert!(make(&w, ONE, json!({"prompt": "p", "name": "x".repeat(NAME_CHARS)})).is_ok(), "the longest name is let through");
}

#[test]
fn a_name_an_agent_gives_passes_the_cleaner_and_one_with_a_hidden_character_or_an_escape_is_refused_not_mended() {
    let w = room();
    for name in ["rev\u{202e}iewer", "rev\u{200b}iewer", "rev\u{e0041}iewer", "rev\u{1b}[31miewer", "rev\u{1b}]0;x\u{7}iewer", "\u{fe0f}", "rev\u{ad}iewer", "rev\u{3164}iewer", "rev\u{1d173}iewer"] {
        let got = make(&w, ONE, json!({"prompt": "p", "name": name}));
        assert!(got.as_ref().is_err_and(|e| e.starts_with("bad_request: create ")), "{name:?}: {got:?}");
    }
    assert_eq!(cards(&w, ONE), Value::Null, "nothing was put to the person");
}

#[test]
fn a_name_cannot_end_its_quote_on_the_card_or_fake_a_field_of_it_and_the_session_gets_the_name_the_card_showed() {
    let w = room();
    waits(&w, ONE, json!({"prompt": "p", "name": "x' in '/etc | mode: auto | [(a)]"}));
    let card = cards(&w, ONE)[0]["prompt"].as_str().unwrap().to_string();
    assert!(card.starts_with("create session 'x_ in _/etc _ mode: auto _ __a__' in '/work/api' | mode: auto (bypass) | "), "{card}");
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    perform_with(&w.shared, ONE, &plain("x' in '/etc | mode: auto | [(a)]"), "p", &starter(&w, NEW, &seen, &inbox)).unwrap();
    assert_eq!(seen.lock().unwrap()[0].settings.name.as_deref(), Some("x_ in _/etc _ mode: auto _ __a__ - made by writer"));
}

#[test]
fn the_prompt_is_cleaned_once_at_the_call_and_the_card_the_page_and_the_session_all_get_the_same_string() {
    let w = room();
    let dirty = "\u{1b}[31mred\u{1b}[0m\u{202e}evil\u{200b}\r\nsecond";
    let got = waits(&w, ONE, json!({"prompt": dirty}));
    let card = cards(&w, ONE)[0]["prompt"].as_str().unwrap().to_string();
    assert!(card.contains(" | 14 B, 2 lines | 11 hidden characters removed | \"redevil"), "{card}");
    assert!(!card.chars().any(|c| c.is_control() || c == '\u{202e}'), "{card:?}");
    let generation = w.with(ONE, |s| s.generation as i64);
    let page = crate::pending::text_page(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), 0, crate::pending::PAGE).unwrap();
    assert_eq!((page["text"].as_str(), page["total"].as_u64()), (Some("redevil\nsecond"), Some(14)), "the person reads what the new session will get");
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    perform_with(&w.shared, ONE, "", page["text"].as_str().unwrap(), &starter(&w, NEW, &seen, &inbox)).unwrap();
    assert_eq!(first_input(&inbox).split_once('\n').unwrap().1, "redevil\nsecond");
    for prompt in ["\u{200b}\u{e0041}", "\u{2800}", "\u{0301}", " ", "\n", "\u{1b}[31m\u{2800}", "\u{1b}]0;t\u{7}\u{200b}\u{2800}"] {
        let got = make(&w, ONE, json!({"prompt": prompt}));
        assert!(got.as_ref().is_err_and(|e| e.starts_with("bad_request: ") && e.contains("draw nothing")), "a prompt of nothing that draws is no prompt: {prompt:?} {got:?}");
    }
}

#[test]
fn a_word_and_a_heart_with_its_selector_are_prompts_and_get_a_card() {
    let w = room();
    for (prompt, shown) in [("word", "word"), ("\u{2764}\u{fe0f}", "\u{2764}")] {
        let got = waits(&w, ONE, json!({"prompt": prompt}));
        assert!(cards(&w, ONE)[0]["prompt"].as_str().unwrap().contains(shown), "{prompt:?}");
        let generation = w.with(ONE, |s| s.generation as i64);
        core::approval_answer(&w.shared, ONE, generation, got["approval_id"].as_i64().unwrap(), "deny", false).unwrap();
    }
}

#[test]
fn a_create_counts_in_the_same_minute_as_a_send_and_a_refused_one_counts() {
    let w = room();
    let (window, held, now) = (Mutex::default(), Arc::default(), Instant::now());
    for _ in 0..10 {
        let _ = calls::tests::call_in(&w, ONE, &calls::tests::ask(TWO, ""), &window, now, &held); // refused (empty): it counts
        let _ = call(&w.shared, ONE, &arguments(json!({"kind": "bash"})), &window, now, &held); // refused (bad kind): it counts
    }
    let got = call(&w.shared, ONE, &arguments(json!({"prompt": "p"})), &window, now, &held);
    assert!(got.as_ref().is_err_and(|e| e.starts_with("send_limit: at most 20 calls a minute from one session; wait ")), "{got:?}");
    let sent = calls::tests::call_in(&w, ONE, &calls::tests::ask(TWO, "hello"), &window, now, &held);
    assert!(sent.as_ref().is_err_and(|e| e.starts_with("send_limit: ")), "ten sends and ten creates are twenty calls: {sent:?}");
    assert!(call(&w.shared, TWO, &arguments(json!({"prompt": "p"})), &window, now, &held).is_ok(), "another session has a window of its own");
}

#[test]
fn a_chain_that_is_too_long_is_refused_at_the_call_in_the_words_of_send_and_the_person_is_not_asked() {
    let w = room();
    w.with(ONE, |s| s.incoming_hops = 3);
    waits(&w, ONE, json!({"prompt": "the fourth step"}));
    w.with(ONE, |s| s.incoming_hops = 4);
    let got = make(&w, ONE, json!({"prompt": "the fifth step"}));
    assert!(got.as_ref().is_err_and(|e| e.starts_with("chain_depth: this would be step 5 in a row") && e.contains("limit is 4") && e.contains("tell the person")), "{got:?}");
    assert_eq!(cards(&w, ONE).as_array().unwrap().len(), 1);
}

#[test]
fn four_running_sessions_of_the_caller_stop_the_fifth_and_only_running_ones_count() {
    let w = room();
    for id in &EXTRA[..3] {
        mark(&w, id, ONE);
    }
    waits(&w, ONE, json!({"prompt": "the fourth"}));
    mark(&w, EXTRA[3], ONE);
    let got = make(&w, ONE, json!({"prompt": "the fifth"}));
    assert!(got.as_ref().is_err_and(|e| e.starts_with("send_limit: 4 sessions that you made are running; the limit is 4 for one session and 8 in all") && e.contains("close one")), "{got:?}");
    assert!(make(&w, TWO, json!({"prompt": "another caller"})).is_ok(), "the count is the caller's own");
    w.with(EXTRA[3], |s| s.state = State::Exited);
    assert!(make(&w, ONE, json!({"prompt": "the fifth"})).is_ok(), "a session that ended is not running");
    w.with(EXTRA[3], |s| s.state = State::Running);
    w.with(EXTRA[2], |s| s.state = State::Disconnected);
    assert!(make(&w, ONE, json!({"prompt": "the sixth"})).is_ok(), "a closed one has no program");
    w.with(EXTRA[2], |s| s.state = State::Running);
    w.with(EXTRA[1], |s| s.purged = true);
    assert!(make(&w, ONE, json!({"prompt": "the seventh"})).is_ok(), "a deleted one is gone");
}

#[test]
fn eight_running_sessions_that_agents_made_stop_every_caller() {
    let w = room();
    for id in &EXTRA[..4] {
        mark(&w, id, ONE);
    }
    for id in &EXTRA[4..] {
        mark(&w, id, TWO);
    }
    let got = make(&w, THREE, json!({"prompt": "p"}));
    assert!(got.as_ref().is_err_and(|e| e.starts_with("send_limit: 8 sessions that agents made are running; the limit is 8 in all and 4 for one session") && e.contains("close one")), "{got:?}");
    w.with(EXTRA[7], |s| s.state = State::Exited);
    assert!(make(&w, THREE, json!({"prompt": "p"})).is_ok());
}

#[test]
fn a_session_the_person_made_is_not_counted_against_any_cap() {
    let w = room();
    for id in &EXTRA {
        w.with(id, |s| s.made_by = None);
    }
    assert!(make(&w, ONE, json!({"prompt": "p"})).is_ok(), "eleven sessions run, and a person made all of them");
}

#[test]
fn the_card_says_the_model_and_the_effort_the_new_session_gets_which_are_the_callers_and_the_program_decides_where_the_caller_chose_nothing() {
    let w = room();
    let card = |from: &str| cards(&w, from)[0]["prompt"].as_str().unwrap().to_string();
    w.with(ONE, |s| (s.settings.model, s.settings.effort) = (Some("haiku".into()), Some("low".into())));
    waits(&w, ONE, json!({"prompt": "p"}));
    assert_eq!(card(ONE), format!("create session in '/work/api' | mode: auto (bypass) | model: haiku, effort: low | {STARTS} | 1 B | \"p\""));
    w.with(TWO, |s| s.settings.model = Some("claude-sonnet-4-5".into()));
    waits(&w, TWO, json!({"prompt": "p"}));
    assert!(card(TWO).starts_with("create session in a scratch folder | mode: auto (bypass) | model: claude-sonnet-4-5, effort: the program's default | "), "{}", card(TWO));
    w.with(THREE, |s| s.settings.effort = Some("high".into()));
    waits(&w, THREE, json!({"prompt": "p"}));
    assert!(card(THREE).contains(" | model: the program's default, effort: high | "), "{}", card(THREE));
    waits(&w, EXTRA[0], json!({"prompt": "p"}));
    assert!(card(EXTRA[0]).contains(&format!(" | {DEFAULT_MODEL} | {STARTS} | ")) && !card(EXTRA[0]).contains("effort"), "a caller with nothing chosen: the program's default, said once: {}", card(EXTRA[0]));
}

#[test]
fn a_yes_gives_the_new_session_the_model_and_the_effort_the_caller_has_then_and_none_stays_none() {
    let w = room();
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    w.with(ONE, |s| (s.settings.model, s.settings.effort) = (Some("haiku".into()), Some("low".into())));
    perform_with(&w.shared, ONE, "", "p", &starter(&w, NEW, &seen, &inbox)).unwrap();
    w.with(TWO, |s| s.settings.effort = Some("high".into()));
    perform_with(&w.shared, TWO, "", "p", &starter(&w, OTHER_NEW, &seen, &inbox)).unwrap();
    perform_with(&w.shared, THREE, "", "p", &starter(&w, "s4000000000000002", &seen, &inbox)).unwrap();
    let got: Vec<_> = seen.lock().unwrap().iter().map(|spec| (spec.settings.model.clone(), spec.settings.effort.clone())).collect();
    assert_eq!(got, [(Some("haiku".into()), Some("low".into())), (None, Some("high".into())), (None, None)], "what the caller chose is copied as it is; a choice left to the program stays left to it");
    assert_eq!(w.with(NEW, |s| (s.settings.model.clone(), s.settings.effort.clone())), (Some("haiku".into()), Some("low".into())), "and the session made has it");
}

/// A start that makes a session in the room the way the host would: it takes what the spec says, and the receiver of its first prompt is read back from `inbox`.
fn starter<'a>(w: &'a World, id: &'static str, seen: &'a Mutex<Vec<Spec>>, inbox: &'a Mutex<Option<Receiver<String>>>) -> impl Fn(&Spec) -> Result<String, &'static str> + 'a {
    move |spec| {
        let (tx, rx) = sync_channel(8);
        let mut s = Session::new(id.into(), false, &w.dir.0.join("log"), 100, 100);
        s.input_tx = Some(tx);
        (s.provider, s.settings, s.mode, s.account) = (spec.kind, spec.settings.clone(), spec.mode, spec.account.clone());
        w.shared.sessions.write().unwrap().push(Arc::new(Mutex::new(s)));
        *inbox.lock().unwrap() = Some(rx);
        seen.lock().unwrap().push(spec.clone());
        Ok(id.into())
    }
}

fn first_input(inbox: &Mutex<Option<Receiver<String>>>) -> String {
    let line = inbox.lock().unwrap().as_ref().unwrap().try_recv().unwrap();
    String::from_utf8(crate::payload::unhex(line.split(' ').nth(2).unwrap()).unwrap()).unwrap()
}

#[test]
fn a_yes_makes_a_session_in_the_folder_of_the_caller_in_auto_mode_and_delivers_the_prompt_as_a_step_on_the_chain() {
    let w = room();
    w.with(ONE, |s| (s.incoming_hops, s.account) = (2, Some("main".into())));
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    let got = perform_with(&w.shared, ONE, "reviewer", "review it", &starter(&w, NEW, &seen, &inbox)).unwrap();
    let spec = Spec {
        kind: "claude",
        settings: Settings { cwd: Some(FOLDER.into()), name: Some("reviewer - made by writer".into()), model: None, effort: None, ..Settings::default() },
        account: Some("main".into()),
        mode: "auto",
    };
    assert_eq!(*seen.lock().unwrap(), [spec], "the folder, the account and the mode are the caller's and the host's, the name is the agent's with who made it");
    assert!(got.starts_with(&format!("created session {NEW} 'reviewer - made by writer' in '{FOLDER}', mode: auto (bypass); the first prompt (9 B) was delivered (inline)")), "{got}");
    assert_eq!(w.with(NEW, |s| (s.made_by.clone(), s.incoming_hops)), (Some(ONE.to_string()), 3), "it is marked, and the prompt is a step on the chain");
    let text = first_input(&inbox);
    let (head, body) = text.split_once('\n').unwrap();
    assert!(head.starts_with("[from session 'writer' (s012345) | ") && head.ends_with(" | message | 9 B]"), "{head}");
    assert_eq!(body, "review it");
    let unnamed = perform_with(&w.shared, ONE, "", "again", &starter(&w, OTHER_NEW, &seen, &inbox)).unwrap();
    assert!(unnamed.starts_with(&format!("created session {OTHER_NEW} 'made by writer' in '{FOLDER}'")), "{unnamed}");
    assert_eq!(seen.lock().unwrap()[1].settings.name.as_deref(), Some("made by writer"), "a session without a name from the agent still says who made it");
}

#[test]
fn claude_can_ask_for_codex_and_codex_can_ask_for_claude_resolving_the_target_providers_account_and_default_model() {
    let w = room();
    let (acc_claude, acc_codex) = {
        let mut accs = w.shared.accounts.lock().unwrap();
        (accs.create("claude", "Claude-Main").unwrap().id, accs.create("codex", "Codex-Main").unwrap().id)
    };
    w.with(ONE, |s| {
        s.provider = "claude";
        s.account = Some(acc_claude.clone());
        s.settings.model = Some("claude-sonnet-4-5".into());
        s.settings.effort = Some("high".into());
    });
    w.with(TWO, |s| {
        s.provider = "codex";
        s.account = Some(acc_codex.clone());
        s.settings.name = Some("Codex-L".into());
        s.settings.cwd = Some(FOLDER.into());
        s.settings.model = Some("gpt-5".into());
        s.settings.effort = Some("medium".into());
    });

    // Claude -> Codex (empty prompt: opens ready tab, does not forward claude-sonnet-4-5 to codex)
    let got_c2x = waits(&w, ONE, json!({"kind": "codex", "name": "Codex-Tab"}));
    assert!(got_c2x["message"].as_str().unwrap().contains("creation of a new codex session 'Codex-Tab' in '/work/api'"), "{got_c2x}");
    let card_c2x = cards(&w, ONE)[0]["prompt"].as_str().unwrap().to_string();
    assert_eq!(card_c2x, format!("create codex session 'Codex-Tab' in '/work/api' | mode: auto (bypass) | {DEFAULT_MODEL} | {OPENS_READY}"));
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    let done_c2x = perform_with(&w.shared, ONE, &encode_target("codex", "folder", "Codex-Tab"), "", &starter(&w, NEW, &seen, &inbox)).unwrap();
    assert!(done_c2x.contains(&format!("created codex session {NEW} 'Codex-Tab - made by writer' in '{FOLDER}', mode: auto (bypass); the new tab is open and ready (no initial prompt sent)")), "{done_c2x}");
    assert_eq!(
        seen.lock().unwrap()[0],
        Spec {
            kind: "codex",
            settings: Settings { cwd: Some(FOLDER.into()), name: Some("Codex-Tab - made by writer".into()), model: None, effort: None, ..Settings::default() },
            account: Some(acc_codex.clone()),
            mode: "auto",
        }
    );
    assert!(inbox.lock().unwrap().as_ref().unwrap().try_recv().is_err(), "empty-prompt tab gets no initial message");
    assert_eq!(w.with(NEW, |s| (s.provider, s.made_by.clone(), s.incoming_hops)), ("codex", Some(ONE.to_string()), 1));

    // Codex -> Claude (with prompt + global scope: resolves acc_claude, sets cwd = None)
    let got_x2c = waits(&w, TWO, json!({"kind": "claude", "scope": "global", "name": "Claude-Global", "prompt": "check logs"}));
    assert!(got_x2c["message"].as_str().unwrap().contains("creation of a new claude session 'Claude-Global' in global scope"), "{got_x2c}");
    let card_x2c = cards(&w, TWO)[0]["prompt"].as_str().unwrap().to_string();
    assert_eq!(card_x2c, format!("create claude session 'Claude-Global' in global scope | mode: auto (bypass) | {DEFAULT_MODEL} | {STARTS} | 10 B | \"check logs\""));
    let done_x2c = perform_with(&w.shared, TWO, &encode_target("claude", "global", "Claude-Global"), "check logs", &starter(&w, OTHER_NEW, &seen, &inbox)).unwrap();
    assert!(done_x2c.contains(&format!("created claude session {OTHER_NEW} 'Claude-Global - made by Codex-L' in global scope, mode: auto (bypass); the first prompt (10 B) was delivered")), "{done_x2c}");
    assert_eq!(
        seen.lock().unwrap()[1],
        Spec {
            kind: "claude",
            settings: Settings { cwd: None, name: Some("Claude-Global - made by Codex-L".into()), model: None, effort: None, ..Settings::default() },
            account: Some(acc_claude),
            mode: "auto",
        }
    );
    assert_eq!(first_input(&inbox).split_once('\n').unwrap().1, "check logs");
}

#[test]
fn omitting_kind_on_a_codex_caller_defaults_to_codex_with_the_callers_account_and_model() {
    let w = room();
    w.with(TWO, |s| {
        s.provider = "codex";
        s.account = Some("codex_acc".into());
        s.settings.model = Some("gpt-5".into());
        s.settings.effort = Some("high".into());
    });
    waits(&w, TWO, json!({"name": "Same-Kind"}));
    let card = cards(&w, TWO)[0]["prompt"].as_str().unwrap().to_string();
    assert_eq!(card, format!("create codex session 'Same-Kind' in a scratch folder | mode: auto (bypass) | model: gpt-5, effort: high | {OPENS_READY}"));
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    perform_with(&w.shared, TWO, &encode_target("codex", "folder", "Same-Kind"), "", &starter(&w, NEW, &seen, &inbox)).unwrap();
    assert_eq!(seen.lock().unwrap()[0].kind, "codex");
    assert_eq!(seen.lock().unwrap()[0].account.as_deref(), Some("codex_acc"));
    assert_eq!(seen.lock().unwrap()[0].settings.model.as_deref(), Some("gpt-5"));
}

#[test]
fn the_name_of_the_new_session_says_who_made_it_and_never_goes_over_the_longest_name() {
    let w = room();
    w.with(ONE, |s| s.settings.name = Some("w".repeat(60)));
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    perform_with(&w.shared, ONE, &"n".repeat(NAME_CHARS), "p", &starter(&w, NEW, &seen, &inbox)).unwrap();
    let name = seen.lock().unwrap()[0].settings.name.clone().unwrap();
    assert_eq!(name, format!("{} - made by {}", "n".repeat(NAME_CHARS), "w".repeat(MADE_BY_CHARS)));
    assert!(crate::folders::valid_name(&name) && name.chars().count() <= crate::folders::MAX_NAME, "{name}");
    w.with(ONE, |s| s.settings.name = None);
    perform_with(&w.shared, ONE, "x", "p", &starter(&w, OTHER_NEW, &seen, &inbox)).unwrap();
    assert_eq!(seen.lock().unwrap()[1].settings.name.as_deref(), Some("x - made by api"), "a caller without a name is called by its folder");
    w.with(ONE, |s| s.settings.name = Some("abcdefghijk lmn".into()));
    perform_with(&w.shared, ONE, "", "p", &starter(&w, "s4000000000000002", &seen, &inbox)).unwrap();
    assert_eq!(seen.lock().unwrap()[2].settings.name.as_deref(), Some("made by abcdefghijk"), "a label cut at a space does not leave the space in the name");
}

#[test]
fn a_caller_in_a_scratch_folder_makes_a_session_in_a_scratch_folder_too() {
    let w = room();
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    let got = perform_with(&w.shared, TWO, "", "hello", &starter(&w, NEW, &seen, &inbox)).unwrap();
    assert_eq!(seen.lock().unwrap()[0].settings.cwd, None);
    assert!(got.starts_with(&format!("created session {NEW} 'made by reader' in a scratch folder, mode: auto (bypass);")), "{got}");
}

#[test]
fn the_chain_and_the_caps_are_asked_again_at_the_yes_and_nothing_is_made_when_they_stop_it() {
    let w = room();
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    w.with(ONE, |s| s.incoming_hops = 4);
    let got = perform_with(&w.shared, ONE, "", "p", &starter(&w, NEW, &seen, &inbox));
    assert!(got.as_ref().is_err_and(|e| e.starts_with("chain_depth: this would be step 5")), "{got:?}");
    w.with(ONE, |s| s.incoming_hops = 0);
    for id in &EXTRA[..4] {
        mark(&w, id, ONE);
    }
    let got = perform_with(&w.shared, ONE, "", "p", &starter(&w, NEW, &seen, &inbox));
    assert!(got.as_ref().is_err_and(|e| e.starts_with("send_limit: 4 sessions that you made are running")), "{got:?}");
    for id in &EXTRA[..4] {
        mark(&w, id, TWO);
    }
    for id in &EXTRA[4..] {
        mark(&w, id, THREE);
    }
    let got = perform_with(&w.shared, ONE, "", "p", &starter(&w, NEW, &seen, &inbox));
    assert!(got.as_ref().is_err_and(|e| e.starts_with("send_limit: 8 sessions that agents made are running")), "{got:?}");
    assert!(seen.lock().unwrap().is_empty() && w.with(ONE, |s| s.incoming_hops) == 0, "the program was never started");
}

#[test]
fn a_caller_that_ended_before_the_answer_creates_nothing() {
    let w = room();
    let (seen, inbox) = (Mutex::default(), Mutex::default());
    assert!(perform_with(&w.shared, "s9999999999999999", "", "p", &starter(&w, NEW, &seen, &inbox)).is_err(), "a caller that is not there asks for nothing");
    for end in [|s: &mut Session| s.purged = true, |s: &mut Session| s.state = State::Exited, |s: &mut Session| s.state = State::Disconnected] {
        let w = room();
        w.with(ONE, end);
        let got = perform_with(&w.shared, ONE, "", "p", &starter(&w, NEW, &seen, &inbox));
        assert!(got.as_ref().is_err_and(|e| e.starts_with("session_exited: ")), "{got:?}");
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn a_session_that_could_not_be_started_leaves_nothing_and_says_why_in_words_the_model_reads() {
    let w = room();
    let before = count(&w);
    for (code, words) in [
        ("busy", "busy: the host could not start the session just now"),
        ("folder_not_directory", "folder_not_directory: the folder of this session is not usable any more"),
        ("unknown_account", "unknown_account: the account of this session is not there any more"),
        ("option_unavailable", "option_unavailable: the model or the effort of this session is not offered by the program any more"),
        ("host_failed", "host_failed: the host refused it"),
    ] {
        let got = perform_with(&w.shared, ONE, "", "p", &|_| Err(code));
        assert!(got.as_ref().is_err_and(|e| e.starts_with(words)), "{code}: {got:?}");
    }
    assert_eq!(count(&w), before);
}

#[test]
fn a_prompt_that_could_not_be_delivered_takes_the_new_session_away_again() {
    let w = room();
    let before = count(&w);
    let start = |spec: &Spec| {
        let mut s = Session::new(NEW.into(), false, &w.dir.0.join("log"), 100, 100);
        (s.provider, s.settings, s.mode) = (spec.kind, spec.settings.clone(), spec.mode);
        w.shared.sessions.write().unwrap().push(Arc::new(Mutex::new(s)));
        Ok(NEW.into())
    };
    let got = perform_with(&w.shared, ONE, "", "p", &start);
    assert!(got.as_ref().is_err_and(|e| e.starts_with("session_exited: ")), "{got:?}");
    assert!(core::find(&w.shared, NEW).is_none() && count(&w) == before, "a session without its prompt is not left behind");
}

#[test]
fn two_yeses_at_once_cannot_get_past_the_cap_together() {
    let w = room();
    for id in &EXTRA[..3] {
        mark(&w, id, ONE);
    }
    let dir = w.dir.0.join("log");
    let inboxes: Arc<Mutex<Vec<Receiver<String>>>> = Arc::default();
    let slow = |id: &'static str| {
        let (shared, dir, inboxes) = (w.shared.clone(), dir.clone(), inboxes.clone());
        std::thread::spawn(move || {
            let start = |spec: &Spec| {
                std::thread::sleep(Duration::from_millis(200));
                let (tx, rx) = sync_channel(8);
                inboxes.lock().unwrap().push(rx);
                let mut s = Session::new(id.into(), false, &dir, 100, 100);
                (s.input_tx, s.provider, s.settings, s.mode) = (Some(tx), spec.kind, spec.settings.clone(), spec.mode);
                shared.sessions.write().unwrap().push(Arc::new(Mutex::new(s)));
                Ok(id.into())
            };
            perform_with(&shared.clone(), ONE, "", "p", &start).map(|_| ())
        })
    };
    let (first, second) = (slow(NEW), slow(OTHER_NEW));
    let results = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1, "{results:?}");
    assert!(results.iter().any(|r| r.as_ref().is_err_and(|e| e.starts_with("send_limit: 4 sessions"))), "{results:?}");
}

#[test]
fn a_no_makes_nothing_and_the_agent_is_told_in_words_that_say_what_it_asked_and_a_yes_for_the_session_is_not_offered() {
    let w = room();
    let before = count(&w);
    waits(&w, ONE, json!({"prompt": "p", "name": "reviewer"}));
    let generation = w.with(ONE, |s| s.generation as i64);
    assert_eq!(core::approval_answer(&w.shared, ONE, generation, 1, "allow", true), Err("bad_request"), "each session asks again");
    assert_eq!(cards(&w, ONE).as_array().unwrap().len(), 1, "the card stays");
    let done = core::approval_answer(&w.shared, ONE, generation, 1, "deny", false).unwrap();
    assert_eq!(done["outcome"], "declined");
    let note = w.delivered(ONE).unwrap();
    assert!(note.contains("declined: the person did not allow this creation of a new session 'reviewer' in '/work/api'. Do not ask again"), "{note}");
    assert_eq!(count(&w), before);
    assert_eq!(core::status(&w.shared).unwrap()["sessions"][0].get("allowed"), None);
}

#[test]
fn the_audit_line_names_the_tool_the_session_and_the_result_and_never_a_word_of_the_prompt() {
    let line = audit_line(ONE, "send_limit");
    assert_eq!(line, format!("lico-host: call session={ONE} tool=create result=send_limit"));
    assert_eq!(audit_line(NEW, "created"), format!("lico-host: call session={NEW} tool=create result=created"));
}
