//! The calls of an agent that act, on the tool connection: `send` (S03b, with the person's approval from S04); `create` (S04c) is in `create` and `history` (S07) in `history`. The caller is the
//! session of the token and nobody else (no argument names it). The limits of AC-04 are kept here, in memory and nowhere else: calls per minute of one session, and (in
//! `transfer`) the depth. What the agent asks for does not happen at the call: the call leaves a card on the agent's row (`pending`) and happens when the person says yes.

use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::codex::sanitize;
use crate::core::{self, Shared};
use crate::leads;
use crate::pending::{self, Asked, Request};
use crate::transfer::{self, By, MAX_HOPS};

/// Calls per minute of one session. Every `send`, `create`, `history`, `capture`, `clipboard_read` and `clipboard_write` counts, a refused one too; `sessions` and `read_handoff` do not.
pub const PER_MINUTE: usize = 20;
const MINUTE: Duration = Duration::from_secs(60);
const KEPT: usize = 2 * PER_MINUTE; // a session that goes on asking does not make its list grow
pub(crate) const CARD_CHARS: usize = 200; // what the window shows of a card (`Session::add_card` cuts at the same number)
const LABEL_CHARS: usize = 60;

/// When each session called the host in the last minute: a sliding window.
#[derive(Default)]
pub(crate) struct Window {
    calls: HashMap<String, VecDeque<Instant>>,
}

impl Window {
    /// Count a call of `session` made at `now`. Err: the seconds to wait before a call would be let through (the refused call counts, so asking again sooner only lengthens it).
    fn take(&mut self, session: &str, now: Instant) -> Result<(), u64> {
        let gone = |at: &Instant| now.saturating_duration_since(*at) >= MINUTE;
        self.calls.retain(|_, at| at.back().is_some_and(|last| !gone(last)));
        let at = self.calls.entry(session.to_string()).or_default();
        while at.front().is_some_and(gone) {
            at.pop_front();
        }
        let refused = at.len() >= PER_MINUTE;
        if at.len() < KEPT {
            at.push_back(now);
        }
        if !refused {
            return Ok(());
        }
        // the next call is let through when enough of the earlier ones (this refused one among them) have left for fewer than PER_MINUTE to be in the minute
        let leaves = at[at.len() - PER_MINUTE] + MINUTE;
        Err(leaves.saturating_duration_since(now).as_millis().div_ceil(1000).max(1) as u64)
    }
}

pub(crate) fn window() -> &'static Mutex<Window> {
    static WINDOW: OnceLock<Mutex<Window>> = OnceLock::new();
    WINDOW.get_or_init(Mutex::default)
}

/// The tool `send`: the text of `arguments` to the session `to`, from the session that holds the token. The text is not sent at the call, unless the person has
/// allowed this session to send to that one for as long as it lives (`keep`): the call asks the person and says so.
pub fn send(shared: &Shared, caller: &str, arguments: &Map<String, Value>) -> Result<String, String> {
    pending::sweep(shared, caller);
    send_with(shared, caller, arguments, window(), Instant::now(), pending::held_text())
}

/// Count a call of `caller` in the minute's window. Err: the refusal the model reads. `send`, `create` and `history` are counted together, one window for a session.
pub(crate) fn counted(window: &Mutex<Window>, caller: &str, now: Instant) -> Result<(), String> {
    window.lock().unwrap_or_else(|e| e.into_inner()).take(caller, now).map_err(|wait| format!("send_limit: at most {PER_MINUTE} calls a minute from one session; wait {wait} seconds and try again"))
}

