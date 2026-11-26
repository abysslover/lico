//! The tests of the `agy` adapter that need no process: the mapping against the fixtures of `tests/fixtures/agy_acp/` (the same files the Python reference is held to), its bounds,
//! and the order of the conversation. The process, the threads and the session are tried with the simulator by `tests/test_agy_session.py`.

use serde_json::{json, Value};
use std::time::{Duration, Instant};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use super::conn::{Conn, Effect, INIT_WAIT, OPEN_WAIT, PROTOCOL};
use super::{Asked, Handle, Msg};
use crate::driver::Link;
use super::map::{answer, approval_prompt, map_stream, request_id_ok, valid_options, Approval, Mapped, Mapper, Rec, MAX_PENDING, MAX_PROMPT, MAX_RECORD, MAX_TOOLS, SIGN_IN_WORDS};

const FIXTURES: [(&str, &str); 3] = [
    ("turn_with_permission", include_str!("../../../../../tests/fixtures/agy_acp/turn_with_permission.json")),
    ("turn_signed_out", include_str!("../../../../../tests/fixtures/agy_acp/turn_signed_out.json")),
    ("resume_with_replay", include_str!("../../../../../tests/fixtures/agy_acp/resume_with_replay.json")),
];

fn texts(mapped: &Mapped) -> Vec<String> {
    mapped.records.iter().map(|r| r.text().to_string()).collect()
}

fn agent(message: Value) -> Value {
    json!({"dir": "agent", "msg": message})
}

fn host(message: Value) -> Value {
    json!({"dir": "host", "msg": message})
}

fn update(body: Value) -> Value {
    agent(json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s", "update": body}}))
}

fn chunk(text: &str) -> Value {
    update(json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}))
}

fn options() -> Value {
    json!([{"optionId": "a", "name": "Allow", "kind": "allow_once"}, {"optionId": "r", "name": "Reject", "kind": "reject_once"}])
}

fn ask_with(ident: Value, opts: Value) -> Value {
    agent(json!({"jsonrpc": "2.0", "id": ident, "method": "session/request_permission", "params": {"sessionId": "s", "toolCall": {"toolCallId": "c", "title": "T", "kind": "execute", "rawInput": {"command": "echo hi\nrm -rf /"}}, "options": opts}}))
}

fn ask(ident: Value) -> Value {
    ask_with(ident, options())
}

fn prompt_sent() -> Value {
    host(json!({"jsonrpc": "2.0", "id": 3, "method": "session/prompt", "params": {}}))
}

