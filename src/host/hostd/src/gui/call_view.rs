//! Top-bar Call Room tabs (`⇄ <Label A>·<Label B>`), chronological right-pane Call Room view,
//! live human intervention (`call.say`), `[ Hang up ]` (`call.hangup`), and clickable `[⇄ call ...]`
//! / `[peer reply ... | call cXXXXXX ...]` transcript jump cards (`M08-P02`).

use std::collections::{HashMap, VecDeque};

use cosmic_text::{Buffer, Color, Metrics, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;

use crate::transfer;
use super::controls::{Action, Rect};
use super::panel::sendto::extract_call_id7;
use super::panel::AsyncJob;
use super::sidebar_render::clip_cols;
use super::{clean, emit_ime, App, View};

#[derive(Clone, Debug)]
pub struct CallDetailEntry {
    pub hhmm: String,
    pub sender_id: String,
    pub sender_label: String,
    pub sender_kind: String,
    pub kind: String,
    pub round: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct CallDetail {
    pub call_id: String,
    pub title: String,
    pub session_a: String,
    pub session_b: String,
    pub provider_a: String,
    pub provider_b: String,
    pub label_a: String,
    pub label_b: String,
    pub status: String,
    pub active: bool,
    pub paused: bool,
    pub owner_authorized: bool,
    pub round: u32,
    pub max_rounds: u32,
    pub updated_at: u64,
    pub entries: Vec<CallDetailEntry>,
}

#[derive(Clone, Default)]
pub struct CallTabGeom {
    pub call_id: String,
    pub label: String,
    pub active: bool,
    pub selected: bool,
    pub status: String,
    pub round: u32,
    pub rect: Rect,
    pub close_rect: Rect,
}

#[derive(Default)]
pub struct CallViewState {
    pub open_tabs: Vec<String>,
    pub active_call: Option<String>,
    pub details: HashMap<String, CallDetail>,
    pub tab_items_geom: Vec<CallTabGeom>,
    pub hangup_rect: Option<Rect>,
    pub jump_a_rect: Option<Rect>,
    pub jump_b_rect: Option<Rect>,
}

fn fmt_hhmm(epoch_secs: u64) -> String {
    let day_secs = epoch_secs % 86_400;
    format!("{:02}:{:02}", day_secs / 3600, (day_secs % 3600) / 60)
}

fn parse_call_detail(v: &Value) -> Option<CallDetail> {
    let c = if v.get("call").is_some() { &v["call"] } else { v };
    let call_id = c["call_id"].as_str()?.to_string();
    let session_a = c["session_a"].as_str()?.to_string();
    let session_b = c["session_b"].as_str()?.to_string();
    let label_a = c["label_a"].as_str().unwrap_or(&session_a).to_string();
    let label_b = c["label_b"].as_str().unwrap_or(&session_b).to_string();
    let title = c["title"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{label_a} \u{00b7} {label_b}"));
    let entries = c["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let at = e["ts"].as_u64().or_else(|| e["at"].as_u64()).unwrap_or(0);
            Some(CallDetailEntry {
                hhmm: e["hhmm"].as_str().map(str::to_string).unwrap_or_else(|| fmt_hhmm(at)),
                sender_id: e["speaker"].as_str().or_else(|| e["sender_id"].as_str()).unwrap_or("").to_string(),
                sender_label: e["speaker_label"].as_str().or_else(|| e["sender_label"].as_str()).unwrap_or("").to_string(),
                sender_kind: e["provider"].as_str().or_else(|| e["sender_kind"].as_str()).unwrap_or("owner").to_string(),
                kind: e["kind"].as_str().unwrap_or("message").to_string(),
                round: e["round"].as_u64().unwrap_or(0) as u32,
                text: e["text"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();
    Some(CallDetail {
        call_id,
        title,
        session_a,
        session_b,
        provider_a: c["provider_a"].as_str().unwrap_or("claude").to_string(),
        provider_b: c["provider_b"].as_str().unwrap_or("codex").to_string(),
        label_a,
        label_b,
        status: c["status"].as_str().unwrap_or("idle").to_string(),
        active: c["active"] == json!(true),
        paused: c["paused"] == json!(true),
        owner_authorized: c["owner_authorized"] == json!(true),
        round: c["round"].as_u64().unwrap_or(0) as u32,
        max_rounds: c["max_rounds"].as_u64().unwrap_or(4) as u32,
        updated_at: c["updated_at"].as_u64().unwrap_or(0),
        entries,
    })
}

fn format_call_lines(detail: &CallDetail) -> VecDeque<(i64, String)> {
    let mut lines = VecDeque::new();
    let mut seq: i64 = 1;
    let mut push = |text: String| {
        lines.push_back((seq, clean(&text)));
        seq += 1;
    };
    let auth_tag = if detail.owner_authorized { "owner-authorized" } else { "pending-auth" };
    push(format!(
        "[tool] \u{21c4} Call Room '{}' ({}) \u{00b7} {} ({}) \u{2194} {} ({}) \u{00b7} {auth_tag} \u{00b7} max {} auto rounds",
        detail.title,
        &detail.call_id[..detail.call_id.len().min(7)],
        detail.label_a,
        detail.provider_a,
        detail.label_b,
        detail.provider_b,
        detail.max_rounds
    ));
    if detail.entries.is_empty() {
        push("No messages in this Call Room yet. Type below (@AgentName or @all) to start talking.".into());
        return lines;
    }
    for entry in &detail.entries {
        let hhmm = entry.hhmm.as_str();
        match entry.sender_kind.as_str() {
            "owner" => {
                push(format!(">> [{} | {hhmm}]", entry.sender_label));
                for raw in entry.text.lines() {
                    push(if raw.is_empty() { ">>".into() } else { format!(">> {raw}") });
                }
            }
            "host" => {
                push(format!("[tool] [Lico | {hhmm}] {}", entry.text));
            }
            prov => {
                let sid7 = transfer::id7(&entry.sender_id);
                let sz = transfer::size_text(entry.text.len());
                let speaker = format!("{} \u{00b7} {prov}", entry.sender_label.replace('\'', ""));
                if entry.kind == "peer_reply" {
                    let r_sfx = if entry.round > 0 {
                        format!(" | round {}/{}", entry.round, detail.max_rounds)
                    } else {
                        String::new()
                    };
                    push(format!(">> [peer reply from '{speaker}' ({sid7}) | {hhmm} | {sz}{r_sfx}]"));
                } else {
                    push(format!(">> [from session '{speaker}' ({sid7}) | {hhmm} | message | {sz}]"));
                }
                for raw in entry.text.lines() {
                    push(if raw.is_empty() { ">>".into() } else { format!(">> {raw}") });
                }
            }
        }
    }
    lines
}

impl App {
    pub(crate) fn active_call_open(&self) -> bool {
        self.ui.panel.is_none() && self.ui.calls.active_call.is_some()
    }

    pub(crate) fn has_prompt_target(&self) -> bool {
        !self.session.is_empty() || self.ui.calls.active_call.is_some()
    }

    pub(crate) fn prompt_target_id(&self) -> String {
        self.ui
            .calls
            .active_call
            .clone()
            .unwrap_or_else(|| self.session.clone())
    }

    pub(crate) fn park_active_call_draft(&mut self) {
        if let Some(prev_cid) = self.ui.calls.active_call.take() {
            let pending = std::mem::take(&mut self.preedit);
            self.ed.park(&prev_cid, &pending);
            if !pending.is_empty() {
                self.stale_pending = true;
                self.stale_empty = false;
            }
        }
    }

    pub(crate) fn open_call_tab(&mut self, call_id: &str) {
        if call_id.is_empty() {
            return;
        }
        if self.ui.panel.is_some() {
            self.panel_close();
        }
        if self.ui.calls.active_call.as_deref() == Some(call_id) && self.view.is_some() {
            self.fetch_call_detail(call_id);
            self.dirty = true;
            return;
        }
        if !self.session.is_empty() {
            self.hide();
        } else {
            self.park_active_call_draft();
        }
        if !self.ui.calls.open_tabs.iter().any(|c| c == call_id) {
            self.ui.calls.open_tabs.push(call_id.to_string());
        }
        self.ui.calls.active_call = Some(call_id.to_string());
        self.ui.scroll = 0;
        self.ui.sel = None;
        self.ui.behind = false;
        self.ui.history_pending = false;
        self.ui.composed = None;
        self.load_fonts();
        let (fp, lh) = (self.font_px(), self.line_h());
        let scale = self.ui.scale.max(1.0);
        if let Some(fs) = self.font_system.as_mut() {
            let buffer = Buffer::new(fs, Metrics::new(fp * scale, lh * scale));
            self.view = Some(View {
                lines: VecDeque::new(),
                surface: None,
                buffer,
                swash: SwashCache::new(),
                session_rgb: 0xFF00_3848,
                seq_rgb: 0xFF00_0000,
            });
        }
        self.ed.resume(call_id);
        self.rebuild_active_call_lines();
        self.fetch_call_detail(call_id);
        self.refresh_list();
        self.dirty = true;
    }

    pub(crate) fn close_call_tab(&mut self, call_id: &str) {
        self.ui.calls.open_tabs.retain(|c| c != call_id);
        if self.ui.calls.active_call.as_deref() == Some(call_id) {
            self.park_active_call_draft();
            if let Some(next_call) = self.ui.calls.open_tabs.last().cloned() {
                self.open_call_tab(&next_call);
            } else if let Some(first_sid) = self.ui.numbered.first().cloned() {
                self.switch_to(&first_sid);
            } else {
                self.go_start();
            }
        }
        self.dirty = true;
    }

    pub(crate) fn sync_open_calls_after_status(&mut self) {
        let valid: Vec<String> = self.ui.sidebar.calls.summaries.iter().map(|c| c.call_id.clone()).collect();
        self.ui.calls.open_tabs.retain(|cid| valid.contains(cid));
        if let Some(active_cid) = self.ui.calls.active_call.clone() {
            if !valid.contains(&active_cid) {
                self.close_call_tab(&active_cid);
                return;
            }
            let needs_fetch = match (
                self.ui.sidebar.calls.summaries.iter().find(|s| s.call_id == active_cid),
                self.ui.calls.details.get(&active_cid),
            ) {
                (Some(sum), Some(det)) => {
                    sum.updated_at != det.updated_at
                        || sum.entry_count != det.entries.len()
                        || sum.status != det.status
                        || sum.round != det.round
                        || sum.title != det.title
                }
                (Some(_), None) => true,
                _ => false,
            };
            if needs_fetch {
                self.fetch_call_detail(&active_cid);
            }
        }
    }

    pub(crate) fn fetch_call_detail(&mut self, call_id: &str) {
        if self.socket.is_empty() || call_id.is_empty() {
            return;
        }
        let cid = call_id.to_string();
        let _ = self.spawn_host_call(json!({"op": "call.get", "call": call_id}), 6, move |_| {
            AsyncJob::CallGet { call_id: cid }
        });
    }

    pub(crate) fn send_call_say(&mut self, call_id: &str, text: String) {
        if self.socket.is_empty() || call_id.is_empty() {
            return;
        }
        let cid = call_id.to_string();
        let sent_text = text.clone();
        let _ = self.spawn_host_call(
            json!({"op": "call.say", "call": call_id, "text": text}),
            8,
            move |_| AsyncJob::CallSay {
                call_id: cid,
                text: sent_text,
            },
        );
        emit_ime("sent", &self.ed.text().to_string());
    }

    pub(crate) fn hangup_active_call(&mut self) {
        let Some(cid) = self.ui.calls.active_call.clone() else { return };
        if self.socket.is_empty() {
            return;
        }
        let _ = self.spawn_host_call(json!({"op": "call.hangup", "call": cid}), 6, |_| {
            AsyncJob::CallAction
        });
        self.ui.flash = Some("Paused auto rounds.".into());
        self.dirty = true;
    }

    pub(crate) fn start_call_between(&mut self, from_sid: &str, to_sid: &str) {
        if self.socket.is_empty() {
            return;
        }
        let _ = self.spawn_host_call(
            json!({"op": "call.start", "from": from_sid, "to": to_sid}),
            6,
            |_| AsyncJob::CallStart,
        );
    }

    pub(crate) fn on_call_async(&mut self, job: AsyncJob, reply: Value) {
        match job {
            AsyncJob::CallGet { call_id } => {
                if reply["ok"] == json!(1) {
                    if let Some(det) = parse_call_detail(&reply) {
                        self.ui.calls.details.insert(call_id.clone(), det);
                        if self.ui.calls.active_call.as_deref() == Some(call_id.as_str()) {
                            self.rebuild_active_call_lines();
                        }
                        self.dirty = true;
                    }
                }
            }
            AsyncJob::CallSay { call_id, text } => {
                if reply["ok"] == json!(1) {
                    self.send("{\"op\":\"status\"}");
                    self.fetch_call_detail(&call_id);
                } else {
                    let err = reply["error"].as_str().unwrap_or("refused");
                    if self.ui.calls.active_call.as_deref() == Some(call_id.as_str()) && self.ed.is_empty() {
                        self.ed.put(&text);
                    }
                    self.ui.flash = Some(format!("Call Room send refused: {err}"));
                    self.dirty = true;
                }
            }
            AsyncJob::CallStart => {
                if reply["ok"] == json!(1) {
                    if let Some(det) = parse_call_detail(&reply) {
                        let cid = det.call_id.clone();
                        self.ui.calls.details.insert(cid.clone(), det);
                        self.send("{\"op\":\"status\"}");
                        self.open_call_tab(&cid);
                        self.ui.flash = Some("Opened Call Room.".into());
                    }
                } else {
                    let err = reply["error"].as_str().unwrap_or("unknown");
                    self.ui.flash = Some(format!("Could not start Call Room: {err}"));
                }
                self.dirty = true;
            }
            AsyncJob::CallAction => {
                self.send("{\"op\":\"status\"}");
                if let Some(cid) = self.ui.calls.active_call.clone() {
                    self.fetch_call_detail(&cid);
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    pub(crate) fn rebuild_active_call_lines(&mut self) {
        let Some(cid) = self.ui.calls.active_call.clone() else { return };
        let lines = match self.ui.calls.details.get(&cid) {
            Some(det) => format_call_lines(det),
            None => {
                let mut v = VecDeque::new();
                v.push_back((1, "[tool] Loading Call Room transcript...".to_string()));
                v
            }
        };
        if let Some(view) = self.view.as_mut() {
            view.lines = lines;
            self.ui.scroll = self.ui.scroll.min(view.lines.len().saturating_sub(1));
        }
        self.ui.composed = None;
        self.dirty = true;
    }

    pub(crate) fn active_call_header(&self) -> Option<String> {
        let cid = self.ui.calls.active_call.as_ref()?;
        if let Some(det) = self.ui.calls.details.get(cid) {
            let state_txt = if det.paused {
                "paused".to_string()
            } else if det.active {
                format!("active round {}/{}", det.round.max(1), det.max_rounds)
            } else {
                "idle".to_string()
            };
            Some(format!(
                "\u{21c4} Call Room: {} ({}) \u{00b7} {} ({}) | {state_txt}",
                det.label_a, det.provider_a, det.label_b, det.provider_b
            ))
        } else if let Some(sum) = self.ui.sidebar.calls.summaries.iter().find(|s| &s.call_id == cid) {
            Some(format!("\u{21c4} Call Room: {} | {}", sum.title, sum.status))
        } else {
            Some(format!("\u{21c4} Call Room {cid}"))
        }
    }

    pub(crate) fn active_call_status_line(&self, panel_shown: bool) -> Option<String> {
        if panel_shown || self.ui.scroll != 0 {
            return None;
        }
        let cid = self.ui.calls.active_call.as_ref()?;
        let (sa, sb) = self
            .ui
            .calls
            .details
            .get(cid)
            .map(|d| (d.session_a.as_str(), d.session_b.as_str()))
            .or_else(|| {
                self.ui
                    .sidebar
                    .calls
                    .summaries
                    .iter()
                    .find(|s| &s.call_id == cid)
                    .map(|s| (s.session_a.as_str(), s.session_b.as_str()))
            })?;
        let now = self.clock_now();
        let mut busy_parts = Vec::new();
        for sid in [sa, sb] {
            if let Some(r) = self.ui.rows.iter().find(|r| r.session == sid) {
                if let Some(w) = r.working_for(now) {
                    busy_parts.push(format!("{} working ({})", r.name(), super::ui::clock_text(w)));
                }
            }
        }
        if busy_parts.is_empty() {
            None
        } else {
            Some(format!("\u{2026} {}", busy_parts.join(" \u{00b7} ")))
        }
    }

    pub(crate) fn call_hotkey(&mut self, event: &KeyEvent) -> bool {
        if !self.active_call_open() || event.repeat {
            return false;
        }
        let ctrl = self.ui.mods.control_key() && !self.ui.mods.alt_key();
        let shift = self.ui.mods.shift_key();
        let letter = super::key_letter(event);
        if ctrl && letter == Some('k') && (shift || self.ed.is_empty()) {
            self.hangup_active_call();
            return true;
        }
        false
    }

    /// Resolve a stationary click on line `raw_idx` in a session transcript: if that line (or its
    /// enclosing `>> [...]` card header) carries a Call Room short id (`cXXXXXX`), open that Call Room tab.
    pub(crate) fn try_jump_call_from_line(&mut self, raw_idx: usize) -> bool {
        if self.active_call_open() || self.ui.panel.is_some() {
            return false;
        }
        let cid7 = {
            let Some(view) = self.view.as_ref() else { return false };
            let Some((_, clicked)) = view.lines.get(raw_idx) else { return false };
            if !(clicked == ">>" || clicked.starts_with(">> ")) {
                return false;
            }
            let mut found = None;
            for i in (0..=raw_idx).rev() {
                let Some((_, line)) = view.lines.get(i) else { break };
                if !(line == ">>" || line.starts_with(">> ")) {
                    break;
                }
                if let Some(c7) = extract_call_id7(line) {
                    found = Some(c7.to_string());
                    break;
                }
                if super::panel::sendto::header_look(line).is_some() {
                    break;
                }
            }
            found
        };
        let Some(prefix) = cid7 else { return false };
        let full_id = self
            .ui
            .sidebar
            .calls
            .summaries
            .iter()
            .find(|c| c.call_id.starts_with(&prefix))
            .map(|c| c.call_id.clone());
        if let Some(cid) = full_id {
            self.ui.sel = None;
            self.open_call_tab(&cid);
            self.ui.flash = Some("Opened Call Room.".into());
            true
        } else {
            false
        }
    }

    pub(crate) fn draw_call_room_controls(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        list_w: usize,
        chat_top: f32,
    ) {
        self.ui.calls.hangup_rect = None;
        self.ui.calls.jump_a_rect = None;
        self.ui.calls.jump_b_rect = None;
        if !self.active_call_open() || list_w >= width {
            return;
        }
        let Some(cid) = self.ui.calls.active_call.clone() else { return };
        let meta = self
            .ui
            .calls
            .details
            .get(&cid)
            .map(|d| {
                (
                    d.session_a.clone(),
                    clip_cols(&d.label_a, 12),
                    d.session_b.clone(),
                    clip_cols(&d.label_b, 12),
                    d.paused,
                )
            })
            .or_else(|| {
                self.ui
                    .sidebar
                    .calls
                    .summaries
                    .iter()
                    .find(|s| s.call_id == cid)
                    .map(|s| {
                        (
                            s.session_a.clone(),
                            clip_cols(&s.label_a, 12),
                            s.session_b.clone(),
                            clip_cols(&s.label_b, 12),
                            s.paused,
                        )
                    })
            });
        let Some((sa, la, sb, lb, paused)) = meta else { return };

        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let y0 = chat_top + s;
        let y1 = chat_top + lh + s;
        let x_right = width as f32 - 6.0 * s;

        let h_lbl = if paused { "Paused" } else { "Hang up" };
        let h_w = controls.width(fs, h_lbl) + 14.0 * s;
        let h_rect = [x_right - h_w, y0, x_right, y1];
        let jb_lbl = format!("-> {lb}");
        let jb_w = controls.width(fs, &jb_lbl) + 12.0 * s;
        let jb_rect = [h_rect[0] - 4.0 * s - jb_w, y0, h_rect[0] - 4.0 * s, y1];
        let ja_lbl = format!("-> {la}");
        let ja_w = controls.width(fs, &ja_lbl) + 12.0 * s;
        let ja_rect = [jb_rect[0] - 4.0 * s - ja_w, y0, jb_rect[0] - 4.0 * s, y1];
        let strip_left = if ja_rect[0] > list_w as f32 + 220.0 * s {
            ja_rect[0] - 6.0 * s
        } else if jb_rect[0] > list_w as f32 + 220.0 * s {
            jb_rect[0] - 6.0 * s
        } else {
            h_rect[0] - 6.0 * s
        };
        super::controls::fill(buf, width, height, [strip_left, chat_top, width as f32, y1 + s], 0xFF0E_1318);
        let (h_fill, h_col) = if paused {
            (0xFF28_261A, Color::rgb(0xFA, 0xD2, 0x6E))
        } else {
            (0xFF5A_2226, Color::rgb(0xFF, 0xE6, 0xE6))
        };
        controls.button(fs, swash, buf, width, height, h_rect, h_lbl, h_fill, h_col, Action::CallHangup);
        self.ui.calls.hangup_rect = Some(h_rect);

        if jb_rect[0] > list_w as f32 + 220.0 * s {
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                jb_rect,
                &jb_lbl,
                0xFF1D_2E42,
                Color::rgb(0x9C, 0xD6, 0xFF),
                Action::CallJumpSession(sb),
            );
            self.ui.calls.jump_b_rect = Some(jb_rect);

            if ja_rect[0] > list_w as f32 + 220.0 * s {
                controls.button(
                    fs,
                    swash,
                    buf,
                    width,
                    height,
                    ja_rect,
                    &ja_lbl,
                    0xFF1D_2E42,
                    Color::rgb(0x9C, 0xD6, 0xFF),
                    Action::CallJumpSession(sa),
                );
                self.ui.calls.jump_a_rect = Some(ja_rect);
            }
        }
    }

    pub(crate) fn calls_json(&self) -> Value {
        let tabs: Vec<Value> = self
            .ui
            .calls
            .tab_items_geom
            .iter()
            .map(|t| {
                json!({
                    "call_id": t.call_id,
                    "label": t.label,
                    "active": t.active,
                    "selected": t.selected,
                    "status": t.status,
                    "round": t.round,
                    "rect": t.rect,
                    "close_rect": t.close_rect,
                })
            })
            .collect();
        let active_detail = self
            .ui
            .calls
            .active_call
            .as_ref()
            .and_then(|cid| self.ui.calls.details.get(cid))
            .map(|d| {
                json!({
                    "call_id": d.call_id,
                    "title": d.title,
                    "status": d.status,
                    "active": d.active,
                    "paused": d.paused,
                    "owner_authorized": d.owner_authorized,
                    "round": d.round,
                    "max_rounds": d.max_rounds,
                    "entry_count": d.entries.len(),
                })
            });
        json!({
            "active_call": self.ui.calls.active_call,
            "open_tabs": self.ui.calls.open_tabs,
            "tabs": tabs,
            "hangup_rect": self.ui.calls.hangup_rect,
            "jump_a_rect": self.ui.calls.jump_a_rect,
            "jump_b_rect": self.ui.calls.jump_b_rect,
            "detail": active_detail,
        })
    }
}