fn send_with(shared: &Shared, caller: &str, arguments: &Map<String, Value>, window: &Mutex<Window>, now: Instant, held: &Arc<AtomicUsize>) -> Result<String, String> {
    counted(window, caller, now)?;
    let raw_to = arguments.get("to").and_then(Value::as_str).filter(|to| !to.is_empty() && to.chars().count() <= 64);
    let text = arguments.get("text").and_then(Value::as_str).filter(|text| !text.is_empty());
    let (Some(raw_to), Some(text)) = (raw_to, text) else { return Err(refusal(shared, caller, "bad_request")) };
    // the text is cleaned here, once: the card, the pages of it, the window, the command, the record and the receiver all get this string
    let cleaned = leads::agent_text(text);
    let (stripped_done, marked_done) = crate::call_room::strip_call_done(cleaned.text.as_str());
    let text = if marked_done && !stripped_done.is_empty() && !crate::folders::shows_nothing(&stripped_done) {
        stripped_done.as_str()
    } else {
        cleaned.text.as_str()
    };
    let removed = cleaned.removed;
    if text.is_empty() || crate::folders::shows_nothing(text) {
        return Err(refusal(shared, caller, "bad_request"));
    }
    let to_owned = crate::mention::resolve_agent_target(shared, caller, raw_to).map_err(|code| refusal(shared, caller, code))?;
    let to = to_owned.as_str();
    if to == caller {
        return Err("bad_request: a session cannot send to itself".into());
    }
    // the tool connection is for provider sessions (Claude and Codex): an agent does not send to synthetic non-provider sessions
    if core::find(shared, to).is_some_and(|found| {
        let s = found.lock().unwrap();
        !s.purged && !matches!(s.provider, "claude" | "codex")
    }) {
        return Err("not_allowed: agents send to provider sessions only; that session is not one".into());
    }
    // what is already known to be refused is refused now, so that the person is not asked for a send that could not happen
    let label = receiver_label(shared, caller, to).map_err(|code| refusal(shared, caller, code))?;
    let burst_step = crate::call_room::check_or_advance_burst(shared, caller, to)?;
    let burst_allowed = burst_step.as_ref().is_some_and(|(cid, _)| crate::call_room::is_owner_authorized(shared, cid));
    if pending::kept(shared, caller, "send", to) || peer_reply_authorized(shared, caller, to) || burst_allowed {
        AUTHORIZED_SEND.with(|c| c.set(true));
        BURST_ROUND.with(|c| c.set(burst_step.as_ref().map_or(0, |(_, r)| *r)));
        CALL_DONE_FLAG.with(|c| c.set(marked_done));
        let sent_res = transfer::send(shared, Some(caller), to, "text", text, By::Session);
        AUTHORIZED_SEND.with(|c| c.set(false));
        BURST_ROUND.with(|c| c.set(0));
        CALL_DONE_FLAG.with(|c| c.set(false));
        return match sent_res {
            Ok(done) => Ok(json!({"sent": true, "to": to, "delivery": done["delivery"], "size": done["size"], "receiver": done["receiver"]}).to_string()),
            Err(code) => Err(refusal(shared, caller, code)),
        };
    }
    let (card, who) = (send_card(&label, to, text, removed), format!("session '{label}' ({})", transfer::id7(to)));
    let asked = pending::ask(shared, caller, &Request { tool: "send", target: to, who: &who, card: &card, text }, pending::now(), held)?;
    let (id, again) = match asked {
        Asked::New(id) => (id, false),
        Asked::Again(id) => (id, true),
    };
    let mut reply = json!({
        "status": "pending", "approval_id": id,
        "message": format!("Nothing was sent yet. The person has been asked to approve this send to {who}. Lico will tell you here when they decide; you can end your turn. A restart of Lico drops the request."),
    });
    if again {
        reply["already_pending"] = json!(true);
    }
    Ok(reply.to_string())
}