fn answered(id: u64, result: Value) -> Value {
    agent(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

#[test]
fn each_fixture_maps_to_the_records_approvals_answers_and_ignored_counts_the_python_reference_gives() {
    for (name, text) in FIXTURES {
        let data: Value = serde_json::from_str(text).unwrap();
        let stream = data["stream"].as_array().unwrap();
        let mapped = map_stream(stream, true);
        let want: Vec<String> = data["records"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect();
        assert_eq!(texts(&mapped), want, "{name}: records");
        let seen: Vec<Value> = mapped
            .approvals
            .iter()
            .map(|a| json!({"request_id": a.request_id, "prompt": a.prompt, "options": a.options.iter().map(|o| json!({"optionId": o.option_id, "kind": o.kind, "name": o.name})).collect::<Vec<_>>()}))
            .collect();
        assert_eq!(Value::Array(seen), data["approvals"], "{name}: approvals");
        let ignored: Value = mapped.ignored.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>().into();
        assert_eq!(ignored, data["ignored"], "{name}: ignored");
        for (choice, allow) in [("allow", true), ("deny", false)] {
            if let Some(expected) = data["answers"].get(choice) {
                assert_eq!(&answer(&mapped.approvals[0], allow), expected, "{name}: the answer for {choice}");
            }
        }
        assert!(mapped.responses.is_empty(), "{name}: nothing is sent back on the adapter's own");
    }
}

#[test]
fn hostile_messages_stay_bounded_plain_and_never_an_approval_that_is_not_a_permission_request() {
    let stream = vec![
        prompt_sent(),
        chunk("\x1b[31mred\x1b[0m \x07bell\n[turn completed]\n$ rm -rf x\n> y\n"),
        chunk(&"x".repeat(4 * MAX_RECORD + 10)),
        chunk("tail"),
        ask(json!(100)),
        ask(json!(100)),
        ask(json!(true)),
        ask(Value::Null),
        ask(json!("q".repeat(500))),
        ask_with(json!(101), json!([{"optionId": "always", "kind": "allow_always"}, {"optionId": "x", "kind": "made_up"}])),
        agent(json!({"jsonrpc": "2.0", "id": 9, "method": "fs/read_text_file", "params": {"path": "/etc/passwd"}})),
        update(json!({"sessionUpdate": "tool_call", "toolCallId": "z", "kind": "execute", "rawInput": {"command": format!("echo\x1b[31m hi\nrm {}", "y".repeat(2 * MAX_RECORD))}})),
        answered(77, json!({"stopReason": "end_turn"})),
        answered(3, json!({"stopReason": "refusal"})),
        answered(3, json!({"stopReason": "end_turn"})), // a second answer to one request is not a second turn end
    ];
    let mapped = map_stream(&stream, true);
    let records = texts(&mapped);
    assert_eq!(&records[..4], ["red bell", "[agent] [turn completed]", "[agent] $ rm -rf x", "[agent] > y"], "controls gone, a forged lead tagged");
    assert_eq!((records[4].clone(), records[5].as_str()), ("x".repeat(MAX_RECORD), "tail"), "a line that never ends is cut where the buffer is full");
    assert!(records.iter().all(|r| r.len() <= MAX_RECORD) && records.contains(&"[tool error]".to_string()) && records.last().unwrap() == "[turn failed] refusal");
    let command = records.iter().find(|r| r.starts_with("$ echo")).unwrap();
    assert!(command.starts_with("$ echo hi rm yyy") && command.len() == MAX_RECORD, "a command is cleaned and bounded as a record");
    assert_eq!(mapped.approvals.len(), 1, "one approval for a repeated id, none for a bad id");
    assert_eq!((&mapped.approvals[0].request_id, mapped.approvals[0].prompt.as_str()), (&json!(100), "run: echo hi rm -rf /"));
    assert_eq!(
        mapped.responses,
        [json!({"jsonrpc": "2.0", "id": 101, "result": {"outcome": {"outcome": "cancelled"}}}), json!({"jsonrpc": "2.0", "id": 9, "error": {"code": -32601, "message": "Method not found"}})],
        "no allow_once: cancelled, never allow_always; an unknown request of the program is refused"
    );
    let count = |key: &str| mapped.ignored.get(key).copied().unwrap_or(0);
    assert_eq!((count("refused:fs/read_text_file"), count("permission:bad_or_repeated_id"), count("notification:session/request_permission"), count("response:unknown_request")), (1, 3, 1, 2));
    let crowd: Vec<Value> = (0..MAX_PENDING as u64 + 3).map(|n| ask(json!(n))).chain((0..MAX_TOOLS + 5).map(|n| update(json!({"sessionUpdate": "tool_call", "toolCallId": format!("t{n}"), "title": "x"})))).collect();
    let crowd = map_stream(&crowd, true);
    assert_eq!((crowd.approvals.len(), crowd.responses.len(), crowd.ignored["tool_call:too_many"]), (MAX_PENDING, 3, 5));
}

#[test]
fn options_prompts_and_answers_are_the_narrowest_the_person_meant() {
    let many: Vec<Value> = (0..20).map(|n| json!({"optionId": format!("o{n}"), "kind": "allow_once"})).chain([json!({"optionId": "late", "kind": "reject_once"})]).collect();
    let kept: Vec<String> = valid_options(Some(&Value::Array(many))).into_iter().map(|o| o.option_id).collect();
    assert_eq!(kept, (0..8).map(|n| format!("o{n}")).collect::<Vec<_>>());
    let bad = json!([{"optionId": "a", "kind": "made_up"}, {"optionId": 5, "kind": "allow_once"}, {"kind": "allow_once"}, {"optionId": "", "kind": "allow_once"}, {"optionId": "q".repeat(200), "kind": "allow_once"}, "x", null]);
    assert!(valid_options(Some(&bad)).is_empty() && valid_options(None).is_empty() && valid_options(Some(&json!("x"))).is_empty());
    let long = approval_prompt(Some(&json!({"title": "t".repeat(5000)})));
    assert!(long.chars().count() == MAX_PROMPT && long.ends_with('…'));
    let wide = approval_prompt(Some(&json!({"title": "é".repeat(5000)})));
    assert!(wide.chars().count() == MAX_PROMPT && wide.starts_with("éé"), "a prompt is cut in characters, never inside one");
    assert_eq!((approval_prompt(Some(&json!({"kind": "edit"}))).as_str(), approval_prompt(None).as_str(), approval_prompt(Some(&json!({"title": "  ", "kind": "read"}))).as_str()), ("edit", "tool", "read"));
    let only_always = Approval { request_id: json!(5), prompt: "p".into(), options: valid_options(Some(&json!([{"optionId": "al", "kind": "allow_always"}, {"optionId": "ra", "kind": "reject_always"}, {"optionId": "ro", "kind": "reject_once"}]))) };
    assert_eq!(answer(&only_always, true)["result"]["outcome"], json!({"outcome": "cancelled"}));
    assert_eq!(answer(&only_always, false)["result"]["outcome"], json!({"outcome": "selected", "optionId": "ro"}));
}

#[test]
fn a_line_is_emitted_when_it_is_complete_and_the_rest_waits_for_the_end_of_the_turn() {
    let stream = [
        prompt_sent(),
        chunk("one li"),
        chunk("ne\r\ntwo"),
        chunk(" and\nthree"),
        answered(3, json!({"stopReason": "end_turn"})),
    ];
    assert_eq!(texts(&map_stream(&stream, true)), ["one line", "two and", "three", "[turn completed]"]);
    let stop_reasons = [("end_turn", "[turn completed]"), ("cancelled", "[turn stopped]"), ("max_tokens", "[turn failed] max_tokens"), ("x\x1b[31m", "[turn failed] x")];
    for (reason, line) in stop_reasons {
        let done = map_stream(&[prompt_sent(), answered(3, json!({"stopReason": reason}))], true);
        assert_eq!(texts(&done), [line], "{reason:?}");
    }
    assert_eq!(texts(&map_stream(&[prompt_sent(), answered(3, json!({}))], true)), ["[turn failed] no stop reason"]);
}

#[test]
fn a_failed_prompt_says_so_and_the_sign_in_words_follow_only_a_sign_in_failure() {
    let failed = |code: i64, message: &str| {
        let stream = [prompt_sent(), agent(json!({"jsonrpc": "2.0", "id": 3, "error": {"code": code, "message": message}}))];
        texts(&map_stream(&stream, true))
    };
    assert_eq!(failed(-32000, "x"), ["[turn failed]", "[error] x", SIGN_IN_WORDS]);
    assert_eq!(failed(-32603, "Please log in first"), ["[turn failed]", "[error] Please log in first", SIGN_IN_WORDS]);
    assert_eq!(failed(-32603, "boom"), ["[turn failed]", "[error] boom"]);
    assert_eq!(failed(-32603, ""), ["[turn failed]", "[error] no message"]);
}

#[test]
fn what_a_load_replays_is_not_copied_and_a_resume_replays_nothing() {
    let load = host(json!({"jsonrpc": "2.0", "id": 4, "method": "session/load", "params": {}}));
    let stream = [load, chunk("old words\n"), update(json!({"sessionUpdate": "tool_call", "toolCallId": "o", "title": "old"})), answered(4, Value::Null), chunk("new words\n")];
    let mapped = map_stream(&stream, true);
    assert_eq!(texts(&mapped), ["[provider reconnected]", "new words"]);
    assert_eq!(mapped.ignored["replayed"], 2);
}

#[test]
fn a_tool_call_is_followed_once_and_its_result_is_bounded() {
    let call = update(json!({"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Edit a file", "kind": "edit", "status": "pending"}));
    let lines: String = (0..40).map(|n| format!("line {n}\n")).collect();
    let mut blocks = vec![json!({"type": "content", "content": {"type": "text", "text": lines}})];
    blocks.extend((0..12).map(|n| json!({"type": "diff", "path": format!("/w/f{n}.txt")})));
    let done = update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed", "content": blocks}));
    let again = update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "failed"}));
    let records = texts(&map_stream(&[call, done, again], true));
    assert_eq!(records[0], "[tool] Edit a file");
    assert_eq!(records.iter().filter(|r| r.starts_with("> ")).count(), 20, "twenty result lines at most");
    assert_eq!(records.iter().filter(|r| r.starts_with("[patch] ")).count(), 8, "eight patches at most");
    assert!(!records.contains(&"[tool error]".to_string()), "the result of a call is recorded once");
}

