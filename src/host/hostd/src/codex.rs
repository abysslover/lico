//! The Codex app-server adapter: one provider process per session, JSON lines on its
//! standard input and output. Provider messages become one-line host records and
//! approvals (the rules of `lico_protocol/codex_mapping.py`, which this must match);
//! host inputs become exactly one `turn/start` or `turn/steer` per request id; a provider
//! that goes away is reported as disconnected, never as idle.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::{self, Shared};
use crate::session::MAX_APPROVALS;
use crate::session::{InputState, Session, State};

const MAX_PROVIDER_LINE: usize = 256 * 1024;
const MAX_RECORD: usize = 4096;
const MAX_PROMPT: usize = 200;
const MAX_OPEN_ITEMS: usize = 64; // partial lines kept at once
const READY_WAIT: Duration = Duration::from_secs(10);

enum Pending {
    Init,
    ThreadStart,
    ThreadResume,
    Turn(String),
    Steer(String, String), // request id and its text: a refused steer starts a turn instead
}

pub struct Link {
    tx: SyncSender<(String, Option<String>)>,
    pub active_turn: Option<String>,
    next_rpc: i64,
    pending: HashMap<i64, Pending>,
    pub approval_map: HashMap<i64, Value>,
    buffers: HashMap<String, String>,
    pub ready: bool,
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

// ------------------------------------------------------------------ sanitizing
/// Remove terminal escape sequences and control characters from untrusted provider text.
pub fn sanitize(text: &str) -> String {
    // 1. OSC sequences: ESC ] ... (BEL | ESC \)
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && chars.get(i + 1) == Some(&']') {
            let mut j = i + 2;
            let mut end = None;
            while j < chars.len() {
                if chars[j] == '\u{7}' {
                    end = Some(j + 1);
                    break;
                }
                if chars[j] == '\u{1b}' {
                    if chars.get(j + 1) == Some(&'\\') {
                        end = Some(j + 2);
                    }
                    break;
                }
                j += 1;
            }
            if let Some(e) = end {
                i = e;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // 2. CSI sequences: ESC [ params intermediates final
    let chars = out;
    let mut out: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && chars.get(i + 1) == Some(&'[') {
            let mut j = i + 2;
            while j < chars.len() && "0123456789;?".contains(chars[j]) {
                j += 1;
            }
            while j < chars.len() && (' '..='/').contains(&chars[j]) {
                j += 1;
            }
            if j < chars.len() && ('@'..='~').contains(&chars[j]) {
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // 3. other two-character escapes, 4. control characters
    let chars = out;
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\u{1b}' && chars.get(i + 1).map_or(false, |n| ('@'..='_').contains(n)) {
            i += 2;
            continue;
        }
        let code = c as u32;
        if c == '\t' || c == '\n' || c == '\r' {
            result.push(' '); // a record is one line: a line break inside it would forge a second record
        } else if !(code <= 8 || code == 0x0b || code == 0x0c || (0x0e..=0x1f).contains(&code) || (0x7f..=0x9f).contains(&code)
            || code == 0x061c || code == 0x200e || code == 0x200f || code == 0x2028 || code == 0x2029
            || (0x202a..=0x202e).contains(&code) || (0x2066..=0x2069).contains(&code)) {
            result.push(c);
        }
        i += 1;
    }
    result
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn record(s: &mut Session, line: &str) {
    s.append(clip(&sanitize(line), MAX_RECORD).into_bytes());
}

// ------------------------------------------------------------------- sending
fn send(link: &Link, line: Value, request_id: Option<String>) -> Result<(), &'static str> {
    match link.tx.try_send((line.to_string(), request_id)) {
        Ok(()) => Ok(()),
        Err(mpsc::TrySendError::Full(_)) => Err("busy"),
        Err(mpsc::TrySendError::Disconnected(_)) => Err("session_disconnected"),
    }
}

fn rpc(link: &mut Link, method: &str, params: Value, pending: Pending, request_id: Option<String>) -> Result<(), &'static str> {
    link.next_rpc += 1;
    let id = link.next_rpc;
    send(link, json!({"id": id, "method": method, "params": params}), request_id)?;
    link.pending.insert(id, pending);
    Ok(())
}

/// One host input becomes one provider turn, or steers the turn that is running.
pub fn send_input(s: &mut Session, request_id: &str, text: &str) -> Result<(), &'static str> {
    let thread = s.codex_thread.clone().ok_or("busy")?;
    let link = s.codex.as_mut().ok_or("session_disconnected")?;
    if !link.ready {
        return Err("busy");
    }
    let input = json!([{"type": "text", "text": text, "text_elements": []}]);
    match link.active_turn.clone() {
        Some(turn) => rpc(link, "turn/steer", json!({"threadId": thread, "expectedTurnId": turn, "input": input}), Pending::Steer(request_id.to_string(), text.to_string()), Some(request_id.to_string())),
        None => rpc(link, "turn/start", json!({"threadId": thread, "input": input}), Pending::Turn(request_id.to_string()), Some(request_id.to_string())),
    }
}

/// A host approval answer becomes the provider's decision for exactly that request.
pub fn answer(s: &mut Session, number: i64, allow: bool) -> Result<(), &'static str> {
    let link = s.codex.as_mut().ok_or("session_disconnected")?;
    let request_id = link.approval_map.get(&number).cloned().ok_or("unknown_approval")?;
    let decision = if allow { "accept" } else { "decline" };
    send(link, json!({"id": request_id, "result": {"decision": decision}}), None)?;
    link.approval_map.remove(&number);
    Ok(())
}

