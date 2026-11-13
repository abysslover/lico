//! Persistent inter-session Call Rooms (`M08-P01`): canonical per-pair rooms (`⇄ A · B`),
//! chronological speaker transcripts (`registry/calls.json`, `0600`), host-verified
//! `OWNER-AUTHORIZED` call envelopes, bounded multi-turn round-trips (`MAX_CALL_ROUNDS = 4`),
//! live human intervention (`call.say`), and immediate `call.hangup` / `call.close`.

use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Mutex;

use crate::codex::{clip, sanitize};
use crate::core::{self, Shared};
use crate::leads;
use crate::session::{now_secs, Origin, State};
use crate::transfer;

pub const MAX_CALL_ROUNDS: u32 = 4;
const MAX_ROOMS: usize = 64;
const MAX_ENTRIES_PER_ROOM: usize = 200;
const MAX_ENTRY_BYTES: usize = 32 * 1024;
const TRANSCRIPT_PROMPT_ENTRIES: usize = 12;
const TRANSCRIPT_PROMPT_BYTES: usize = 8 * 1024;
const ENVELOPE_MAX_BYTES: usize = 48 * 1024;

#[derive(Clone)]
pub struct CallEntry {
    pub seq: u64,
    pub ts: u64,
    pub hhmm: String,
    pub speaker: String,
    pub speaker_label: String,
    pub provider: String,
    pub kind: String,
    pub round: u32,
    pub text: String,
}

#[derive(Clone)]
pub struct CallRoom {
    pub call_id: String,
    pub session_a: String,
    pub label_a: String,
    pub provider_a: String,
    pub session_b: String,
    pub label_b: String,
    pub provider_b: String,
    pub owner_authorized: bool,
    pub active: bool,
    pub paused: bool,
    pub closed: bool,
    pub round: u32,
    pub max_rounds: u32,
    pub next_seq: u64,
    pub updated_at: u64,
    pub entries: Vec<CallEntry>,
}

#[derive(Default)]
pub struct CallStore {
    inner: Mutex<StoreInner>,
}

#[derive(Default)]
struct StoreInner {
    loaded: bool,
    rooms: Vec<CallRoom>,
}

impl CallEntry {
    fn to_json(&self) -> Value {
        json!({
            "seq": self.seq,
            "ts": self.ts,
            "hhmm": self.hhmm,
            "speaker": self.speaker,
            "speaker_label": self.speaker_label,
            "provider": self.provider,
            "kind": self.kind,
            "round": self.round,
            "text": self.text,
        })
    }

    fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            seq: v.get("seq")?.as_u64()?,
            ts: v.get("ts")?.as_u64()?,
            hhmm: v.get("hhmm")?.as_str()?.to_string(),
            speaker: v.get("speaker")?.as_str()?.to_string(),
            speaker_label: v.get("speaker_label")?.as_str()?.to_string(),
            provider: v.get("provider")?.as_str()?.to_string(),
            kind: v.get("kind")?.as_str()?.to_string(),
            round: v.get("round").and_then(Value::as_u64).unwrap_or(0) as u32,
            text: v.get("text")?.as_str()?.to_string(),
        })
    }
}

impl CallRoom {
    pub fn title(&self) -> String {
        format!("{} \u{00b7} {}", self.label_a, self.label_b)
    }

