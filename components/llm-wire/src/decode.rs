//! Reading a reply into events, in any split of the bytes into pieces. OpenAI streams are server-sent events (lines `data: ...` separated by a blank line, `[DONE]`
//! at the end); Ollama streams are one JSON object per line; a whole body is one JSON object. The decoder buffers at most one line (or one whole body) and the calls
//! being built, so its memory does not depend on the length of the answer. Calls are given only just before `Done`: a reply that is cut or fails never gives one.

use serde_json::Value;

use crate::failure::classify;
use crate::types::{Api, Done, Event, Failure, Framing, StopReason, StreamError, ToolCall, Usage};

/// The longest stream line (or server-sent event) that is read.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// The longest whole body that is read.
pub const MAX_WHOLE_BYTES: usize = 4 * 1024 * 1024;
/// The most bytes of arguments one call may have.
pub const MAX_CALL_ARGUMENT_BYTES: usize = 64 * 1024;
/// The most calls one answer may have.
pub const MAX_CALLS: usize = 16;
const RAW_CUT: usize = 2048;

/// A call as it is being built: OpenAI sends it in pieces by `index`, Ollama in one piece.
struct Partial {
    index: usize,
    id: Option<String>,
    name: String,
    args: String,
    value: Option<Value>,
}

pub struct Decoder {
    api: Api,
    framing: Framing,
    buf: Vec<u8>,
    scanned: usize,
    event: Vec<u8>,
    has_event: bool,
    finished: bool,
    /// Set while the last bytes are read: a last line that does not parse was cut, it is not malformed.
    ending: bool,
    out: Vec<Event>,
    thinking: bool,
    calls: Vec<Partial>,
    reason: Option<StopReason>,
    usage: Option<Usage>,
}