// ----------------------------------------------------------------- the process
fn spawn_provider(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>, resume: Option<String>) -> Result<(), &'static str> {
    let id = session.lock().unwrap().id.clone();
    let workspace = shared.cfg.workspace_root.join(&id);
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&workspace).map_err(|_| "busy")?;
    let mut command = Command::new(&shared.cfg.codex);
    command.args(&shared.cfg.codex_args).current_dir(&workspace).env_clear();
    for key in ["PATH", "HOME", "LANG", "CODEX_HOME"] {
        // the provider finds its own login state through these; nothing else of the host's environment is passed
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
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
    let mut link = Link { tx, active_turn: None, next_rpc: 0, pending: HashMap::new(), approval_map: HashMap::new(), buffers: HashMap::new(), ready: false };
    let init = json!({"clientInfo": {"name": "lico", "title": "Lico", "version": "0.0.0"}, "capabilities": {}});
    if rpc(&mut link, "initialize", init, Pending::Init, None).is_err() {
        unsafe { kill(-(pid as i32), 9) };
        let _ = child.wait();
        return Err("busy");
    }
    {
        let mut s = session.lock().unwrap();
        s.agent_pid = pid;
        s.process_group = true;
        s.codex = Some(link);
        s.state = State::Running;
        if let Some(thread) = &resume {
            s.codex_thread = Some(thread.clone());
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
    let cwd = workspace.to_string_lossy().to_string();
    thread::spawn(move || {
        for line in core::bounded_lines(stdout, MAX_PROVIDER_LINE) {
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                handle_message(&reader_session, &message, &cwd);
            }
        }
        unsafe { kill(-(pid as i32), 9) }; // its output ended: nothing of it, helpers included, is kept alive
        let _ = child.wait();
        let mut s = reader_session.lock().unwrap();
        if s.agent_pid == pid {
            s.agent_pid = 0; // reaped: the number may now belong to another process
        }
        if s.state == State::Running && s.codex.is_some() {
            s.state = State::Disconnected;
            s.codex = None; // its approvals and unanswered requests are stale now
            s.approvals.clear();
            record(&mut s, "[provider disconnected]");
        }
    });
    let deadline = Instant::now() + READY_WAIT;
    while Instant::now() < deadline {
        if session.lock().unwrap().codex.as_ref().map_or(false, |l| l.ready) {
            return Ok(());
        }
        if session.lock().unwrap().state != State::Running {
            return Err("busy");
        }
        thread::sleep(Duration::from_millis(5));
    }
    // never ready: the process is stopped and the session is left as it was before (disconnected to resume, nothing to create)
    let mut s = session.lock().unwrap();
    s.stop_process();
    s.codex = None;
    s.approvals.clear();
    s.state = if resume.is_some() { State::Disconnected } else { State::Exited };
    Err("busy")
}

pub fn start(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>) -> Result<(), &'static str> {
    spawn_provider(shared, session, None)
}