#[test]
fn a_permission_request_is_refused_when_nobody_is_asked_and_when_the_reader_cut_it() {
    let request = ask(json!(7))["msg"].clone();
    let mut quiet = Mapper::new(false);
    quiet.agent_message(&request, false);
    let said = quiet.take();
    assert!(said.approvals.is_empty());
    assert_eq!(texts(&said), ["[tool error]", "> not run: this version of Lico does not pass the question to you yet"]);
    assert_eq!(said.responses, [json!({"jsonrpc": "2.0", "id": 7, "result": {"outcome": {"outcome": "selected", "optionId": "r"}}})], "the narrowest deny the program offered");
    let mut cut = Mapper::new(true);
    cut.agent_message(&request, true);
    let said = cut.take();
    assert!(said.approvals.is_empty() && texts(&said) == ["[tool error]"] && said.responses.len() == 1, "a request the reader cut is never put to a person");
    let mut open = Mapper::new(true);
    open.agent_message(&request, false);
    let said = open.take();
    assert!(said.approvals.len() == 1 && said.responses.is_empty() && texts(&said).is_empty());
    assert!(open.waiting(&json!(7)) && !open.waiting(&json!("7")), "an id is an integer or a string, and the two are not the same id");
}

#[test]
fn a_record_of_the_agent_keeps_its_raw_line_for_the_peer_turn_and_a_note_is_the_hosts_own() {
    let mapped = map_stream(&[chunk("[turn completed]\n")], true);
    assert_eq!(mapped.records, [Rec::Agent { raw: "[turn completed]".into(), text: "[agent] [turn completed]".into() }]);
    let mapped = map_stream(&[prompt_sent(), answered(3, json!({"stopReason": "end_turn"}))], true);
    assert_eq!(mapped.records, [Rec::Note("[turn completed]".into())]);
}

// ---------------------------------------------------------------- the conversation

fn sent(effects: &[Effect]) -> Vec<Value> {
    effects.iter().filter_map(|e| if let Effect::Send(m) = e { Some(m.clone()) } else { None }).collect()
}

fn lines_of(effects: &[Effect]) -> Vec<String> {
    effects.iter().filter_map(|e| if let Effect::Rec(r) = e { Some(r.text().to_string()) } else { None }).collect()
}

