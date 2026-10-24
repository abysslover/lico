//! The Claude Code adapter: one `claude -p` process per session speaking newline-delimited JSON
//! (stream-json) on standard input and output. It maps the provider's messages onto host records,
//! input states and approvals (the rules of `lico_protocol/claude_mapping.py`, which this must
//! match). The message shapes come from the provider's documentation, not from a recording; what
//! the documentation does not say is listed as unverified in the capability record.
//!
//! Differences from the Codex adapter: there is no handshake before an input (a user message can be
//! written at once), no steer (further user messages are queued by the provider), and an input is
//! acknowledged when the provider replays the user message with the uuid the host gave it.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::codex::{clip, sanitize};
use crate::core::{self, Shared};
use crate::session::{InputState, Session, State, MAX_APPROVALS};

const MAX_LINE: usize = 256 * 1024;
const MAX_RECORD: usize = 4096;
const MAX_PROMPT: usize = 200;
const MAX_RESULT_LINES: usize = 20;
const MAX_TRACKED_INPUTS: usize = 1024; // user messages written and not yet acknowledged
const MAX_REQUEST_INPUT: usize = 16 * 1024; // a permission request whose input is larger is refused, not held
const PERMISSION_SUBTYPE: &str = "can_use_tool"; // assumed: the documentation does not state it (unverified)

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub struct Link {
    tx: SyncSender<(String, Option<String>)>,
    pending_inputs: HashMap<String, String>, // uuid given to the provider -> host request id, until replayed
    approvals: HashMap<i64, (String, Value)>, // approval number -> (provider request id, the tool input to hand back)
}

fn record(s: &mut Session, line: &str) {
    s.append(clip(&sanitize(line), MAX_RECORD).into_bytes());
}

pub fn note(s: &mut Session, line: &str) {
    record(s, line);
}

fn send(link: &Link, line: Value, request_id: Option<String>) -> Result<(), &'static str> {
    match link.tx.try_send((line.to_string(), request_id)) {
        Ok(()) => Ok(()),
        Err(mpsc::TrySendError::Full(_)) => Err("busy"),
        Err(mpsc::TrySendError::Disconnected(_)) => Err("session_disconnected"),
    }
}

fn new_uuid() -> String {
    let h = core::random_hex(16);
    // version 4 and the variant bits, so the provider sees an ordinary uuid
    format!("{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

/// One host input is one user message; the provider queues it behind a turn that is running.
pub fn send_input(s: &mut Session, request_id: &str, text: &str) -> Result<(), &'static str> {
    let session_id = s.codex_thread.clone();
    let link = s.claude.as_mut().ok_or("session_disconnected")?;
    if link.pending_inputs.len() >= MAX_TRACKED_INPUTS {
        return Err("busy");
    }
    let uuid = new_uuid();
    let mut message = json!({
        "type": "user", "uuid": uuid, "parent_tool_use_id": null,
        "message": {"role": "user", "content": [{"type": "text", "text": text}]},
    });
    if let Some(id) = session_id {
        message["session_id"] = json!(id);
    }
    link.pending_inputs.insert(uuid.clone(), request_id.to_string());
    if let Err(code) = send(link, message, Some(request_id.to_string())) {
        link.pending_inputs.remove(&uuid);
        return Err(code);
    }
    Ok(())
}

/// A host approval answer becomes the provider's decision for exactly that request.
pub fn answer(s: &mut Session, number: i64, allow: bool) -> Result<(), &'static str> {
    let link = s.claude.as_mut().ok_or("session_disconnected")?;
    let (request_id, input) = link.approvals.get(&number).cloned().ok_or("unknown_approval")?;
    let decision = if allow {
        json!({"behavior": "allow", "updatedInput": input})
    } else {
        json!({"behavior": "deny", "message": "denied by the owner"})
    };
    send(link, json!({"type": "control_response", "response": {"subtype": "success", "request_id": request_id, "response": decision}}), None)?;
    link.approvals.remove(&number);
    Ok(())
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub fn start(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>) -> Result<(), &'static str> {
    spawn(shared, session, None)
}