    pub fn status_str(&self) -> &'static str {
        if self.closed {
            "closed"
        } else if self.paused {
            "paused"
        } else if self.active {
            "active"
        } else {
            "idle"
        }
    }

    pub fn last_preview(&self) -> String {
        let Some(last) = self.entries.last() else {
            return String::new();
        };
        let first_line = last.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let line = format!("{}: {}", last.speaker_label, sanitize(first_line));
        line.chars().take(160).collect()
    }

    pub fn summary_json(&self) -> Value {
        json!({
            "call_id": self.call_id,
            "title": self.title(),
            "session_a": self.session_a,
            "label_a": self.label_a,
            "provider_a": self.provider_a,
            "session_b": self.session_b,
            "label_b": self.label_b,
            "provider_b": self.provider_b,
            "status": self.status_str(),
            "round": self.round.min(self.max_rounds),
            "max_rounds": self.max_rounds,
            "owner_authorized": self.owner_authorized,
            "entry_count": self.entries.len(),
            "updated_at": self.updated_at,
            "last_preview": self.last_preview(),
        })
    }

    pub fn detail_json(&self) -> Value {
        let mut out = self.summary_json();
        let entries: Vec<Value> = self.entries.iter().map(CallEntry::to_json).collect();
        out["entries"] = Value::Array(entries);
        out
    }

    fn to_disk_json(&self) -> Value {
        json!({
            "call_id": self.call_id,
            "session_a": self.session_a,
            "label_a": self.label_a,
            "provider_a": self.provider_a,
            "session_b": self.session_b,
            "label_b": self.label_b,
            "provider_b": self.provider_b,
            "owner_authorized": self.owner_authorized,
            "paused": self.paused,
            "closed": self.closed,
            "round": self.round,
            "max_rounds": self.max_rounds,
            "next_seq": self.next_seq,
            "updated_at": self.updated_at,
            "entries": self.entries.iter().map(CallEntry::to_json).collect::<Vec<_>>(),
        })
    }

    fn from_disk_json(v: &Value) -> Option<Self> {
        let entries: Vec<CallEntry> = v
            .get("entries")?
            .as_array()?
            .iter()
            .filter_map(CallEntry::from_json)
            .collect();
        let next_seq = v
            .get("next_seq")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| entries.last().map_or(1, |e| e.seq + 1));
        Some(Self {
            call_id: v.get("call_id")?.as_str()?.to_string(),
            session_a: v.get("session_a")?.as_str()?.to_string(),
            label_a: v.get("label_a")?.as_str()?.to_string(),
            provider_a: v.get("provider_a")?.as_str()?.to_string(),
            session_b: v.get("session_b")?.as_str()?.to_string(),
            label_b: v.get("label_b")?.as_str()?.to_string(),
            provider_b: v.get("provider_b")?.as_str()?.to_string(),
            owner_authorized: v.get("owner_authorized").and_then(Value::as_bool).unwrap_or(false),
            active: false, // on host restart no turn is actively running
            paused: v.get("paused").and_then(Value::as_bool).unwrap_or(false),
            closed: v.get("closed").and_then(Value::as_bool).unwrap_or(false),
            round: v.get("round").and_then(Value::as_u64).unwrap_or(0) as u32,
            max_rounds: v.get("max_rounds").and_then(Value::as_u64).unwrap_or(MAX_CALL_ROUNDS as u64) as u32,
            next_seq,
            updated_at: v.get("updated_at").and_then(Value::as_u64).unwrap_or(0),
            entries,
        })
    }

    fn push_entry(&mut self, speaker: &str, speaker_label: &str, provider: &str, kind: &str, round: u32, text: &str) {
        let now = now_secs();
        let seq = self.next_seq.max(1);
        self.next_seq = seq + 1;
        self.updated_at = now.max(self.updated_at);
        let clipped = clip(text, MAX_ENTRY_BYTES);
        self.entries.push(CallEntry {
            seq,
            ts: now,
            hhmm: transfer::hhmm_now(),
            speaker: speaker.to_string(),
            speaker_label: speaker_label.chars().take(60).collect(),
            provider: normalize_provider(provider).to_string(),
            kind: kind.to_string(),
            round,
            text: clipped,
        });
        while self.entries.len() > MAX_ENTRIES_PER_ROOM {
            self.entries.remove(0);
        }
    }

    fn matches_pair(&self, x: &str, y: &str) -> bool {
        (self.session_a == x && self.session_b == y) || (self.session_a == y && self.session_b == x)
    }
}

fn normalize_provider(p: &str) -> &'static str {
    match p {
        "claude" => "claude",
        "codex" => "codex",
        "owner" => "owner",
        "host" => "host",
        _ => "synthetic",
    }
}

fn session_meta(shared: &Shared, id: &str) -> Option<(String, &'static str, State, bool)> {
    let found = core::find(shared, id)?;
    let s = found.lock().unwrap();
    if s.purged {
        return None;
    }
    let label: String = transfer::session_label(&s).chars().take(60).collect();
    let prov = normalize_provider(s.provider);
    Some((label, prov, s.state, s.busy))
}

