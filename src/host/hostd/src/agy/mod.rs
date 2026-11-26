//! The adapter of the session kind `agy` (M25): Antigravity's command line program run as `agy acp`, a standard Agent Client Protocol server (JSON lines on standard input and
//! output), as a session of its own. It plugs in through the driver port (`driver.rs`) and is registered once by the host after its arguments are read; no hub names it.
//!
//! Three parts: `map.rs` (what the program says becomes transcript lines, the rules of `lico_protocol/agy_mapping.py`), `conn.rs` (the order of the protocol, with no input or
//! output of its own) and this file (the process, its threads, and the application of what the conversation decides to the session). The program runs as the person, in the folder
//! the person chose (or in a scratch workspace), with a minimal environment: no token and no key of the host or of another provider reaches it, and Lico never reads what it keeps
//! for its own sign-in (`~/.gemini`). A permission request is put to the person as an approval of the session (the session's mode `default`), or answered with the one-shot
//! yes at once and said in the transcript (the mode `auto`, the host's own default for every session); an answer is never wider than one request, and the program is never
//! started with the flag that skips its permissions.

mod conn;
mod map;

use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::core::Shared;
use crate::driver::{Driver, Link, Opaque, Target};
use crate::lines::{self, Line};
use crate::session::{InputState, Session, State, MAX_APPROVALS};
use conn::{Conn, Effect};
use map::Rec;

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// The longest line of the program that is held (a longer one is cut where it is a long string, and said in the transcript).
const MAX_LINE: usize = 256 * 1024;
/// Inputs taken and not yet answered by the program: one more is refused as `busy`.
const MAX_QUEUED: usize = 16;
const QUEUE: usize = 1024;
/// The whole environment of the program: the directories it looks in for its own sign-in and settings are the person's own, and nothing else is passed.
const KEPT_ENV: [&str; 7] = ["PATH", "LANG", "HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"];
/// What the host always puts after the arguments it was given (`--agy-arg`): the mode of the program, and no telemetry. `--dangerously-skip-permissions` is never passed.
const FIXED_ARGS: [&str; 2] = ["acp", "--disable-telemetry"];

pub struct Agy {
    program: PathBuf,
    args: Vec<String>,
}

impl Agy {
    pub fn new(program: PathBuf, args: Vec<String>) -> Agy {
        Agy { program, args }
    }

    fn command(&self, workspace: &std::path::Path) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args).args(FIXED_ARGS).current_dir(workspace).env_clear();
        for key in KEPT_ENV {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        command
    }
}

enum Msg {
    Input { request_id: String, text: String },
    Answer { request_id: Value, allow: bool },
    Line(Line),
    Closed,
    Stop,
}

/// The approvals of the session that were put to the person: the number the session lists, and the request of the program it stands for.
type Asked = Arc<Mutex<BTreeMap<i64, Value>>>;

struct Handle {
    tx: SyncSender<Msg>,
    alive: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    asked: Asked,
}

impl Link for Handle {
    fn send_input(&mut self, request_id: &str, text: &str) -> Result<(), &'static str> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("session_disconnected");
        }
        if self.pending.fetch_add(1, Ordering::SeqCst) >= MAX_QUEUED {
            self.pending.fetch_sub(1, Ordering::SeqCst);
            return Err("busy");
        }
        match self.tx.try_send(Msg::Input { request_id: request_id.to_string(), text: text.to_string() }) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.pending.fetch_sub(1, Ordering::SeqCst);
                Err(if matches!(error, mpsc::TrySendError::Full(_)) { "busy" } else { "session_disconnected" })
            }
        }
    }

    fn stop(&mut self) {
        let _ = self.tx.try_send(Msg::Stop); // the process is killed by the caller (`stop_process`); this only ends the thread that watches it
    }

    fn purge(&mut self) {} // the conversation is kept by the program itself, in its own place: nothing of it is Lico's to remove

    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn answer(&mut self, approval_id: i64, allow: bool) -> Result<(), &'static str> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err("session_disconnected");
        }
        let mut asked = self.asked.lock().unwrap();
        let request_id = asked.get(&approval_id).cloned().ok_or("unknown_approval")?;
        match self.tx.try_send(Msg::Answer { request_id, allow }) {
            Ok(()) => {
                asked.remove(&approval_id); // one approval, one answer
                Ok(())
            }
            Err(error) => Err(if matches!(error, mpsc::TrySendError::Full(_)) { "busy" } else { "session_disconnected" }),
        }
    }
}

