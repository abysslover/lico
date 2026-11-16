//! The adapter of the session kind `local` against the fake server of `lico-llm-fake`, in one process with a host built by hand: a private state folder, no real model, no
//! keyring (a store in memory), no billable turn. The loop itself is proved on its own in `lico-toolloop`; these tests prove the wiring.

use super::*;
use crate::folders::Settings;
use crate::session::Origin;
use lico_llm_fake::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::RwLock;
use std::time::Duration;

#[derive(Default)]
struct MemKeys(Mutex<HashMap<SocketAddr, String>>);

impl Keys for MemKeys {
    fn get(&self, addr: SocketAddr) -> Option<String> {
        self.0.lock().unwrap().get(&addr).cloned()
    }
}

struct World {
    shared: Arc<Shared>,
    kind: &'static str,
    keys: Arc<MemKeys>,
    dir: PathBuf,
}

fn world(name: &str) -> World {
    let dir = std::env::temp_dir().join(format!("lico_local_{name}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(dir.join("log")).unwrap();
    std::fs::create_dir_all(dir.join("accounts")).unwrap();
    let cfg = crate::core::Config {
        login_bin: None, login_args: Vec::new(), login_timeout: 60, log_dir: dir.join("log"), agent: PathBuf::new(),
        retain: 500, window: 20, queue_bytes: 1 << 20, payload: 4096, max_sessions: 8, codex: PathBuf::new(), codex_args: Vec::new(),
        claude: PathBuf::new(), claude_args: Vec::new(), cred_bin: PathBuf::new(), cred_args: Vec::new(), job_bin: PathBuf::new(),
        cred_dir: dir.join("cred"), workspace_root: dir.join("work"), handoff_dir: dir.join("handoff"),
        accounts_dir: dir.join("accounts"), registry_dir: dir.join("registry"), ssh_bin: PathBuf::new(), ssh_config: None, text: true, realistic_text: false,
    };
    let accounts = crate::accounts::Accounts::load(&cfg.accounts_dir).unwrap();
    let shared = Arc::new(Shared {
        cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now(),
        pending_creates: AtomicUsize::new(0), accounts: Mutex::new(accounts), logins: Mutex::new(HashMap::new()), clip: Default::default(), calls: Default::default(),
    });
    let kind: &'static str = Box::leak(format!("local-{name}").into_boxed_str());
    let keys = Arc::new(MemKeys::default());
    crate::driver::register(Arc::new(Local::with_kind(keys.clone(), kind)));
    World { shared, kind, keys, dir }
}

fn settings(addr: SocketAddr, api: crate::driver::Api, context: u32) -> Settings {
    Settings { model: Some("fake".into()), target: Some(Target { addr, api, model: "fake".into(), context_tokens: context }), ..Settings::default() }
}

fn create(w: &World, addr: SocketAddr, api: crate::driver::Api, context: u32) -> (String, i64) {
    let reply = crate::new_session::session_create(&w.shared, w.kind, 0, 0, 4096, None, "auto", settings(addr, api, context)).unwrap();
    (reply["session"].as_str().unwrap().to_string(), reply["generation"].as_i64().unwrap())
}

fn say(w: &World, id: &str, generation: i64, request: &str, text: &str) {
    crate::core::input(&w.shared, id, Some(generation), request, text, Origin::Person, None).unwrap();
}

fn busy(w: &World, id: &str) -> bool {
    crate::core::find(&w.shared, id).unwrap().lock().unwrap().busy
}

/// Waits until the session is idle again (the turn ended), at most 10 seconds.
fn wait_idle(w: &World, id: &str) {
    let end = Instant::now() + Duration::from_secs(10);
    while busy(w, id) {
        assert!(Instant::now() < end, "the turn did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn log(w: &World, id: &str) -> Vec<String> {
    let last = crate::core::find(&w.shared, id).unwrap().lock().unwrap().last_seq();
    let reply = crate::core::history(&w.shared, id, last + 1, 200).unwrap();
    reply["lines"].as_array().unwrap().iter().map(|l| l[1].as_str().unwrap().to_string()).collect()
}

fn record(w: &World, id: &str) -> Vec<Message> {
    Convo::open(&w.shared.cfg.registry_dir.join("local").join(format!("{id}.jsonl"))).unwrap().0.messages().to_vec()
}

fn text(t: &str) -> Piece {
    Piece::Text(t.into())
}

fn sse(pieces: &[Piece]) -> Behaviour {
    Behaviour::Answer(Reply::sse(openai_sse(pieces)))
}

const CHAT: &str = "/v1/chat/completions";

fn has(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

#[test]
fn a_message_streams_the_answer_into_the_log_and_both_sides_into_the_record() {
    let w = world("stream");
    let fake = Fake::start(Script::new().on("POST", CHAT, sse(&[text("Hello "), text("there\nsecond line"), Piece::Finish("stop".into()), Piece::End]))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "hi lico");
    wait_idle(&w, &id);
    let lines = log(&w, &id);
    assert!(has(&lines, "Hello there") && has(&lines, "second line") && has(&lines, "[turn completed]"), "{lines:?}");
    assert_eq!(record(&w, &id), vec![Message::User("hi lico".into()), Message::Assistant { text: "Hello there\nsecond line".into(), calls: vec![] }]);
    let body: Value = serde_json::from_slice(&fake.requests()[0].body).unwrap();
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1], json!({"role": "user", "content": "hi lico"}));
    let names: Vec<&str> = body["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["sessions", "send", "create", "history", "read_handoff", "rename", "capture", "clipboard_read", "computer", "llm_endpoints"], "the host's tools, in its order, without clipboard writing while that is switched off");
    assert_eq!(fake.requests()[0].header("authorization"), None, "no key unless the server asks for one");
}

#[test]
fn a_tool_round_runs_the_hosts_tool_and_the_model_gets_its_result() {
    let w = world("tool");
    let fake = Fake::start(
        Script::new()
            .times(1, "POST", CHAT, sse(&[Piece::Call { id: Some("c1".into()), name: "sessions".into(), args: "{}".into(), split: 2 }, Piece::Finish("tool_calls".into()), Piece::End]))
            .on("POST", CHAT, sse(&[text("two sessions"), Piece::Finish("stop".into()), Piece::End])),
    )
    .unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "what sessions are there");
    wait_idle(&w, &id);
    let lines = log(&w, &id);
    assert!(has(&lines, "[tool sessions] done") && has(&lines, "two sessions") && has(&lines, "[turn completed]"), "{lines:?}");
    let m = record(&w, &id);
    assert_eq!(m.len(), 4);
    assert!(matches!(&m[2], Message::Tool { call_id, name, content } if call_id == "c1" && name == "sessions" && content.contains("sessions")), "{:?}", m[2]);
    let second: Value = serde_json::from_slice(&fake.requests()[1].body).unwrap();
    assert_eq!(second["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect::<Vec<_>>(), ["system", "user", "assistant", "tool"]);
}

#[test]
fn a_tool_that_needs_a_card_ends_the_turn_waiting_and_the_decision_message_starts_the_next() {
    let w = world("card");
    let fake = Fake::start(
        Script::new()
            .times(1, "POST", CHAT, sse(&[Piece::Call { id: Some("c1".into()), name: "llm_endpoints".into(), args: "{}".into(), split: 0 }, Piece::Finish("tool_calls".into()), Piece::End]))
            .on("POST", CHAT, sse(&[text("ok, noted"), Piece::Finish("stop".into()), Piece::End])),
    )
    .unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "look for model servers");
    wait_idle(&w, &id);
    let lines = log(&w, &id);
    assert!(has(&lines, "[tool llm_endpoints] waiting for the person") && has(&lines, "[waiting for the person]"), "{lines:?}");
    assert_eq!(fake.requests().len(), 1, "the model is not asked again while the person decides");
    assert!(matches!(&record(&w, &id)[2], Message::Tool { content, .. } if content.contains("pending")), "the waiting answer is in the record");
    // the host's message about the decision is an input like any other
    crate::core::input(&w.shared, &id, Some(generation), "n1", "[host] the person allowed llm_endpoints", Origin::Host, None).unwrap();
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), "ok, noted") && fake.requests().len() == 2);
}

#[test]
fn a_key_is_looked_for_only_when_the_server_asks_and_is_never_logged_or_recorded() {
    let w = world("key");
    let fake = Fake::start(
        Script::new()
            .times(1, "POST", CHAT, Behaviour::Answer(Reply::json(401, &openai_error("Incorrect API key provided", "invalid_api_key"))))
            .on("POST", CHAT, sse(&[text("welcome"), Piece::Finish("stop".into()), Piece::End])),
    )
    .unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    w.keys.0.lock().unwrap().insert(fake.addr(), "SECRET-KEY-123".into());
    say(&w, &id, generation, "r1", "hello");
    wait_idle(&w, &id);
    let seen = fake.requests();
    assert_eq!((seen[0].authorization(), seen[1].authorization()), (None, Some("Bearer SECRET-KEY-123")));
    let everything = format!("{:?} {:?}", log(&w, &id), record(&w, &id));
    assert!(has(&log(&w, &id), "welcome") && !everything.contains("SECRET-KEY-123"), "the key is nowhere in the log or the record");
    // no key stored: the person is told what to do, and no key is invented
    let fake = Fake::start(Script::new().on("POST", CHAT, Behaviour::Answer(Reply::json(401, &openai_error("no key", "invalid_api_key"))))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "hello");
    wait_idle(&w, &id);
    let lines = log(&w, &id);
    assert!(has(&lines, "[turn failed]") && has(&lines, &format!("lico local-key set {}", fake.addr())), "{lines:?}");
    assert_eq!(record(&w, &id), vec![Message::User("hello".into())]);
}

#[test]
fn a_server_that_is_gone_is_said_so_and_the_next_message_tries_again() {
    let w = world("gone");
    let fake = Fake::start(Script::new().times(1, "POST", CHAT, Behaviour::Close).on("POST", CHAT, sse(&[text("back again"), Piece::Finish("stop".into()), Piece::End]))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "first");
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), "[turn failed]") && has(&log(&w, &id), "the answer was cut"), "{:?}", log(&w, &id));
    say(&w, &id, generation, "r2", "second");
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), "back again"), "the session recovered on the next message");
    // nobody listens at all
    let addr = fake.addr();
    fake.stop();
    say(&w, &id, generation, "r3", "third");
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), &format!("no server at {addr}")), "{:?}", log(&w, &id));
    let state = crate::core::find(&w.shared, &id).unwrap().lock().unwrap().state;
    assert!(state == State::Running, "the session itself stays");
}

