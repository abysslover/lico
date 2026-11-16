//! The adapter of the session kind `local` (M16-P06): a Local LLM server on this machine as a session of its own. The host asks the model itself (`lico-llm-http`,
//! `lico-llm-wire`), keeps the conversation in an owner-only record (`lico-convo`) and runs the tool loop (`lico-toolloop`) with the tools and approvals every session has.
//! It plugs in through the driver port (`driver.rs`) and is registered once, from the root; no module of the cycle names it.

mod hosttools;
mod keys;
mod model;

pub use keys::{clear as clear_key, set as set_key, Keyring, Keys};

use lico_convo::Convo;
use lico_llm_http as http;
use lico_llm_wire::{Message, StopReason, StreamError};
use lico_toolloop::{run_turn, Config, Env, Failed, LimitKind, Limits, ModelError, Out, Outcome, Ports, RecordPort, TurnEnd};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::core::Shared;
use crate::driver::{Driver, Link, Opaque, Target};
use crate::session::{Session, State};

const MAX_RECORD: usize = 4096;
const QUEUE: usize = 16;

pub struct Local {
    keys: Arc<dyn Keys>,
    kind: &'static str,
}

impl Local {
    pub fn new(keys: Arc<dyn Keys>) -> Local {
        Local { keys, kind: "local" }
    }

    /// A driver of another kind name (a test registers its own, so that tests do not share one).
    #[cfg(test)]
    pub fn with_kind(keys: Arc<dyn Keys>, kind: &'static str) -> Local {
        Local { keys, kind }
    }
}

struct Job {
    text: String,
}

struct Handle {
    tx: Option<SyncSender<Job>>,
    cancel: http::Cancel,
    path: std::path::PathBuf,
}

impl Link for Handle {
    fn send_input(&mut self, _request_id: &str, text: &str) -> Result<(), &'static str> {
        match self.tx.as_ref().ok_or("session_disconnected")?.try_send(Job { text: text.to_string() }) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err("busy"),
            Err(mpsc::TrySendError::Disconnected(_)) => Err("session_disconnected"),
        }
    }
    fn stop(&mut self) {
        self.cancel.cancel();
        self.tx = None; // the worker ends after the turn it is in (which the cancel ends) and does not wait for more
    }
    fn purge(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Driver for Local {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn accepts(&self, target: Option<&Target>) -> Result<(), &'static str> {
        match target {
            Some(t) if !t.model.is_empty() => Ok(()),
            _ => Err("bad_request"),
        }
    }

    fn start(&self, shared: &Opaque, session: &Opaque) -> Result<Box<dyn Link>, &'static str> {
        let shared: Arc<Shared> = shared.clone().downcast().map_err(|_| "bad_request")?;
        let session: Arc<Mutex<Session>> = session.clone().downcast().map_err(|_| "bad_request")?;
        let (id, target) = {
            let s = session.lock().unwrap();
            (s.id.clone(), s.settings.target.clone().ok_or("bad_request")?)
        };
        let path = shared.cfg.registry_dir.join("local").join(format!("{id}.jsonl"));
        let (convo, opened) = match Convo::open(&path) {
            Ok(done) => done,
            Err(e) => {
                note(&session, &format!("[the conversation record cannot be opened: {}]", describe(&e)));
                return Err("bad_request");
            }
        };
        if opened.repaired_bytes > 0 {
            note(&session, &format!("[the last, unfinished message of the record ({} bytes) was cut off]", opened.repaired_bytes));
        }
        let cancel = http::Cancel::new();
        let (tx, rx) = mpsc::sync_channel::<Job>(QUEUE);
        let model = model::HostModel::new(target, self.keys.clone(), cancel.clone(), session.clone());
        let worker = Worker { shared, session, id, convo, model, cancel: cancel.clone() };
        std::thread::Builder::new().name("local-session".into()).spawn(move || worker.run(rx)).map_err(|_| "busy")?;
        Ok(Box::new(Handle { tx: Some(tx), cancel, path }))
    }
}