impl Driver for Agy {
    fn kind(&self) -> &'static str {
        "agy"
    }

    fn accepts(&self, target: Option<&Target>) -> Result<(), &'static str> {
        match target {
            None => Ok(()),
            Some(_) => Err("option_unavailable"), // the model is the program's own
        }
    }

    fn takes_folder(&self) -> bool {
        true
    }

    fn start(&self, shared: &Opaque, session: &Opaque) -> Result<Box<dyn Link>, &'static str> {
        let shared: Arc<Shared> = shared.clone().downcast().map_err(|_| "bad_request")?;
        let session: Arc<Mutex<Session>> = session.clone().downcast().map_err(|_| "bad_request")?;
        if session.lock().unwrap().purged {
            return Err("busy"); // a deleted session gets no workspace and no process
        }
        let workspace = crate::folders::working_dir(&shared, &session)?;
        let stored = session.lock().unwrap().codex_thread.clone();
        let mut child = crate::procs::spawn_provider_process(self.command(&workspace)).map_err(|_| "busy")?;
        let pid = child.id();
        let stop = |child: &mut Child| {
            unsafe { kill(-(pid as i32), 9) };
            let _ = child.wait();
        };
        let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
            stop(&mut child);
            return Err("busy");
        };
        {
            let mut s = session.lock().unwrap();
            if s.purged {
                drop(s);
                stop(&mut child); // the session was purged while this process started: it is stopped, never attached
                return Err("busy");
            }
            s.agent_pid = pid;
            s.process_group = true;
            s.provider = "agy";
        }
        let (tx, rx) = mpsc::sync_channel::<Msg>(QUEUE);
        let (wtx, wrx) = mpsc::sync_channel::<String>(QUEUE);
        let alive = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicUsize::new(0));
        let asked: Asked = Arc::new(Mutex::new(BTreeMap::new()));
        let mut stdin = stdin;
        thread::spawn(move || {
            for line in wrx {
                if stdin.write_all(line.as_bytes()).and_then(|_| stdin.write_all(b"\n")).and_then(|_| stdin.flush()).is_err() {
                    return;
                }
            }
        });
        let reader_tx = tx.clone();
        thread::spawn(move || {
            for line in lines::lines(stdout, MAX_LINE) {
                if reader_tx.send(Msg::Line(line)).is_err() {
                    return;
                }
            }
            let _ = reader_tx.send(Msg::Closed);
        });
        let actor = Actor {
            shared,
            session,
            pid,
            writer: wtx,
            alive: alive.clone(),
            pending: pending.clone(),
            asked: asked.clone(),
            conn: Conn::new(workspace.to_string_lossy().into_owned(), stored, true), // a request is held for the person; the mode of the session decides who answers it (`apply`)
        };
        thread::spawn(move || actor.run(child, rx));
        Ok(Box::new(Handle { tx, alive, pending, asked }))
    }
}

struct Actor {
    shared: Arc<Shared>,
    session: Arc<Mutex<Session>>,
    pid: u32,
    writer: SyncSender<String>,
    alive: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    asked: Asked,
    conn: Conn,
}