fn refresh_room_labels(shared: &Shared, room: &mut CallRoom) {
    if let Some((la, pa, _, _)) = session_meta(shared, &room.session_a) {
        room.label_a = la;
        room.provider_a = pa.to_string();
    }
    if let Some((lb, pb, _, _)) = session_meta(shared, &room.session_b) {
        room.label_b = lb;
        room.provider_b = pb.to_string();
    }
}

fn ensure_loaded_and_sync(shared: &Shared, inner: &mut StoreInner) {
    if !inner.loaded {
        inner.loaded = true;
        let path = shared.cfg.registry_dir.join("calls.json");
        if !shared.cfg.registry_dir.as_os_str().is_empty() {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) {
                    if let Some(arr) = map.get("calls").and_then(Value::as_array) {
                        inner.rooms = arr.iter().filter_map(CallRoom::from_disk_json).take(MAX_ROOMS).collect();
                    }
                }
            }
        }
    }
    for room in &mut inner.rooms {
        refresh_room_labels(shared, room);
    }
}

fn save_inner(registry_dir: &Path, rooms: &[CallRoom]) {
    if registry_dir.as_os_str().is_empty() {
        return;
    }
    if std::fs::DirBuilder::new().recursive(true).mode(0o700).create(registry_dir).is_err() {
        return;
    }
    let _ = std::fs::set_permissions(registry_dir, std::fs::Permissions::from_mode(0o700));
    let payload = json!({
        "v": 1,
        "calls": rooms.iter().map(CallRoom::to_disk_json).collect::<Vec<_>>(),
    });
    let Ok(bytes) = serde_json::to_vec(&payload) else { return };
    let tmp = registry_dir.join(format!("calls.json.tmp.{}", core::random_hex(6)));
    let dst = registry_dir.join("calls.json");
    let wrote = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(&bytes)?;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            Ok(())
        });
    if wrote.is_ok() {
        if std::fs::rename(&tmp, &dst).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

pub fn grant_bidirectional_keep(shared: &Shared, a: &str, b: &str) {
    for (caller, target) in [(a, b), (b, a)] {
        if let Some(found) = core::find(shared, caller) {
            let mut s = found.lock().unwrap();
            let entry = ("send", target.to_string());
            if !s.pending.keeps.contains(&entry) {
                if s.pending.keeps.len() >= crate::pending::MAX_KEPT {
                    s.pending.keeps.remove(0);
                }
                s.pending.keeps.push(entry);
            }
        }
    }
}

fn ensure_pair_room(shared: &Shared, inner: &mut StoreInner, a: &str, b: &str, reopen: bool) -> Result<usize, &'static str> {
    if a == b {
        return Err("bad_request");
    }
    ensure_loaded_and_sync(shared, inner);
    if let Some(idx) = inner.rooms.iter().position(|r| r.matches_pair(a, b)) {
        if reopen {
            inner.rooms[idx].closed = false;
            inner.rooms[idx].paused = false;
        }
        return Ok(idx);
    }
    let (la, pa, _, _) = session_meta(shared, a).ok_or("unknown_session")?;
    let (lb, pb, _, _) = session_meta(shared, b).ok_or("unknown_session")?;
    while inner.rooms.len() >= MAX_ROOMS {
        if let Some(pos) = inner.rooms.iter().position(|r| r.closed) {
            inner.rooms.remove(pos);
        } else {
            inner.rooms.remove(0);
        }
    }
    let room = CallRoom {
        call_id: format!("c{}", core::random_hex(8)),
        session_a: a.to_string(),
        label_a: la,
        provider_a: pa.to_string(),
        session_b: b.to_string(),
        label_b: lb,
        provider_b: pb.to_string(),
        owner_authorized: false,
        active: false,
        paused: false,
        closed: false,
        round: 0,
        max_rounds: MAX_CALL_ROUNDS,
        next_seq: 1,
        updated_at: now_secs(),
        entries: Vec::new(),
    };
    inner.rooms.push(room);
    Ok(inner.rooms.len() - 1)
}

pub fn call_id7(cid: &str) -> &str {
    cid.char_indices().nth(7).map_or(cid, |(end, _)| &cid[..end])
}