fn describe(e: &lico_convo::Error) -> String {
    match e {
        lico_convo::Error::Corrupt { line } => format!("line {line} is damaged"),
        lico_convo::Error::NotOwnerOnly => "the file can be read by others".into(),
        lico_convo::Error::NotARegularFile => "it is not a regular file".into(),
        lico_convo::Error::Full => "it is full".into(),
        other => format!("{other:?}"),
    }
}

/// A line of the host for the window (`[...]`), cleaned.
fn note(session: &Arc<Mutex<Session>>, text: &str) {
    let mut s = session.lock().unwrap();
    if !s.purged {
        crate::codex::note(&mut s, text);
    }
}

/// A line of what the model said, as every adapter records agent text.
fn agent_line(session: &Arc<Mutex<Session>>, line: &str) {
    let mut s = session.lock().unwrap();
    if s.purged {
        return;
    }
    s.peer_turn.push_line(line);
    s.append(crate::leads::agent_record(line, MAX_RECORD).into_bytes());
}

struct Worker {
    shared: Arc<Shared>,
    session: Arc<Mutex<Session>>,
    id: String,
    convo: Convo,
    model: model::HostModel,
    cancel: http::Cancel,
}

struct Clock<'a> {
    cancel: &'a http::Cancel,
    started: Instant,
}

impl Env for Clock<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

struct Record<'a>(&'a mut Convo);

impl RecordPort for Record<'_> {
    fn append(&mut self, m: &Message) -> Result<(), String> {
        self.0.append(m).map_err(|e| describe(&e))
    }
}

impl Worker {
    fn run(mut self, rx: Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            if self.cancel.is_cancelled() {
                break;
            }
            self.turn(job);
        }
    }

    fn turn(&mut self, job: Job) {
        let config = Config { tools: hosttools::tool_defs(&self.shared), limits: Limits::default() };
        let mut history = self.convo.messages().to_vec();
        let mut tools = hosttools::HostTools { shared: self.shared.clone(), id: self.id.clone() };
        let clock = Clock { cancel: &self.cancel, started: Instant::now() };
        let mut pending = String::new();
        let session = self.session.clone();
        let end = {
            let mut record = Record(&mut self.convo);
            let mut ports = Ports { model: &mut self.model, tools: &mut tools, record: &mut record, env: &clock };
            run_turn(&mut ports, &mut history, Message::User(job.text), &config, &mut |out| match out {
                Out::Text(piece) => {
                    pending.push_str(&piece);
                    while let Some(at) = pending.find('\n') {
                        let line: String = pending.drain(..=at).collect();
                        agent_line(&session, line.trim_end_matches(['\n', '\r']));
                    }
                }
                Out::Thinking => {}
                Out::Tool { name, outcome, .. } => {
                    let how = match outcome {
                        Outcome::Done => "done",
                        Outcome::Waiting => "waiting for the person",
                        Outcome::Refused => "refused",
                    };
                    note(&session, &format!("[tool {name}] {how}"));
                }
            })
        };
        if !pending.is_empty() {
            agent_line(&self.session, pending.trim_end_matches(['\n', '\r']));
        }
        self.finish(&end);
    }

    /// What the window shows when a turn ends, the same steps as the provider adapters take: the end line, the peer reply, the idle state, the deferred replies, the notice.
    fn finish(&self, end: &TurnEnd) {
        let (peer_done, name, cwd) = {
            let mut s = self.session.lock().unwrap();
            if s.purged {
                return;
            }
            let ok = matches!(end, TurnEnd::Finished(_) | TurnEnd::Waiting);
            for line in end_lines(end, &self.model.target) {
                crate::codex::note(&mut s, &line);
            }
            let peer_done = if ok {
                let label = crate::transfer::session_label(&s);
                let id = s.id.clone();
                s.peer_turn.finish(&id, label)
            } else {
                s.peer_turn.drop_active();
                None
            };
            s.mark_finished();
            s.set_busy(false);
            if s.state == State::Running {
                crate::calls::flush_deferred(&mut s);
            }
            (peer_done, s.settings.name.clone(), s.settings.cwd.clone())
        };
        crate::calls::deliver_peer_reply(&self.shared, peer_done);
        crate::notify::on_turn_finished(&self.shared, &self.id, name.as_deref(), cwd.as_deref());
    }
}