#[test]
fn closing_the_session_cancels_a_running_stream_within_about_a_second_and_keeps_the_record() {
    let w = world("stop");
    let fake = Fake::start(Script::new().on("POST", CHAT, Behaviour::Answer(Reply::sse(openai_sse(&[text("first line\n"), text("b"), Piece::End])).ending(Ending::Silent(2))))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "go on");
    let end = Instant::now() + Duration::from_secs(5);
    while fake.requests().is_empty() || !has(&log(&w, &id), "first line") {
        assert!(Instant::now() < end, "the stream did not start");
        std::thread::sleep(Duration::from_millis(20));
    }
    let started = Instant::now();
    crate::lifecycle::session_close_opts(&w.shared, &id, generation, None, false, false).unwrap();
    // the close itself marks the session idle; it is the worker that ends its turn, and it says so, when the cancel reaches the open connection
    while !has(&log(&w, &id), "[turn stopped]") {
        assert!(started.elapsed() < Duration::from_millis(2500), "the worker did not stop: {:?}", log(&w, &id));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(record(&w, &id), vec![Message::User("go on".into())], "the cut answer was not kept");
    assert!(crate::core::find(&w.shared, &id).unwrap().lock().unwrap().state == State::Disconnected);
}

#[test]
fn a_resume_after_a_close_reads_the_record_again_and_the_model_sees_the_earlier_turns() {
    let w = world("resume");
    let fake = Fake::start(Script::new().on("POST", CHAT, sse(&[text("noted"), Piece::Finish("stop".into()), Piece::End]))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "remember the number 42");
    wait_idle(&w, &id);
    crate::lifecycle::session_close_opts(&w.shared, &id, generation, None, false, false).unwrap();
    let resumed = crate::core::session_resume(&w.shared, &id, generation).unwrap();
    let generation = resumed["generation"].as_i64().unwrap();
    say(&w, &id, generation, "r2", "what was the number");
    wait_idle(&w, &id);
    let second: Value = serde_json::from_slice(&fake.requests()[1].body).unwrap();
    let roles: Vec<&str> = second["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["system", "user", "assistant", "user"]);
    assert_eq!(second["messages"][1]["content"], "remember the number 42");
    // a purge removes the record with the session
    let path = w.shared.cfg.registry_dir.join("local").join(format!("{id}.jsonl"));
    assert!(path.exists());
    crate::core::session_purge(&w.shared, &id, generation).unwrap();
    assert!(!path.exists());
}

#[test]
fn a_damaged_record_is_not_opened_and_says_so_and_a_torn_last_line_is_cut() {
    let w = world("damage");
    let fake = Fake::start(Script::new().on("POST", CHAT, sse(&[text("ok"), Piece::Finish("stop".into()), Piece::End]))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "one");
    wait_idle(&w, &id);
    crate::lifecycle::session_close_opts(&w.shared, &id, generation, None, false, false).unwrap();
    let path = w.shared.cfg.registry_dir.join("local").join(format!("{id}.jsonl"));
    let good = std::fs::read(&path).unwrap();
    let mut torn = good.clone();
    torn.extend_from_slice(b"{\"v\":1,\"t\":\"user\",\"te");
    std::fs::write(&path, &torn).unwrap();
    let resumed = crate::core::session_resume(&w.shared, &id, generation).unwrap();
    assert!(has(&log(&w, &id), "unfinished message of the record"), "{:?}", log(&w, &id));
    let generation = resumed["generation"].as_i64().unwrap();
    crate::lifecycle::session_close_opts(&w.shared, &id, generation, None, false, false).unwrap();
    let mut bad = good;
    bad.splice(0..0, b"not json\n".iter().copied());
    std::fs::write(&path, &bad).unwrap();
    assert_eq!(crate::core::session_resume(&w.shared, &id, generation).unwrap_err(), "bad_request");
    assert!(has(&log(&w, &id), "cannot be opened") && has(&log(&w, &id), "line 1 is damaged"), "{:?}", log(&w, &id));
}

#[test]
fn a_server_that_cannot_call_tools_is_asked_again_without_them_and_the_session_says_it_is_text_only() {
    let w = world("chatonly");
    let fake = Fake::start(
        Script::new()
            .times(1, "POST", CHAT, Behaviour::Answer(Reply::json(400, &openai_error("registry.ollama.ai/library/gemma does not support tools", ""))))
            .on("POST", CHAT, sse(&[text("plain answer"), Piece::Finish("stop".into()), Piece::End])),
    )
    .unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "r1", "hello");
    wait_idle(&w, &id);
    let seen = fake.requests();
    assert_eq!(seen.len(), 2);
    let (first, second): (Value, Value) = (serde_json::from_slice(&seen[0].body).unwrap(), serde_json::from_slice(&seen[1].body).unwrap());
    assert!(first.get("tools").is_some() && second.get("tools").is_none());
    assert!(has(&log(&w, &id), "text only") && has(&log(&w, &id), "plain answer"));
    say(&w, &id, generation, "r2", "again");
    wait_idle(&w, &id);
    assert!(serde_json::from_slice::<Value>(&fake.requests()[2].body).unwrap().get("tools").is_none(), "the session stays text only");
}

#[test]
fn an_old_part_of_the_conversation_that_no_longer_fits_the_window_is_left_out_and_said_and_a_message_too_big_for_it_is_refused() {
    let w = world("window");
    let fake = Fake::start(Script::new().on("POST", CHAT, sse(&[text("ok"), Piece::Finish("stop".into()), Piece::End]))).unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 4096);
    for n in 1..=4 {
        say(&w, &id, generation, &format!("r{n}"), &format!("{n}{}", "x".repeat(3000)));
        wait_idle(&w, &id);
    }
    assert!(log(&w, &id).iter().any(|l| l.contains("oldest turns are no longer sent to the model")), "{:?}", log(&w, &id));
    let last: Value = serde_json::from_slice(&fake.requests().last().unwrap().body).unwrap();
    let users = last["messages"].as_array().unwrap().iter().filter(|m| m["role"] == "user").count();
    assert!(users < 4 && last["messages"][1]["content"].as_str().unwrap().starts_with(&format!("{}", 4 - users + 1)), "whole oldest turns were dropped: {users} users");
    assert_eq!(record(&w, &id).len(), 8, "nothing was dropped from the record");
    say(&w, &id, generation, "big", &"y".repeat(20_000));
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), "the window is 4096"), "{:?}", log(&w, &id));
    // the window the person did not choose is 8,192 tokens
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::OpenAi, 0);
    say(&w, &id, generation, "big", &"z".repeat(40_000));
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), "the window is 8192"), "{:?}", log(&w, &id));
}

