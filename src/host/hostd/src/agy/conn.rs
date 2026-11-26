//! The conversation with `agy acp` as a state machine with no input or output of its own: what comes in (the start, an input of the person, a message of the program, the end of its
//! output, the clock) goes in as a call, and what is to happen comes out as effects that the shell (`mod.rs`) applies to the process and to the session. That keeps the order of the
//! protocol testable without a process: nothing is sent before `initialize` is answered, a conversation is opened (`session/new`, or `session/resume` and `session/load` for a stored
//! one) before the first prompt, one prompt is in flight at a time and the inputs wait behind it, and a request that must be answered has a deadline.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::map::{Approval, Mapper, Rec};
use crate::lines;

/// The version of the Agent Client Protocol this host speaks (the record `capabilities.json` pins it).
pub const PROTOCOL: u64 = 1;
/// How long `initialize` may take, and how long opening a conversation may take (a stored one is read from disk).
pub const INIT_WAIT: Duration = Duration::from_secs(20);
pub const OPEN_WAIT: Duration = Duration::from_secs(60);

/// What is to be done; the shell applies the effects in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A message for the program.
    Send(Value),
    /// A line of the transcript.
    Rec(Rec),
    /// An input of the person was written to the program, the program has taken it up, or it failed.
    Submitted(String),
    Observed(String),
    Failed(String),
    /// The id of the conversation (to be kept, so that a restart can bring it back) and the model the program names.
    Session(String),
    Model(String),
    /// A turn ended: how (`ok` only for a turn that was completed), and whether another input is already going out.
    Turn { ok: bool, more: bool },
    /// A permission request that waits for the person (only when the conversation was made to ask).
    Approval(Approval),
    /// The connection cannot go on: the program is ended.
    Fatal,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Idle,
    Init,
    Opening,
    Ready,
    Dead,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Way {
    New,
    Resume,
    Load,
}

impl Way {
    fn method(self) -> &'static str {
        match self {
            Way::New => "session/new",
            Way::Resume => "session/resume",
            Way::Load => "session/load",
        }
    }
}

struct Flight {
    request_id: String,
    observed: bool,
}

/// Whether a message of the program shows that it took the prompt up: a word, a thought, a plan or a tool call of the turn, or a permission request. The updates of the session
/// itself (the commands it offers, the usage, the mode) arrive right after `session/new`, when the first prompt may already be out, and prove nothing about it.
fn took_up(message: &Value) -> bool {
    match message.get("method").and_then(Value::as_str) {
        Some("session/request_permission") => true,
        Some("session/update") => {
            let kind = message.get("params").and_then(|p| p.get("update")).and_then(|u| u.get("sessionUpdate")).and_then(Value::as_str);
            matches!(kind, Some("agent_message_chunk" | "agent_thought_chunk" | "plan" | "tool_call" | "tool_call_update"))
        }
        _ => false,
    }
}

pub struct Conn {
    mapper: Mapper,
    phase: Phase,
    next_id: u64,
    cwd: String,
    /// The conversation to bring back, as the registry kept it.
    stored: Option<String>,
    can_resume: bool,
    can_load: bool,
    way: Way,
    open_id: Option<Value>,
    queue: VecDeque<(String, String)>,
    flight: Option<Flight>,
    deadline: Option<(Instant, &'static str)>,
    /// The permission requests that wait for the person.
    held: Vec<Approval>,
}

impl Conn {
    pub fn new(cwd: String, stored: Option<String>, ask: bool) -> Conn {
        Conn {
            mapper: Mapper::new(ask),
            phase: Phase::Idle,
            next_id: 0,
            cwd,
            stored,
            can_resume: false,
            can_load: false,
            way: Way::New,
            open_id: None,
            queue: VecDeque::new(),
            flight: None,
            deadline: None,
            held: Vec::new(),
        }
    }

    fn send(&mut self, out: &mut Vec<Effect>, message: Value) {
        self.mapper.host_message(&message);
        out.push(Effect::Send(message));
    }

    fn request(&mut self, out: &mut Vec<Effect>, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = json!(self.next_id);
        self.send(out, json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    pub fn start(&mut self, now: Instant) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.phase != Phase::Idle {
            return out;
        }
        self.phase = Phase::Init;
        self.deadline = Some((now + INIT_WAIT, "initialize"));
        let params = json!({
            "protocolVersion": PROTOCOL,
            "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
            "clientInfo": {"name": "lico", "version": env!("CARGO_PKG_VERSION")},
        });
        self.request(&mut out, "initialize", params);
        out
    }

    /// The input of the person: it waits behind the turn that is running, and behind the opening of the conversation.
    pub fn input(&mut self, request_id: &str, text: &str) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.phase == Phase::Dead {
            out.push(Effect::Failed(request_id.to_string()));
            return out;
        }
        self.queue.push_back((request_id.to_string(), text.to_string()));
        self.pump(&mut out);
        out
    }

