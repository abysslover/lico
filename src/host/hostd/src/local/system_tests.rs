//! The system tests of the session kind `local` (M16-P06): the real `lico` binary (`LICO_BIN`) as the host, on a private state folder and socket, with the fake server of
//! `lico-llm-fake` as the model. They are on the short named list `tests/system_tests.toml` and are run alone by `cargo xtask system` (each is ignored in the ordinary unit
//! run). Each needs the real process boundary (the host binary, its socket, its registry and log) that no component test has. No real server, no keyring, no billable turn,
//! no discovery of the machine's real sockets (the host is given a process table of its own), nothing outside a private temporary folder.

use lico_llm_fake::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const IGNORED: &str = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN set";
const CHAT: &str = "/v1/chat/completions";

struct Rig {
    dir: PathBuf,
    socket: String,
    child: Child,
    fake: Option<Fake>,
}

impl Rig {
    fn start(name: &str, script: Script) -> Rig {
        let binary = std::env::var("LICO_BIN").unwrap_or_else(|_| panic!("LICO_BIN must name the release binary (cargo xtask system sets it); a system test never skips"));
        let dir = std::env::temp_dir().join(format!("lico_system_{name}_{}", crate::core::random_hex(6)));
        let state = dir.join("state");
        std::fs::create_dir_all(dir.join("proc/net")).unwrap();
        for table in ["tcp", "tcp6"] {
            std::fs::write(dir.join("proc/net").join(table), "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n").unwrap();
        }
        let socket = dir.join("host.sock").to_string_lossy().into_owned();
        let child = Command::new(binary)
            .args(["host", "--socket", &socket, "--text", "--sessions", "0", "--producing", "0"])
            .args(["--state-dir", state.to_str().unwrap(), "--log-dir", state.join("log").to_str().unwrap(), "--registry-dir", state.join("registry").to_str().unwrap(), "--accounts-dir", state.join("accounts").to_str().unwrap()])
            .args(["--llm-proc-root", dir.join("proc").to_str().unwrap()])
            .env("HOME", &dir)
            .env("XDG_RUNTIME_DIR", &dir)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(dir.join("host.err")).unwrap())
            .spawn()
            .expect("the host starts");
        let rig = Rig { dir, socket, child, fake: Some(Fake::start(script).unwrap()) };
        let end = Instant::now() + Duration::from_secs(15);
        while std::os::unix::net::UnixStream::connect(&rig.socket).is_err() {
            assert!(Instant::now() < end, "the host did not open its socket");
            std::thread::sleep(Duration::from_millis(50));
        }
        rig
    }

    fn call(&self, request: Value) -> Value {
        crate::cli::call_once(&self.socket, request)
    }

    fn create(&self) -> (String, i64) {
        let addr = self.fake.as_ref().unwrap().addr();
        let reply = self.call(json!({"op": "session.create", "kind": "local", "endpoint": addr.to_string(), "api": "openai", "model": "fake", "name": "system"}));
        assert_eq!(reply["ok"], json!(1), "{reply}");
        (reply["session"].as_str().unwrap().to_string(), reply["generation"].as_i64().unwrap())
    }

    fn say(&self, id: &str, generation: i64, request: &str, text: &str) {
        let reply = self.call(json!({"op": "input", "session": id, "generation": generation, "request_id": request, "text": text}));
        assert_eq!(reply["ok"], json!(1), "{reply}");
    }

    fn lines(&self, id: &str) -> Vec<String> {
        let listed = self.call(json!({"op": "session.list"}));
        let last = listed["sessions"].as_array().and_then(|a| a.iter().find(|s| s["session"] == json!(id))).and_then(|s| s["seq"].as_i64()).unwrap_or(0);
        let reply = self.call(json!({"op": "history", "session": id, "before": last + 1, "limit": 200}));
        reply["lines"].as_array().map(|l| l.iter().filter_map(|x| x[1].as_str().map(str::to_string)).collect()).unwrap_or_default()
    }

    /// Waits until a line of the session's log has `needle`, at most `secs` seconds; returns the log.
    fn wait_for(&self, id: &str, needle: &str, secs: u64) -> Vec<String> {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            let lines = self.lines(id);
            if lines.iter().any(|l| l.contains(needle)) {
                return lines;
            }
            assert!(Instant::now() < end, "no line with {needle:?} in {secs} s: {lines:?}\nthe host said: {}", std::fs::read_to_string(self.dir.join("host.err")).unwrap_or_default().chars().take(2000).collect::<String>());
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn record(&self, id: &str) -> Vec<Value> {
        let path = self.dir.join("state/registry/local").join(format!("{id}.jsonl"));
        std::fs::read_to_string(path).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(fake) = self.fake.take() {
            fake.stop();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn sse(pieces: &[Piece]) -> Behaviour {
    Behaviour::Answer(Reply::sse(openai_sse(pieces)))
}

fn text(t: &str) -> Piece {
    Piece::Text(t.into())
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN set"]
fn a_message_through_a_local_session_streams_lines_into_the_log() {
    let _ = IGNORED;
    let rig = Rig::start("stream", Script::new().on("POST", CHAT, sse(&[text("Hello from the fake\nsecond line"), Piece::Finish("stop".into()), Piece::End])));
    let (id, generation) = rig.create();
    rig.say(&id, generation, "r1", "hi lico");
    let lines = rig.wait_for(&id, "[turn completed]", 15);
    assert!(lines.iter().any(|l| l.contains("Hello from the fake")) && lines.iter().any(|l| l.contains("second line")), "{lines:?}");
    let record = rig.record(&id);
    assert_eq!((record.len(), record[0]["t"].as_str(), record[1]["t"].as_str()), (2, Some("user"), Some("assistant")));
    let listed = rig.call(json!({"op": "session.list"}));
    assert!(listed.to_string().contains(&id) && listed.to_string().contains("\"running\""), "the session is listed and running: {listed}");
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN set"]
fn a_tool_that_needs_a_card_waits_the_person_allows_and_the_answer_ends() {
    let rig = Rig::start(
        "card",
        Script::new()
            .times(1, "POST", CHAT, sse(&[Piece::Call { id: Some("c1".into()), name: "llm_endpoints".into(), args: "{}".into(), split: 0 }, Piece::Finish("tool_calls".into()), Piece::End]))
            .on("POST", CHAT, sse(&[text("found nothing to report"), Piece::Finish("stop".into()), Piece::End])),
    );
    let (id, generation) = rig.create();
    rig.say(&id, generation, "r1", "look for model servers on this machine");
    rig.wait_for(&id, "[waiting for the person]", 15);
    assert_eq!(rig.fake.as_ref().unwrap().requests().len(), 1, "the model is not asked again while the person decides");
    let result = rig.record(&id).into_iter().find(|m| m["t"] == "tool").expect("the waiting result is in the record");
    let pending: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
    let approval = pending["approval_id"].as_i64().expect("the answer names the card");
    let answered = rig.call(json!({"op": "approval.answer", "session": id, "generation": generation, "approval_id": approval, "answer": "allow"}));
    assert_eq!(answered["ok"], json!(1), "{answered}");
    let lines = rig.wait_for(&id, "found nothing to report", 20);
    assert!(lines.iter().filter(|l| l.contains("[turn completed]")).count() >= 1, "{lines:?}");
    assert_eq!(rig.fake.as_ref().unwrap().requests().len(), 2);
    let second: Value = serde_json::from_slice(&rig.fake.as_ref().unwrap().requests()[1].body).unwrap();
    let roles: Vec<&str> = second["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool", "user"], "the call, its waiting result and the host's message about the decision");
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN set"]
fn a_stop_cancels_a_running_stream_within_about_a_second() {
    let rig = Rig::start("stop", Script::new().on("POST", CHAT, Behaviour::Answer(Reply::sse(openai_sse(&[text("first line\n"), text("never finished"), Piece::End])).ending(Ending::Silent(2)))));
    let (id, generation) = rig.create();
    rig.say(&id, generation, "r1", "go on");
    rig.wait_for(&id, "first line", 15);
    let started = Instant::now();
    let closed = rig.call(json!({"op": "session.close", "session": id, "generation": generation}));
    assert_eq!(closed["ok"], json!(1), "{closed}");
    rig.wait_for(&id, "[turn stopped]", 5);
    assert!(started.elapsed() < Duration::from_millis(2500), "{:?}", started.elapsed());
    let record = rig.record(&id);
    assert_eq!(record.len(), 1, "the cut answer was not kept: {record:?}");
    assert_eq!(rig.call(json!({"op": "session.list"}))["ok"], json!(1), "the host is still serving");
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN set"]
fn a_server_that_vanishes_is_said_so_and_the_session_recovers_on_the_next_message() {
    let mut rig = Rig::start("gone", Script::new().times(1, "POST", CHAT, Behaviour::Close).on("POST", CHAT, sse(&[text("back again"), Piece::Finish("stop".into()), Piece::End])));
    let (id, generation) = rig.create();
    rig.say(&id, generation, "r1", "first");
    let lines = rig.wait_for(&id, "the answer was cut", 15);
    assert!(lines.iter().any(|l| l.contains("[turn failed]")));
    rig.say(&id, generation, "r2", "second");
    rig.wait_for(&id, "back again", 15);
    let addr = rig.fake.as_ref().unwrap().addr();
    rig.fake.take().unwrap().stop();
    rig.say(&id, generation, "r3", "third");
    rig.wait_for(&id, &format!("no server at {addr}"), 15);
    let listed = rig.call(json!({"op": "session.list"}));
    assert!(listed.to_string().contains(&id), "the session itself stays: {listed}");
}
