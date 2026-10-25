//! The first view of the primary client (`--ui`): a list of sessions with their real state and any
//! pending approval, the selected session with provider, account and model beside the input, older
//! history read on demand, and line-wise selection with an explicit copy. The host is the source of
//! every fact shown here; nothing is inferred from a quiet stream.

use std::collections::HashMap;
use std::io::Write;
use std::num::NonZeroU32;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use cosmic_text::{Attrs, AttrsList, Buffer, BufferLine, Color, Family, FontSystem, LineEnding, Metrics, Scroll, Shaping, SwashCache, Wrap};
use serde_json::{json, Value};
use softbuffer::Surface;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::Window;

use super::{clean, fill_rect, json_escape, App, BACKGROUND, FONT_PX, LINE_HEIGHT};

pub const LIST_W: usize = 280;
const LIST_TOP: usize = 40;
const HISTORY_PAGE: i64 = 100;
const SCROLLBACK_CAP: usize = 2000; // lines kept while reading older history; beyond it the newest are let go
const COPY_LIMIT: usize = 60 * 1024; // one X11 property transfer, no INCR

#[derive(Clone, Default)]
pub struct Row {
    pub session: String,
    pub provider: String,
    pub account: String,
    pub model: String,
    pub state: String,
    pub state_at: u64,
    pub activity: String,
    pub generation: i64,
    pub approvals: Vec<(i64, String)>,
    pub issue: String,
    pub mode: String,
}

#[derive(Default)]
pub struct UiState {
    pub enabled: bool,
    pub markers: bool,
    pub rows: Vec<Row>,
    pub labels: HashMap<String, String>,
    pub list_sig: String,
    pub list_buffer: Option<Buffer>,
    pub list_swash: Option<SwashCache>,
    pub surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    pub buttons: Option<(Buffer, Buffer)>,
    pub scroll: usize,
    pub behind: bool, // the live tail is not buffered while far into older history
    pub oldest_known: i64,
    pub history_pending: bool,
    pub history_requests: u64,
    pub history_lines: u64,
    pub sel: Option<(usize, usize)>,
    pub dragging: bool,
    pub mouse: (f64, f64),
    pub mods: ModifiersState,
    pub hit_lines: Vec<(f32, f32, usize)>,
    pub hit_rows: Vec<(f32, f32, String)>,
    pub allow_rect: Option<[f32; 4]>,
    pub deny_rect: Option<[f32; 4]>,
    pub ticks: u64,
    pub copied: u64,
    pub clip_tx: Option<mpsc::Sender<String>>,
    pub header: String,
    pub bar: Option<String>,
    pub window_span: (usize, usize),
    pub proxy: Option<winit::event_loop::EventLoopProxy<super::Ev>>, // for the answer of a paste, which is fetched off the event loop
    pub paste_pending: bool,
    pub hint_buffer: Option<(String, Buffer)>, // the permanent hint line at the bottom, shaped again only when its text changes
    pub numbered: Vec<String>, // the sessions in the order of the list: Alt+1..9 pick by this number
    pub host_notice: Option<String>, // the host runs another build than this window (or was replaced): one line, always shown
    pub flash: Option<String>, // a one-line notice outside the panels, until the next key
    pub panel: Option<super::panel::Panel>, // new session, accounts and sign-in, in place of the transcript
    pub scale: f32, // the display scale factor; every size here is a logical size times this
}

fn display_cols(text: &str) -> usize {
    text.chars().map(|c| if matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0x1F300..=0x1FAFF) { 2 } else { 1 }).sum()
}