    fn pump(&mut self, out: &mut Vec<Effect>) {
        if self.phase != Phase::Ready || self.flight.is_some() {
            return;
        }
        let Some((request_id, text)) = self.queue.pop_front() else { return };
        let params = json!({"sessionId": self.session_id(), "prompt": [{"type": "text", "text": text}]});
        self.request(out, "session/prompt", params);
        self.flight = Some(Flight { request_id: request_id.clone(), observed: false });
        out.push(Effect::Submitted(request_id));
    }

    fn session_id(&self) -> String {
        self.stored.clone().unwrap_or_default()
    }

    fn open(&mut self, out: &mut Vec<Effect>, now: Instant, way: Way) {
        self.phase = Phase::Opening;
        self.way = way;
        self.deadline = Some((now + OPEN_WAIT, way.method()));
        let mut params = json!({"cwd": self.cwd, "mcpServers": []});
        if way != Way::New {
            params["sessionId"] = json!(self.stored.clone().unwrap_or_default());
        }
        let id = self.request(out, way.method(), params);
        self.open_id = Some(id);
    }

    fn fatal(&mut self, out: &mut Vec<Effect>, line: &str) {
        self.phase = Phase::Dead;
        self.deadline = None;
        self.mapper.flush();
        self.drain_records(out);
        out.push(Effect::Rec(Rec::Note(line.to_string())));
        self.fail_queue(out);
        out.push(Effect::Fatal);
    }

    /// The inputs that were taken and never written to the program did not reach it: they are failed, so that the person may send them again. (One that was written stays as it
    /// is: whether the program took it up is not known, and an ambiguous input is never sent a second time by the host.)
    fn fail_queue(&mut self, out: &mut Vec<Effect>) {
        out.extend(self.queue.drain(..).map(|(request_id, _)| Effect::Failed(request_id)));
    }

    fn drain_records(&mut self, out: &mut Vec<Effect>) {
        let mapped = self.mapper.take();
        out.extend(mapped.records.into_iter().map(Effect::Rec));
    }

    /// A message of the program, parsed. `cut` says that the reader cut a string of it.
    pub fn agent(&mut self, now: Instant, message: &Value, cut: bool) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.phase == Phase::Dead || self.phase == Phase::Idle {
            return out;
        }
        // a stored conversation that `session/resume` could not bring back is tried with `session/load` before anything is said about it
        let refused_open = message.get("method").is_none() && message.get("error").is_some_and(Value::is_object) && message.get("id").is_some() && message.get("id") == self.open_id.as_ref();
        if self.phase == Phase::Opening && self.way == Way::Resume && self.can_load && refused_open {
            self.mapper.drop_request(message.get("id").unwrap());
            self.open(&mut out, now, Way::Load);
            return out;
        }
        self.mapper.agent_message(message, cut);
        let mapped = self.mapper.take();
        let took_up = took_up(message);
        out.extend(mapped.records.into_iter().map(Effect::Rec));
        for approval in mapped.approvals {
            self.held.push(approval.clone());
            out.push(Effect::Approval(approval));
        }
        for response in mapped.responses {
            self.send(&mut out, response);
        }
        if took_up {
            if let Some(flight) = self.flight.as_mut().filter(|f| !f.observed) {
                flight.observed = true;
                out.push(Effect::Observed(flight.request_id.clone()));
            }
        }
        for answered in mapped.answered {
            if self.phase == Phase::Dead {
                break;
            }
            self.answered(&mut out, now, &answered.method, &answered.id, answered.failed, &answered.result);
        }
        if self.flight.is_none() && self.phase != Phase::Dead {
            self.mapper.flush(); // words after the end of a turn are not held back for the next one
            self.drain_records(&mut out);
        }
        out
    }