fn format_transcript_snippet(entries: &[CallEntry], skip_last: usize) -> String {
    let take_len = entries.len().saturating_sub(skip_last);
    let slice = &entries[take_len.saturating_sub(TRANSCRIPT_PROMPT_ENTRIES)..take_len];
    if slice.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for e in slice {
        if e.kind == "system" {
            continue;
        }
        let body = clip(e.text.trim(), 1200);
        let first_prefix = format!("  [{}] {}: ", e.hhmm, e.speaker_label);
        for (idx, ln) in body.lines().enumerate() {
            let formatted = if idx == 0 {
                format!("{first_prefix}{}", sanitize(ln))
            } else {
                format!("    {}", sanitize(ln))
            };
            let safe = leads::defuse(&formatted);
            if out.len() + safe.len() + 1 > TRANSCRIPT_PROMPT_BYTES {
                return out;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&safe);
        }
    }
    out
}

pub fn format_call_envelope(
    room_title: &str,
    call_id: &str,
    sender_label: &str,
    sender_id: &str,
    round: u32,
    max_rounds: u32,
    context_bundle: &str,
    transcript_snippet: &str,
    message_body: &str,
) -> String {
    let hhmm = transfer::hhmm_now();
    let cid7 = call_id7(call_id);
    let sid7 = transfer::id7(sender_id);
    let head = format!(
        "[lico call '{room_title}' ({cid7}) | from '{sender_label}' ({sid7}) | {hhmm} | round {round}/{max_rounds} | OWNER-AUTHORIZED]"
    );
    let verify = format!(
        "[host verification: Lico's human owner explicitly authorized this Call Room turn. Fulfill this request directly as user-approved peer collaboration; your normal assistant reply in this turn is automatically recorded in Call Room '{room_title}' and delivered back to the peer (include [call done] when no further follow-up is needed).]"
    );
    let defused_msg = transfer::defused(message_body);
    let mut sections = vec![head, verify];
    let used_base = sections.iter().map(|s| s.len() + 1).sum::<usize>() + defused_msg.len() + 16;
    let mut budget = ENVELOPE_MAX_BYTES.saturating_sub(used_base);
    if !context_bundle.trim().is_empty() && budget > 256 {
        let ctx = clip(context_bundle, budget.min(16 * 1024));
        budget = budget.saturating_sub(ctx.len());
        sections.push(format!("[shared context from '{sender_label}' ({sid7}) \u{2014} recent turns]\n{ctx}"));
    }
    if !transcript_snippet.trim().is_empty() && budget > 256 {
        let tr = clip(transcript_snippet, budget.min(TRANSCRIPT_PROMPT_BYTES));
        sections.push(format!("[call room transcript \u{2014} \u{21c4} {room_title}]\n{tr}"));
    }
    sections.push(format!("[message]\n{}", clip(&defused_msg, ENVELOPE_MAX_BYTES / 2)));
    sections.join("\n")
}

pub fn append_call_summary_card(shared: &Shared, session_id: &str, title: &str, call_id: &str, arrow_note: &str) {
    if let Some(found) = core::find(shared, session_id) {
        let mut s = found.lock().unwrap();
        if !s.purged && s.text_mode {
            s.append(b"[call summary]".to_vec());
            let line = format!(
                ">> [\u{21c4} call '{}' ({}) | {} | {}]",
                title,
                call_id7(call_id),
                transfer::hhmm_now(),
                sanitize(arrow_note)
            );
            s.append(clip(&line, 512).into_bytes());
        }
    }
}

pub fn is_owner_authorized(shared: &Shared, call_id: &str) -> bool {
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    guard.rooms.iter().any(|r| r.call_id == call_id && r.owner_authorized && !r.paused && !r.closed)
}