/// Whether `to` can take a send from `caller` at all, and what it is called: the answers `transfer::send` would give at the question, so that they come at the call.
fn receiver_label(shared: &Shared, caller: &str, to: &str) -> Result<String, &'static str> {
    if core::find(shared, caller).map_or(0, |found| found.lock().unwrap().incoming_hops) + 1 > MAX_HOPS {
        return Err("chain_depth");
    }
    let found = core::find(shared, to).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    match s.state {
        _ if s.purged => return Err("unknown_session"),
        crate::session::State::Disconnected => return Err("session_disconnected"),
        crate::session::State::Exited => return Err("session_exited"),
        crate::session::State::Running => {}
    }
    Ok(transfer::session_label(&s).chars().take(LABEL_CHARS).collect())
}

/// The line on the card of a send: to whom, how much, and the start of the text, in one line of at most `CARD_CHARS`.
fn send_card(label: &str, to: &str, text: &str, removed: usize) -> String {
    card_line(&format!("send to '{label}' ({})", transfer::id7(to)), text, removed)
}

thread_local! {
    static AUTHORIZED_SEND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static BURST_ROUND: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static CALL_DONE_FLAG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CREATE_PROMPT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn is_create_prompt() -> bool {
    CREATE_PROMPT.with(|c| c.get())
}

pub(crate) fn with_create_prompt<R>(f: impl FnOnce() -> R) -> R {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            CREATE_PROMPT.with(|c| c.set(self.0));
        }
    }
    let prev = CREATE_PROMPT.with(|c| c.replace(true));
    let _guard = Reset(prev);
    f()
}

const MAX_PEER_QUEUE: usize = 8;
const MAX_DEFERRED_REPLIES: usize = 16;
const MAX_PEER_REPLY: usize = 32 * 1024;

pub struct DeferredReply {
    pub request_id: String,
    pub send_text: String,
    pub shown_text: String,
    pub reply_slot: Option<(String, String, u32)>,
}

pub enum PeerDone {
    Reply {
        caller_id: String,
        from_id: String,
        from_label: String,
        call_id: Option<String>,
        round: u32,
        reply_text: String,
        explicit_sent: bool,
        settled_burst_call: Option<String>,
        assistant_text: String,
    },
    AssistantTurn {
        from_id: String,
        assistant_text: String,
        settled_burst_call: Option<String>,
    },
}

#[derive(Default)]
pub struct PeerTurn {
    pub active: Option<(String, Option<String>, u32, bool)>,
    pub queue: VecDeque<(String, Option<String>, u32)>,
    pub reply_buf: String,
    pub assistant_buf: String,
    pub sent_in_turn: bool,
    pub turn_fifo: VecDeque<Option<(String, String, u32, bool)>>,
    pub next_reply_slot: Option<(String, String, u32)>,
    pub deferred_replies: VecDeque<DeferredReply>,
}

impl PeerTurn {
    pub fn clear(&mut self) {
        self.active = None;
        self.queue.clear();
        self.reply_buf.clear();
        self.assistant_buf.clear();
        self.sent_in_turn = false;
        self.next_reply_slot = None;
        for slot in &mut self.turn_fifo { *slot = None; }
    }

    pub fn reset_process(&mut self) {
        self.clear();
        self.turn_fifo.clear();
        self.deferred_replies.clear();
    }

    pub fn on_record_input(&mut self, is_person: bool) {
        let slot = self.next_reply_slot.take().map(|(p, c, r)| (p, c, r, false));
        if is_person { self.clear(); }
        if self.turn_fifo.len() < 64 { self.turn_fifo.push_back(if is_person { None } else { slot }); }
    }

    pub fn merge_steered_turn(&mut self) {
        if self.turn_fifo.len() >= 2 {
            if let Some(Some(steered)) = self.turn_fifo.pop_back() {
                if let Some(front @ None) = self.turn_fifo.front_mut() { *front = Some(steered); }
            }
        }
    }

    pub fn mark_burst_continued(&mut self, round: u32) {
        if let Some(Some((_, _, r, cont))) = self.turn_fifo.front_mut() { (*r, *cont) = (round, true); }
    }