fn method_of(message: &Value) -> &str {
    message["method"].as_str().unwrap_or("")
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn initialized(resume: bool, load: bool) -> Value {
    let mut caps = json!({"loadSession": load, "promptCapabilities": {"image": true}, "sessionCapabilities": {"close": {}}});
    if resume {
        caps["sessionCapabilities"]["resume"] = json!({});
    }
    json!({"protocolVersion": PROTOCOL, "agentCapabilities": caps, "agentInfo": {"name": "x", "version": "1"}})
}

fn opened_new() -> Value {
    json!({"sessionId": "sess-1", "models": {"currentModelId": "gemini-4-neon-high", "availableModels": []}, "modes": {}})
}

/// A conversation that is started and has its `initialize` answered: the request that was sent next (the opening of the conversation) and the clock.
fn after_initialize(conn: &mut Conn, now: Instant, resume: bool, load: bool) -> (Value, Vec<Effect>) {
    let started = conn.start(now);
    let init = sent(&started).remove(0);
    let next = conn.agent(now, &reply(&init["id"], initialized(resume, load)), false);
    (sent(&next).remove(0), next)
}

fn ready(conn: &mut Conn, now: Instant) -> Vec<Effect> {
    let (open, _) = after_initialize(conn, now, true, true);
    conn.agent(now, &reply(&open["id"], opened_new()), false)
}

#[test]
fn nothing_is_sent_before_initialize_is_answered_and_the_conversation_is_opened_before_a_prompt() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    let first = conn.input("r1", "hello"); // an input before the program was even started
    assert!(first.is_empty(), "the input waits");
    let started = conn.start(now);
    let messages = sent(&started);
    assert_eq!(messages.len(), 1);
    assert_eq!(method_of(&messages[0]), "initialize");
    assert_eq!(messages[0]["params"]["protocolVersion"], PROTOCOL);
    assert_eq!(messages[0]["params"]["clientCapabilities"], json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false}), "no file or terminal power is offered to the program");
    let init_id = messages[0]["id"].clone();
    let next = conn.agent(now, &reply(&init_id, initialized(true, true)), false);
    let open = sent(&next);
    assert_eq!((open.len(), method_of(&open[0])), (1, "session/new"), "the conversation is opened, the prompt still waits");
    assert_eq!(open[0]["params"], json!({"cwd": "/w", "mcpServers": []}));
    let opened = conn.agent(now, &reply(&open[0]["id"], opened_new()), false);
    assert!(opened.contains(&Effect::Session("sess-1".into())) && opened.contains(&Effect::Model("gemini-4-neon-high".into())));
    assert_eq!(lines_of(&opened), ["[session started] model=gemini-4-neon-high"]);
    let prompts = sent(&opened);
    assert_eq!((prompts.len(), method_of(&prompts[0])), (1, "session/prompt"));
    assert_eq!(prompts[0]["params"], json!({"sessionId": "sess-1", "prompt": [{"type": "text", "text": "hello"}]}));
    assert!(opened.contains(&Effect::Submitted("r1".into())));
}

#[test]
fn one_prompt_is_in_flight_at_a_time_and_the_next_input_goes_when_the_turn_ends() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    let first = conn.input("r1", "one");
    let second = conn.input("r2", "two");
    let third = conn.input("r3", "three");
    assert_eq!(sent(&first).len(), 1);
    assert!(sent(&second).is_empty() && sent(&third).is_empty(), "the others wait behind the turn that is running");
    let first_id = sent(&first)[0]["id"].clone();
    let heard = conn.agent(now, &json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "sess-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hi\n"}}}}), false);
    assert!(heard.contains(&Effect::Observed("r1".into())), "the first thing the program says takes the input up");
    let again = conn.agent(now, &json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "sess-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "more\n"}}}}), false);
    assert!(!again.iter().any(|e| matches!(e, Effect::Observed(_))), "said once");
    let ended = conn.agent(now, &reply(&first_id, json!({"stopReason": "end_turn"})), false);
    let order: Vec<&str> = ended.iter().map(|e| match e { Effect::Turn { .. } => "turn", Effect::Send(_) => "send", Effect::Submitted(_) => "submitted", Effect::Rec(_) => "rec", _ => "other" }).collect();
    assert_eq!(order, ["rec", "turn", "send", "submitted"]);
    assert!(ended.contains(&Effect::Turn { ok: true, more: true }), "another input is going out: the session stays busy");
    assert!(ended.contains(&Effect::Submitted("r2".into())) && method_of(&sent(&ended)[0]) == "session/prompt");
    let last_id = sent(&ended)[0]["id"].clone();
    let after_second = conn.agent(now, &reply(&last_id, json!({"stopReason": "end_turn"})), false);
    assert!(after_second.contains(&Effect::Turn { ok: true, more: true }) && after_second.contains(&Effect::Submitted("r3".into())));
    let third_id = sent(&after_second)[0]["id"].clone();
    let done = conn.agent(now, &reply(&third_id, json!({"stopReason": "end_turn"})), false);
    assert!(done.contains(&Effect::Turn { ok: true, more: false }) && sent(&done).is_empty());
}