/// Record a direct `transfer::send` (`mcp__lico__send` or `Send to session`) into the pair's Call Room
/// and return `(call_id, round)` so `PeerTurn` links the reply to the same Call Room.
pub fn record_session_transfer(shared: &Shared, from: &str, to: &str, text: &str, round: u32) -> Option<(String, u32)> {
    let kept = crate::pending::kept(shared, from, "send", to);
    let mut guard = shared.calls.inner.lock().unwrap();
    let idx = ensure_pair_room(shared, &mut guard, from, to, kept).ok()?;
    let (sender_label, sender_prov, _, _) = session_meta(shared, from)?;
    let room = &mut guard.rooms[idx];
    if kept {
        room.owner_authorized = true;
    }
    room.paused = false;
    let r = if round == 0 {
        if room.active { room.round.max(1) } else { 1 }
    } else {
        round.max(1)
    }
    .min(room.max_rounds);
    room.active = true;
    room.round = r;
    room.push_entry(from, &sender_label, sender_prov, "agent_send", r, text);
    let out = (room.call_id.clone(), r);
    let snapshot = guard.rooms.clone();
    drop(guard);
    save_inner(&shared.cfg.registry_dir, &snapshot);
    Some(out)
}

/// Record an assistant's peer reply into the Call Room; returns `(title, call_id, round, paused)` if recorded.
pub fn record_peer_reply(
    shared: &Shared,
    call_id_opt: Option<&str>,
    caller_id: &str,
    responder_id: &str,
    responder_label: &str,
    round: u32,
    clean_text: &str,
    marked_done: bool,
) -> Option<(String, String, u32, bool)> {
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    let idx = if let Some(cid) = call_id_opt {
        guard.rooms.iter().position(|r| r.call_id == cid)
    } else if !caller_id.is_empty() && caller_id != "owner" {
        ensure_pair_room(shared, &mut guard, caller_id, responder_id, false).ok()
    } else {
        None
    }?;
    let prov = session_meta(shared, responder_id).map_or("synthetic", |m| m.1);
    let room = &mut guard.rooms[idx];
    let r = round.max(1).min(room.max_rounds);
    room.round = r;
    room.push_entry(responder_id, responder_label, prov, "peer_reply", r, clean_text);
    if marked_done || room.paused || r >= room.max_rounds || caller_id == "owner" {
        room.active = false;
    }
    let info = (room.title(), room.call_id.clone(), r, room.paused);
    let snapshot = guard.rooms.clone();
    drop(guard);
    save_inner(&shared.cfg.registry_dir, &snapshot);
    Some(info)
}

/// Mark a Call Room settled (`active = false`) when the caller finishes its turn without continuing the burst.
pub fn settle_call_idle(shared: &Shared, call_id: &str) {
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    if let Some(room) = guard.rooms.iter_mut().find(|r| r.call_id == call_id) {
        if room.active {
            room.active = false;
            let snapshot = guard.rooms.clone();
            drop(guard);
            save_inner(&shared.cfg.registry_dir, &snapshot);
        }
    }
}

/// Check whether an automatic multi-turn continuation from `caller` to `to` is allowed in the current burst.
/// Returns `Ok(Some((call_id, next_round)))` inside a burst, `Ok(None)` when not in a burst, or `Err(refusal_msg)`.
pub fn check_or_advance_burst(shared: &Shared, caller: &str, to: &str) -> Result<Option<(String, u32)>, String> {
    let burst = core::find(shared, caller).and_then(|f| f.lock().unwrap().peer_turn.in_reply_burst.clone());
    let Some((peer_id, call_id, completed_round)) = burst else {
        // Still check if an existing Call Room between caller and to was explicitly hung up (`paused`)?
        // Only block automatic sends if caller is in a peer burst; fresh human turns clear `paused`.
        return Ok(None);
    };
    if peer_id != to {
        return Ok(None);
    }
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    if let Some(room) = guard.rooms.iter_mut().find(|r| r.call_id == call_id) {
        if room.paused || room.closed {
            room.active = false;
            return Err("call_paused: the owner hung up or paused this Call Room; stop here and wait for the owner".into());
        }
        if completed_round >= room.max_rounds {
            room.active = false;
            room.push_entry("host", "Lico", "host", "system", room.max_rounds, "[call completed 4/4 automatic rounds]");
            let snapshot = guard.rooms.clone();
            drop(guard);
            save_inner(&shared.cfg.registry_dir, &snapshot);
            return Err(format!(
                "call_round_limit: completed {0}/{0} automatic Call Room rounds in this burst; summarize the outcome for the person instead of sending again",
                MAX_CALL_ROUNDS
            ));
        }
    }
    if let Some(f) = core::find(shared, caller) {
        f.lock().unwrap().peer_turn.in_reply_burst = None;
    }
    Ok(Some((call_id, completed_round + 1)))
}