impl Decoder {
    pub fn new(api: Api, framing: Framing) -> Decoder {
        Decoder { api, framing, buf: Vec::new(), scanned: 0, event: Vec::new(), has_event: false, finished: false, ending: false, out: Vec::new(), thinking: false, calls: Vec::new(), reason: None, usage: None }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// More bytes of the reply, in any piece; gives what they complete.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        if self.finished {
            return Vec::new();
        }
        self.buf.extend_from_slice(bytes);
        match self.framing {
            Framing::Whole => {
                if self.buf.len() > MAX_WHOLE_BYTES {
                    self.fail(StreamError::TooLarge);
                }
            }
            Framing::Stream => self.pump(),
        }
        std::mem::take(&mut self.out)
    }

    /// The end of the bytes: the last line, then the end of the answer (or `Truncated` when the reply did not reach its own end).
    pub fn finish(&mut self) -> Vec<Event> {
        if self.finished {
            return Vec::new();
        }
        match self.framing {
            Framing::Whole => self.whole(),
            Framing::Stream => {
                self.ending = true;
                if !self.buf.is_empty() {
                    let rest = std::mem::take(&mut self.buf);
                    self.scanned = 0;
                    self.line(&rest);
                }
                if !self.finished && self.api == Api::OpenAi {
                    self.dispatch();
                }
                if !self.finished {
                    // an OpenAI stream that gave its finish reason but lost its `[DONE]` is complete; anything else is cut
                    if self.api == Api::OpenAi && self.reason.is_some() {
                        self.complete();
                    } else {
                        self.fail(StreamError::Truncated);
                    }
                }
            }
        }
        std::mem::take(&mut self.out)
    }

    // ---- framing -----------------------------------------------------------------------------------------------------------

    /// Reads every complete line of the buffer; the bytes after the last newline wait for the next piece.
    fn pump(&mut self) {
        let mut start = 0usize;
        let mut from = self.scanned.max(start);
        while !self.finished {
            let Some(rel) = self.buf[from..].iter().position(|b| *b == b'\n') else { break };
            let end = from + rel;
            let mut line = &self.buf[start..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            if line.len() > MAX_LINE_BYTES {
                self.fail(StreamError::TooLarge);
                break;
            }
            let owned = line.to_vec();
            start = end + 1;
            from = start;
            self.line(&owned);
        }
        if self.finished {
            self.buf.clear();
            self.scanned = 0;
            return;
        }
        self.buf.drain(..start);
        self.scanned = self.buf.len();
        if self.buf.len() > MAX_LINE_BYTES {
            self.fail(StreamError::TooLarge);
        }
    }

    fn line(&mut self, line: &[u8]) {
        match self.api {
            Api::OpenAi => self.sse_line(line),
            Api::Ollama => {
                let text = String::from_utf8_lossy(line);
                let text = text.trim();
                if text.is_empty() {
                    return;
                }
                match serde_json::from_str::<Value>(text) {
                    Ok(v) => self.ollama(&v),
                    Err(_) if self.ending => self.fail(StreamError::Truncated),
                    Err(e) => self.fail(StreamError::Malformed(format!("a line is not JSON: {e}"))),
                }
            }
        }
    }

    /// One line of a server-sent event: a blank line ends the event; `data` lines are joined; comments and other fields are ignored.
    fn sse_line(&mut self, line: &[u8]) {
        if line.is_empty() {
            self.dispatch();
            return;
        }
        if line[0] == b':' {
            return;
        }
        let (field, value) = match line.iter().position(|b| *b == b':') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line, &line[line.len()..]),
        };
        if field != b"data" {
            return;
        }
        let value = value.strip_prefix(b" ").unwrap_or(value);
        if self.has_event {
            self.event.push(b'\n');
        }
        self.event.extend_from_slice(value);
        self.has_event = true;
        if self.event.len() > MAX_LINE_BYTES {
            self.fail(StreamError::TooLarge);
        }
    }

    /// The server-sent event that was collected, if any.
    fn dispatch(&mut self) {
        if !self.has_event || self.finished {
            self.event.clear();
            self.has_event = false;
            return;
        }
        let data = String::from_utf8_lossy(&self.event).into_owned();
        self.event.clear();
        self.has_event = false;
        let data = data.trim();
        if data == "[DONE]" {
            self.complete();
            return;
        }
        match serde_json::from_str::<Value>(data) {
            Ok(v) => self.openai(&v, false),
            Err(_) if self.ending => self.fail(StreamError::Truncated),
            Err(e) => self.fail(StreamError::Malformed(format!("an event is not JSON: {e}"))),
        }
    }

    fn whole(&mut self) {
        let body = std::mem::take(&mut self.buf);
        let text = String::from_utf8_lossy(&body);
        if text.trim().is_empty() {
            return self.fail(StreamError::Truncated);
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => {
                match self.api {
                    Api::OpenAi => self.openai(&v, true),
                    Api::Ollama => self.ollama(&v),
                }
                if !self.finished {
                    self.complete();
                }
            }
            Err(e) => self.fail(StreamError::Malformed(format!("the body is not JSON: {e}"))),
        }
    }

    // ---- the two readings of a chunk ---------------------------------------------------------------------------------------

    /// An OpenAI chunk: a stream's `choices[0].delta`, or a whole body's `choices[0].message`.
    fn openai(&mut self, v: &Value, whole: bool) {
        if v.get("error").is_some() {
            return self.server_error(v);
        }
        if let Some(u) = usage_of(&v["usage"], "prompt_tokens", "completion_tokens") {
            self.usage = Some(u);
        }
        let Some(choice) = v["choices"].as_array().and_then(|c| c.first()) else { return };
        let node = &choice[if whole { "message" } else { "delta" }];
        for key in ["reasoning_content", "reasoning"] {
            if node[key].as_str().is_some_and(|s| !s.is_empty()) {
                self.think();
            }
        }
        if let Some(text) = node["content"].as_str().filter(|s| !s.is_empty()) {
            self.out.push(Event::Text(text.to_string()));
        }
        if let Some(calls) = node["tool_calls"].as_array() {
            for (position, c) in calls.iter().enumerate() {
                if self.finished {
                    return;
                }
                let index = c["index"].as_u64().map(|i| i as usize).unwrap_or(position);
                self.openai_call(index, c);
            }
        }
        if let Some(r) = choice["finish_reason"].as_str() {
            self.reason = Some(match r {
                "stop" => StopReason::Stop,
                "tool_calls" | "function_call" => StopReason::ToolCalls,
                "length" => StopReason::Length,
                _ => StopReason::Other,
            });
        }
    }

    fn openai_call(&mut self, index: usize, c: &Value) {
        let at = match self.calls.iter().position(|p| p.index == index) {
            Some(at) => at,
            None => {
                if self.calls.len() >= MAX_CALLS {
                    return self.fail(StreamError::TooLarge);
                }
                self.calls.push(Partial { index, id: None, name: String::new(), args: String::new(), value: None });
                self.calls.len() - 1
            }
        };
        let p = &mut self.calls[at];
        if let Some(id) = c["id"].as_str().filter(|s| !s.is_empty()) {
            p.id.get_or_insert_with(|| id.to_string());
        }
        let f = &c["function"];
        if let Some(name) = f["name"].as_str().filter(|s| !s.is_empty()) {
            if p.name.is_empty() {
                p.name = name.to_string();
            } else if p.name != name {
                p.name.push_str(name);
            }
        }
        match &f["arguments"] {
            Value::String(piece) => {
                if p.args.len() + piece.len() > MAX_CALL_ARGUMENT_BYTES {
                    return self.fail(StreamError::TooLarge);
                }
                p.args.push_str(piece);
            }
            v @ Value::Object(_) => p.value = Some(v.clone()),
            _ => {}
        }
    }

    /// An Ollama object: a stream's line, or a whole body.
    fn ollama(&mut self, v: &Value) {
        if v.get("error").is_some() {
            return self.server_error(v);
        }
        let m = &v["message"];
        if m["thinking"].as_str().is_some_and(|s| !s.is_empty()) {
            self.think();
        }
        if let Some(text) = m["content"].as_str().filter(|s| !s.is_empty()) {
            self.out.push(Event::Text(text.to_string()));
        }
        if let Some(calls) = m["tool_calls"].as_array() {
            for c in calls {
                if self.calls.len() >= MAX_CALLS {
                    return self.fail(StreamError::TooLarge);
                }
                let f = &c["function"];
                let (mut args, mut value) = (String::new(), None);
                match &f["arguments"] {
                    Value::String(s) => {
                        if s.len() > MAX_CALL_ARGUMENT_BYTES {
                            return self.fail(StreamError::TooLarge);
                        }
                        args = s.clone();
                    }
                    other if !other.is_null() => {
                        if other.to_string().len() > MAX_CALL_ARGUMENT_BYTES {
                            return self.fail(StreamError::TooLarge);
                        }
                        value = Some(other.clone());
                    }
                    _ => {}
                }
                let index = self.calls.len();
                self.calls.push(Partial { index, id: None, name: f["name"].as_str().unwrap_or("").to_string(), args, value });
            }
        }
        if v["done"].as_bool() == Some(true) {
            self.reason = Some(match v["done_reason"].as_str() {
                Some("stop") | None => StopReason::Stop,
                Some("length") => StopReason::Length,
                Some(_) => StopReason::Other,
            });
            if let (Some(p), Some(c)) = (v["prompt_eval_count"].as_u64(), v["eval_count"].as_u64()) {
                self.usage = Some(Usage { prompt_tokens: p, completion_tokens: c });
            }
            self.complete();
        }
    }

    // ---- the end -----------------------------------------------------------------------------------------------------------

    fn think(&mut self) {
        if !self.thinking {
            self.thinking = true;
            self.out.push(Event::Thinking);
        }
    }

    fn server_error(&mut self, v: &Value) {
        let failure: Failure = classify(self.api, 0, v.to_string().as_bytes());
        self.fail(StreamError::Server(failure));
    }

    fn fail(&mut self, e: StreamError) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.calls.clear();
        self.buf.clear();
        self.event.clear();
        self.out.push(Event::Error(e));
    }

    /// The good end: the calls, then `Done`.
    fn complete(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let mut calls = std::mem::take(&mut self.calls);
        calls.sort_by_key(|p| p.index);
        let any = !calls.is_empty();
        let mut used: Vec<String> = Vec::new();
        let mut next = 1u32;
        for p in calls {
            let id = match p.id.filter(|i| plain_id(i) && !used.contains(i)) {
                Some(id) => id,
                None => loop {
                    let candidate = format!("call_{next}");
                    next += 1;
                    if !used.contains(&candidate) {
                        break candidate;
                    }
                },
            };
            used.push(id.clone());
            self.out.push(call_event(id, p.name, p.args, p.value));
        }
        let reason = match self.reason {
            Some(StopReason::Stop) | None if any => StopReason::ToolCalls,
            Some(r) => r,
            None => StopReason::Stop,
        };
        self.out.push(Event::Done(Done { reason, usage: self.usage }));
        self.buf.clear();
        self.event.clear();
    }
}