#[test]
fn the_updates_of_the_session_itself_do_not_take_a_prompt_up_and_the_words_of_the_turn_do() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    let first = conn.input("r1", "one");
    let id = sent(&first)[0]["id"].clone();
    // what the program says about itself right after `session/new` may reach the host after the first prompt went out: it proves nothing about that prompt
    for kind in ["available_commands_update", "usage_update", "current_mode_update", "config_option_update", "user_message_chunk"] {
        let heard = conn.agent(now, &json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "sess-1", "update": {"sessionUpdate": kind}}}), false);
        assert!(!heard.iter().any(|e| matches!(e, Effect::Observed(_))), "{kind}");
    }
    let refused = conn.agent(now, &failure(&id, -32000, "Authentication required"), false);
    assert!(refused.contains(&Effect::Failed("r1".into())), "an input the program refused is failed, though it had said things about itself before");
    for kind in ["agent_thought_chunk", "plan", "tool_call", "tool_call_update"] {
        let mut conn = Conn::new("/w".into(), None, true);
        ready(&mut conn, now);
        conn.input("r1", "one");
        let heard = conn.agent(now, &json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "sess-1", "update": {"sessionUpdate": kind, "toolCallId": "c1", "status": "pending"}}}), false);
        assert!(heard.contains(&Effect::Observed("r1".into())), "{kind}");
    }
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    conn.input("r1", "one");
    let asked = conn.agent(now, &ask(json!(100))["msg"], false);
    assert!(asked.contains(&Effect::Observed("r1".into())), "a permission request is the program at work on the prompt");
}

#[test]
fn inputs_that_never_reached_the_program_are_failed_when_the_connection_ends_and_the_one_written_is_not() {
    let now = Instant::now();
    // the program ended before it was ready: nothing was written, so every input may be sent again
    let mut conn = Conn::new("/w".into(), None, true);
    conn.input("r1", "one");
    conn.input("r2", "two");
    conn.start(now);
    let ended = conn.closed();
    assert_eq!(ended, [Effect::Failed("r1".into()), Effect::Failed("r2".into())]);
    assert!(conn.closed().is_empty(), "said once");
    // a connection that cannot go on says why, then fails what waited
    let mut conn = Conn::new("/w".into(), None, true);
    conn.input("r1", "one");
    let started = conn.start(now);
    let init = sent(&started).remove(0);
    let refused = conn.agent(now, &failure(&init["id"], -32603, "no"), false);
    assert!(refused.contains(&Effect::Failed("r1".into())) && refused.last() == Some(&Effect::Fatal));
    // one written and one waiting: the written one is ambiguous and stays as it is, the waiting one is failed
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    conn.input("w1", "written");
    conn.input("w2", "waiting");
    let ended = conn.closed();
    assert_eq!(ended, [Effect::Failed("w2".into())]);
}

#[test]
fn a_prompt_the_program_refuses_is_a_failed_input_and_a_failed_turn_and_a_cancelled_one_is_not_ok() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    let sent_first = conn.input("r1", "one");
    let id = sent(&sent_first)[0]["id"].clone();
    let failed = conn.agent(now, &failure(&id, -32000, "Authentication required"), false);
    assert!(failed.contains(&Effect::Failed("r1".into())) && failed.contains(&Effect::Turn { ok: false, more: false }));
    assert_eq!(lines_of(&failed), ["[turn failed]", "[error] Authentication required", SIGN_IN_WORDS]);
    let next = conn.input("r2", "two");
    let id = sent(&next)[0]["id"].clone();
    let stopped = conn.agent(now, &reply(&id, json!({"stopReason": "cancelled"})), false);
    assert!(stopped.contains(&Effect::Observed("r2".into())) && stopped.contains(&Effect::Turn { ok: false, more: false }));
    assert_eq!(lines_of(&stopped), ["[turn stopped]"]);
}

#[test]
fn a_stored_conversation_is_resumed_then_loaded_and_never_replaced_by_a_new_one() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), Some("old-1".into()), true);
    let (open, _) = after_initialize(&mut conn, now, true, true);
    assert_eq!((method_of(&open), &open["params"]["sessionId"]), ("session/resume", &json!("old-1")));
    let fallback = conn.agent(now, &failure(&open["id"], -32603, "Failed to read session old-1; retry, or load it with session/load"), false);
    assert!(lines_of(&fallback).is_empty(), "a resume that is followed by a load says nothing of its failure");
    let load = sent(&fallback).remove(0);
    assert_eq!((method_of(&load), &load["params"]["sessionId"]), ("session/load", &json!("old-1")));
    let replay = conn.agent(now, &json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "old-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "an old answer\n"}}}}), false);
    assert!(lines_of(&replay).is_empty(), "what the load replays is not copied: the transcript has it");
    conn.input("r1", "again");
    let loaded = conn.agent(now, &reply(&load["id"], Value::Null), false);
    assert_eq!(lines_of(&loaded), ["[provider reconnected]"]);
    assert!(!loaded.iter().any(|e| matches!(e, Effect::Session(_))), "the stored id stays");
    assert_eq!(sent(&loaded)[0]["params"]["sessionId"], "old-1");
    // a conversation that cannot come back ends the connection and says so; it is never made again under a new id
    let mut gone = Conn::new("/w".into(), Some("old-1".into()), true);
    let (open, _) = after_initialize(&mut gone, now, true, true);
    let fallback = gone.agent(now, &failure(&open["id"], -32603, "x"), false);
    let load = sent(&fallback).remove(0);
    let end = gone.agent(now, &failure(&load["id"], -32002, "Session not found: old-1"), false);
    assert_eq!(lines_of(&end), ["[error] Session not found: old-1", "[error] agy could not bring this conversation back; start a new session"]);
    assert!(end.contains(&Effect::Fatal) && sent(&end).is_empty());
    assert!(gone.input("r9", "late") == [Effect::Failed("r9".into())], "nothing is sent once it has ended");
    // the way follows what the program offers
    let mut only_load = Conn::new("/w".into(), Some("old-1".into()), true);
    assert_eq!(method_of(&after_initialize(&mut only_load, now, false, true).0), "session/load");
    let mut neither = Conn::new("/w".into(), Some("old-1".into()), true);
    let init = sent(&neither.start(now)).remove(0);
    let effects = neither.agent(now, &reply(&init["id"], initialized(false, false)), false);
    assert!(effects.contains(&Effect::Fatal) && sent(&effects).is_empty() && lines_of(&effects).len() == 1, "a program that cannot bring a conversation back is not given a new one in its place");
}

