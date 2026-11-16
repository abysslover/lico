//! The hubs through the driver port (M16-P06): a session of a registered kind is created, fed, closed, resumed, restored and purged by the host through the port, proved
//! with a dummy driver that records what it is asked (no network, no model, no adapter).

use crate::core::{Config, Shared};
use crate::driver::{self, Api, Driver, Link, Opaque, Target};
use crate::folders::Settings;
use crate::session::{Origin, State};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

type Log = Arc<Mutex<Vec<String>>>;

struct Dummy {
    kind: &'static str,
    log: Log,
}

struct DummyLink(Log);

impl Link for DummyLink {
    fn send_input(&mut self, request_id: &str, text: &str) -> Result<(), &'static str> {
        if text == "REFUSE" {
            return Err("busy");
        }
        self.0.lock().unwrap().push(format!("input {request_id} {text}"));
        Ok(())
    }
    fn stop(&mut self) {
        self.0.lock().unwrap().push("stop".into());
    }
    fn purge(&mut self) {
        self.0.lock().unwrap().push("purge".into());
    }
}

impl Driver for Dummy {
    fn kind(&self) -> &'static str {
        self.kind
    }
    fn accepts(&self, target: Option<&Target>) -> Result<(), &'static str> {
        match target {
            None => Err("bad_request"),
            Some(t) if t.context_tokens == 777 => Err("busy"), // the driver's own rule about what it can serve
            Some(_) => Ok(()),
        }
    }
    fn start(&self, shared: &Opaque, session: &Opaque) -> Result<Box<dyn Link>, &'static str> {
        // the handles are the host's own types
        assert!(shared.clone().downcast::<Shared>().is_ok() && session.clone().downcast::<Mutex<crate::session::Session>>().is_ok());
        self.log.lock().unwrap().push("start".into());
        Ok(Box::new(DummyLink(self.log.clone())))
    }
}

fn shared(dir: &std::path::Path) -> Arc<Shared> {
    std::fs::create_dir_all(dir.join("log")).unwrap();
    std::fs::create_dir_all(dir.join("accounts")).unwrap();
    let cfg = Config {
        login_bin: None, login_args: Vec::new(), login_timeout: 60, log_dir: dir.join("log"), agent: PathBuf::new(),
        retain: 500, window: 20, queue_bytes: 1 << 20, payload: 4096, max_sessions: 8, codex: PathBuf::new(), codex_args: Vec::new(),
        claude: PathBuf::new(), claude_args: Vec::new(), cred_bin: PathBuf::new(), cred_args: Vec::new(), job_bin: PathBuf::new(),
        cred_dir: dir.join("cred"), workspace_root: dir.join("work"), handoff_dir: dir.join("handoff"),
        accounts_dir: dir.join("accounts"), registry_dir: dir.join("registry"), ssh_bin: PathBuf::new(), ssh_config: None, text: true, realistic_text: false,
    };
    let accounts = crate::accounts::Accounts::load(&cfg.accounts_dir).unwrap();
    Arc::new(Shared {
        cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now(),
        pending_creates: AtomicUsize::new(0), accounts: Mutex::new(accounts), logins: Mutex::new(HashMap::new()), clip: Default::default(), calls: Default::default(),
    })
}

fn world(name: &str) -> (Arc<Shared>, Log, &'static str) {
    let dir = std::env::temp_dir().join(format!("lico_driver_hubs_{name}_{}", crate::core::random_hex(6)));
    let kind: &'static str = Box::leak(format!("dummy-{name}").into_boxed_str());
    let log: Log = Default::default();
    driver::register(Arc::new(Dummy { kind, log: log.clone() }));
    (shared(&dir), log, kind)
}

fn settings(model: &str) -> Settings {
    Settings { model: Some(model.into()), target: Some(Target { addr: "127.0.0.1:11434".parse().unwrap(), api: Api::Ollama, model: model.into(), context_tokens: 4096 }), ..Settings::default() }
}

