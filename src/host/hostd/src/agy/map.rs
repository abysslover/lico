//! The mapping of Agent Client Protocol messages (`agy acp`) to host records and approvals: the Rust port of `src/lico_protocol/agy_mapping.py`, which it must match
//! (the three fixtures of `tests/fixtures/agy_acp/` hold both to the same output). A host record is one UTF-8 line; everything the program says is untrusted display data.
//!
//! The port adds only what the line reader of the host makes necessary: a string it had to cut is shown with the note of what was left out (`lines::split_cut`), and a
//! permission request that was cut is refused, never put to a person. With nothing cut the output is the reference's, byte for byte.
//!
//! A message is mapped as it comes: `host_message` sees what the host sent (the requests it makes and the answers it gives), `agent_message` what the program sent.

use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::codex::{clip, sanitize};
use crate::lines;

pub const MAX_RECORD: usize = 4096;
pub const MAX_PROMPT: usize = 200;
const MAX_RESULT_LINES: usize = 20;
const MAX_PATCHES: usize = 8;
const MAX_NAME: usize = 60;
const MAX_ID: usize = 128;
const MAX_OPTIONS: usize = 8;
/// Approvals waiting at once; a request over this is cancelled at once.
pub const MAX_PENDING: usize = 8;
/// Tool calls followed at once; one more is not followed.
pub const MAX_TOOLS: usize = 256;
/// Characters of a line that has not ended; the line is cut here.
const MAX_PENDING_LINE: usize = 4 * MAX_RECORD;
const OPTION_KINDS: [&str; 4] = ["allow_once", "allow_always", "reject_once", "reject_always"];
/// The code the ACP documentation gives for "authenticate first".
pub const AUTH_REQUIRED: i64 = -32000;
pub const SIGN_IN_WORDS: &str = "[error] agy is not signed in: open a Terminal tab, run agy, sign in, then send the message again";
const SILENT_UPDATES: [&str; 7] = ["agent_thought_chunk", "user_message_chunk", "plan", "available_commands_update", "usage_update", "current_mode_update", "config_option_update"];
const NOT_ASKED: &str = "> not run: this version of Lico does not pass the question to you yet";

/// One line of the transcript: what the agent said (the host tags a line that looks like one of its own, and the peer turn keeps the raw line), or a note of the host's own.
#[derive(Debug, Clone, PartialEq)]
pub enum Rec {
    Agent { raw: String, text: String },
    Note(String),
}