#[test]
fn for_an_ollama_server_the_window_and_tool_support_are_asked_of_the_server_when_the_person_gave_none() {
    let w = world("ollama");
    let show = json!({"capabilities": ["completion"], "model_info": {"llama.context_length": 2048}}).to_string();
    let fake = Fake::start(
        Script::new()
            .on("POST", "/api/show", Behaviour::Answer(Reply::json(200, &show)))
            .on("POST", "/api/chat", Behaviour::Answer(Reply::ndjson(ollama_ndjson(&[text("hi from ollama"), Piece::End])))),
    )
    .unwrap();
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::Ollama, 0);
    say(&w, &id, generation, "r1", "hello");
    wait_idle(&w, &id);
    assert!(has(&log(&w, &id), "cannot call tools") && has(&log(&w, &id), "hi from ollama"), "{:?}", log(&w, &id));
    let chat = fake.requests().into_iter().find(|r| r.path == "/api/chat").unwrap();
    let body: Value = serde_json::from_slice(&chat.body).unwrap();
    assert!(body.get("tools").is_none(), "the model said it has no tools");
    assert_eq!(body["options"]["num_ctx"], 2048, "the window is the model's own");
    // a window the person chose is not overruled by the server
    let (id, generation) = create(&w, fake.addr(), crate::driver::Api::Ollama, 4096);
    say(&w, &id, generation, "r1", "hello again");
    wait_idle(&w, &id);
    let chat = fake.requests().into_iter().filter(|r| r.path == "/api/chat").last().unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&chat.body).unwrap()["options"]["num_ctx"], 4096);
}