/// Bring a disconnected session back: a new provider process resumes the same provider session, and
/// the execution generation moves on so every older target is stale.
pub fn resume(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>) -> Result<(), &'static str> {
    let provider_session = {
        let s = session.lock().unwrap();
        if s.state != State::Disconnected {
            return Err("busy");
        }
        s.codex_thread.clone()
    };
    spawn(shared, session, provider_session)?;
    let mut s = session.lock().unwrap();
    record(&mut s, "[provider reconnected]");
    Ok(())
}

fn spawn(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>, resume: Option<String>) -> Result<(), &'static str> {
    let id = session.lock().unwrap().id.clone();
    let workspace = shared.cfg.workspace_root.join(&id);
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&workspace).map_err(|_| "busy")?;
    let mut command = Command::new(&shared.cfg.claude);
    command.args(&shared.cfg.claude_args).args([
        "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--replay-user-messages", "--permission-mode", "default",
    ]);
    if let Some(provider_session) = &resume {
        command.args(["--resume", provider_session]);
    }
    command.current_dir(&workspace).env_clear();
    for key in ["PATH", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let bound = session.lock().unwrap().account.clone();
    match bound {
        // a session bound to an account sees only that account's private home
        Some(account) => {
            let (homes, credential) = {
                let accounts = shared.accounts.lock().unwrap();
                (accounts.homes(&account).ok_or("busy")?, accounts.credential_ref(&account))
            };
            command.env("HOME", homes.home).env(homes.variable, homes.provider_home);
            if let Some(reference) = credential {
                // the key reaches the provider only through a helper that reads the keyring: the command names the
                // reference, never the key, and the helper needs the keyring's bus to answer
                let helper = format!("{} --data-dir {} helper --ref {}", shell_quote(&shared.cfg.cred_bin.to_string_lossy()), shell_quote(&shared.cfg.cred_dir.to_string_lossy()), reference);
                command.arg("--settings").arg(json!({"apiKeyHelper": helper}).to_string());
                for key in ["DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"] {
                    if let Some(value) = std::env::var_os(key) {
                        command.env(key, value);
                    }
                }
            }
        }
        None => {
            for key in ["HOME", "CLAUDE_CONFIG_DIR"] {
                if let Some(value) = std::env::var_os(key) {
                    command.env(key, value);
                }
            }
        }
    }
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = core::spawn_provider_process(command).map_err(|_| "busy")?;
    let pid = child.id();
    let (Some(stdout), Some(mut stdin)) = (child.stdout.take(), child.stdin.take()) else {
        unsafe { kill(-(pid as i32), 9) };
        let _ = child.wait();
        return Err("busy");
    };
    let (tx, rx) = mpsc::sync_channel::<(String, Option<String>)>(1024);
    {
        let mut s = session.lock().unwrap();
        s.agent_pid = pid;
        s.process_group = true;
        s.provider = "claude";
        s.claude = Some(Link { tx, pending_inputs: HashMap::new(), approvals: HashMap::new() });
        s.state = State::Running;
        if let Some(provider_session) = &resume {
            s.codex_thread = Some(provider_session.clone());
        }
    }
    let writer_session = session.clone();
    thread::spawn(move || {
        for (line, request_id) in rx {
            if stdin.write_all(line.as_bytes()).and_then(|_| stdin.write_all(b"\n")).and_then(|_| stdin.flush()).is_err() {
                return;
            }
            if let Some(rid) = request_id {
                writer_session.lock().unwrap().advance_input(&rid, InputState::Submitted);
            }
        }
    });
    let reader_session = session.clone();
    thread::spawn(move || {
        for line in core::bounded_lines(stdout, MAX_LINE) {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                handle(&reader_session, &message);
            }
        }
        unsafe { kill(-(pid as i32), 9) }; // its output ended: nothing of it, helpers included, is kept alive
        let _ = child.wait();
        let mut s = reader_session.lock().unwrap();
        if s.agent_pid == pid {
            s.agent_pid = 0; // reaped: the number may now belong to another process
        }
        if s.state == State::Running && s.claude.is_some() {
            s.state = State::Disconnected;
            s.claude = None; // its approvals and unanswered requests are stale now
            s.approvals.clear();
            record(&mut s, "[provider disconnected]");
        }
    });
    Ok(())
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn approval_prompt(tool: &str, input: &Value) -> String {
    let text = match (tool, input["command"].as_str()) {
        ("Bash", Some(command)) => format!("run: {command}"),
        _ => format!("{tool}: {}", serde_json::to_string(input).unwrap_or_default()),
    };
    let shown = sanitize(&text);
    if shown.chars().count() > MAX_PROMPT {
        format!("{}…", shown.chars().take(MAX_PROMPT - 1).collect::<String>())
    } else {
        shown
    }
}