#[cfg(test)]
impl Rec {
    pub fn text(&self) -> &str {
        match self {
            Rec::Agent { text, .. } | Rec::Note(text) => text,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Opt {
    pub option_id: String,
    pub kind: String,
    pub name: String,
}

/// A permission request waiting for the person.
#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub request_id: Value,
    pub prompt: String,
    pub options: Vec<Opt>,
}

/// An answer of the program to a request of the host, once it was matched with the request.
#[derive(Debug, Clone)]
pub struct Answered {
    pub method: String,
    pub id: Value,
    pub failed: bool,
    pub result: Value,
}

#[derive(Debug, Default)]
pub struct Mapped {
    pub records: Vec<Rec>,
    /// Pending when asked: the request id, the prompt and the options.
    pub approvals: Vec<Approval>,
    /// What the adapter sends back on its own, in order (a refusal, a cancelled outcome, an error for an unknown method).
    pub responses: Vec<Value>,
    pub ignored: BTreeMap<String, u32>,
    pub answered: Vec<Answered>,
}

impl Mapped {
    fn ignore(&mut self, key: impl Into<String>) {
        *self.ignored.entry(key.into()).or_insert(0) += 1;
    }

    fn emit(&mut self, line: &str) {
        self.records.push(Rec::Note(clip(&sanitize(line), MAX_RECORD)));
    }

    fn emit_agent(&mut self, line: &str) {
        self.records.push(Rec::Agent { raw: line.to_string(), text: crate::leads::agent_record(line, MAX_RECORD) });
    }

    /// `prefix` and `text` as one line, and the note of a text the reader cut as a line of its own.
    fn emit_text(&mut self, prefix: &str, text: &str) {
        for line in lines::with_note(prefix, text) {
            self.emit(&line);
        }
    }
}

/// The first `limit` characters of `text`.
fn first_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// A string of the program as a short plain name: cleaned, then cut to `limit` characters (a value that is not a string is empty).
fn clip_name(value: Option<&Value>, limit: usize) -> String {
    first_chars(&sanitize(value.and_then(Value::as_str).unwrap_or("")), limit)
}

/// A JSON-RPC id the host can echo back exactly: an integer or a short string (not a boolean, not a number with a fraction).
pub fn request_id_ok(value: &Value) -> bool {
    match value {
        Value::Number(n) => n.is_i64() || n.is_u64(),
        Value::String(s) => !s.is_empty() && s.chars().count() <= MAX_ID,
        _ => false,
    }
}

fn is_sign_in(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["auth", "signin", "sign in", "sign-in", "login", "log in", "log-in", "credential", "apikey", "api key", "api-key"].iter().any(|w| lower.contains(w))
}

/// The options of a permission request that the host understands: an id and one of the four kinds, at most eight, in the order given.
pub fn valid_options(options: Option<&Value>) -> Vec<Opt> {
    let mut kept = Vec::new();
    for option in options.and_then(Value::as_array).into_iter().flatten() {
        let id = option.get("optionId").and_then(Value::as_str).filter(|id| !id.is_empty() && id.chars().count() <= MAX_ID);
        let kind = option.get("kind").and_then(Value::as_str).filter(|k| OPTION_KINDS.contains(k));
        if let (Some(id), Some(kind)) = (id, kind) {
            kept.push(Opt { option_id: id.to_string(), kind: kind.to_string(), name: clip_name(option.get("name"), MAX_NAME) });
        }
        if kept.len() >= MAX_OPTIONS {
            break;
        }
    }
    kept
}

/// What the person is asked: the command for a command, the title otherwise.
pub fn approval_prompt(tool_call: Option<&Value>) -> String {
    let empty = Value::Null;
    let call = tool_call.unwrap_or(&empty);
    let command = call.get("rawInput").and_then(|raw| raw.get("command")).and_then(Value::as_str);
    let text = match (call.get("kind").and_then(Value::as_str), command) {
        (Some("execute"), Some(command)) => format!("run: {}", sanitize(command)),
        _ => match (call.get("title").and_then(Value::as_str), call.get("kind").and_then(Value::as_str)) {
            (Some(title), _) if !title.trim().is_empty() => sanitize(title),
            (_, Some(kind)) => sanitize(kind),
            _ => "tool".to_string(),
        },
    };
    if text.chars().count() <= MAX_PROMPT {
        text
    } else {
        format!("{}…", first_chars(&text, MAX_PROMPT - 1))
    }
}

/// The response to a permission request: exactly the narrowest option the person's answer means (`allow_once` for allow, `reject_once` for deny), and `cancelled` when the
/// program did not offer it: an answer is never widened to `allow_always` or `reject_always`.
pub fn answer(approval: &Approval, allow: bool) -> Value {
    let want = if allow { "allow_once" } else { "reject_once" };
    let outcome = match approval.options.iter().find(|o| o.kind == want) {
        Some(option) => json!({"outcome": "selected", "optionId": option.option_id}),
        None => json!({"outcome": "cancelled"}),
    };
    json!({"jsonrpc": "2.0", "id": approval.request_id, "result": {"outcome": outcome}})
}

fn error_message(error: &Value) -> String {
    clip_name(error.get("message"), 200)
}

fn error_records(error: &Value) -> Vec<String> {
    let message = error_message(error);
    let mut lines = vec!["[turn failed]".to_string(), format!("[error] {}", if message.is_empty() { "no message" } else { &message })];
    if error.get("code").and_then(Value::as_i64) == Some(AUTH_REQUIRED) || is_sign_in(&message) {
        lines.push(SIGN_IN_WORDS.to_string());
    }
    lines
}

pub struct Mapper {
    out: Mapped,
    buffer: String,
    /// Host request id (as JSON text) -> method, until the answer comes.
    methods: HashMap<String, String>,
    /// Host requests of `session/load` that have not been answered: what they replay is not copied.
    loading: HashSet<String>,
    /// Tool call id -> its result has been recorded.
    tools: HashMap<String, bool>,
    /// Agent request ids (as JSON text) of permission requests waiting for the person.
    pending: HashSet<String>,
    /// Whether a permission request may be put to a person; when not, each one is refused with a line that says why.
    ask: bool,
}

impl Mapper {
    pub fn new(ask: bool) -> Mapper {
        Mapper { out: Mapped::default(), buffer: String::new(), methods: HashMap::new(), loading: HashSet::new(), tools: HashMap::new(), pending: HashSet::new(), ask }
    }

    /// What was mapped since the last call.
    pub fn take(&mut self) -> Mapped {
        std::mem::take(&mut self.out)
    }

    pub fn flush(&mut self) {
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.out.emit_agent(&line);
        }
    }

    fn text(&mut self, piece: &str) {
        self.buffer.push_str(piece);
        let mut start = 0;
        while let Some(found) = self.buffer[start..].find('\n') {
            let end = start + found;
            self.out.emit_agent(self.buffer[start..end].trim_end_matches('\r'));
            start = end + 1;
        }
        self.buffer.drain(..start);
        if self.buffer.len() > MAX_PENDING_LINE && self.buffer.chars().count() > MAX_PENDING_LINE {
            self.flush();
        }
    }

    fn update(&mut self, update: Option<&Value>) {
        if !self.loading.is_empty() {
            return self.out.ignore("replayed"); // what a load replays is the conversation the log already holds
        }
        let kind = update.and_then(|u| u.get("sessionUpdate")).and_then(Value::as_str);
        match (kind, update) {
            (Some("agent_message_chunk"), Some(update)) => match update.get("content") {
                Some(content) if content.get("type").and_then(Value::as_str) == Some("text") && content.get("text").is_some_and(Value::is_string) => {
                    let (head, note) = lines::split_cut(content["text"].as_str().unwrap_or(""));
                    self.text(head);
                    if let Some(note) = note {
                        self.flush();
                        self.out.emit(&note);
                    }
                }
                _ => self.out.ignore("agent_message_chunk:not_text"),
            },
            (Some("tool_call"), Some(update)) => self.tool_call(update),
            (Some("tool_call_update"), Some(update)) => self.tool_update(update),
            (Some(kind), _) if SILENT_UPDATES.contains(&kind) => self.out.ignore(kind),
            _ => {
                let kind = first_chars(&sanitize(kind.unwrap_or("")), MAX_NAME);
                self.out.ignore(format!("update:{kind}"));
            }
        }
    }

    fn tool_call(&mut self, update: &Value) {
        let Some(call_id) = update.get("toolCallId").and_then(Value::as_str).filter(|id| !id.is_empty() && id.chars().count() <= MAX_ID) else {
            return self.out.ignore("tool_call:bad_id");
        };
        if self.tools.contains_key(call_id) {
            return self.tool_update(update);
        }
        if self.tools.len() >= MAX_TOOLS {
            return self.out.ignore("tool_call:too_many");
        }
        self.flush();
        self.tools.insert(call_id.to_string(), false);
        let command = update.get("rawInput").and_then(|raw| raw.get("command")).and_then(Value::as_str);
        match (update.get("kind").and_then(Value::as_str), command) {
            (Some("execute"), Some(command)) => self.out.emit_text("$ ", command),
            _ => {
                let title = update.get("title").filter(|t| t.as_str().is_some_and(|t| !t.trim().is_empty()));
                let name = clip_name(title.or(update.get("kind")), MAX_NAME);
                self.out.emit(&format!("[tool] {name}"));
            }
        }
        self.tool_update(update);
    }

    fn tool_update(&mut self, update: &Value) {
        let call_id = update.get("toolCallId").and_then(Value::as_str);
        let status = update.get("status").and_then(Value::as_str);
        let Some(call_id) = call_id.filter(|id| self.tools.get(*id) == Some(&false)) else { return };
        if !matches!(status, Some("completed" | "failed")) {
            return;
        }
        self.flush();
        self.tools.insert(call_id.to_string(), true);
        if status == Some("failed") {
            self.out.emit("[tool error]");
        }
        let (mut patches, mut shown) = (0, 0);
        for block in update.get("content").and_then(Value::as_array).into_iter().flatten() {
            let inner = block.get("content");
            let kind = block.get("type").and_then(Value::as_str);
            if let (Some("content"), Some(inner)) = (kind, inner) {
                if inner.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(text) = inner.get("text").and_then(Value::as_str) {
                        for line in lines::shown(text).split('\n') {
                            if shown >= MAX_RESULT_LINES {
                                break;
                            }
                            self.out.emit(&format!("> {}", line.trim_end_matches('\r')));
                            shown += 1;
                        }
                    }
                }
            } else if kind == Some("diff") && patches < MAX_PATCHES {
                let path = first_chars(&sanitize(block.get("path").and_then(Value::as_str).unwrap_or("")), 120);
                self.out.emit(&format!("[patch] {path}"));
                patches += 1;
            }
        }
    }