impl Actor {
    fn run(mut self, mut child: Child, rx: Receiver<Msg>) {
        let effects = self.conn.start(Instant::now());
        let mut fatal = self.apply(effects, false);
        while !fatal {
            let got = match self.conn.due() {
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            let (effects, heard) = match got {
                Ok(Msg::Input { request_id, text }) => (self.conn.input(&request_id, &text), false),
                Ok(Msg::Answer { request_id, allow }) => (self.conn.answer(&request_id, allow), false),
                Ok(Msg::Line(Line::Text { text, bytes })) => match serde_json::from_str::<Value>(&text) {
                    Ok(message) => (self.conn.agent(Instant::now(), &message, lines::has_cut(&message)), true),
                    Err(_) if bytes > MAX_LINE => (self.conn.omitted(bytes), true), // too long to hold, and what was held is not a whole message
                    Err(_) => (Vec::new(), false),
                },
                Ok(Msg::Line(Line::Skipped { bytes })) => (self.conn.omitted(bytes), true),
                Ok(Msg::Stop) | Ok(Msg::Closed) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => (self.conn.expired(Instant::now()), false),
            };
            fatal = self.apply(effects, heard);
        }
        self.alive.store(false, Ordering::SeqCst);
        self.asked.lock().unwrap().clear(); // nobody is left to answer
        let tail = self.conn.closed(); // what was said and not yet ended as a line
        self.apply(tail, false);
        unsafe { kill(-(self.pid as i32), 9) }; // its output ended (or it is to end): nothing of it, helpers included, is kept alive
        let _ = child.wait();
        let mut s = self.session.lock().unwrap();
        let mine = s.agent_pid == self.pid; // a close or a purge zeroed it, and a resume may have put its own process there
        if mine {
            s.agent_pid = 0; // reaped: the number may now belong to another process
            if s.state != State::Exited {
                if s.state == State::Running {
                    s.set_state(State::Disconnected);
                }
                s.set_busy(false);
                s.driver = None;
                s.drop_program_approvals();
                s.append(b"[provider disconnected]".to_vec());
            }
        }
    }

    /// Applies what the conversation decided to the session, in order, and says whether the connection cannot go on. A process that is not the session's any more (a close, a
    /// purge, a resume after it) changes nothing.
    fn apply(&mut self, effects: Vec<Effect>, heard: bool) -> bool {
        let mut fatal = false;
        let mut answers: Vec<(Value, bool)> = Vec::new();
        let mut asked_of = None;
        let mut peer_done = None;
        let mut turn_ended: Option<(String, Option<String>, Option<String>)> = None;
        {
            let mut s = self.session.lock().unwrap();
            if s.agent_pid != self.pid {
                return false;
            }
            if heard {
                s.heard();
            }
            for effect in effects {
                match effect {
                    Effect::Send(message) => {
                        let _ = self.writer.try_send(message.to_string());
                    }
                    Effect::Rec(Rec::Note(text)) => s.append(text.into_bytes()),
                    Effect::Rec(Rec::Agent { raw, text }) => {
                        s.peer_turn.push_line(&raw);
                        s.append(text.into_bytes());
                    }
                    Effect::Submitted(request_id) => s.advance_input(&request_id, InputState::Submitted),
                    Effect::Observed(request_id) => s.advance_input(&request_id, InputState::Observed),
                    Effect::Failed(request_id) => s.advance_input(&request_id, InputState::Failed),
                    Effect::Session(id) => {
                        s.codex_thread = Some(id);
                        crate::registry::mark_dirty();
                    }
                    Effect::Model(model) => s.model = Some(model),
                    Effect::Turn { ok, more } => {
                        let _ = self.pending.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
                        if !more {
                            s.set_busy(false);
                        }
                        s.drop_program_approvals(); // a request the turn left unanswered is over with it
                        self.asked.lock().unwrap().clear();
                        if ok {
                            let label = crate::transfer::session_label(&s);
                            let sid = s.id.clone();
                            peer_done = s.peer_turn.finish(&sid, label);
                        } else {
                            s.peer_turn.drop_active();
                        }
                        s.mark_finished();
                        crate::calls::flush_deferred(&mut s);
                        turn_ended = Some((s.id.clone(), s.settings.name.clone(), s.settings.worktree_repo.clone().or_else(|| s.settings.cwd.clone())));
                    }
                    Effect::Approval(approval) if s.mode == "auto" => {
                        s.append(format!("[auto-allowed] {}", approval.prompt).into_bytes()); // the mode is the person's choice; what was allowed is said
                        answers.push((approval.request_id, true));
                    }
                    Effect::Approval(approval) if s.approvals.len() >= MAX_APPROVALS => answers.push((approval.request_id, false)), // too many are waiting: this one is denied once
                    Effect::Approval(approval) => {
                        s.approval_counter += 1;
                        let number = s.approval_counter;
                        self.asked.lock().unwrap().insert(number, approval.request_id);
                        s.add_approval(number, approval.prompt);
                        asked_of = Some((s.id.clone(), s.settings.name.clone(), s.settings.worktree_repo.clone().or_else(|| s.settings.cwd.clone())));
                    }
                    Effect::Fatal => fatal = true,
                }
            }
        }
        crate::calls::deliver_peer_reply(&self.shared, peer_done);
        if let Some((id, name, cwd)) = turn_ended {
            crate::notify::on_turn_finished(&self.shared, &id, name.as_deref(), cwd.as_deref());
        }
        if let Some((id, name, cwd)) = asked_of {
            crate::notify::on_approval_needed(&self.shared, &id, name.as_deref(), cwd.as_deref());
        }
        for (request_id, allow) in answers {
            let effects = self.conn.answer(&request_id, allow);
            fatal |= self.apply(effects, false);
        }
        fatal
    }
}

#[cfg(test)]
mod tests;
