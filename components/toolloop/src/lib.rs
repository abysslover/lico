//! One turn of a local session as pure logic over four ports (see `SPEC.md`): the person's message goes in, the model is asked, its tool calls are handed to the host
//! one by one within limits, their results go back, until the model is done, a tool waits for the person, or a limit is reached. The crate has no way to reach a
//! file, a network, a process, the environment or a clock: everything outside goes through the ports, and a tool is run only if it was offered.

use lico_llm_wire::{Event, Failure, Message, StopReason, StreamError, ToolCall, ToolDef};
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_rounds: u32,
    pub max_calls_per_answer: usize,
    pub max_calls_per_turn: usize,
    pub max_result_bytes: usize,
    pub max_args_bytes: usize,
    pub max_answer_bytes: usize,
    pub turn_ms: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_rounds: 8, max_calls_per_answer: 4, max_calls_per_turn: 16, max_result_bytes: 8 * 1024, max_args_bytes: 8 * 1024, max_answer_bytes: 256 * 1024, turn_ms: 600_000 }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub tools: Vec<ToolDef>,
    pub limits: Limits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolOutcome {
    Done(String),
    Waiting(String),
    Refused(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Waiting,
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    Failure(Failure),
    Gone,
    Timeout,
    Cancelled,
    TooBig { needed: u32, window: u32 },
    Other(String),
}

pub trait ModelPort {
    fn turn(&mut self, messages: &[Message], tools: &[ToolDef], sink: &mut dyn FnMut(Event) -> bool) -> Result<(), ModelError>;
}

pub trait ToolPort {
    fn call(&mut self, call: &ToolCall) -> ToolOutcome;
}

pub trait RecordPort {
    fn append(&mut self, m: &Message) -> Result<(), String>;
}

pub trait Env {
    fn cancelled(&self) -> bool;
    fn now_ms(&self) -> u64;
}

pub struct Ports<'a> {
    pub model: &'a mut dyn ModelPort,
    pub tools: &'a mut dyn ToolPort,
    pub record: &'a mut dyn RecordPort,
    pub env: &'a dyn Env,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    Text(String),
    Thinking,
    Tool { name: String, call_id: String, outcome: Outcome },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    Rounds,
    CallsPerTurn,
    Time,
    AnswerSize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failed {
    Model(Failure),
    Stream(StreamError),
    Port(ModelError),
    Record(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnEnd {
    Finished(StopReason),
    Waiting,
    Limit(LimitKind),
    Failed(Failed),
    Cancelled,
}

/// A call the model made, before the loop has looked at it.
enum Made {
    Good(ToolCall),
    Bad { id: String, name: String, reason: String },
}

fn cut(s: &str, max: usize) -> String {
    const NOTE: &str = "\n[cut: the result is longer than the limit]";
    let boundary = |mut at: usize| {
        while !s.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    if s.len() <= max {
        s.to_string()
    } else if max <= NOTE.len() {
        s[..boundary(max)].to_string()
    } else {
        format!("{}{NOTE}", &s[..boundary(max - NOTE.len())])
    }
}

fn not_run(why: &str) -> (String, Outcome) {
    (format!("not run: {why}"), Outcome::Refused)
}

/// One turn: `input` is recorded and appended to `history`, then rounds of asking the model and running its calls (see `SPEC.md`).
pub fn run_turn(ports: &mut Ports, history: &mut Vec<Message>, input: Message, config: &Config, out: &mut dyn FnMut(Out)) -> TurnEnd {
    let limits = &config.limits;
    let start = ports.env.now_ms();
    let over_time = |env: &dyn Env| env.now_ms().saturating_sub(start) >= limits.turn_ms;
    if let Err(e) = ports.record.append(&input) {
        return TurnEnd::Failed(Failed::Record(e));
    }
    history.push(input);
    let (mut rounds, mut calls_total) = (0u32, 0usize);
    loop {
        if ports.env.cancelled() {
            return TurnEnd::Cancelled;
        }
        if over_time(ports.env) {
            return TurnEnd::Limit(LimitKind::Time);
        }
        if rounds >= limits.max_rounds {
            return TurnEnd::Limit(LimitKind::Rounds);
        }
        rounds += 1;
        let mut text = String::new();
        let mut made: Vec<Made> = Vec::new();
        let (mut finished, mut error, mut too_long, mut cancelled) = (None, None, false, false);
        let asked = {
            let env = ports.env;
            ports.model.turn(&history[..], &config.tools, &mut |event| {
                match event {
                    Event::Text(t) => {
                        text.push_str(&t);
                        out(Out::Text(t));
                        if text.len() > limits.max_answer_bytes {
                            too_long = true;
                            return false;
                        }
                    }
                    Event::Thinking => out(Out::Thinking),
                    Event::ToolCall(c) => made.push(Made::Good(c)),
                    Event::BadCall { id, name, reason, .. } => made.push(Made::Bad { id, name, reason }),
                    Event::Done(d) => finished = Some(d),
                    Event::Error(e) => error = Some(e),
                }
                if env.cancelled() {
                    cancelled = true;
                    return false;
                }
                true
            })
        };
        match asked {
            Err(ModelError::Cancelled) => return TurnEnd::Cancelled,
            Err(ModelError::Failure(f)) => return TurnEnd::Failed(Failed::Model(f)),
            Err(e) => return TurnEnd::Failed(Failed::Port(e)),
            Ok(()) => {}
        }
        if cancelled {
            return TurnEnd::Cancelled;
        }
        if too_long {
            return TurnEnd::Limit(LimitKind::AnswerSize);
        }
        if let Some(e) = error {
            return TurnEnd::Failed(Failed::Stream(e));
        }
        let Some(done) = finished else { return TurnEnd::Failed(Failed::Stream(StreamError::Truncated)) };
        // the answer is recorded before anything of it runs
        let mut recorded = Vec::new();
        let mut verdicts: Vec<Option<String>> = Vec::new();
        for m in &made {
            match m {
                Made::Good(c) if c.arguments.to_string().len() > limits.max_args_bytes => {
                    recorded.push(ToolCall { id: c.id.clone(), name: c.name.clone(), arguments: json!({}) });
                    verdicts.push(Some(format!("error: the arguments are larger than {} bytes and were not run", limits.max_args_bytes)));
                }
                Made::Good(c) => {
                    recorded.push(c.clone());
                    verdicts.push(None);
                }
                Made::Bad { id, name, reason } => {
                    recorded.push(ToolCall { id: id.clone(), name: if name.is_empty() { "unknown".into() } else { name.clone() }, arguments: json!({}) });
                    verdicts.push(Some(format!("error: {reason}; call it again with a JSON object for the arguments")));
                }
            }
        }
        let assistant = Message::Assistant { text, calls: recorded.clone() };
        if let Err(e) = ports.record.append(&assistant) {
            return TurnEnd::Failed(Failed::Record(e));
        }
        history.push(assistant);
        if recorded.is_empty() {
            return TurnEnd::Finished(done.reason);
        }
        let (mut waiting, mut calls_hit) = (false, false);
        let mut stop: Option<TurnEnd> = None;
        for (i, call) in recorded.iter().enumerate() {
            let (content, outcome) = if let Some(end) = &stop {
                not_run(if *end == TurnEnd::Cancelled { "the turn was cancelled" } else { "the time limit of the turn was reached" })
            } else if ports.env.cancelled() {
                stop = Some(TurnEnd::Cancelled);
                not_run("the turn was cancelled")
            } else if over_time(ports.env) {
                stop = Some(TurnEnd::Limit(LimitKind::Time));
                not_run("the time limit of the turn was reached")
            } else if i >= limits.max_calls_per_answer {
                calls_total += 1;
                not_run(&format!("more than {} calls in one answer", limits.max_calls_per_answer))
            } else if calls_total >= limits.max_calls_per_turn {
                calls_hit = true;
                not_run(&format!("more than {} calls in this turn", limits.max_calls_per_turn))
            } else if let Some(why) = &verdicts[i] {
                calls_total += 1;
                (why.clone(), Outcome::Refused)
            } else if !config.tools.iter().any(|t| t.name == call.name) {
                calls_total += 1;
                (format!("error: the tool {} is not offered", call.name), Outcome::Refused)
            } else {
                calls_total += 1;
                match ports.tools.call(call) {
                    ToolOutcome::Done(t) => (t, Outcome::Done),
                    ToolOutcome::Waiting(t) => {
                        waiting = true;
                        (t, Outcome::Waiting)
                    }
                    ToolOutcome::Refused(t) => (t, Outcome::Refused),
                }
            };
            let result = Message::Tool { call_id: call.id.clone(), name: call.name.clone(), content: cut(&content, limits.max_result_bytes) };
            if let Err(e) = ports.record.append(&result) {
                return TurnEnd::Failed(Failed::Record(e));
            }
            history.push(result);
            out(Out::Tool { name: call.name.clone(), call_id: call.id.clone(), outcome });
        }
        if let Some(end) = stop {
            return end;
        }
        if calls_hit {
            return TurnEnd::Limit(LimitKind::CallsPerTurn);
        }
        if waiting {
            return TurnEnd::Waiting;
        }
    }
}