/// Bring a disconnected session back: a new provider process resumes the same thread, and
/// the execution generation moves on so every older target is stale.
pub fn resume(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>) -> Result<(), &'static str> {
    let thread = {
        let s = session.lock().unwrap();
        if s.state != State::Disconnected {
            return Err("busy");
        }
        s.codex_thread.clone()
    };
    spawn_provider(shared, session, thread)?;
    let mut s = session.lock().unwrap();
    record(&mut s, "[provider reconnected]");
    Ok(())
}

// ------------------------------------------------------------------- messages
fn handle_message(session: &Arc<Mutex<Session>>, message: &Value, cwd: &str) {
    let mut s = session.lock().unwrap();
    let method = message.get("method").and_then(Value::as_str);
    // an id is a number or a short string: it is echoed back to the provider, so it must stay small
    if !message.get("id").map_or(true, |id| id.is_i64() || id.as_str().map_or(false, |s| s.len() <= 64)) {
        return;
    }
    match (method, message.get("id")) {
        (None, Some(id)) => response(&mut s, id, message, cwd),
        (Some(method), Some(id)) => server_request(&mut s, id.clone(), method, message.get("params").unwrap_or(&Value::Null)),
        (Some(method), None) => notification(&mut s, method, message.get("params").unwrap_or(&Value::Null)),
        _ => {}
    }
}

fn response(s: &mut Session, id: &Value, message: &Value, cwd: &str) {
    let Some(rpc_id) = id.as_i64() else { return };
    let Some(link) = s.codex.as_mut() else { return };
    let Some(pending) = link.pending.remove(&rpc_id) else { return };
    let failed = message.get("error").map(|e| e.get("message").and_then(Value::as_str).unwrap_or("error").to_string());
    match pending {
        Pending::Init => {
            let _ = send(link, json!({"method": "initialized"}), None);
            let params = json!({"cwd": cwd, "approvalPolicy": "untrusted", "sandbox": "read-only", "approvalsReviewer": "user"});
            let resumed = s.codex_thread.clone();
            let link = s.codex.as_mut().unwrap();
            let _ = match resumed {
                Some(thread) => rpc(link, "thread/resume", json!({"threadId": thread, "excludeTurns": true, "cwd": cwd, "approvalPolicy": "untrusted", "sandbox": "read-only", "approvalsReviewer": "user"}), Pending::ThreadResume, None),
                None => rpc(link, "thread/start", params, Pending::ThreadStart, None),
            };
        }
        Pending::ThreadStart | Pending::ThreadResume => {
            if let Some(thread) = message.pointer("/result/thread/id").and_then(Value::as_str) {
                s.codex_thread = Some(thread.to_string());
                if let Some(link) = s.codex.as_mut() {
                    link.ready = true;
                }
            }
        }
        Pending::Steer(rid, text) if failed.is_some() => {
            // the turn ended before the steer arrived and the provider refused it, so nothing was
            // applied: the input starts a turn of its own (and is still never sent twice)
            let thread = s.codex_thread.clone().unwrap_or_default();
            if let Some(link) = s.codex.as_mut() {
                link.active_turn = None;
                let input = json!([{"type": "text", "text": text, "text_elements": []}]);
                if rpc(link, "turn/start", json!({"threadId": thread, "input": input}), Pending::Turn(rid.clone()), None).is_err() {
                    s.advance_input(&rid, InputState::Failed);
                }
            }
        }
        Pending::Turn(rid) | Pending::Steer(rid, _) => match failed {
            None => {
                if let Some(turn) = message.pointer("/result/turn/id").and_then(Value::as_str) {
                    if let Some(link) = s.codex.as_mut() {
                        link.active_turn = Some(turn.to_string());
                    }
                }
                s.advance_input(&rid, InputState::Observed);
            }
            Some(reason) => {
                s.advance_input(&rid, InputState::Failed);
                record(s, &format!("[input refused] {reason}"));
            }
        },
    }
}