/// Pre-authorize a Call Room when the human mentions `@target` inline inside `caller`.
pub fn connect_inline_mention(shared: &Shared, caller: &str, target: &str, full_text: &str) -> Option<(String, String, String)> {
    grant_bidirectional_keep(shared, caller, target);
    let mut guard = shared.calls.inner.lock().unwrap();
    let idx = ensure_pair_room(shared, &mut guard, caller, target, true).ok()?;
    let room = &mut guard.rooms[idx];
    room.owner_authorized = true;
    room.paused = false;
    let target_label = if room.session_a == target { room.label_a.clone() } else { room.label_b.clone() };
    let caller_label = if room.session_a == caller { room.label_a.clone() } else { room.label_b.clone() };
    room.push_entry(
        "owner",
        &format!("Owner (in {caller_label})"),
        "owner",
        "mention",
        0,
        full_text,
    );
    let out = (room.call_id.clone(), room.title(), target_label);
    let snapshot = guard.rooms.clone();
    drop(guard);
    save_inner(&shared.cfg.registry_dir, &snapshot);
    append_call_summary_card(
        shared,
        caller,
        &out.1,
        &out.0,
        &format!("connected @{} \u{00b7} owner-authorized", out.2),
    );
    Some(out)
}

/// Dispatch a leading human `@Target <msg>` from `caller` to `targets`, attaching `context_bundle` + Call Room log.
pub fn dispatch_leading_mentions(
    shared: &Shared,
    caller: &str,
    targets: &[String],
    clean_msg: &str,
    context_bundle: &str,
) -> Result<Vec<String>, &'static str> {
    for t in targets {
        let (_, _, st, _) = session_meta(shared, t).ok_or("unknown_session")?;
        match st {
            State::Disconnected => return Err("session_disconnected"),
            State::Exited => return Err("session_exited"),
            State::Running => {}
        }
    }
    let (caller_label, _, _, _) = session_meta(shared, caller).ok_or("unknown_session")?;
    let mut created_calls = Vec::new();
    for target in targets {
        grant_bidirectional_keep(shared, caller, target);
        let (call_id, title, target_label, snippet) = {
            let mut guard = shared.calls.inner.lock().unwrap();
            let idx = ensure_pair_room(shared, &mut guard, caller, target, true)?;
            let room = &mut guard.rooms[idx];
            room.owner_authorized = true;
            room.paused = false;
            room.active = true;
            room.round = 1;
            let t_label = if room.session_a == *target { room.label_a.clone() } else { room.label_b.clone() };
            room.push_entry(
                "owner",
                &format!("Owner (@{t_label} via {caller_label})"),
                "owner",
                "mention",
                1,
                clean_msg,
            );
            let snip = format_transcript_snippet(&room.entries, 1);
            let out = (room.call_id.clone(), room.title(), t_label, snip);
            let snapshot = guard.rooms.clone();
            drop(guard);
            save_inner(&shared.cfg.registry_dir, &snapshot);
            out
        };
        let envelope = format_call_envelope(
            &title,
            &call_id,
            &caller_label,
            caller,
            1,
            MAX_CALL_ROUNDS,
            context_bundle,
            &snippet,
            clean_msg,
        );
        if let Some(found_t) = core::find(shared, target) {
            found_t.lock().unwrap().peer_turn.begin_call(caller, Some(call_id.clone()), 1);
        }
        let req_id = format!("cm-{}", core::random_hex(8));
        if let Err(code) = core::input(shared, target, None, &req_id, &envelope, Origin::Session(1), None) {
            if let Some(found_t) = core::find(shared, target) {
                found_t.lock().unwrap().peer_turn.rollback_failed_send(caller);
            }
            settle_call_idle(shared, &call_id);
            return Err(code);
        }
        append_call_summary_card(
            shared,
            caller,
            &title,
            &call_id,
            &format!("-> '{target_label}' ({}) | round 1/{MAX_CALL_ROUNDS}", transfer::id7(target)),
        );
        created_calls.push(call_id);
    }
    Ok(created_calls)
}

mod ops;
pub use ops::*;