    pub fn mark_burst_settled(&mut self) {
        if let Some(Some((_, _, _, cont))) = self.turn_fifo.front_mut() { *cont = true; }
    }

    pub fn begin_call(&mut self, caller_id: &str, call_id: Option<String>, round: u32) {
        if let Some((c, cid, r, _)) = self.active.as_mut() {
            if c == caller_id {
                if call_id.is_some() {
                    *cid = call_id;
                    *r = round;
                }
                return;
            }
        }
        if self.active.is_none() {
            self.active = Some((caller_id.to_string(), call_id, round.max(1), false));
            self.reply_buf.clear();
        } else if self.queue.len() < MAX_PEER_QUEUE {
            self.queue.push_back((caller_id.to_string(), call_id, round.max(1)));
        }
    }

    pub fn attach_call(&mut self, caller_id: &str, call_id: String, round: u32) {
        if let Some((_, cid, r)) = self.queue.iter_mut().rev().find(|item| item.0 == caller_id && item.1.is_none()) {
            *cid = Some(call_id);
            *r = round.max(1);
            return;
        }
        if let Some((c, cid, r, _)) = self.active.as_mut() {
            if c == caller_id {
                *cid = Some(call_id);
                *r = round.max(1);
            }
        }
    }

    pub fn cancel_call(&mut self, call_id: &str) {
        self.queue.retain(|(_, cid, _)| cid.as_deref() != Some(call_id));
        if self.next_reply_slot.as_ref().is_some_and(|(_, cid, _)| cid == call_id) { self.next_reply_slot = None; }
        for item in &mut self.deferred_replies {
            if item.reply_slot.as_ref().is_some_and(|(_, cid, _)| cid == call_id) { item.reply_slot = None; }
        }
        for slot in &mut self.turn_fifo {
            if slot.as_ref().is_some_and(|(_, cid, _, _)| cid == call_id) { *slot = None; }
        }
        if self.active.as_ref().is_some_and(|(_, cid, _, _)| cid.as_deref() == Some(call_id)) {
            self.active = self.queue.pop_front().map(|(c, cid, r)| (c, cid, r, false));
            self.reply_buf.clear();
        }
    }

    pub fn rollback_failed_send(&mut self, caller_id: &str) {
        if self.queue.back().is_some_and(|(c, _, _)| c == caller_id) {
            self.queue.pop_back();
        } else if self.active.as_ref().is_some_and(|(c, _, _, _)| c == caller_id) && self.reply_buf.is_empty() {
            self.active = self.queue.pop_front().map(|(c, cid, r)| (c, cid, r, false));
        }
    }

    pub fn is_answering(&self, target_id: &str) -> bool { self.active.as_ref().is_some_and(|(c, _, _, _)| c == target_id) }
    pub fn active_call_id(&self) -> Option<String> { self.active.as_ref().and_then(|(_, cid, _, _)| cid.clone()) }

    pub fn note_outbound_to(&mut self, target_id: &str) -> Option<(Option<String>, u32)> {
        if let Some((caller, cid, r, sent)) = self.active.as_mut() {
            if caller == target_id {
                *sent = true;
                return Some((cid.clone(), *r));
            }
        }
        None
    }

    pub fn push_line(&mut self, raw_line: &str) {
        let cleaned = crate::codex::sanitize(raw_line);
        if self.assistant_buf.len() < 16 * 1024 {
            if !self.assistant_buf.is_empty() { self.assistant_buf.push('\n'); }
            let rem = (16usize * 1024).saturating_sub(self.assistant_buf.len());
            self.assistant_buf.push_str(&crate::codex::clip(&cleaned, rem));
        }
        if self.active.is_some() && self.reply_buf.len() < MAX_PEER_REPLY {
            if !self.reply_buf.is_empty() { self.reply_buf.push('\n'); }
            let rem = MAX_PEER_REPLY.saturating_sub(self.reply_buf.len());
            self.reply_buf.push_str(&crate::codex::clip(&cleaned, rem));
        }
    }