fn server_request(s: &mut Session, id: Value, method: &str, params: &Value) {
    let Some(link) = s.codex.as_mut() else { return };
    let approval = matches!(method, "item/commandExecution/requestApproval" | "item/fileChange/requestApproval");
    if approval {
        let prompt = if method.ends_with("commandExecution/requestApproval") {
            format!("run: {}", params.get("command").and_then(Value::as_str).unwrap_or("a command"))
        } else {
            let reason = params.get("reason").and_then(Value::as_str).map(|r| format!(": {r}")).unwrap_or_default();
            format!("change files{reason}")
        };
        if link.approval_map.values().any(|known| *known == id) {
            return; // the provider repeated a request id: one approval, never two
        }
        if s.approvals.len() >= MAX_APPROVALS {
            // more than a person can answer: refuse to the provider instead of holding it
            let _ = send(link, json!({"id": id, "error": {"code": -32000, "message": "too many pending approvals"}}), None);
            return;
        }
        s.approval_counter += 1;
        let number = s.approval_counter;
        link.approval_map.insert(number, id);
        let shown = sanitize(&prompt);
        let shown = if shown.chars().count() > MAX_PROMPT { format!("{}…", shown.chars().take(MAX_PROMPT - 1).collect::<String>()) } else { shown };
        s.add_approval(number, shown);
    } else {
        // tokens, tools, attestations, elicitations and permission grants are not supplied by this client
        let _ = send(link, json!({"id": id, "error": {"code": -32601, "message": "unsupported by this client"}}), None);
    }
}

fn flush(s: &mut Session, item: &str) {
    let item = clip(item, 128);
    let item = item.as_str();
    let rest = s.codex.as_mut().and_then(|l| l.buffers.remove(item)).unwrap_or_default();
    if !rest.is_empty() {
        record(s, rest.trim_end_matches('\r'));
    }
}

fn delta(s: &mut Session, item: &str, text: &str) {
    let item = clip(item, 128);
    let item = item.as_str();
    let Some(link) = s.codex.as_mut() else { return };
    let whole = format!("{}{}", link.buffers.get(item).map(String::as_str).unwrap_or(""), text);
    let mut parts: Vec<&str> = whole.split('\n').collect();
    let mut rest = parts.pop().unwrap_or("").to_string();
    // a partial line is kept only up to one record; a longer one is written out now
    let mut lines: Vec<String> = parts.into_iter().map(|p| p.trim_end_matches('\r').to_string()).collect();
    if rest.len() >= MAX_RECORD {
        lines.push(std::mem::take(&mut rest));
    }
    link.buffers.insert(item.to_string(), rest);
    if link.buffers.len() > MAX_OPEN_ITEMS {
        // too many items with an open line: write out every partial line and start again
        lines.extend(link.buffers.drain().map(|(_, partial)| partial).filter(|partial| !partial.is_empty()));
    }
    for line in lines {
        record(s, &line);
    }
}

fn notification(s: &mut Session, method: &str, params: &Value) {
    let text = |p: &str| params.pointer(p).and_then(Value::as_str).unwrap_or("").to_string();
    match method {
        "item/agentMessage/delta" | "item/commandExecution/outputDelta" => delta(s, &text("/itemId"), &text("/delta")),
        "item/started" => match text("/item/type").as_str() {
            "commandExecution" => record(s, &format!("$ {}", text("/item/command"))),
            "fileChange" => {
                let count = params.pointer("/item/changes").and_then(Value::as_array).map_or(0, Vec::len);
                record(s, &format!("[patch] {count} change(s)"));
            }
            _ => {}
        },
        "item/completed" => {
            flush(s, &text("/item/id"));
            if text("/item/type") == "commandExecution" {
                match params.pointer("/item/exitCode").and_then(Value::as_i64) {
                    Some(code) => record(s, &format!("[exit {code}]")),
                    None => record(s, "[exit unknown]"),
                }
            }
        }
        "turn/started" => {
            if let Some(link) = s.codex.as_mut() {
                link.active_turn = params.pointer("/turn/id").and_then(Value::as_str).map(str::to_string);
            }
            record(s, "[turn started]");
        }
        "turn/completed" => {
            if let Some(link) = s.codex.as_mut() {
                link.active_turn = None;
            }
            record(s, &format!("[turn {}]", text("/turn/status")));
        }
        "error" => record(s, &format!("[error] {}", text("/error/message"))),
        "serverRequest/resolved" => {
            if let (Some(link), Some(resolved)) = (s.codex.as_mut(), params.get("requestId")) {
                let numbers: Vec<i64> = link.approval_map.iter().filter(|(_, v)| *v == resolved).map(|(n, _)| *n).collect();
                for number in numbers {
                    link.approval_map.remove(&number);
                    s.approvals.retain(|(n, _)| *n != number);
                }
            }
        }
        _ => {} // thread state and everything else is not shown
    }
}