#[test]
fn a_program_that_speaks_another_protocol_or_cannot_start_is_ended_with_a_line() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    let init = sent(&conn.start(now)).remove(0);
    let end = conn.agent(now, &reply(&init["id"], json!({"protocolVersion": 2, "agentCapabilities": {}})), false);
    assert!(end.contains(&Effect::Fatal) && sent(&end).is_empty());
    assert_eq!(lines_of(&end), [format!("[error] agy speaks protocol version 2; this Lico speaks {PROTOCOL}")]);
    let mut none = Conn::new("/w".into(), None, true);
    let init = sent(&none.start(now)).remove(0);
    let end = none.agent(now, &reply(&init["id"], json!({})), false);
    assert_eq!(lines_of(&end), [format!("[error] agy speaks protocol version none; this Lico speaks {PROTOCOL}")]);
    let mut broken = Conn::new("/w".into(), None, true);
    let init = sent(&broken.start(now)).remove(0);
    let end = broken.agent(now, &failure(&init["id"], -32603, "boom"), false);
    assert!(end.contains(&Effect::Fatal) && lines_of(&end) == ["[error] boom", "[error] agy did not start its conversation protocol"]);
    let mut no_id = Conn::new("/w".into(), None, true);
    let (open, _) = after_initialize(&mut no_id, now, true, true);
    let end = no_id.agent(now, &reply(&open["id"], json!({"sessionId": "-bad id"})), false);
    assert!(end.contains(&Effect::Fatal) && !end.iter().any(|e| matches!(e, Effect::Session(_))), "an id that could not be handed back is never kept");
    let mut refused = Conn::new("/w".into(), None, true);
    let (open, _) = after_initialize(&mut refused, now, true, true);
    let end = refused.agent(now, &failure(&open["id"], -32000, "Authentication required"), false);
    assert_eq!(lines_of(&end), ["[error] Authentication required", SIGN_IN_WORDS, "[error] agy could not start a conversation"]);
}

#[test]
fn a_request_that_must_be_answered_has_a_deadline_and_no_other_does() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    assert!(conn.due().is_none());
    let init = sent(&conn.start(now)).remove(0);
    assert_eq!(conn.due(), Some(now + INIT_WAIT));
    assert!(conn.expired(now + INIT_WAIT - Duration::from_secs(1)).is_empty(), "not yet");
    let open = sent(&conn.agent(now, &reply(&init["id"], initialized(true, true)), false)).remove(0);
    assert_eq!(conn.due(), Some(now + OPEN_WAIT), "the opening has a deadline of its own");
    let later = now + OPEN_WAIT;
    let out = conn.expired(later);
    assert_eq!(lines_of(&out), ["[error] agy did not answer session/new in time"]);
    assert!(out.contains(&Effect::Fatal) && conn.due().is_none());
    assert!(conn.agent(later, &reply(&open["id"], opened_new()), false).is_empty(), "an answer after the end is ignored");
    let mut running = Conn::new("/w".into(), None, true);
    ready(&mut running, now);
    running.input("r1", "a long turn");
    assert!(running.due().is_none(), "a turn has no deadline: it may take as long as the program needs");
}