    pub fn finish(&mut self, self_id: &str, self_label: String) -> Option<PeerDone> {
        let settled_burst_call = self.turn_fifo.pop_front().flatten().and_then(|(_, cid, _, cont)| (!cont).then_some(cid));
        let asst = if std::mem::take(&mut self.sent_in_turn) { String::new() } else { std::mem::take(&mut self.assistant_buf) };
        if let Some((caller_id, call_id, round, explicit_sent)) = self.active.take() {
            let reply_text = std::mem::take(&mut self.reply_buf);
            self.active = self.queue.pop_front().map(|(c, cid, r)| (c, cid, r, false));
            return Some(PeerDone::Reply { caller_id, from_id: self_id.to_string(), from_label: self_label, call_id, round, reply_text, explicit_sent, settled_burst_call, assistant_text: asst });
        }
        if settled_burst_call.is_some() || asst.contains('@') {
            return Some(PeerDone::AssistantTurn { from_id: self_id.to_string(), assistant_text: asst, settled_burst_call });
        }
        None
    }

    pub fn drop_active(&mut self) {
        let _ = self.turn_fifo.pop_front();
        self.active = self.queue.pop_front().map(|(c, cid, r)| (c, cid, r, false));
        self.reply_buf.clear();
        self.assistant_buf.clear();
        self.sent_in_turn = false;
    }
}

fn peer_reply_authorized(shared: &Shared, caller: &str, to: &str) -> bool {
    let (answering, call_id) = core::find(shared, caller).map_or((false, None), |f| {
        let s = f.lock().unwrap();
        (s.busy && s.peer_turn.is_answering(to), s.peer_turn.active_call_id())
    });
    let call_ok = call_id.as_deref().is_some_and(|cid| crate::call_room::is_owner_authorized(shared, cid));
    answering && (call_ok || pending::kept(shared, to, "send", caller))
}

pub(crate) fn try_defer_peer_reply(
    s: &mut crate::session::Session,
    request_id: &str,
    send_text: &str,
    shown_text: &str,
    origin: crate::session::Origin,
) -> bool {
    let has_sink = s.claude.is_some() || s.codex.is_some() || s.input_tx.is_some();
    if !s.busy
        || !has_sink
        || !matches!(origin, crate::session::Origin::PeerReply | crate::session::Origin::PeerReplySession(_))
        || s.peer_turn.deferred_replies.len() >= MAX_DEFERRED_REPLIES
    {
        return false;
    }
    s.accept_deferred_peer(request_id, origin, crate::session::now_secs());
    let reply_slot = s.peer_turn.next_reply_slot.take();
    s.peer_turn.deferred_replies.push_back(DeferredReply {
        request_id: request_id.to_string(),
        send_text: send_text.to_string(),
        shown_text: shown_text.to_string(),
        reply_slot,
    });
    true
}