fn usage_of(u: &Value, prompt: &str, completion: &str) -> Option<Usage> {
    Some(Usage { prompt_tokens: u[prompt].as_u64()?, completion_tokens: u[completion].as_u64()? })
}

/// A server's id is kept when it is at most 64 characters of letters, digits, `_` and `-`.
fn plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn cut(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// A finished call: whole with an object for arguments, or a `BadCall` that says why not.
fn call_event(id: String, name: String, args: String, value: Option<Value>) -> Event {
    let bad = |id: String, name: String, raw: String, reason: &str| Event::BadCall { id, name, raw: cut(&raw, RAW_CUT), reason: reason.to_string() };
    if name.is_empty() {
        return bad(id, name, args, "the call has no name");
    }
    let parsed = match value {
        Some(v) => Ok(v),
        None => parse_args(&args),
    };
    let as_received = |other: Value| if args.is_empty() { other.to_string() } else { args.clone() };
    match parsed {
        Ok(v @ Value::Object(_)) => Event::ToolCall(ToolCall { id, name, arguments: v }),
        Ok(other) => bad(id, name, as_received(other), "the arguments are not a JSON object"),
        Err(()) => bad(id, name, args, "the arguments are not valid JSON"),
    }
}

fn parse_args(s: &str) -> Result<Value, ()> {
    if s.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(s).map_err(|_| ())
}