fn create(shared: &Arc<Shared>, kind: &str) -> (String, i64) {
    let reply = crate::new_session::session_create(shared, kind, 0, 0, 4096, None, "auto", settings("m1")).unwrap();
    (reply["session"].as_str().unwrap().to_string(), reply["generation"].as_i64().unwrap())
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

#[test]
fn a_session_of_a_driver_kind_is_started_by_the_driver_kept_by_the_host_and_written_to_the_registry() {
    let (shared, log, kind) = world("create");
    let (id, generation) = create(&shared, kind);
    assert_eq!(entries(&log), ["start"]);
    let found = crate::core::find(&shared, &id).unwrap();
    {
        let s = found.lock().unwrap();
        assert!(s.state == State::Running);
        assert_eq!((s.provider, s.driver.is_some(), s.account.clone(), s.mode), (kind, true, None, "auto"));
        assert_eq!(s.settings.target.as_ref().unwrap().model, "m1");
        assert_eq!(s.generation as i64, generation);
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(shared.cfg.registry_dir.join("sessions.json")).unwrap()).unwrap();
    let entry = &saved["sessions"][0];
    assert_eq!((entry["provider"].as_str(), entry["local_endpoint"].as_str(), entry["local_api"].as_str(), entry["local_context"].as_u64(), entry["chosen_model"].as_str()), (Some(kind), Some("127.0.0.1:11434"), Some("ollama"), Some(4096), Some("m1")));
    assert!(entry["account"].is_null() && entry["scratch"] == json!(true));
}

#[test]
fn what_a_driver_session_cannot_have_is_refused_before_anything_starts() {
    let (shared, log, kind) = world("refuse");
    let ask = |chosen: Settings, account: Option<&str>| crate::new_session::session_create(&shared, kind, 0, 0, 4096, account, "auto", chosen);
    assert_eq!(ask(Settings { target: None, ..settings("m") }, None).unwrap_err(), "bad_request", "no target");
    assert_eq!(ask(Settings { model: None, ..settings("m") }, None).unwrap_err(), "bad_request", "the target's model and the chosen model must agree");
    assert_eq!(ask(Settings { effort: Some("high".into()), ..settings("m") }, None).unwrap_err(), "option_unavailable");
    assert_eq!(ask(Settings { cwd: Some("/tmp".into()), ..settings("m") }, None).unwrap_err(), "option_unavailable");
    assert_eq!(ask(Settings { worktree_branch: Some("b".into()), ..settings("m") }, None).unwrap_err(), "option_unavailable");
    assert_eq!(ask(Settings { resume_id: Some("abc".into()), ..settings("m") }, None).unwrap_err(), "option_unavailable");
    assert_eq!(ask(Settings { resume_latest: true, ..settings("m") }, None).unwrap_err(), "option_unavailable");
    assert_eq!(ask(settings("m"), Some("a123456789ab")).unwrap_err(), "bad_request", "a driver session has no account");
    let mut picky = settings("m");
    picky.target.as_mut().unwrap().context_tokens = 777;
    assert_eq!(ask(picky, None).unwrap_err(), "busy", "the driver decides whether its own choices are enough");
    assert!(entries(&log).is_empty() && shared.sessions.read().unwrap().is_empty(), "nothing started, nothing kept");
}

#[test]
fn an_input_reaches_the_link_marks_the_session_busy_and_is_not_sent_twice() {
    let (shared, log, kind) = world("input");
    let (id, generation) = create(&shared, kind);
    let sent = |request: &str, text: &str| crate::core::input(&shared, &id, Some(generation), request, text, Origin::Person, None);
    let reply = sent("r1", "hello there").unwrap();
    assert_eq!((reply["state"].as_str(), reply["duplicate"].as_bool()), (Some("accepted"), Some(false)));
    assert_eq!(entries(&log), ["start", "input r1 hello there"]);
    assert!(crate::core::find(&shared, &id).unwrap().lock().unwrap().busy, "a turn is in progress");
    assert_eq!(sent("r1", "hello there").unwrap()["duplicate"], json!(true));
    assert_eq!(entries(&log).len(), 2, "a repeated request id is not sent again");
    assert_eq!(sent("r2", "REFUSE").unwrap_err(), "busy", "what the link refuses is refused to the caller");
    assert_eq!(crate::core::input(&shared, &id, Some(generation), "r3", "with a picture", Origin::Person, Some("0".repeat(32).as_str())).unwrap_err(), "option_unavailable", "a local model gets text only");
    assert_eq!(crate::core::input_status(&shared, &id, "r1").unwrap()["state"], json!("accepted"));
}

#[test]
fn close_stops_the_link_and_keeps_the_session_resume_starts_it_again_and_purge_removes_what_the_driver_kept() {
    let (shared, log, kind) = world("life");
    let (id, generation) = create(&shared, kind);
    assert_eq!(crate::core::session_resume(&shared, &id, generation).unwrap_err(), "busy", "a running session is not resumed");
    let closed = crate::lifecycle::session_close_opts(&shared, &id, generation, None, false, false).unwrap();
    assert_eq!(closed["session"], json!(id));
    assert_eq!(entries(&log), ["start", "stop"]);
    {
        let found = crate::core::find(&shared, &id).unwrap();
        let s = found.lock().unwrap();
        assert!(s.state == State::Disconnected && s.driver.is_none() && !s.busy);
    }
    let resumed = crate::core::session_resume(&shared, &id, generation).unwrap();
    assert_eq!(resumed["generation"].as_i64().unwrap(), generation + 1, "every older target is stale");
    assert_eq!(entries(&log), ["start", "stop", "start"]);
    assert!(crate::core::find(&shared, &id).unwrap().lock().unwrap().state == State::Running);
    crate::core::session_purge(&shared, &id, generation + 1).unwrap();
    assert_eq!(entries(&log), ["start", "stop", "start", "stop", "purge"]);
    assert!(crate::core::find(&shared, &id).is_none());
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(shared.cfg.registry_dir.join("sessions.json")).unwrap()).unwrap();
    assert!(saved["sessions"].as_array().unwrap().is_empty(), "a purged session leaves the registry");
}