    fn permission(&mut self, request_id: &Value, params: Option<&Value>, cut: bool) {
        let options = valid_options(params.and_then(|p| p.get("options")));
        let key = request_id.to_string();
        if !request_id_ok(request_id) || self.pending.contains(&key) {
            return self.out.ignore("permission:bad_or_repeated_id"); // a repeated id is one approval, never two
        }
        self.flush();
        let approval = Approval { request_id: request_id.clone(), prompt: approval_prompt(params.and_then(|p| p.get("toolCall"))), options };
        let has_allow = approval.options.iter().any(|o| o.kind == "allow_once");
        if self.pending.len() >= MAX_PENDING || !has_allow || cut || !self.ask {
            self.out.ignore("permission:refused");
            self.out.emit("[tool error]");
            if !self.ask && has_allow && !cut && self.pending.len() < MAX_PENDING {
                self.out.emit(NOT_ASKED);
            }
            self.out.responses.push(answer(&approval, false)); // cancelled when nothing could be denied once
            return;
        }
        self.pending.insert(key);
        self.out.approvals.push(approval);
    }

    /// A message of the program. `cut` says that the reader had to cut a string of it (see `lines::has_cut`).
    pub fn agent_message(&mut self, message: &Value, cut: bool) {
        let ident = message.get("id").filter(|v| !v.is_null());
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            if method == "session/update" {
                self.update(message.get("params").and_then(|p| p.get("update")));
            } else if method == "session/request_permission" && ident.is_some() {
                self.permission(ident.unwrap(), message.get("params"), cut);
            } else if let Some(ident) = ident.filter(|id| request_id_ok(id)) {
                self.out.ignore(format!("refused:{}", first_chars(&sanitize(method), MAX_NAME))); // fs/*, terminal/* and the rest: the host offered none of them
                self.out.responses.push(json!({"jsonrpc": "2.0", "id": ident, "error": {"code": -32601, "message": "Method not found"}}));
            } else {
                self.out.ignore(format!("notification:{}", first_chars(&sanitize(method), MAX_NAME)));
            }
            return;
        }
        let Some(ident) = ident else { return self.out.ignore("unknown") };
        if message.get("result").is_none() && message.get("error").is_none() {
            return self.out.ignore("unknown");
        }
        self.response(ident, message);
    }

    fn response(&mut self, ident: &Value, message: &Value) {
        let key = ident.to_string();
        let method = if request_id_ok(ident) { self.methods.remove(&key) } else { None };
        let result = message.get("result").cloned().unwrap_or(Value::Null);
        let error = message.get("error").filter(|e| e.is_object());
        self.loading.remove(&key);
        let Some(method) = method else { return self.out.ignore("response:unknown_request") };
        self.out.answered.push(Answered { method: method.clone(), id: ident.clone(), failed: error.is_some() || message.get("result").is_none(), result: result.clone() });
        if method == "session/prompt" {
            self.flush();
            if let Some(error) = error {
                for line in error_records(error) {
                    self.out.emit(&line);
                }
            } else {
                let reason = result.get("stopReason").and_then(Value::as_str);
                let line = match reason {
                    Some("end_turn") => "[turn completed]".to_string(),
                    Some("cancelled") => "[turn stopped]".to_string(),
                    _ => {
                        let reason = clip_name(result.get("stopReason"), MAX_NAME);
                        format!("[turn failed] {}", if reason.is_empty() { "no stop reason" } else { &reason })
                    }
                };
                self.out.emit(&line);
            }
        } else if let Some(error) = error {
            self.flush();
            let message = error_message(error);
            self.out.emit(&format!("[error] {}", if message.is_empty() { "no message" } else { &message }));
            if error.get("code").and_then(Value::as_i64) == Some(AUTH_REQUIRED) || is_sign_in(&message) {
                self.out.emit(SIGN_IN_WORDS);
            }
        } else if method == "session/new" && result.get("sessionId").is_some_and(Value::is_string) {
            let model = result.get("models").filter(|m| m.is_object()).map(|m| clip_name(m.get("currentModelId"), MAX_NAME)).unwrap_or_default();
            self.out.emit(&if model.is_empty() { "[session started]".to_string() } else { format!("[session started] model={model}") });
        } else if method == "session/load" || method == "session/resume" {
            self.out.emit("[provider reconnected]");
        } else if method != "initialize" {
            self.out.ignore(format!("response:{method}"));
        }
    }

    /// The host's own request is forgotten: its answer is not shown (the host will try another way).
    pub fn drop_request(&mut self, ident: &Value) {
        let key = ident.to_string();
        self.methods.remove(&key);
        self.loading.remove(&key);
    }

    /// A message of the host: a request it makes (its answer is matched by id), or its answer to a permission request.
    pub fn host_message(&mut self, message: &Value) {
        let ident = message.get("id").filter(|v| !v.is_null());
        if let (Some(method), Some(ident)) = (message.get("method").and_then(Value::as_str), ident.filter(|id| request_id_ok(id))) {
            self.methods.insert(ident.to_string(), method.to_string());
            if method == "session/load" {
                self.loading.insert(ident.to_string());
            }
        } else if let (true, Some(ident)) = (message.get("result").is_some(), ident) {
            self.pending.remove(&ident.to_string()); // the host answered a permission request
        }
    }

    /// A turn ended: the permission requests it left unanswered are over.
    pub fn forget_pending(&mut self) {
        self.pending.clear();
    }

    /// Whether a permission request of the program waits for the person.
    pub fn waiting(&self, request_id: &Value) -> bool {
        self.pending.contains(&request_id.to_string())
    }
}

/// The whole sequence at once, as the reference does: `{"dir": "host" | "agent", "msg": {...}}` items.
#[cfg(test)]
pub fn map_stream(stream: &[Value], ask: bool) -> Mapped {
    let mut mapper = Mapper::new(ask);
    let mut all = Mapped::default();
    for item in stream {
        if item["dir"] == "host" {
            mapper.host_message(&item["msg"]);
        } else {
            mapper.agent_message(&item["msg"], false);
        }
        merge(&mut all, mapper.take());
    }
    mapper.flush();
    merge(&mut all, mapper.take());
    all
}

#[cfg(test)]
fn merge(all: &mut Mapped, part: Mapped) {
    all.records.extend(part.records);
    all.approvals.extend(part.approvals);
    all.responses.extend(part.responses);
    all.answered.extend(part.answered);
    for (k, v) in part.ignored {
        *all.ignored.entry(k).or_insert(0) += v;
    }
}