/// The lines for the end of a turn: a plain word for what happened, and for a failure what the person can do about it.
fn end_lines(end: &TurnEnd, target: &Target) -> Vec<String> {
    let endpoint = target.addr.to_string();
    let failure = |f: &lico_llm_wire::Failure| -> Vec<String> {
        use lico_llm_wire::FailureKind as K;
        let first = match f.kind {
            K::KeyNeeded => format!("[error] the server wants a key; store one with: lico local-key set {endpoint}"),
            K::ModelMissing => format!("[error] the server has no model {}; make a new session with another model", target.model),
            K::ContextTooLong => "[error] the conversation is longer than the model's window; start a new session or choose a larger window".to_string(),
            K::Busy => "[error] the server is busy; send the message again in a moment".to_string(),
            _ => "[error] the server refused the request".to_string(),
        };
        let mut lines = vec!["[turn failed]".to_string(), first];
        if !f.message.is_empty() {
            lines.push(format!("[server] {}", f.message));
        }
        lines
    };
    match end {
        TurnEnd::Finished(StopReason::Length) => vec!["[turn completed] the answer reached the length the server allows".into()],
        TurnEnd::Finished(_) => vec!["[turn completed]".into()],
        TurnEnd::Waiting => vec!["[waiting for the person]".into()],
        TurnEnd::Cancelled => vec!["[turn stopped]".into()],
        TurnEnd::Limit(kind) => vec![format!(
            "[turn stopped] {}",
            match kind {
                LimitKind::Rounds => "the limit of 8 rounds for one message was reached",
                LimitKind::CallsPerTurn => "the limit of 16 tool calls for one message was reached",
                LimitKind::Time => "the limit of 10 minutes for one message was reached",
                LimitKind::AnswerSize => "the answer was longer than 256 KiB",
            }
        )],
        TurnEnd::Failed(Failed::Model(f)) | TurnEnd::Failed(Failed::Stream(StreamError::Server(f))) => failure(f),
        TurnEnd::Failed(Failed::Stream(StreamError::Truncated)) => vec!["[turn failed]".into(), "[error] the answer was cut; nothing was run".into()],
        TurnEnd::Failed(Failed::Stream(StreamError::Malformed(_))) => vec!["[turn failed]".into(), "[error] the server's answer was not understood; nothing was run".into()],
        TurnEnd::Failed(Failed::Stream(StreamError::TooLarge)) => vec!["[turn failed]".into(), "[error] the server's answer was too large; nothing was run".into()],
        TurnEnd::Failed(Failed::Port(ModelError::Gone)) => vec!["[turn failed]".into(), format!("[error] no server at {endpoint}; the next message tries again")],
        TurnEnd::Failed(Failed::Port(ModelError::Timeout)) => vec!["[turn failed]".into(), "[error] the server did not answer in time".into()],
        TurnEnd::Failed(Failed::Port(ModelError::TooBig { needed, window })) => {
            vec!["[turn failed]".into(), format!("[error] this message needs about {needed} tokens and the window is {window}; send a shorter one")]
        }
        TurnEnd::Failed(Failed::Port(ModelError::Cancelled)) => vec!["[turn stopped]".into()],
        TurnEnd::Failed(Failed::Port(ModelError::Failure(f))) => failure(f),
        TurnEnd::Failed(Failed::Port(ModelError::Other(e))) => vec!["[turn failed]".into(), format!("[error] {e}")],
        TurnEnd::Failed(Failed::Record(e)) => vec!["[turn failed]".into(), format!("[error] the conversation record could not be written: {e}")],
    }
}

#[cfg(test)]
mod tests;