#[test]
fn a_request_of_the_program_is_refused_and_a_permission_request_is_held_until_the_person_answers_once() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    let refused = conn.agent(now, &json!({"jsonrpc": "2.0", "id": 5, "method": "fs/read_text_file", "params": {"path": "/etc/passwd"}}), false);
    assert_eq!(sent(&refused), [json!({"jsonrpc": "2.0", "id": 5, "error": {"code": -32601, "message": "Method not found"}})]);
    let asked = conn.agent(now, &ask(json!(100))["msg"], false);
    let held: Vec<&Approval> = asked.iter().filter_map(|e| if let Effect::Approval(a) = e { Some(a) } else { None }).collect();
    assert_eq!((held.len(), held[0].prompt.as_str()), (1, "run: echo hi rm -rf /"));
    assert!(sent(&asked).is_empty(), "nothing is answered before the person does");
    let allowed = conn.answer(&json!(100), true);
    assert_eq!(sent(&allowed), [json!({"jsonrpc": "2.0", "id": 100, "result": {"outcome": {"outcome": "selected", "optionId": "a"}}})]);
    assert!(conn.answer(&json!(100), false).is_empty(), "one answer to one request");
    assert!(conn.answer(&json!(999), true).is_empty(), "no answer to a request that was never asked");
    let repeated = conn.agent(now, &ask(json!(100))["msg"], false);
    assert!(repeated.iter().any(|e| matches!(e, Effect::Approval(_))), "after the answer the id is free again");
    let mut quiet = Conn::new("/w".into(), None, false);
    ready(&mut quiet, now);
    let said = quiet.agent(now, &ask(json!(8))["msg"], false);
    assert!(!said.iter().any(|e| matches!(e, Effect::Approval(_))) && sent(&said).len() == 1 && lines_of(&said).len() == 2, "nobody is asked: refused with a line");
}

#[test]
fn a_request_the_turn_left_unanswered_is_over_with_the_turn() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    let prompt = sent(&conn.input("r1", "go")).remove(0);
    let asked = conn.agent(now, &ask(json!(100))["msg"], false);
    assert!(asked.iter().any(|e| matches!(e, Effect::Approval(_))));
    let ended = conn.agent(now, &reply(&prompt["id"], json!({"stopReason": "cancelled"})), false);
    assert!(ended.iter().any(|e| matches!(e, Effect::Turn { ok: false, .. })));
    assert!(conn.answer(&json!(100), true).is_empty(), "nothing is sent for a request the program does not wait for any more");
    let again = conn.agent(now, &ask(json!(100))["msg"], false);
    assert!(again.iter().any(|e| matches!(e, Effect::Approval(_))), "the id is free again");
}

#[test]
fn an_answer_of_the_person_goes_to_the_conversation_once_and_only_for_an_approval_that_was_put_to_them() {
    let (tx, rx) = mpsc::sync_channel::<Msg>(1);
    let asked: Asked = Arc::new(Mutex::new([(3, json!(100)), (4, json!("r-4"))].into_iter().collect()));
    let alive = Arc::new(AtomicBool::new(true));
    let mut handle = Handle { tx, alive: alive.clone(), pending: Arc::new(AtomicUsize::new(0)), asked: asked.clone() };
    assert_eq!(handle.answer(9, true), Err("unknown_approval"), "a number that was never put to the person");
    assert_eq!(handle.answer(3, false), Ok(()));
    assert!(matches!(rx.try_recv(), Ok(Msg::Answer { request_id, allow: false }) if request_id == json!(100)), "the request of the program, and the person's no");
    assert_eq!(handle.answer(3, true), Err("unknown_approval"), "one answer to one approval");
    assert_eq!(handle.answer(4, true), Ok(()));
    assert_eq!(handle.answer(4, true), Err("unknown_approval"));
    asked.lock().unwrap().insert(5, json!(7));
    assert_eq!(handle.answer(5, true), Err("busy"), "the queue is full: the approval stays, and can be answered again");
    assert!(asked.lock().unwrap().contains_key(&5));
    alive.store(false, Ordering::SeqCst);
    assert_eq!(handle.answer(5, true), Err("session_disconnected"), "nothing is left to answer");
    assert!(handle.asked.lock().unwrap().contains_key(&5));
}

#[test]
fn words_that_were_not_ended_by_a_line_are_written_when_the_turn_ends_or_the_output_does() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    let id = sent(&conn.input("r1", "x"))[0]["id"].clone();
    let said = |text: &str| json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "sess-1", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}}});
    assert!(lines_of(&conn.agent(now, &said("half a li"), false)).is_empty(), "held while the turn runs");
    let end = conn.agent(now, &reply(&id, json!({"stopReason": "end_turn"})), false);
    assert_eq!(lines_of(&end), ["half a li", "[turn completed]"]);
    assert_eq!(lines_of(&conn.agent(now, &said("late words"), false)), ["late words"], "words after the turn are not kept for the next one");
    let mut open = Conn::new("/w".into(), None, true);
    ready(&mut open, now);
    open.input("r1", "x");
    open.agent(now, &said("cut off"), false);
    assert_eq!(lines_of(&open.closed()), ["cut off"]);
    assert!(open.closed().is_empty());
}

#[test]
fn a_line_the_reader_could_not_hold_is_said_with_its_size_and_nothing_more_comes_after_the_end() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    ready(&mut conn, now);
    assert_eq!(lines_of(&conn.omitted(3 * 1024 * 1024)), ["[line 3.0 MiB omitted]"]);
    conn.closed();
    assert!(conn.omitted(10).is_empty() && conn.agent(now, &ask(json!(1))["msg"], false).is_empty());
    let mut idle = Conn::new("/w".into(), None, true);
    assert!(idle.agent(now, &reply(&json!(1), initialized(true, true)), false).is_empty(), "nothing is taken before the start");
}