fn handle(session: &Arc<Mutex<Session>>, message: &Value) {
    let mut s = session.lock().unwrap();
    match message["type"].as_str() {
        Some("system") if message["subtype"] == "init" => {
            if let Some(id) = message["session_id"].as_str().filter(|id| id.len() <= 64) {
                s.codex_thread = Some(id.to_string());
            }
            let model = clip(message["model"].as_str().unwrap_or(""), 60);
            record(&mut s, &format!("[session started] model={model}"));
        }
        Some("assistant") => {
            for block in message["message"]["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("text") => {
                        for line in block["text"].as_str().unwrap_or("").split('\n') {
                            record(&mut s, line.trim_end_matches('\r'));
                        }
                    }
                    Some("tool_use") => match (block["name"].as_str(), block["input"]["command"].as_str()) {
                        (Some("Bash"), Some(command)) => record(&mut s, &format!("$ {command}")),
                        (name, _) => record(&mut s, &format!("[tool] {}", clip(name.unwrap_or(""), 60))),
                    },
                    _ => {}
                }
            }
        }
        Some("user") => {
            // our own message, replayed by the provider, acknowledges that input
            if let Some(uuid) = message["uuid"].as_str() {
                let acknowledged = s.claude.as_mut().and_then(|link| link.pending_inputs.remove(uuid));
                if let Some(request_id) = acknowledged {
                    s.advance_input(&request_id, InputState::Observed);
                    return;
                }
            }
            for block in message["message"]["content"].as_array().into_iter().flatten() {
                if block["type"] == "tool_result" {
                    if block["is_error"] == true {
                        record(&mut s, "[tool error]");
                    }
                    for line in text_of(&block["content"]).split('\n').take(MAX_RESULT_LINES) {
                        record(&mut s, &format!("> {}", line.trim_end_matches('\r')));
                    }
                }
            }
        }
        Some("result") => {
            if message["is_error"] == true || message["subtype"] != "success" {
                record(&mut s, &format!("[turn failed] {}", clip(message["subtype"].as_str().unwrap_or(""), 60)));
                for error in message["errors"].as_array().into_iter().flatten().take(MAX_RESULT_LINES) {
                    record(&mut s, &format!("[error] {}", error.as_str().unwrap_or("")));
                }
            } else {
                record(&mut s, "[turn completed]");
            }
        }
        Some("control_request") => control_request(&mut s, message),
        _ => {}
    }
}

fn control_request(s: &mut Session, message: &Value) {
    // the request id is echoed back to the provider, so it must stay small
    let Some(request_id) = message["request_id"].as_str().filter(|id| !id.is_empty() && id.len() <= 64).map(str::to_string) else { return };
    let request = &message["request"];
    let refuse = |link: &Link, why: &str| {
        let _ = send(link, json!({"type": "control_response", "response": {"subtype": "error", "request_id": request_id, "error": why}}), None);
    };
    let approvals_pending = s.approvals.len();
    let Some(link) = s.claude.as_mut() else { return };
    if request["subtype"] != PERMISSION_SUBTYPE {
        return refuse(link, "unsupported by this client");
    }
    let input = request["input"].clone();
    if link.approvals.values().any(|(known, _)| *known == request_id) {
        return; // the provider repeated a request id: one approval, never two
    }
    if approvals_pending >= MAX_APPROVALS || input.to_string().len() > MAX_REQUEST_INPUT {
        return refuse(link, "too many pending approvals or an input too large");
    }
    s.approval_counter += 1;
    let number = s.approval_counter;
    let prompt = approval_prompt(request["tool_name"].as_str().unwrap_or(""), &input);
    if let Some(link) = s.claude.as_mut() {
        link.approvals.insert(number, (request_id, input));
    }
    s.add_approval(number, prompt);
}