    fn answered(&mut self, out: &mut Vec<Effect>, now: Instant, method: &str, id: &Value, failed: bool, result: &Value) {
        match (self.phase, method) {
            (Phase::Init, "initialize") => {
                self.deadline = None;
                if failed {
                    return self.fatal(out, "[error] agy did not start its conversation protocol");
                }
                if result.get("protocolVersion").and_then(Value::as_u64) != Some(PROTOCOL) {
                    let seen = result.get("protocolVersion").map_or("none".to_string(), |v| crate::codex::clip(&crate::codex::sanitize(&v.to_string()), 20));
                    return self.fatal(out, &format!("[error] agy speaks protocol version {seen}; this Lico speaks {PROTOCOL}"));
                }
                let caps = result.get("agentCapabilities");
                self.can_load = caps.and_then(|c| c.get("loadSession")).and_then(Value::as_bool) == Some(true);
                self.can_resume = caps.and_then(|c| c.get("sessionCapabilities")).and_then(|c| c.get("resume")).is_some_and(|r| !r.is_null());
                let way = match (&self.stored, self.can_resume, self.can_load) {
                    (None, _, _) => Way::New,
                    (Some(_), true, _) => Way::Resume,
                    (Some(_), false, true) => Way::Load,
                    (Some(_), false, false) => return self.fatal(out, "[error] this agy cannot bring an earlier conversation back; start a new session"),
                };
                self.open(out, now, way);
            }
            (Phase::Opening, m) if m == self.way.method() && Some(id) == self.open_id.as_ref() => {
                self.deadline = None;
                if failed {
                    let line = if self.way == Way::New { "[error] agy could not start a conversation" } else { "[error] agy could not bring this conversation back; start a new session" };
                    return self.fatal(out, line);
                }
                if self.way == Way::New {
                    let Some(session) = result.get("sessionId").and_then(Value::as_str).filter(|id| crate::provider_record::valid_provider_session(id)) else {
                        return self.fatal(out, "[error] agy gave no usable conversation id");
                    };
                    self.stored = Some(session.to_string());
                    out.push(Effect::Session(session.to_string()));
                    if let Some(model) = result.get("models").and_then(|m| m.get("currentModelId")).and_then(Value::as_str) {
                        let model = crate::codex::clip(&crate::codex::sanitize(model), 60);
                        if !model.is_empty() {
                            out.push(Effect::Model(model));
                        }
                    }
                }
                self.phase = Phase::Ready;
                self.pump(out);
            }
            (Phase::Ready, "session/prompt") => {
                let Some(flight) = self.flight.take() else { return };
                let ok = !failed && result.get("stopReason").and_then(Value::as_str) == Some("end_turn");
                if !flight.observed {
                    out.push(if failed { Effect::Failed(flight.request_id) } else { Effect::Observed(flight.request_id) }); // an input the program refused was never taken up
                }
                self.end_turn(out, ok);
            }
            _ => {}
        }
    }

    fn end_turn(&mut self, out: &mut Vec<Effect>, ok: bool) {
        self.held.clear(); // a request the turn left unanswered is over with it: the program does not wait for it any more
        self.mapper.forget_pending();
        out.push(Effect::Turn { ok, more: !self.queue.is_empty() });
        self.pump(out);
    }

    /// An answer of the person to a permission request that was put to them.
    pub fn answer(&mut self, request_id: &Value, allow: bool) -> Vec<Effect> {
        let mut out = Vec::new();
        let Some(at) = self.held.iter().position(|a| &a.request_id == request_id) else { return out };
        let approval = self.held.remove(at);
        if self.phase != Phase::Dead && self.mapper.waiting(request_id) {
            self.send(&mut out, super::map::answer(&approval, allow));
        }
        out
    }

    /// A line of the program that could not be held or read: it is said in the transcript with its size.
    pub fn omitted(&mut self, bytes: usize) -> Vec<Effect> {
        if self.phase == Phase::Dead {
            return Vec::new();
        }
        vec![Effect::Rec(Rec::Note(lines::omitted_line(bytes)))]
    }

    /// The moment a request that must be answered runs out, if one is waiting.
    pub fn due(&self) -> Option<Instant> {
        self.deadline.map(|(at, _)| at)
    }

    pub fn expired(&mut self, now: Instant) -> Vec<Effect> {
        let mut out = Vec::new();
        if let Some((_, what)) = self.deadline.filter(|(at, _)| *at <= now) {
            self.fatal(&mut out, &format!("[error] agy did not answer {what} in time"));
        }
        out
    }

    /// The output of the program ended: what was said and not yet written as a line is written.
    pub fn closed(&mut self) -> Vec<Effect> {
        let mut out = Vec::new();
        if self.phase == Phase::Dead {
            return out;
        }
        self.phase = Phase::Dead;
        self.deadline = None;
        self.mapper.flush();
        self.drain_records(&mut out);
        self.fail_queue(&mut out);
        out
    }
}