#[test]
fn a_restart_brings_the_session_back_disconnected_with_its_target_and_resume_starts_the_driver() {
    let (shared, log, kind) = world("restart");
    let (id, _) = create(&shared, kind);
    let accounts = crate::accounts::Accounts::load(&shared.cfg.accounts_dir).unwrap();
    let loaded = crate::registry::load(&shared.cfg.registry_dir, &accounts).unwrap();
    assert_eq!(loaded.len(), 1);
    let (recovered, _) = crate::session::Session::recovered(&loaded[0], &shared.cfg.log_dir, 500, 20);
    assert!(recovered.state == State::Disconnected);
    assert_eq!((recovered.provider, recovered.driver.is_none(), recovered.id.as_str()), (kind, true, id.as_str()));
    assert_eq!(recovered.settings.target, Some(Target { addr: "127.0.0.1:11434".parse().unwrap(), api: Api::Ollama, model: "m1".into(), context_tokens: 4096 }));
    // a restarted host registers its drivers first; a record of a kind that nobody serves stops the load, as an unknown provider always did
    let mut text: Value = serde_json::from_str(&std::fs::read_to_string(shared.cfg.registry_dir.join("sessions.json")).unwrap()).unwrap();
    text["sessions"][0]["provider"] = json!("nobody-serves-this");
    std::fs::write(shared.cfg.registry_dir.join("sessions.json"), text.to_string()).unwrap();
    assert!(crate::registry::load(&shared.cfg.registry_dir, &accounts).err().unwrap().contains("unknown provider"));
    let _ = log;
}

#[test]
fn a_damaged_target_in_the_registry_stops_the_load() {
    let (shared, _log, kind) = world("damage");
    create(&shared, kind);
    let accounts = crate::accounts::Accounts::load(&shared.cfg.accounts_dir).unwrap();
    let path = shared.cfg.registry_dir.join("sessions.json");
    let good: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for (field, value) in [("local_endpoint", json!("192.168.1.5:80")), ("local_endpoint", json!(5)), ("local_api", json!("grpc")), ("local_context", json!(-1)), ("local_context", json!(2_000_000)), ("chosen_model", Value::Null)] {
        let mut bad = good.clone();
        bad["sessions"][0][field] = value.clone();
        std::fs::write(&path, bad.to_string()).unwrap();
        assert!(crate::registry::load(&shared.cfg.registry_dir, &accounts).is_err(), "{field} = {value}");
    }
    std::fs::write(&path, good.to_string()).unwrap();
    assert!(crate::registry::load(&shared.cfg.registry_dir, &accounts).is_ok());
}