fn ago(now: u64, then: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => "<1m".to_string(), // coarse on purpose: a list that redraws every second would never let the client idle
        60..=3599 => format!("{}m", d / 60),
        _ => format!("{}h", d / 3600),
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Row {
    /// The short form for the narrow list: the full wording is in the header of the selected session.
    pub fn list_state_text(&self) -> String {
        match self.state_text().as_str() {
            "waiting for approval" => "approval".into(),
            other => other.to_string(),
        }
    }

    /// What a person reads: the host's state, never a guess (waiting for approval beats working).
    pub fn state_text(&self) -> String {
        if self.state == "running" && !self.approvals.is_empty() {
            return "waiting for approval".into();
        }
        match (self.state.as_str(), self.activity.as_str()) {
            ("running", "working") => "working".into(),
            ("running", "idle") => "idle".into(),
            ("running", _) => "running".into(),
            (other, _) => other.to_string(),
        }
    }
}

impl App {
    // ------------------------------------------------------------------ what the host says
    pub(crate) fn ui_status(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return };
        let mut rows = Vec::new();
        for entry in value["sessions"].as_array().into_iter().flatten() {
            if entry["provider"].as_str().is_none() {
                continue; // synthetic sessions are not shown in this view
            }
            let approvals = entry["approvals"].as_array().into_iter().flatten().filter_map(|a| Some((a["approval_id"].as_i64()?, clean(a["prompt"].as_str()?)))).collect();
            rows.push(Row {
                session: entry["session"].as_str().unwrap_or("").into(),
                provider: entry["provider"].as_str().unwrap_or("").into(),
                account: entry["account"].as_str().unwrap_or("").into(),
                model: clean(entry["model"].as_str().unwrap_or("")),
                state: entry["state"].as_str().unwrap_or("").into(),
                state_at: entry["state_at"].as_u64().unwrap_or(0),
                activity: entry["activity"].as_str().unwrap_or("").into(),
                generation: entry["generation"].as_i64().unwrap_or(1),
                approvals,
                issue: entry["account_issue"].as_str().unwrap_or("").into(),
                mode: entry["mode"].as_str().unwrap_or("").into(),
            });
        }
        // a stable order: sessions keep the host's order, groups appear in the order of their first session
        self.ui.rows = rows;
        if let Some(row) = self.ui.rows.iter().find(|r| r.session == self.session) {
            self.generation = row.generation; // the target's generation, so an input or an answer is never aimed at a stale one
        }
        self.refresh_list();
    }

    pub(crate) fn ui_accounts(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return };
        self.ui.labels = value["accounts"].as_array().into_iter().flatten().filter_map(|a| Some((a["account"].as_str()?.to_string(), clean(a["label"].as_str()?)))).collect();
        self.refresh_list();
    }

    pub(crate) fn ui_history(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return };
        if value["session"].as_str() != Some(self.session.as_str()) {
            return;
        }
        self.ui.history_pending = false;
        self.ui.oldest_known = value["oldest"].as_i64().unwrap_or(0);
        let Some(view) = self.view.as_mut() else { return };
        let entries: Vec<(i64, String)> = value["lines"].as_array().into_iter().flatten().filter_map(|e| Some((e[0].as_i64()?, clean(e[1].as_str()?)))).collect();
        self.ui.history_lines += entries.len() as u64;
        for (seq, text) in entries.into_iter().rev() {
            if view.lines.front().map_or(true, |f| seq < f.0) {
                view.lines.push_front((seq, text));
            }
        }
        if view.lines.len() > SCROLLBACK_CAP {
            while view.lines.len() > SCROLLBACK_CAP {
                view.lines.pop_back(); // the newest are let go; the live tail comes back when the view returns to the bottom
            }
            self.ui.behind = true;
        }
        self.dirty = true;
    }

    pub(crate) fn ui_tick(&mut self) {
        self.ui.ticks += 1;
        self.panel_tick();
        self.send("{\"op\":\"status\"}");
        if self.ui.ticks % 5 == 1 {
            self.send("{\"op\":\"account.list\"}");
        }
    }

    fn selected_row(&self) -> Option<&Row> {
        self.ui.rows.iter().find(|r| r.session == self.session)
    }

    /// The target beside the input: provider, account, model and state of the selected session, as the host reports them.
    fn header_text(&self) -> String {
        let base = self.header_base();
        match &self.ui.host_notice {
            Some(n) => format!("{base} | {n}"),
            None => base,
        }
    }

    fn header_base(&self) -> String {
        match self.selected_row() {
            Some(r) => {
                let model = if r.model.is_empty() { "model not reported".to_string() } else { r.model.clone() };
                let issue = if r.issue.is_empty() { String::new() } else { format!(" | account issue: {}", r.issue) };
                let mode = match r.mode.as_str() {
                    "auto" => " | mode: auto (permissions skipped)",
                    "default" => " | mode: asks for approval",
                    _ => "",
                };
                format!("{} | {} | {} | {}{mode}{issue}", r.provider, self.label_of(&r.account), model, r.state_text())
            }
            None if self.session.is_empty() => "select a session".into(),
            None => format!("{} | not listed", self.session),
        }
    }

    fn label_of(&self, account: &str) -> String {
        if account.is_empty() {
            "no account".into()
        } else {
            self.ui.labels.get(account).cloned().unwrap_or_else(|| account.to_string())
        }
    }

    fn refresh_list(&mut self) {
        let s = self.ui.scale.max(1.0);
        let now = now_secs();
        let mut text = String::new();
        let mut groups: Vec<(String, String)> = Vec::new(); // (provider, account) in order of first appearance
        for row in &self.ui.rows {
            let key = (row.provider.clone(), row.account.clone());
            if !groups.contains(&key) {
                groups.push(key);
            }
        }
        let mut rows_text = Vec::new();
        let mut number = 0usize;
        self.ui.numbered.clear();
        for (provider, account) in &groups {
            rows_text.push(format!("{provider} / {}", self.label_of(account)));
            for row in self.ui.rows.iter().filter(|r| &r.provider == provider && &r.account == account) {
                let marker = if row.session == self.session { ">" } else if !row.approvals.is_empty() { "!" } else { " " };
                let approvals = if row.approvals.is_empty() { String::new() } else { format!(" x{}", row.approvals.len()) };
                let short: String = row.session.chars().take(7).collect();
                number += 1;
                self.ui.numbered.push(row.session.clone());
                let digit = if number <= 9 { number.to_string() } else { " ".to_string() };
                rows_text.push(format!("{marker}{digit} {short} {}{approvals} {}", row.list_state_text(), ago(now, row.state_at)));
            }
        }
        text.push_str(&rows_text.join("\n"));
        let sig = text.clone();
        if sig == self.ui.list_sig && self.ui.list_buffer.is_some() {
            return;
        }
        self.ui.list_sig = sig;
        if let Some(fs) = self.font_system.as_mut() {
            let buffer = self.ui.list_buffer.get_or_insert_with(|| Buffer::new(fs, Metrics::new(FONT_PX * s, LINE_HEIGHT as f32 * s)));
            buffer.set_wrap(Wrap::None);
            buffer.set_size(Some(LIST_W as f32 * s), Some(4000.0));
            buffer.set_text(&text, &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
            buffer.shape_until_scroll(fs, true);
        }
        // the rows' click boxes: one logical line each, in the order they were written
        self.ui.hit_rows.clear();
        let mut y = LIST_TOP as f32 * s;
        for (provider, account) in &groups {
            let _ = (provider, account);
            y += LINE_HEIGHT as f32 * s; // the group heading is not a target
            for row in self.ui.rows.iter().filter(|r| &r.provider == provider && &r.account == account) {
                self.ui.hit_rows.push((y, y + LINE_HEIGHT as f32 * s, row.session.clone()));
                y += LINE_HEIGHT as f32 * s;
            }
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------------ lines of the selected session
    pub(crate) fn push_line(&mut self, seq: i64, text: String) {
        let Some(view) = self.view.as_mut() else { return };
        if self.ui.behind {
            self.cursor = seq; // the tail is not buffered while far into older history; the cursor still follows
            return;
        }
        view.lines.push_back((seq, text));
        if self.ui.scroll > 0 {
            self.ui.scroll += 1; // reading older lines: what is on screen does not move
        }
        while view.lines.len() > SCROLLBACK_CAP {
            view.lines.pop_front();
        }
        view.seq_rgb = 0xFF00_0000 | (seq as u32 & 0xFF_FFFF);
        self.cursor = seq;
        self.dirty = true;
    }

    fn scroll_by(&mut self, lines: isize) {
        let Some(view) = self.view.as_ref() else { return };
        let len = view.lines.len();
        let next = (self.ui.scroll as isize + lines).clamp(0, len.saturating_sub(1) as isize) as usize;
        self.ui.scroll = next;
        let first_seq = view.lines.front().map_or(0, |l| l.0);
        let near_top = next + 2 * 30 >= len;
        if near_top && !self.ui.history_pending && first_seq > self.ui.oldest_known.max(0) {
            self.ui.history_pending = true;
            self.ui.history_requests += 1;
            let msg = format!("{{\"op\":\"history\",\"session\":\"{}\",\"before\":{first_seq},\"limit\":{HISTORY_PAGE}}}", self.session);
            self.send(&msg);
        }
        if next == 0 && self.ui.behind {
            // back at the bottom after reading far: the live tail is fetched again, not guessed
            self.ui.behind = false;
            if let Some(v) = self.view.as_mut() {
                v.lines.clear();
            }
            let msg = format!("{{\"op\":\"subscribe\",\"session\":\"{}\",\"after\":{},\"snapshot\":true}}", self.session, self.cursor);
            self.send(&msg);
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------------ approvals, copy
    fn answer_approval(&mut self, allow: bool) {
        let Some(row) = self.selected_row() else { return };
        let Some((id, _)) = row.approvals.first().cloned() else { return };
        let msg = format!(
            "{{\"op\":\"approval.answer\",\"session\":\"{}\",\"generation\":{},\"approval_id\":{},\"answer\":\"{}\"}}",
            self.session, row.generation, id, if allow { "allow" } else { "deny" }
        );
        self.send(&msg);
        if let Some(row) = self.ui.rows.iter_mut().find(|r| r.session == self.session) {
            row.approvals.remove(0); // answered once: it is not offered again; the next status confirms
        }
        self.refresh_list();
    }

    fn selection_text(&self) -> Option<String> {
        let (a, b) = self.ui.sel?;
        let view = self.view.as_ref()?;
        let (lo, hi) = (a.min(b), a.max(b));
        let lines: Vec<&str> = (lo..=hi).filter_map(|i| view.lines.get(i).map(|l| l.1.as_str())).collect();
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    /// Copy is a user action: only a non-empty selection, only from the key chord.
    fn copy_selection(&mut self) {
        let Some(text) = self.selection_text() else { return };
        self.copy_text(text);
    }

    /// Put text on the clipboard: always the result of a person's own key.
    pub(crate) fn copy_text(&mut self, text: String) {
        if text.len() > COPY_LIMIT {
            return; // larger than one transfer; refused rather than cut
        }
        let tx = self.ui.clip_tx.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<String>();
            thread::spawn(move || clipboard_server(rx));
            tx
        });
        if tx.send(text).is_ok() {
            self.ui.copied += 1;
        }
    }

    // ------------------------------------------------------------------ events
    pub(crate) fn ui_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui.scale = *scale_factor as f32;
                self.ui.list_buffer = None; // laid out again at the new size
                self.ui.list_sig.clear();
                self.ui.buttons = None;
                let s = self.ui.scale.max(1.0);
                if let Some(view) = self.view.as_mut() {
                    view.buffer.set_metrics(Metrics::new(FONT_PX * s, LINE_HEIGHT as f32 * s));
                }
                self.dirty = true;
                false // the window's own handling still runs
            }
            WindowEvent::ModifiersChanged(m) => {
                self.ui.mods = m.state();
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.mouse = (position.x, position.y);
                if self.ui.dragging {
                    if let Some(idx) = self.line_at(position.y as f32) {
                        if let Some((a, _)) = self.ui.sel {
                            self.ui.sel = Some((a, idx));
                            self.dirty = true;
                        }
                    }
                }
                true
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let (x, y) = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
                if *state == ElementState::Pressed {
                    if x < LIST_W as f32 * self.ui.scale.max(1.0) {
                        if self.ui.panel.is_some() {
                            return true; // a panel owns the input line: leaving for another session would carry its text along
                        }
                        if let Some((_, _, id)) = self.ui.hit_rows.iter().find(|r| y >= r.0 && y < r.1).cloned() {
                            if id != self.session {
                                self.switch_to(&id);
                            }
                        }
                    } else if self.inside(self.ui.allow_rect, x, y) {
                        self.answer_approval(true);
                    } else if self.inside(self.ui.deny_rect, x, y) {
                        self.answer_approval(false);
                    } else if let Some(idx) = self.line_at(y) {
                        self.ui.sel = Some((idx, idx));
                        self.ui.dragging = true;
                    } else {
                        self.ui.sel = None;
                    }
                    self.dirty = true;
                } else {
                    self.ui.dragging = false;
                }
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (*y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(p) => (p.y / (LINE_HEIGHT as f32 * self.ui.scale.max(1.0)) as f64) as isize,
                };
                self.scroll_by(lines);
                true
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.ui.flash = None;
                if self.panel_key(event) || self.paste_key(event) || self.approval_key(event) || self.nav_key(event) {
                    return true;
                }
                let chord = self.ui.mods.control_key() && self.ui.mods.shift_key();
                match &event.logical_key {
                    Key::Character(c) if chord && c.eq_ignore_ascii_case("c") => {
                        self.copy_selection();
                        true
                    }
                    Key::Named(NamedKey::Escape) => {
                        if self.ui.sel.is_some() {
                            self.ui.sel = None; // Esc with a selection only drops the selection
                        } else if self.input.is_empty() && self.preedit.is_empty() && !self.session.is_empty() {
                            self.go_start(); // an empty input line: back to the start screen (the session keeps running)
                        }
                        self.dirty = true;
                        true
                    }
                    Key::Named(NamedKey::PageUp) => {
                        self.scroll_by(20);
                        true
                    }
                    Key::Named(NamedKey::PageDown) => {
                        self.scroll_by(-20);
                        true
                    }
                    Key::Named(NamedKey::End) => {
                        let back = self.ui.scroll as isize;
                        self.scroll_by(-back);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Where a paste may go: the input line of a session or the text entry of a panel, never during a composition.
    fn paste_allowed(&self) -> bool {
        self.preedit.is_empty() && if self.ui.panel.is_some() { self.text_mode() } else { !self.session.is_empty() }
    }

    fn paste_notice(&mut self, text: &str) {
        match self.ui.panel.as_mut() {
            Some(p) => p.notice = text.to_string(),
            None => self.ui.flash = Some(text.to_string()),
        }
    }

    /// Ctrl+V and Ctrl+Shift+V paste the clipboard, Shift+Insert the primary selection; only on the person's own key.
    pub(crate) fn paste_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        let (ctrl, shift) = (self.ui.mods.control_key(), self.ui.mods.shift_key());
        let selection: &'static [u8] = match &event.logical_key {
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("v") => b"CLIPBOARD",
            Key::Named(NamedKey::Insert) if shift && !ctrl => b"PRIMARY",
            _ => return false,
        };
        if event.repeat || !self.paste_allowed() || self.ui.paste_pending {
            return true; // the key is ignored where nothing can take text
        }
        let Some(proxy) = self.ui.proxy.clone() else { return true };
        self.ui.paste_pending = true;
        thread::spawn(move || {
            let _ = proxy.send_event(super::Ev::Paste(fetch_selection(selection)));
        });
        true
    }

    /// What the selection's owner sent: it goes onto the input line (never echoed, never submitted).
    pub(crate) fn on_paste(&mut self, result: Result<String, &'static str>) {
        self.ui.paste_pending = false;
        if !self.paste_allowed() {
            return; // the person moved on while the owner was answering
        }
        match result {
            Ok(text) => {
                let line: String = text.split(|c| c == '\n' || c == '\r').next().unwrap_or("").chars().filter(|c| !c.is_control()).collect();
                if line.is_empty() {
                    self.paste_notice("Nothing to paste.");
                } else {
                    self.input.push_str(&line); // a newline ends the paste: Enter stays the person's own key
                }
            }
            Err("too_large") => self.paste_notice("Paste refused: more than 8 KiB."),
            Err("timeout") => self.paste_notice("Paste: the owner of the selection did not answer."),
            Err(_) => self.paste_notice("Nothing to paste."),
        }
        self.dirty = true;
    }

    /// Go to a session: its view is attached, the one before is detached (its draft stays with it).
    pub(crate) fn switch_to(&mut self, id: &str) {
        self.ui.scroll = 0;
        self.ui.sel = None;
        self.ui.behind = false;
        self.ui.history_pending = false;
        self.show(id, None);
        self.dirty = true;
    }

    /// Back to the start screen: no session is shown, nothing is ended.
    pub(crate) fn go_start(&mut self) {
        self.ui.scroll = 0;
        self.ui.sel = None;
        self.ui.behind = false;
        self.ui.history_pending = false;
        self.hide();
        self.dirty = true;
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: next and previous session of the list (wrapping); Alt+1..9: the session with that number.
    pub(crate) fn nav_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if self.ui.panel.is_some() || self.ui.numbered.is_empty() {
            return false;
        }
        let ids = self.ui.numbered.clone();
        let here = ids.iter().position(|id| *id == self.session);
        let target = match &event.logical_key {
            Key::Named(NamedKey::Tab) if self.ui.mods.control_key() => {
                let n = ids.len();
                Some(match (here, self.ui.mods.shift_key()) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                })
            }
            Key::Character(c) if self.ui.mods.alt_key() && !self.ui.mods.control_key() => c.chars().next().and_then(|d| d.to_digit(10)).filter(|d| *d >= 1).map(|d| d as usize - 1).filter(|i| *i < ids.len()),
            _ => None,
        };
        match target {
            Some(i) if !event.repeat => {
                if ids[i] != self.session {
                    self.switch_to(&ids[i]);
                }
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// What the start screen says while no session is shown.
    pub(crate) fn start_lines(&self) -> Vec<String> {
        let mut lines = vec!["No session is open.".to_string(), "Ctrl+N  new session".to_string(), "Ctrl+A  accounts and sign-in".to_string()];
        if !self.ui.numbered.is_empty() {
            lines.push("Alt+1 to Alt+9, Ctrl+Tab or a click on the list opens a session; they keep running when you leave them.".to_string());
        }
        lines
    }

    /// The permanent line at the bottom: the keys that exist, fewer of them when the window is narrow (the right-most go first).
    pub(crate) fn hint_text(&self, width: usize, scale: f32) -> String {
        let escape = if self.ui.panel.is_some() { Some("Esc back out") } else if !self.session.is_empty() { Some("Esc session list") } else { None };
        let mut items = vec!["Ctrl+N new session", "Ctrl+A accounts", "Ctrl+Tab next session", "Alt+1-9 session by number"];
        if let Some(e) = escape {
            items.insert(2, e);
        }
        if self.selected_row().map_or(false, |r| !r.approvals.is_empty()) {
            items.insert(0, "Y allow  N deny");
        }
        let cols = (width as f32 / (8.5 * scale)) as usize;
        let mut text = String::new();
        for item in items {
            let next = if text.is_empty() { item.to_string() } else { format!("{text}   {item}") };
            if next.chars().count() > cols {
                break;
            }
            text = next;
        }
        text
    }

    /// Y allows and N denies the first pending approval of the selected session, while the input line is empty (so typing a word that starts with
    /// one of them is never taken for an answer) and no panel is open.
    pub(crate) fn approval_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if self.ui.panel.is_some() || !self.input.is_empty() || !self.preedit.is_empty() || event.repeat {
            return false;
        }
        if self.ui.mods.control_key() || self.ui.mods.alt_key() || self.selected_row().map_or(true, |r| r.approvals.is_empty()) {
            return false;
        }
        match &event.logical_key {
            Key::Character(c) if c.eq_ignore_ascii_case("y") => self.answer_approval(true),
            Key::Character(c) if c.eq_ignore_ascii_case("n") => self.answer_approval(false),
            _ => return false,
        }
        self.dirty = true;
        true
    }

    /// Ctrl+U clears the whole input line (and only that); true when it did.
    pub(crate) fn ui_ctrl_u(&mut self, key: &Key) -> bool {
        match key {
            Key::Character(c) if self.ui.mods.control_key() && c.eq_ignore_ascii_case("u") && self.preedit.is_empty() => {
                self.input.clear();
                true
            }
            _ => false,
        }
    }

    fn inside(&self, rect: Option<[f32; 4]>, x: f32, y: f32) -> bool {
        rect.map_or(false, |r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
    }

    fn line_at(&self, y: f32) -> Option<usize> {
        self.ui.hit_lines.iter().find(|l| y >= l.0 && y < l.1).map(|l| l.2)
    }

    // ------------------------------------------------------------------ the model, for tests and inspection
    pub(crate) fn ui_dump(&self) {
        let rows: Vec<Value> = self
            .ui
            .rows
            .iter()
            .map(|r| json!({
                "session": r.session, "provider": r.provider, "account": r.account, "label": self.label_of(&r.account), "model": r.model,
                "state": r.state, "state_text": r.state_text(), "activity": r.activity, "approvals": r.approvals.iter().map(|a| json!([a.0, a.1])).collect::<Vec<_>>(),
                "selected": r.session == self.session,
            }))
            .collect();
        let view = self.view.as_ref();
        let dump = json!({
            "ev": "ui", "rows": rows, "selected": self.session, "header": self.header_text(),
            "approval_bar": self.selected_row().and_then(|r| r.approvals.first().map(|(id, prompt)| format!("approval {id}: {prompt}"))),
            "scroll": self.ui.scroll, "behind": self.ui.behind, "lines_buffered": view.map_or(0, |v| v.lines.len()),
            "first_seq": view.and_then(|v| v.lines.front().map(|l| l.0)), "history_requests": self.ui.history_requests, "history_lines": self.ui.history_lines,
            "visible": [self.ui.window_span.0, self.ui.window_span.1], "selection": self.selection_text(), "copied": self.ui.copied,
            "panel": self.panel_json(),
            "hint": self.hint_text(1200 * 2, 1.0), "list_text": self.ui.list_sig, "input": self.shown_input(),
            "start_screen": if self.ui.panel.is_none() && self.session.is_empty() { self.start_lines().join("\n") } else { String::new() },
            "layout": {
                "list_w": (LIST_W as f32 * self.ui.scale.max(1.0)) as usize, "rows": self.ui.hit_rows.iter().map(|r| json!([r.0, r.1, r.2])).collect::<Vec<_>>(),
                "allow": self.ui.allow_rect, "deny": self.ui.deny_rect,
                "lines": self.ui.hit_lines.iter().map(|l| json!([l.0, l.1, l.2])).collect::<Vec<_>>(),
            },
        });
        println!("{dump}");
        let _ = std::io::stdout().flush();
        let _ = json_escape; // (kept for the line-oriented commands)
    }

    // ------------------------------------------------------------------ drawing
    pub(crate) fn draw_ui(&mut self) {
        let Some(window) = self.window.clone() else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        let (width, height) = (w.get() as usize, h.get() as usize);
        let s = self.ui.scale.max(1.0);
        let list_w = (LIST_W as f32 * s) as usize;
        let lh = LINE_HEIGHT as f32 * s;
        let main_w = width.saturating_sub(list_w);
        self.refresh_list();
        // what the main area says: the target beside the input, an approval for it, the transcript window, the draft
        let row = self.selected_row().cloned();
        let hint = self.hint_text(width, s);
        let panel_view = self.panel_view();
        let mut header = panel_view.as_ref().map_or_else(|| self.header_text(), |v| v.0.clone());
        if let (Some(_), Some(n)) = (&panel_view, &self.ui.host_notice) {
            header = format!("{header} | {n}");
        }
        if let (None, Some(flash)) = (&panel_view, &self.ui.flash) {
            header = format!("{header} | {flash}");
        }
        let bar = if panel_view.is_some() { None } else { row.as_ref().and_then(|r| r.approvals.first().map(|(id, prompt)| format!("approval {id}: {prompt}"))) };
        self.ui.header = header.clone();
        self.ui.bar = bar.clone();
        let prompt = if self.code_entry() { format!("> {}", self.shown_input()) } else { format!("> {}[{}]", self.input, self.preedit) }; // a code is never drawn
        let text_w = main_w.saturating_sub((4 + 4) * s as usize); // the text starts 4 logical pixels right of the list and keeps 4 from the window edge
        let cols = (text_w as f32 / (8.5 * s)).max(10.0) as usize;
        let rows_avail = ((height as f32 / lh) as usize).saturating_sub(if bar.is_some() { 5 } else { 4 });
        let (Some(fs), Some(surface)) = (self.font_system.as_mut(), self.ui.surface.as_mut()) else { return };
        if surface.resize(w, h).is_err() {
            return;
        }
        let lines: Vec<(i64, String)> = match &panel_view {
            Some((_, body)) => body.iter().enumerate().map(|(i, l)| (i as i64, l.clone())).collect(),
            None => self.view.as_ref().map(|v| v.lines.iter().cloned().collect()).unwrap_or_default(),
        };
        let end = lines.len().saturating_sub(if panel_view.is_some() { 0 } else { self.ui.scroll.min(lines.len()) });
        let mut first = end;
        let mut used = 0;
        while first > 0 {
            let rows = (display_cols(&lines[first - 1].1).max(1) + cols - 1) / cols;
            if used + rows > rows_avail {
                break;
            }
            used += rows;
            first -= 1;
        }
        let mut text = header;
        if let Some(b) = &bar {
            text.push('\n');
            text.push_str(&clean(b));
        }
        let base = 1 + bar.is_some() as usize;
        // Shaping a mixed-script line is what a frame costs (about 37 of its 39 ms), so a line whose text is unchanged keeps its
        // shaped layout: the buffer's lines are taken apart and put back by text, and only new text is shaped.
        let build = |first: usize, text: &mut String, buffer: &mut Buffer, fs: &mut FontSystem| {
            let mut pool: HashMap<String, Vec<BufferLine>> = HashMap::new();
            for line in std::mem::take(&mut buffer.lines) {
                pool.entry(line.text().to_string()).or_default().push(line);
            }
            let attrs = Attrs::new().family(Family::Monospace);
            let mut wanted: Vec<&str> = text.split('\n').collect();
            wanted.extend(lines[first..end].iter().map(|l| l.1.as_str()));
            wanted.push(&prompt);
            for line_text in wanted {
                let line = pool.get_mut(line_text).and_then(Vec::pop).unwrap_or_else(|| BufferLine::new(line_text, LineEnding::None, AttrsList::new(&attrs), Shaping::Advanced));
                buffer.lines.push(line);
            }
            buffer.set_scroll(Scroll::default());
            buffer.set_size(Some(text_w as f32), Some(height as f32));
            buffer.shape_until_scroll(fs, true);
            // lines put in by hand are not all laid out by the call above (new lines in front of a reused one stayed unshaped and
            // the whole text vanished): every line of the screenful is laid out here
            for i in 0..buffer.lines.len() {
                buffer.line_layout(fs, i);
            }
        };
        let view = self.view.get_or_insert_with(|| {
            // no session selected yet: a text buffer still carries the header and the draft
            let buffer = Buffer::new(fs, Metrics::new(FONT_PX * s, LINE_HEIGHT as f32 * s));
            super::View { lines: Default::default(), surface: None, buffer, swash: SwashCache::new(), session_rgb: 0xFF00_0000, seq_rgb: 0xFF00_0000 }
        });
        build(first, &mut text, &mut view.buffer, fs);
        // the estimate of wrapped rows can be short: if the layout is taller than the area, drop the oldest line and lay out again
        for _ in 0..6 {
            let runs = view.buffer.layout_runs().count();
            if runs <= rows_avail + base + 1 || first >= end {
                break;
            }
            first += 1;
            build(first, &mut text, &mut view.buffer, fs);
        }
        self.ui.window_span = (first, end);
        // click boxes of the lines on screen, from the layout that was just made
        self.ui.hit_lines.clear();
        let mut approval_y = None;
        for run in view.buffer.layout_runs() {
            if bar.is_some() && run.line_i == 1 {
                approval_y = Some(run.line_top);
            }
            if run.line_i >= base && first + (run.line_i - base) < end {
                let idx = first + (run.line_i - base);
                match self.ui.hit_lines.last_mut() {
                    Some(last) if last.2 == idx => last.1 = run.line_top + run.line_height,
                    _ => self.ui.hit_lines.push((run.line_top, run.line_top + run.line_height, idx)),
                }
            }
        }
        self.ui.allow_rect = approval_y.map(|y| [width as f32 - 190.0 * s, y, width as f32 - 100.0 * s, y + lh]);
        self.ui.deny_rect = approval_y.map(|y| [width as f32 - 95.0 * s, y, width as f32 - 5.0 * s, y + lh]);
        let Ok(mut buf) = surface.buffer_mut() else { return };
        buf.fill(BACKGROUND);
        if self.ui.markers {
            // two 16-pixel squares (session and sequence colour) that the pixel-latency harness reads; the product window has none
            fill_rect(&mut buf, width, 0, 0, 16, view.session_rgb);
            fill_rect(&mut buf, width, 16, 0, 16, view.seq_rgb);
        }
        // the list panel: its own column, its own background, the selected row marked
        for y in 0..height {
            let idx = y * width + list_w - 1;
            if let Some(px) = buf.get_mut(idx) {
                *px = 0xFF30_3840;
            }
        }
        for (y0, y1, id) in &self.ui.hit_rows {
            if *id == self.session {
                rect(&mut buf, width, height, 0, *y0 as i32, list_w as i32 - 1, (*y1 - *y0) as i32, 0xFF22_3A58);
            }
        }
        if let Some((a, b)) = self.ui.sel {
            let (lo, hi) = (a.min(b), a.max(b));
            for (y0, y1, idx) in &self.ui.hit_lines {
                if *idx >= lo && *idx <= hi {
                    rect(&mut buf, width, height, list_w as i32, *y0 as i32, main_w as i32, (*y1 - *y0) as i32, 0xFF2A_4A7A);
                }
            }
        }
        let ink = Color::rgb(0xE0, 0xE0, 0xE0);
        let list_swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        if let Some(list) = self.ui.list_buffer.as_mut() {
            blit(list, fs, list_swash, &mut buf, width, height, (4.0 * s) as i32, (LIST_TOP as f32 * s) as i32, list_w as i32 - 2, ink); // clipped to its column
        }
        if let Some(y) = approval_y {
            let (allow, deny) = self.ui.buttons.get_or_insert_with(|| {
                let make = |fs: &mut FontSystem, label: &str| {
                    let mut b = Buffer::new(fs, Metrics::new(FONT_PX * s, LINE_HEIGHT as f32 * s));
                    b.set_size(Some(90.0 * s), Some(lh));
                    b.set_text(label, &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
                    b.shape_until_scroll(fs, true);
                    b
                };
                (make(fs, " Allow"), make(fs, " Deny"))
            });
            let bw = (90.0 * s) as i32;
            rect(&mut buf, width, height, (width as f32 - 190.0 * s) as i32, y as i32, bw, lh as i32, 0xFF2E_6B3A);
            rect(&mut buf, width, height, (width as f32 - 95.0 * s) as i32, y as i32, bw, lh as i32, 0xFF7A_2E2E);
            let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
            blit(allow, fs, swash, &mut buf, width, height, (width as f32 - 190.0 * s) as i32, y as i32, i32::MAX, ink);
            blit(deny, fs, swash, &mut buf, width, height, (width as f32 - 95.0 * s) as i32, y as i32, i32::MAX, ink);
        }
        blit(&mut view.buffer, fs, &mut view.swash, &mut buf, width, height, list_w as i32 + (4.0 * s) as i32, 0, i32::MAX, ink);
        // the permanent hint line, over whatever reaches the bottom row
        let hint_y = (height as f32 - lh) as i32;
        rect(&mut buf, width, height, 0, hint_y, width as i32, lh as i32 + 1, 0xFF18_1E26);
        let entry = self.ui.hint_buffer.get_or_insert_with(|| (String::new(), Buffer::new(fs, Metrics::new(FONT_PX * s, LINE_HEIGHT as f32 * s))));
        if entry.0 != hint {
            entry.1.set_size(Some(width as f32), Some(lh));
            entry.1.set_text(&hint, &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
            entry.1.shape_until_scroll(fs, true);
            entry.0 = hint.clone();
        }
        let hint_swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        blit(&mut entry.1, fs, hint_swash, &mut buf, width, height, (4.0 * s) as i32, hint_y, i32::MAX, Color::rgb(0x9A, 0xA4, 0xB0));
        window.pre_present_notify();
        let _ = buf.present();
    }
}

fn rect(buf: &mut [u32], width: usize, height: usize, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for row in y.max(0)..(y + h).min(height as i32) {
        for col in x.max(0)..(x + w).min(width as i32) {
            if let Some(px) = buf.get_mut(row as usize * width + col as usize) {
                *px = color;
            }
        }
    }
}

fn blit(buffer: &mut Buffer, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, ox: i32, oy: i32, clip_x: i32, ink: Color) {
    buffer.draw(fs, swash, ink, |x, y, gw, gh, color| {
        let alpha = color.a() as u32;
        for row in 0..gh as i32 {
            for col in 0..gw as i32 {
                let (px, py) = (x + col + ox, y + row + oy);
                if px >= 0 && py >= 0 && px < clip_x && (px as usize) < width && (py as usize) < height {
                    let idx = py as usize * width + px as usize;
                    let dst = buf[idx];
                    let mix = |s: u8, shift: u32| {
                        let d = (dst >> shift) & 0xFF;
                        (d + ((s as u32).wrapping_sub(d) as i32 * alpha as i32 / 255) as u32) & 0xFF
                    };
                    buf[idx] = 0xFF00_0000 | mix(color.r(), 16) << 16 | mix(color.g(), 8) << 8 | mix(color.b(), 0);
                }
            }
        }
    });
}

/// Owns the CLIPBOARD selection with the text it was last given, until another client takes it.
/// The text of a selection (`CLIPBOARD` or `PRIMARY`) as UTF-8, asked for through our own helper window on a connection of its own, within 500 ms.
/// An incremental transfer or more than 8 KiB is refused rather than cut.
fn fetch_selection(name: &[u8]) -> Result<String, &'static str> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, WindowClass};
    use x11rb::protocol::Event;
    let (conn, screen) = x11rb::connect(None).map_err(|_| "empty")?;
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().map_err(|_| "empty")?;
    conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).map_err(|_| "empty")?;
    let atom = |n: &[u8]| conn.intern_atom(false, n).ok().and_then(|c| c.reply().ok()).map(|r| r.atom).ok_or("empty");
    let (selection, utf8, property, incr) = (atom(name)?, atom(b"UTF8_STRING")?, atom(b"LICO_PASTE")?, atom(b"INCR")?);
    let owner = conn.get_selection_owner(selection).map_err(|_| "empty")?.reply().map_err(|_| "empty")?.owner;
    if owner == x11rb::NONE {
        return Err("empty");
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    for target in [utf8, u32::from(AtomEnum::STRING)] {
        conn.convert_selection(win, selection, target, property, x11rb::CURRENT_TIME).map_err(|_| "empty")?;
        conn.flush().map_err(|_| "empty")?;
        loop {
            match conn.poll_for_event() {
                Ok(Some(Event::SelectionNotify(e))) if e.requestor == win => {
                    if e.property == x11rb::NONE {
                        break; // this form was refused: the next one
                    }
                    let reply = conn.get_property(true, win, property, AtomEnum::ANY, 0, 2049).map_err(|_| "empty")?.reply().map_err(|_| "empty")?;
                    if reply.type_ == incr || reply.bytes_after > 0 || reply.value.len() > 8192 {
                        return Err("too_large");
                    }
                    return Ok(String::from_utf8_lossy(&reply.value).into_owned());
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    if std::time::Instant::now() > deadline {
                        return Err("timeout");
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return Err("empty"),
            }
        }
    }
    Err("empty")
}

fn clipboard_server(rx: mpsc::Receiver<String>) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent, WindowClass, SELECTION_NOTIFY_EVENT};
    use x11rb::protocol::Event;
    use x11rb::wrapper::ConnectionExt as _;
    let Ok((conn, screen)) = x11rb::connect(None) else { return };
    let root = conn.setup().roots[screen].root;
    let Ok(win) = conn.generate_id() else { return };
    if conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).is_err() {
        return;
    }
    let atom = |name: &[u8]| conn.intern_atom(false, name).ok().and_then(|c| c.reply().ok()).map(|r| r.atom);
    let (Some(clipboard), Some(utf8), Some(targets), Some(text_atom)) = (atom(b"CLIPBOARD"), atom(b"UTF8_STRING"), atom(b"TARGETS"), atom(b"TEXT")) else { return };
    let mut data: Option<Vec<u8>> = None;
    loop {
        if data.is_none() {
            match rx.recv() {
                Ok(text) => data = Some(text.into_bytes()),
                Err(_) => return,
            }
            let _ = conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }
        while let Ok(text) = rx.try_recv() {
            data = Some(text.into_bytes());
            let _ = conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }
        match conn.poll_for_event() {
            Ok(Some(Event::SelectionRequest(r))) => {
                let mut property = r.property;
                let body = data.clone().unwrap_or_default();
                if r.target == targets {
                    let list = [targets, utf8, AtomEnum::STRING.into(), text_atom];
                    let _ = conn.change_property32(PropMode::REPLACE, r.requestor, r.property, AtomEnum::ATOM, &list);
                } else if r.target == utf8 || r.target == text_atom || r.target == u32::from(AtomEnum::STRING) {
                    let _ = conn.change_property8(PropMode::REPLACE, r.requestor, r.property, utf8, &body);
                } else {
                    property = x11rb::NONE;
                }
                let notify = SelectionNotifyEvent { response_type: SELECTION_NOTIFY_EVENT, sequence: 0, time: r.time, requestor: r.requestor, selection: r.selection, target: r.target, property };
                let _ = conn.send_event(false, r.requestor, EventMask::NO_EVENT, notify);
                let _ = conn.flush();
            }
            Ok(Some(Event::SelectionClear(_))) => data = None, // another client owns the clipboard now
            Ok(Some(_)) => {}
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => return,
        }
    }
}