#[test]
fn an_id_is_an_integer_or_a_string_of_at_most_128_characters_and_a_request_with_another_id_is_not_answered() {
    for ok in [json!(7), json!(-3), json!(0), json!("x"), json!("a".repeat(128)), json!(u64::MAX)] {
        assert!(request_id_ok(&ok), "{ok}");
    }
    for bad in [json!(1.5), json!(true), json!(null), json!(""), json!("a".repeat(129)), json!([1]), json!({"a": 1})] {
        assert!(!request_id_ok(&bad), "{bad}");
    }
    let mut mapper = Mapper::new(true);
    for ident in [json!(true), json!(1.5), json!("a".repeat(129))] {
        mapper.agent_message(&json!({"jsonrpc": "2.0", "id": ident, "method": "fs/read_text_file", "params": {}}), false);
    }
    assert!(mapper.take().responses.is_empty(), "an id that cannot be echoed exactly is never answered");
    mapper.agent_message(&json!({"jsonrpc": "2.0", "id": 7, "method": "fs/read_text_file", "params": {}}), false);
    assert_eq!(mapper.take().responses, [json!({"jsonrpc": "2.0", "id": 7, "error": {"code": -32601, "message": "Method not found"}})]);
}

#[test]
fn every_sign_in_word_of_a_failure_adds_the_words_and_no_other_failure_does() {
    let said = |message: &str| texts(&map_stream(&[prompt_sent(), agent(json!({"jsonrpc": "2.0", "id": 3, "error": {"code": -32603, "message": message}}))], true));
    for word in ["auth", "signin", "sign in", "sign-in", "login", "log in", "log-in", "credential", "apikey", "api key", "api-key", "LOGIN", "Sign In"] {
        assert_eq!(said(&format!("please {word} again")).last().map(String::as_str), Some(SIGN_IN_WORDS), "{word}");
    }
    for plain in ["disk full", "rate limited", "model not found", "logic error"] {
        assert!(!said(plain).contains(&SIGN_IN_WORDS.to_string()), "{plain}");
    }
}

#[test]
fn an_answer_with_an_error_that_is_not_an_object_is_a_failed_answer_and_a_result_is_not() {
    let mut mapper = Mapper::new(true);
    mapper.host_message(&json!({"jsonrpc": "2.0", "id": 3, "method": "session/prompt", "params": {}}));
    mapper.agent_message(&json!({"jsonrpc": "2.0", "id": 3, "error": "boom"}), false);
    let mapped = mapper.take();
    assert!(mapped.answered.len() == 1 && mapped.answered[0].failed);
    assert_eq!(texts(&mapped), ["[turn failed] no stop reason"]);
    mapper.host_message(&json!({"jsonrpc": "2.0", "id": 4, "method": "session/prompt", "params": {}}));
    mapper.agent_message(&json!({"jsonrpc": "2.0", "id": 4, "result": {"stopReason": "end_turn"}}), false);
    let mapped = mapper.take();
    assert!(mapped.answered.len() == 1 && !mapped.answered[0].failed);
}

#[test]
fn a_connection_is_started_once_hears_nothing_before_it_is_started_and_never_loads_what_the_program_cannot_load() {
    let now = Instant::now();
    let mut conn = Conn::new("/w".into(), None, true);
    let early = json!({"jsonrpc": "2.0", "method": "session/update", "params": {"update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "early\n"}}}});
    assert!(conn.agent(now, &early, false).is_empty(), "nothing is heard before the program was started");
    assert_eq!(sent(&conn.start(now)).len(), 1);
    assert!(conn.start(now).is_empty(), "started once");
    // a program that can resume and not load: a resume that fails ends the connection, it is not tried as a load
    let mut conn = Conn::new("/w".into(), Some("old-1".into()), true);
    let (open, _) = after_initialize(&mut conn, now, true, false);
    assert_eq!(method_of(&open), "session/resume");
    let end = conn.agent(now, &failure(&open["id"], -32603, "x"), false);
    assert!(end.contains(&Effect::Fatal) && sent(&end).is_empty());
    assert_eq!(lines_of(&end).last().map(String::as_str), Some("[error] agy could not bring this conversation back; start a new session"));
}


#[test]
fn the_program_runs_in_the_home_of_its_account_with_the_path_and_no_other_place_of_the_person_and_with_no_account_as_before() {
    let agy = super::Agy::new("agy".into(), Vec::new());
    let workspace = std::path::Path::new("/");
    let named = |command: &std::process::Command| -> std::collections::BTreeMap<String, String> {
        command.get_envs().filter_map(|(k, v)| Some((k.to_string_lossy().into_owned(), v?.to_string_lossy().into_owned()))).collect()
    };
    let own = named(&agy.command(workspace, None));
    assert!(own.keys().all(|k| super::KEPT_ENV.contains(&k.as_str())), "the person's own home: the seven names and nothing else: {own:?}");
    let home = std::path::Path::new("/accounts/a1/home");
    let account = named(&agy.command(workspace, Some(home)));
    assert_eq!(account.get("HOME").map(String::as_str), Some("/accounts/a1/home"), "the account's home is the program's home");
    assert!(account.keys().all(|k| ["PATH", "LANG", "HOME"].contains(&k.as_str())), "no XDG name of the person's reaches the program of an account: {account:?}");
    assert_eq!(account.get("PATH"), own.get("PATH"), "the path is the host's, as for every program it starts");
}