pub(crate) fn flush_deferred(s: &mut crate::session::Session) {
    while !s.busy {
        let Some(item) = s.peer_turn.deferred_replies.pop_front() else { break };
        s.peer_turn.next_reply_slot = item.reply_slot;
        let sent = if s.codex.is_some() {
            crate::codex::send_input(s, &item.request_id, &item.send_text).map(|()| s.set_busy(true))
        } else if s.claude.is_some() {
            crate::claude::send_input(s, &item.request_id, &item.send_text, None)
        } else if let Some(tx) = s.input_tx.as_ref() {
            let line = format!("I {} {}", item.request_id, crate::payload::hex(item.send_text.as_bytes()));
            match tx.try_send(line) {
                Ok(()) => {
                    s.set_busy(true);
                    Ok(())
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => Err("busy"),
                Err(_) => Err("session_exited"),
            }
        } else {
            Err("session_exited")
        };
        match sent {
            Ok(()) => s.record_flushed_peer(&item.shown_text, crate::session::now_secs()),
            Err(_) => {
                s.peer_turn.next_reply_slot = None;
                s.advance_input(&item.request_id, crate::session::InputState::Failed);
            }
        }
    }
}

pub(crate) fn before_peer_deliver(shared: &Shared, from: Option<&str>, to: &str, by: By) -> Option<(Option<String>, u32)> {
    if by != By::Session { return None; }
    let caller_id = from?;
    let authorized = AUTHORIZED_SEND.with(|c| c.get());
    let reply_meta = core::find(shared, caller_id).and_then(|found| found.lock().unwrap().peer_turn.note_outbound_to(to));
    if reply_meta.is_none() && !is_create_prompt() {
        let round = BURST_ROUND.with(|c| c.get());
        let cid_opt = crate::call_room::peek_pair_call_id(shared, caller_id, to);
        if let Some(found) = core::find(shared, to) {
            let mut s = found.lock().unwrap();
            if matches!(s.provider, "claude" | "codex") {
                s.peer_turn.begin_call(caller_id, cid_opt, round.max(1));
            }
        }
    }
    authorized.then_some(reply_meta).flatten()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn format_explicit_peer_reply(
    shared: &Shared,
    sender_id: &str,
    sender_label: &str,
    to: &str,
    call_id_opt: Option<&str>,
    call_round: u32,
    hhmm: &str,
    text: &str,
) -> String {
    let (_, flag_done) = (BURST_ROUND.with(|c| c.replace(0)), CALL_DONE_FLAG.with(|c| c.replace(false)));
    let (stripped, extra_done) = crate::call_room::strip_call_done(text);
    let marked_done = flag_done || extra_done;
    let trimmed = stripped.trim();
    let clean_text = if trimmed.is_empty() || crate::folders::shows_nothing(trimmed) { text } else { trimmed };
    let both_providers = [sender_id, to]
        .into_iter()
        .all(|id| core::find(shared, id).is_some_and(|f| matches!(f.lock().unwrap().provider, "claude" | "codex")));
    let call_meta = both_providers.then(|| {
        crate::call_room::record_peer_reply(shared, call_id_opt, to, sender_id, sender_label, call_round.max(1), clean_text, marked_done)
    }).flatten();
    if let Some((_, ref cid, r, _)) = call_meta {
        if let Some(found_to) = core::find(shared, to) {
            let eff_r = if marked_done { crate::call_room::MAX_CALL_ROUNDS } else { r };
            found_to.lock().unwrap().peer_turn.next_reply_slot = Some((sender_id.to_string(), cid.clone(), eff_r));
        }
    }
    let call_suffix = match &call_meta {
        Some((_, cid, r, _)) => format!(" | call {} round {r}/{}", crate::call_room::call_id7(cid), crate::call_room::MAX_CALL_ROUNDS),
        None => String::new(),
    };
    let head = format!("[peer reply from '{}' ({}) | {} | {}{}]", sender_label, transfer::id7(sender_id), hhmm, transfer::size_text(clean_text.len()), call_suffix);
    format!("{head}\n{}", transfer::defused(clean_text))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn after_peer_deliver(
    shared: &Shared,
    from: Option<&str>,
    to: &str,
    text: &str,
    by: By,
    ok: bool,
    explicit_recorded: bool,
    input_ok: bool,
) {
    if explicit_recorded {
        if !input_ok {
            if let Some(found_to) = core::find(shared, to) {
                found_to.lock().unwrap().peer_turn.next_reply_slot = None;
            }
        }
        return;
    }
    let (round, marked_done) = (BURST_ROUND.with(|c| c.replace(0)), CALL_DONE_FLAG.with(|c| c.replace(false)));
    if is_create_prompt() {
        return;
    }
    if !ok && by == By::Session {
        if let (Some(caller_id), Some(found)) = (from, core::find(shared, to)) {
            found.lock().unwrap().peer_turn.rollback_failed_send(caller_id);
        }
        return;
    }
    if ok && by == By::Session {
        if let Some(sender) = from {
            let both_providers = [sender, to].into_iter().all(|id| {
                core::find(shared, id).is_some_and(|f| matches!(f.lock().unwrap().provider, "claude" | "codex"))
            });
            if both_providers {
                if let Some((cid, r)) = crate::call_room::record_session_transfer(shared, sender, to, text, round, marked_done) {
                    if let Some(found_to) = core::find(shared, to) {
                        found_to.lock().unwrap().peer_turn.attach_call(sender, cid, r);
                    }
                }
            }
        }
    }
}

pub(crate) fn deliver_peer_reply(shared: &Shared, done: Option<PeerDone>) {
    let Some(done) = done else { return };
    let (caller_id, from_id, from_label, call_id, round, reply_text, explicit_sent, settled_burst_call, assistant_text) = match done {
        PeerDone::AssistantTurn { from_id, assistant_text, settled_burst_call } => {
            if let Some(cid) = settled_burst_call { crate::call_room::settle_call_idle(shared, &cid); }
            crate::mention::dispatch_assistant_mentions(shared, &from_id, None, &assistant_text);
            return;
        }
        PeerDone::Reply { caller_id, from_id, from_label, call_id, round, reply_text, explicit_sent, settled_burst_call, assistant_text } => {
            (caller_id, from_id, from_label, call_id, round, reply_text, explicit_sent, settled_burst_call, assistant_text)
        }
    };
    if let Some(cid) = settled_burst_call { crate::call_room::settle_call_idle(shared, &cid); }
    if !assistant_text.is_empty() {
        crate::mention::dispatch_assistant_mentions(shared, &from_id, Some(&caller_id), &assistant_text);
    }
    if explicit_sent || reply_text.trim().is_empty() { return; }
    let cleaned = leads::agent_text(&reply_text).text;
    if cleaned.trim().is_empty() || crate::folders::shows_nothing(&cleaned) { return; }
    let (stripped, marked_done) = crate::call_room::strip_call_done(&cleaned);
    let trimmed = stripped.trim();
    if marked_done && (trimmed.is_empty() || crate::folders::shows_nothing(trimmed)) {
        if let Some(cid) = call_id.as_deref() { crate::call_room::settle_call_idle(shared, cid); }
        return;
    }
    let display_clean = if trimmed.is_empty() || crate::folders::shows_nothing(trimmed) { cleaned } else { trimmed.to_string() };
    let call_meta = crate::call_room::record_peer_reply(shared, call_id.as_deref(), &caller_id, &from_id, &from_label, round, &display_clean, marked_done);
    if caller_id.is_empty() || caller_id == "owner" || call_meta.as_ref().is_some_and(|m| m.3) { return; }
    if let Some((_, ref cid, r, _)) = call_meta {
        if let Some(found_caller) = core::find(shared, &caller_id) {
            let eff_r = if marked_done { crate::call_room::MAX_CALL_ROUNDS } else { r };
            found_caller.lock().unwrap().peer_turn.next_reply_slot = Some((from_id.clone(), cid.clone(), eff_r));
        }
    }
    let body = crate::codex::clip(&transfer::defused(&display_clean), 56 * 1024);
    let call_suffix = match &call_meta {
        Some((_, cid, r, _)) => format!(" | call {} round {r}/{}", crate::call_room::call_id7(cid), crate::call_room::MAX_CALL_ROUNDS),
        None => String::new(),
    };
    let head = format!("[peer reply from '{}' ({}) | {} | {}{}]", from_label, transfer::id7(&from_id), transfer::hhmm_now(), transfer::size_text(display_clean.len()), call_suffix);
    let payload = format!("{head}\n{body}");
    let req_id = format!("pr-{}", core::random_hex(8));
    if core::input(shared, &caller_id, None, &req_id, &payload, crate::session::Origin::PeerReply, None).is_err() {
        if let Some(found_caller) = core::find(shared, &caller_id) { found_caller.lock().unwrap().peer_turn.next_reply_slot = None; }
    }
}

/// What a card says after its head: how much text there is (the cleaned text, which is what is sent), what the cleaning took out, and the opening quote of the start of it.
pub(crate) fn card_tail(text: &str, removed: usize) -> String {
    let lines = transfer::line_count(text);
    let size = if lines > 1 { format!("{}, {}", transfer::size_text(text.len()), transfer::lines_word(lines)) } else { transfer::size_text(text.len()) };
    let removed = match removed {
        0 => String::new(),
        1 => " | 1 hidden character removed".to_string(),
        n => format!(" | {n} hidden characters removed"),
    };
    format!(" | {size}{removed} | \"")
}

/// A card's line: `head`, the tail (`card_tail`) and the start of the text, in one line of at most `CARD_CHARS`.
pub(crate) fn card_line(head: &str, text: &str, removed: usize) -> String {
    let head = format!("{head}{}", card_tail(text, removed));
    let first = text.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let first = sanitize(first).replace('"', "'");
    let room = CARD_CHARS.saturating_sub(head.chars().count() + 2);
    let cut = first.chars().count() > room;
    let shown: String = first.chars().take(room).collect();
    format!("{head}{shown}{}\"", if cut { "\u{2026}" } else { "" })
}

/// What the person's yes does for a request of `caller`. Err: the refusal the host gives now, in the words the model reads.
pub(crate) fn perform(shared: &Arc<Shared>, caller: &str, id: i64, tool: &str, target: &str, who: &str, text: &str) -> Result<String, String> {
    match tool {
        "create" => crate::create::perform(shared, caller, target, text),
        "history" => crate::history::perform(shared, caller, target, text),
        "capture" | "clipboard_read" | "clipboard_write" => crate::screentools::perform(shared, caller, id, tool, target, who, text),
        "computer" => crate::computer::perform(shared, caller),
        "send" => match transfer::send(shared, Some(caller), target, "text", text, By::Session) {
            Ok(done) => Ok(format!(
                "{} delivered to {who} ({}); the receiver was {} (its `[peer reply from ...]` will arrive in a separate turn once it finishes; acknowledge delivery briefly without assuming the reply was missed)",
                transfer::size_text(text.len()),
                done["delivery"].as_str().unwrap_or("inline"),
                done["receiver"].as_str().unwrap_or("idle")
            )),
            Err(code) => Err(refusal(shared, caller, code)),
        },
        _ => Err(refusal(shared, caller, "refused")),
    }
}

/// What the model reads for a refusal: the name of the error, then what to do about it.
pub(crate) fn refusal(shared: &Shared, caller: &str, code: &str) -> String {
    let what = match code {
        "unknown_session" => "there is no session with that id; ask the sessions tool for the ids".to_string(),
        "session_disconnected" => "that session has no running program; the person can resume it".into(),
        "session_exited" => "the program of that session has ended".into(),
        "busy" => "the host could not take the text just now; try again in a moment".into(),
        "too_large" => "the text is too large; split it, or write it to a file and ask the person to hand it over".into(),
        "bad_request" => "send needs `to` (a session id from the sessions tool) and `text` (not empty, and not only characters that draw nothing)".into(),
        "chain_depth" => {
            let hops = core::find(shared, caller).map_or(0, |found| found.lock().unwrap().incoming_hops) + 1;
            format!("this would be step {hops} in a row of sessions that pass text on without a person, and the limit is {MAX_HOPS}; stop here and tell the person")
        }
        _ => "the host refused it".into(),
    };
    format!("{code}: {what}")
}

#[cfg(test)]
#[path = "calls/tests.rs"]
pub(crate) mod tests;