#[test]
fn the_request_checks_accept_the_new_arguments_of_a_registered_kind_and_nothing_loose() {
    let (_shared, _log, kind) = world("ipc");
    let request = |extra: Value| -> Map<String, Value> {
        let mut m = json!({"op": "session.create", "kind": kind, "endpoint": "127.0.0.1:11434", "api": "openai", "model": "qwen", "context_tokens": 8192}).as_object().unwrap().clone();
        for (k, v) in extra.as_object().unwrap() {
            m.insert(k.clone(), v.clone());
        }
        m
    };
    assert!(crate::ipc::valid_request("session.create", &request(json!({}))));
    assert!(crate::ipc::valid_request("session.create", &request(json!({"name": "my local one"}))));
    for loose in [json!({"endpoint": "192.168.1.5:11434"}), json!({"endpoint": "localhost:11434"}), json!({"endpoint": "0.0.0.0:80"}), json!({"api": "grpc"}), json!({"context_tokens": 100}), json!({"context_tokens": 2_000_000}), json!({"kind": "no-such-kind"}), json!({"model": "bad model!"}), json!({"endpoint": 5}), json!({"surprise": 1})] {
        assert!(!crate::ipc::valid_request("session.create", &request(loose.clone())), "{loose}");
    }
    assert!(!crate::ipc::valid_request("session.rename", &json!({"op": "session.rename", "session": "s1", "name": "x", "endpoint": "127.0.0.1:1"}).as_object().unwrap().clone()), "the new arguments belong to session.create only");
}

#[test]
fn a_deleted_session_is_not_started_by_a_driver_and_a_deferred_reply_goes_to_the_link() {
    let (shared, log, kind) = world("odd");
    let driver = driver::get(kind).unwrap();
    let ghost = Arc::new(Mutex::new(crate::session::Session::new("sdeadbeef".into(), false, &shared.cfg.log_dir, 500, 20)));
    ghost.lock().unwrap().purged = true;
    assert_eq!(crate::core::start_driver(&driver, &shared, &ghost).unwrap_err(), "busy");
    assert!(entries(&log).is_empty(), "a deleted session gets no work");
    // a reply of another session that waited for this one to be idle goes to the link as any input does
    let (id, _) = create(&shared, kind);
    let found = crate::core::find(&shared, &id).unwrap();
    let mut s = found.lock().unwrap();
    s.set_busy(false);
    s.peer_turn.deferred_replies.push_back(crate::calls::DeferredReply { request_id: "peer1".into(), send_text: "a peer's reply".into(), shown_text: "a peer's reply".into(), reply_slot: None });
    crate::calls::flush_deferred(&mut s);
    assert_eq!(entries(&log), ["start", "input peer1 a peer's reply"]);
    assert!(s.busy && s.peer_turn.deferred_replies.is_empty());
}

#[test]
fn the_socket_door_and_the_http_door_make_the_same_session_of_a_driver_kind() {
    let request = |kind: &str| -> Map<String, Value> {
        json!({"op": "session.create", "kind": kind, "endpoint": "127.0.0.1:11434", "api": "ollama", "model": "qwen", "context_tokens": 4096, "name": "mine"}).as_object().unwrap().clone()
    };
    let mut answers = Vec::new();
    for via_http in [false, true] {
        let (shared, log, kind) = world(if via_http { "door_http" } else { "door_socket" });
        let map = request(kind);
        assert!(crate::ipc::valid_request("session.create", &map), "the one check serves both doors");
        let reply = crate::ipc::dispatch_request(&shared, "session.create", &map, via_http).expect("the operation is served at this door").unwrap();
        let found = crate::core::find(&shared, reply["session"].as_str().unwrap()).unwrap();
        let s = found.lock().unwrap();
        answers.push((reply.keys().cloned().collect::<Vec<_>>(), s.provider == kind, s.settings.name.clone(), s.settings.target.clone(), entries(&log)));
    }
    assert_eq!(answers[0], answers[1], "the same answer, the same session, the same start from either door");
    assert_eq!(answers[0].2.as_deref(), Some("mine"));
}
