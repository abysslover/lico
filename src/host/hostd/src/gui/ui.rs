//! The first view of the primary client (`--ui`): a list of sessions with their real state and any
//! pending approval, the selected session with provider, account and model beside the input, older
//! history read on demand, and line-wise selection with an explicit copy. The host is the source of
//! every fact shown here; nothing is inferred from a quiet stream.

use std::collections::HashMap;
use std::io::Write;
use std::num::NonZeroU32;
use std::sync::{mpsc, Arc};
use std::time::Instant;

use cosmic_text::{Buffer, BufferLine, Color, FontSystem, LineEnding, Metrics, Scroll, Shaping, SwashCache, SwashContent};
use serde_json::{json, Value};
use softbuffer::Surface;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::Window;

use super::controls::{self, Action, Controls};
#[path = "attn.rs"]
mod attn;
#[path = "row.rs"]
pub(crate) mod row;
#[path = "reconnect.rs"]
pub(super) mod reconnect;
pub(crate) use row::{clock_text, memrow, Row};
#[path = "clip.rs"]
pub(super) mod clip;
#[path = "search.rs"]
mod search;
#[path = "tabs.rs"]
mod tabs;
#[path = "telemetry.rs"]
mod telemetry;
#[path = "toolbar.rs"]
mod toolbar;
use super::menu::MenuGeom;
use super::scroll::{self, Cause, Composed, Extent, LineRec, Overlay};
use super::{clean, fill_rect, json_escape, App, BACKGROUND};

pub const LIST_W: usize = 330; // wide enough for the longest state and the memory figure at the right end of a row
const LIST_TOP: usize = 40;
const HISTORY_PAGE: i64 = 100;
const SCROLLBACK_CAP: usize = 2000; // lines kept while reading older history; beyond it the newest are let go
pub(crate) const COPY_LIMIT: usize = 60 * 1024; // one X11 property transfer, no INCR

#[derive(Clone)]
pub enum ListItem {
    Heading(String),
    Row(String, String, Option<memrow::Cell>), // session id, text, memory figure
}

/// What the frames cost, measured by the window itself (microseconds, since the start): the numbers the dump shows so a cost is read, not guessed.
#[derive(Default, Clone)]
pub struct FrameStats {
    pub frames: u64,
    pub total_us: u64,
    pub layout_us: u64, // placing the transcript lines (shaping new text)
    pub paint_us: u64, // controls, bar, text blits
    pub present_us: u64,
    pub last_us: u64,
    pub causes: [u64; Cause::ALL.len()], // how each frame was drawn: a shift, or the cause of a full repaint (see scroll.rs)
    pub kept_rows: u64, // pixel rows that a shift kept instead of drawing
}

#[derive(Default)]
pub struct UiState {
    pub stats: FrameStats,
    pub faces: super::fonts::Faces,
    pub enabled: bool,
    pub markers: bool,
    pub workbench: bool,
    pub maximized_requested: bool,
    pub rows: Vec<Row>,
    pub labels: HashMap<String, String>, pub ssh_hosts: HashMap<String, String>,
    pub quotas: HashMap<String, (Option<u8>, Option<u8>)>,
    pub quota_resets: HashMap<String, (Option<u64>, Option<u64>)>,
    pub list_sig: String,
    pub list_swash: Option<SwashCache>,
    pub surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    pub scroll: usize,
    pub behind: bool, // the live tail is not buffered while far into older history
    pub oldest_known: i64,
    pub history_pending: bool,
    pub history_requests: u64,
    pub history_lines: u64,
    pub sel: Option<(usize, usize)>,
    pub dragging: bool,
    pub drag_origin: Option<(f32, f32, Option<usize>, bool)>,
    pub drag_moved: bool,
    pub mouse: (f64, f64),
    pub mods: ModifiersState,
    pub hit_lines: Vec<(f32, f32, usize, u16)>,
    pub controls: Controls, // every drawn box and what a click on it means
    pub list_items: Vec<ListItem>,
    pub menu: bool, // the bar is shown (the product window; the measurement client only with --menu)
    pub menu_open: Option<usize>,
    pub menu_sel: usize,
    pub menu_geom: MenuGeom,
    pub toolbar_geom: toolbar::ToolbarGeom,
    pub tab_scroll: usize, pub tab_ensure_visible: bool, pub tabs_geom: tabs::TabsGeom, pub telemetry_geom: telemetry::TelemetryGeom, pub sidebar: super::sidebar::SidebarState, pub calls: super::call_view::CallViewState, pub mention: super::mention_popup::MentionPopupState, pub expanded_tools: std::collections::HashSet<(String, i64)>,
    pub ticks: u64,
    pub copied: u64,
    pub clip_tx: Option<mpsc::Sender<clip::Job>>,
    pub header: String,
    pub bar: Option<String>,
    pub window_span: (usize, usize),
    pub proxy: Option<winit::event_loop::EventLoopProxy<super::Ev>>, // for the answer of a paste, which is fetched off the event loop
    pub paste_pending: bool,
    pub numbered: Vec<String>, // the sessions in the order of the list: Alt+1..9 pick by this number
    pub host_notice: Option<String>, // the host runs another build than this window (or was replaced): one line, always shown
    pub flash: Option<String>, // a one-line notice outside the panels, until the next key
    pub panel: Option<super::panel::Panel>, // new session, accounts and sign-in, in place of the transcript
    pub scale: f32, // the display scale factor; every size here is a logical size times this
    pub composed: Option<Composed>, // the last frame in the surface's buffer, for the scroll step
    pub no_scroll_step: bool, // measurement client only: every frame is a full repaint
    pub fonts_gen: u64, // changes when the font system is replaced
    pub fixed_now: Option<u64>, // tests only: the second that the clocks of working sessions count from
    pub clock: Option<Instant>, // tests only: the instant that the dwell of a card counts to
    pub card_seen: Option<(String, i64, Instant)>, // the card of the host's own that was last drawn: its session, its id, and the instant of its first frame
    pub clicked: Option<(std::time::Instant, String)>, // the last click on a row of the list: a second one soon after is a double click
    pub mem: memrow::Look, // the host's and the window's own memory, and the row under the mouse
    pub clipboard_write: bool, // the host's setting `clipboard_write` (whether the agent tool is offered)
    pub notifications_off: bool, pub group_by_workspace: bool, pub font_px: u8, pub show_events: bool, pub show_hidden: bool, // view settings
    pub line_styles: Vec<u8>, // per-buffer-line style tag (`0` non-transcript, `1 + (role & STYLE_MASK)` transcript)
    pub seen_finished: HashMap<String, i64>, // last seen `finished_seq` per viewed session
    pub window_title: String, // last X11 title set by `sync_window_title`
    pub search: search::SearchBar, // on-demand find bar (`Ctrl+F` / `Ctrl+Shift+F`)
    pub real_screen: Vec<super::realbar::RealEntry>, // active time-bounded real-screen grants
    pub saved_cwd: String, pub saved_view: String, pub saved_session: String, pub restored_once: bool, pub panel_rows: usize, pub panel_cols: usize, pub async_seq: u64, // M05/M06 workspace & async state
}

fn display_cols(text: &str) -> usize {
    text.chars().map(|c| if super::editor::wide(c) { 2 } else { 1 }).sum()
}

impl App {
    /// The second that the clocks count from (the wall clock; a test may fix it so that two windows read the same one).
    pub(crate) fn clock_now(&self) -> u64 {
        self.ui.fixed_now.unwrap_or_else(now_secs)
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl App {
    // ------------------------------------------------------------------ what the host says
    pub(crate) fn ui_status(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return };
        let rows: Vec<Row> = value["sessions"].as_array().into_iter().flatten().filter_map(Row::of_status).collect(); // synthetic sessions are not shown in this view
        for r in &rows {
            if !r.account.is_empty() && (r.weekly_remaining_pct.is_some() || r.short_remaining_pct.is_some()) {
                self.ui.quotas.insert(r.account.clone(), (r.weekly_remaining_pct, r.short_remaining_pct));
            }
            if !r.account.is_empty() && (r.weekly_resets_at.is_some() || r.short_resets_at.is_some()) {
                self.ui.quota_resets.insert(r.account.clone(), (r.weekly_resets_at, r.short_resets_at));
            }
        }
        // a stable order: sessions keep the host's order, groups appear in the order of their first session
        self.ui.rows = rows;
        self.ui.mem.read(&value);
        self.sync_settings(&value["settings"]);
        self.sync_calls_from_status(&value);
        let real_next: Vec<super::realbar::RealEntry> = value["real_screen"].as_array().into_iter().flatten().filter_map(super::realbar::RealEntry::of_json).collect();
        if real_next != self.ui.real_screen {
            self.ui.real_screen = real_next;
            self.dirty = true;
        }
        if let Some(row) = self.ui.rows.iter().find(|r| r.session == self.session) {
            self.generation = row.generation; // the target's generation, so an input or an answer is never aimed at a stale one
        }
        self.refresh_list();
        self.maybe_restore_workspace();
        self.maybe_init_sidebar();
    }

    pub(crate) fn ui_accounts(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return };
        self.sync_accounts_value(&value);
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
        self.card_tick();
        self.send("{\"op\":\"status\"}");
        if self.ui.ticks % 5 == 1 {
            self.send("{\"op\":\"account.list\"}");
        }
    }

    /// The last line of the transcript while the selected session works: "… working (37 s)", or how long it has said nothing.
    /// Not part of the transcript (nothing is stored or copied); shown only at the live end and not behind a panel.
    pub(crate) fn status_line(&self, panel_shown: bool) -> Option<String> {
        if let Some(cs) = self.active_call_status_line(panel_shown) {
            return Some(cs);
        }
        if panel_shown || self.ui.scroll != 0 || self.ui.behind {
            return None;
        }
        let row = self.selected_row()?;
        let now = self.clock_now();
        match (row.working_for(now), row.silent_for(now)) {
            (Some(w), Some(q)) => Some(format!("… no output for {} (working {})", clock_text(q), clock_text(w))),
            (Some(w), None) => Some(format!("… working ({})", clock_text(w))),
            _ => None,
        }
    }

    pub(crate) fn selected_row(&self) -> Option<&Row> {
        self.ui.rows.iter().find(|r| r.session == self.session)
    }

    /// The target beside the input: provider, account, model and state of the selected session, as the host reports them.
    fn header_text(&self) -> String {
        let mut text = self.header_base();
        if let Some(n) = &self.ui.host_notice {
            text = format!("{text} | {n}");
        }
        if let Some(flash) = &self.ui.flash {
            text = format!("{text} | {flash}");
        }
        text
    }

    fn header_base(&self) -> String {
        if let Some(ch) = self.active_call_header() {
            return ch;
        }
        match self.selected_row() {
            Some(r) => r.header_line(&self.row_label(r), self.clock_now(), self.ui.menu, self.ui.quotas.get(&r.account).copied()),
            None if self.session.is_empty() => "select a session".into(),
            None => format!("{} | not listed", self.session),
        }
    }

    pub(crate) fn label_of(&self, account: &str) -> String {
        if account.is_empty() { return "no account".into(); }
        let base = self.ui.labels.get(account).cloned().unwrap_or_else(|| account.to_string());
        self.ui.ssh_hosts.get(account).map_or_else(|| base.clone(), |h| format!("{base} [ssh: {h}]"))
    }

    // ------------------------------------------------------------------ lines of the selected session
    pub(crate) fn push_line(&mut self, seq: i64, text: String) {
        let turn_edge = self.ui.enabled && text.starts_with("[turn ");
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
        if turn_edge {
            self.send("{\"op\":\"status\"}"); // a turn began or ended: the clock is told so now, not at the next tick
        }
    }

    pub(crate) fn scroll_by(&mut self, lines: isize) {
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
    /// Put text on the clipboard: the result of a person's own key, mouse selection, or `lico clipboard write`.
    pub(crate) fn copy_text(&mut self, text: String) {
        if text.len() > COPY_LIMIT {
            return; // larger than one transfer; refused rather than cut
        }
        if self.clip_sender().send(clip::Job::text(text)).is_ok() {
            self.ui.copied += 1;
        }
    }

    // ------------------------------------------------------------------ events
    pub(crate) fn ui_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::Resized(_) => {
                self.ui.composed = None;
                self.ui.list_sig.clear();
                if let Some(view) = self.view.as_ref() { self.ui.scroll = self.ui.scroll.min(view.lines.len().saturating_sub(1)); }
                self.dirty = true;
                false
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui.scale = *scale_factor as f32;
                self.ui.list_sig.clear(); // every label is shaped again at the new size (the controls notice the scale)
                let s = self.ui.scale.max(1.0);
                let (fp, lh) = (self.font_px() * s, self.line_h() * s);
                if let Some(view) = self.view.as_mut() { view.buffer.lines.clear(); view.buffer.set_metrics(Metrics::new(fp, lh)); }
                self.dirty = true;
                false // the window's own handling still runs
            }
            WindowEvent::ModifiersChanged(m) => {
                self.ui.mods = m.state();
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.mouse = (position.x, position.y);
                self.dirty |= self.hover_moved();
                if self.ed_mouse_drag(position.x as f32, position.y as f32) {
                    return true;
                }
                self.main_mouse_drag(position.x as f32, position.y as f32);
                true
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let (x, y) = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
                if *state == ElementState::Pressed {
                    self.ui.flash = None;
                    let hit = self.ui.controls.hit_at(x, y);
                    if !matches!(&hit, Some(Action::Session(_) | Action::Tab(_))) { self.ui.clicked = None; }
                    if self.ui.menu_open.is_some() || hit.is_some() || !self.ed.contains(x, y) { self.ed.clear_selection(); }
                    if let Some(open) = self.ui.menu_open {
                        match hit {
                            Some(Action::Item(m, i)) => self.menu_run(m, i),
                            Some(Action::Menu(m)) if m == open => self.menu_close(),
                            Some(Action::Menu(m)) => self.menu_open_at(m),
                            _ => self.menu_close(), // a click elsewhere only closes the menu
                        }
                    } else if self.host_popup_click(hit.clone()) || self.sidebar_popup_click(hit.clone()) {
                    } else {
                        if !matches!(&hit, Some(Action::SidebarFilter)) { self.ui.sidebar.filter_focused = false; }
                        match hit {
                            Some(ref a) if self.llm_click(a) || self.native_click(a) || self.newchat_click(a) || self.sidebar_click(a) => {}
                            Some(Action::Menu(m)) => self.menu_open_at(m),
                            Some(Action::Toolbar(i)) => self.toolbar_click(i),
                            Some(Action::Session(id) | Action::Tab(id)) => self.row_click(id),
                            Some(Action::CloseTab(id)) => self.close_tab_by_id(id),
                            Some(Action::CallTab(cid)) => self.open_call_tab(&cid),
                            Some(Action::CloseCallTab(cid)) => self.close_call_tab(&cid),
                            Some(Action::CallHangup) => self.hangup_active_call(),
                            Some(Action::CallJumpSession(sid)) => self.switch_to(&sid),
                            Some(Action::MentionItem(i)) => { let _ = self.complete_mention_index(i); }
                            Some(Action::TabScroll(d)) => self.step_tab_scroll(d),
                            Some(Action::NewTab) => self.form_open(),
                            Some(Action::NewGlobal) => self.form_open_global(),
                            Some(Action::FocusSessionDir) => self.focus_active_session_dir(),
                            Some(Action::Allow) => self.answer_approval(true),
                            Some(Action::Deny) => self.answer_approval(false),
                            Some(Action::ReadCard) => self.card_open(),
                            Some(Action::ModalBackdrop | Action::SidebarNewChat | Action::NewChatAccount(_) | Action::NewChatMode(_) | Action::NewChatOk | Action::NewChatCancel | Action::NewChatOptions | Action::NativeYes | Action::NativeNo | Action::NativeLater | Action::LlmScan | Action::LlmClose | Action::LlmAllUsers) => {}
                            Some(Action::StopRealScreen) => self.stop_real_screen(),
                            Some(Action::Item(..) | Action::HostItem(..) | Action::SidebarHostItem(..) | Action::SidebarTab(..) | Action::SidebarHost | Action::SidebarDotfiles | Action::SidebarRefresh | Action::SidebarNewHere | Action::SidebarFilter | Action::SidebarFilterClear | Action::SidebarGlobal | Action::SidebarActiveDir | Action::SidebarJumpActive | Action::SidebarParent | Action::SidebarEntry(..) | Action::SidebarChatEntry(..) | Action::SidebarCallEntry(..) | Action::SidebarCallClose(..) | Action::SidebarCallStartPeer(..) | Action::SidebarPage(..) | Action::SidebarNavBack | Action::SidebarNavForward | Action::SidebarBookmarkToggle | Action::SidebarBookmarksBtn | Action::SidebarBookmarkItem(..) | Action::SidebarBookmarkRemove(..)) => {}
                            None if x < self.effective_list_w(self.ui.scale) as f32 => {}
                            None if self.ed_mouse_press(x, y) => {}
                            None => self.main_mouse_press(x, y),
                        }
                    }
                    self.dirty = true;
                } else {
                    self.main_mouse_release();
                }
                true
            }
            WindowEvent::MouseInput { state, button, .. } if *button != MouseButton::Left => self.middle_press(*state, *button), // the middle button over the box pastes the primary selection
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (*y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(p) => (p.y / (self.line_h() * self.ui.scale.max(1.0)) as f64) as isize,
                };
                if !self.files_wheel(lines) && !self.sidebar_wheel(lines) { self.scroll_by(lines); }
                true
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                (self.ui.flash, self.ui.clicked) = (None, None);
                if self.llm_key(event) || self.native_key(event) || self.newchat_key(event) || self.real_screen_hotkey(event) || self.menu_key(event) || self.sidebar_key(event) || self.panel_key(event) || self.search_key(event) || self.paste_key(event) || self.approval_key(event) || self.call_hotkey(event) || self.mention_key(event) || self.edit_key(event) || self.nav_key(event) {
                    return true;
                }
                let ctrl = self.ui.mods.control_key() && !self.ui.mods.alt_key();
                let chord = ctrl && self.ui.mods.shift_key();
                let letter = super::key_letter(event);
                match &event.logical_key {
                    _ if ctrl && letter == Some('c') => {
                        if !event.repeat { self.copy_selection(); }
                        true
                    }
                    Key::Named(NamedKey::Insert) if ctrl && !self.ui.mods.shift_key() => {
                        if !event.repeat { self.copy_selection(); }
                        true
                    }
                    _ if chord && letter == Some('p') => {
                        if !event.repeat { self.capture_to_input(); }
                        true
                    }
                    Key::Named(NamedKey::Escape) => {
                        if self.ui.sel.is_some() {
                            self.ui.sel = None; // Esc with a selection only drops the selection
                        } else if self.ed.clear_selection() {
                        } else if !self.drop_chip() && self.ed.is_empty() && self.preedit.is_empty() && (self.active_call_open() || !self.session.is_empty()) { // a picture waiting in the box goes first
                            self.go_start(); // an empty input line: back to the start screen (the session keeps running)
                        }
                        self.dirty = true;
                        true
                    }
                    Key::Named(NamedKey::PageUp) => { self.scroll_by(20); true }
                    Key::Named(NamedKey::PageDown) => { self.scroll_by(-20); true }
                    Key::Named(NamedKey::End) => { self.scroll_by(-(self.ui.scroll as isize)); true }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Go to a session: its view is attached, the one before is detached (its draft stays with it).
    pub(crate) fn switch_to(&mut self, id: &str) {
        self.park_active_call_draft();
        if let Some(r) = self.ui.rows.iter_mut().find(|r| r.session == id) {
            r.tab_closed = false;
        }
        self.ui.scroll = 0;
        self.ui.sel = None;
        self.ui.behind = false;
        self.ui.history_pending = false;
        self.show(id, None);
        self.sync_active_tab_scroll();
        self.remember_session_view(id);
        self.sidebar_sync_to_active_session();
        if self.ui.search.open && !self.ui.search.scope_all && self.ui.search.pending_jump.is_none() && !self.ui.search.query.is_empty() {
            self.run_search();
        }
        self.dirty = true;
    }

    /// Back to the start screen: no session is shown, nothing is ended.
    pub(crate) fn go_start(&mut self) {
        self.park_active_call_draft();
        self.ui.scroll = 0;
        self.ui.sel = None;
        self.ui.behind = false;
        self.ui.history_pending = false;
        self.hide();
        self.remember_start_view();
        self.dirty = true;
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: next and previous session of the list (wrapping); Alt+1..9: the session with that number.
    pub(crate) fn nav_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if self.ui.numbered.is_empty() {
            return false;
        }
        let ids = self.ui.numbered.clone();
        let target = match &event.logical_key {
            Key::Named(NamedKey::Tab) if self.ui.mods.control_key() => {
                if !event.repeat {
                    self.panel_close();
                    self.step_session(if self.ui.mods.shift_key() { -1 } else { 1 });
                }
                return true;
            }
            Key::Character(c) if self.ui.mods.alt_key() && !self.ui.mods.control_key() => c.chars().next().and_then(|d| d.to_digit(10)).filter(|d| *d >= 1).map(|d| d as usize - 1).filter(|i| *i < ids.len()),
            _ => None,
        };
        match target {
            Some(i) if !event.repeat => {
                self.panel_close();
                if ids[i] != self.session {
                    self.switch_to(&ids[i]);
                }
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// The next (or previous) session of the list, wrapping; the first (or last) when none is shown.
    pub(crate) fn step_session(&mut self, delta: isize) {
        let ids = self.ui.numbered.clone();
        let n = ids.len() as isize;
        if n == 0 {
            return;
        }
        let here = ids.iter().position(|id| *id == self.session).map(|i| i as isize);
        let i = match (here, delta > 0) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), _) => (i + delta).rem_euclid(n),
        };
        if ids[i as usize] != self.session {
            self.switch_to(&ids[i as usize]);
        }
    }

    fn line_at(&self, y: f32) -> Option<usize> {
        self.ui.hit_lines.iter().find(|l| y >= l.0 && y < l.1).map(|l| l.2)
    }

    // ------------------------------------------------------------------ the model, for tests and inspection
    pub(crate) fn ui_dump(&self) {
        let rows = self.rows_dump_json();
        let view = self.view.as_ref();
        let causes: serde_json::Map<String, Value> = Cause::ALL.iter().zip(self.ui.stats.causes).map(|(c, n)| (c.name().to_string(), json!(n))).collect();
        let frame = json!({
            "frames": self.ui.stats.frames, "total_us": self.ui.stats.total_us, "layout_us": self.ui.stats.layout_us, "paint_us": self.ui.stats.paint_us,
            "present_us": self.ui.stats.present_us, "last_us": self.ui.stats.last_us, "lines_shaped": self.ui.faces.lines, "lines_exact": self.ui.faces.exact,
            "words_exact": self.ui.faces.w_old, "probes": self.ui.faces.probes, "label_shapes": self.ui.controls.shapes, "kept_rows": self.ui.stats.kept_rows,
            "scroll_step": !self.ui.no_scroll_step, "causes": causes,
        });
        let layout = json!({
            "list_w": self.effective_list_w(self.ui.scale),
            "window_size": self.window.as_ref().map(|w| [w.inner_size().width, w.inner_size().height]),
            "maximized": self.window.as_ref().is_some_and(|w| w.is_maximized()),
            "maximized_requested": self.ui.maximized_requested,
            "rows": self.ui.controls.hits.iter().filter_map(|h| match &h.action { Action::Session(id) => Some(json!([h.rect[1], h.rect[3], id])), _ => None }).collect::<Vec<_>>(),
            "dim": self.ui.controls.dimmed,
            "allow": self.ui.controls.hits.iter().find(|h| h.action == Action::Allow).map(|h| h.rect),
            "deny": self.ui.controls.hits.iter().find(|h| h.action == Action::Deny).map(|h| h.rect),
            "lines": self.ui.hit_lines.iter().map(|l| json!([l.0, l.1, l.2])).collect::<Vec<_>>(),
        });
        let settings = json!({"clipboard_write": self.ui.clipboard_write, "notifications": !self.ui.notifications_off, "group_by": if self.ui.group_by_workspace { "workspace" } else { "account" }, "font_px": self.font_px() as i64, "show_events": self.ui.show_events});
        let shown_lines = self.shown_lines_dump();
        let memory = json!({"total": memrow::total(&self.ui.rows, &self.ui.mem), "hover": self.ui.mem.hover});
        let dump = json!({
            "ev": "ui", "rows": rows, "selected": self.session, "header": self.header_text(), "activity_line": self.status_line(self.panel_view().is_some()).unwrap_or_default(),
            "approval_bar": self.approval_row_any_panel().and_then(|r| r.approvals.first().map(|(id, prompt)| format!("approval {id}: {prompt}"))),
            "card_seen": self.ui.card_seen.is_some(), "card_ready": self.card_ready(),
            "scroll": self.ui.scroll, "behind": self.ui.behind, "lines_buffered": view.map_or(0, |v| v.lines.len()), "shown_lines": shown_lines,
            "first_seq": view.and_then(|v| v.lines.front().map(|l| l.0)), "history_requests": self.ui.history_requests, "history_lines": self.ui.history_lines,
            "visible": [self.ui.window_span.0, self.ui.window_span.1], "selection": self.selection_text(), "copied": self.ui.copied,
            "panel": self.panel_json(), "menu": self.menu_json(), "toolbar": self.toolbar_json(), "tabs": self.tabs_json(), "telemetry": self.telemetry_json(), "sidebar": self.sidebar_json(), "calls": self.calls_json(), "mention_popup": self.mention_popup_json(), "frame": frame,
            "hint": self.hint_text(1200 * 2, 1.0), "list_text": self.ui.list_sig, "window_title": self.desired_window_title(),
            "search": self.search_json(), "memory": memory, "settings": settings, "workspace_state": self.workspace_state_json(), "real_screen": self.real_screen_json(),
            "input": self.shown_input(), "editor": self.editor_json(),
            "start_screen": if self.ui.panel.is_none() && self.session.is_empty() && !self.active_call_open() { self.start_lines().join("\n") } else { String::new() },
            "layout": layout,
        });
        println!("{dump}");
        let _ = std::io::stdout().flush();
        let _ = json_escape; // (kept for the line-oriented commands)
    }

    // ------------------------------------------------------------------ drawing
    pub(crate) fn draw_ui(&mut self) {
        let started = Instant::now();
        let Some(window) = self.window.clone() else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        let Some(mut surface) = self.ui.surface.take() else { return };
        let mut drawn = None;
        if surface.resize(w, h).is_ok() {
            if let Ok(mut buf) = surface.buffer_mut() {
                let age = buf.age();
                if let Some((laid_out, painted)) = self.compose(&mut buf, w.get() as usize, h.get() as usize, age) {
                    window.pre_present_notify();
                    drawn = Some((laid_out, painted, buf.present().is_ok()));
                }
            }
        }
        self.ui.surface = Some(surface);
        let Some((laid_out, painted, presented)) = drawn else { return };
        if !presented {
            self.ui.composed = None; // what the buffer holds is not known
        }
        self.card_frame(presented);
        let done = Instant::now();
        let us = |a: Instant, b: Instant| b.duration_since(a).as_micros() as u64;
        let stats = &mut self.ui.stats;
        stats.frames += 1;
        stats.layout_us += us(started, laid_out);
        stats.paint_us += us(laid_out, painted);
        stats.present_us += us(painted, done);
        stats.last_us = us(started, done);
        stats.total_us += stats.last_us;
    }

    /// Composes the window into `buf` (`age` as softbuffer says it: 1 when the buffer still holds the last presented frame) and says when the layout
    /// was done and the painting; none when nothing could be drawn. The scroll step (see scroll.rs) lives here.
    pub(crate) fn compose(&mut self, buf: &mut [u32], width: usize, height: usize, age: u8) -> Option<(Instant, Instant)> {
        let mut buf = buf;
        let s = self.ui.scale.max(1.0);
        let list_w = self.effective_list_w(s);
        let (fp, line_h) = (self.font_px(), self.line_h());
        let lh = line_h * s;
        let (menu_h, bar_h) = toolbar::bar_heights(self.ui.menu, s, line_h);
        let real_entries = self.ui.real_screen.clone();
        let top = bar_h + super::realbar::height_px_with(&real_entries, s, line_h);
        let tab_h = tabs::height_px(self.tabs_shown(), s, line_h);
        let chat_top = top + tab_h;
        let list_top = if self.ui.menu { top + 4.0 * s } else { LIST_TOP as f32 * s };
        let main_w = width.saturating_sub(list_w);
        self.refresh_list();
        // what the main area says: the target beside the input, an approval modal if pending, the transcript window, the draft
        let hint = self.hint_text(width, s);
        let text_w = main_w.saturating_sub((4 + 4) * s as usize); // the text starts 4 logical pixels right of the list and keeps 4 from the window edge
        let cols = (text_w as f32 / (self.char_w() * s)).max(10.0) as usize;
        let area_rows = ((height as f32 - chat_top) / lh) as usize;
        let pv = self.prompt_view(cols, area_rows); // the input box and the line under it, or the one line of a panel
        let n_prompt = pv.rows.len() + pv.note.is_some() as usize;
        if self.ui.panel.is_some() { (self.ui.panel_rows, self.ui.panel_cols) = (area_rows.saturating_sub(3 + n_prompt), cols); }
        let panel_view = self.panel_view();
        let show_telem = self.show_telemetry_bar();
        let mut header = panel_view.as_ref().map_or_else(|| self.header_text(), |v| v.0.clone());
        if panel_view.is_some() {
            if let Some(n) = &self.ui.host_notice {
                header = format!("{header} | {n}");
            }
            if let Some(flash) = &self.ui.flash {
                header = format!("{header} | {flash}");
            }
        }
        let menu_open = self.ui.menu_open;
        let sb_mode = self.sidebar_shown();
        let overlay_menu = menu_open.is_some() || self.files_host_popup_open() || self.sidebar_host_popup_open() || self.sidebar_bookmarks_popup_open() || self.active_call_open() || self.mention_popup_shown();
        let menu_sel = self.ui.menu_sel;
        let menu_entries = menu_open.map(|m| self.menu_entries(m)).unwrap_or_default();
        let tool_entries = if self.ui.menu { self.toolbar_entries() } else { Vec::new() };
        let list_items = self.ui.list_items.clone();
        let total = memrow::total(&self.ui.rows, &self.ui.mem);
        let session_sel = self.session.clone();
        let bar = if panel_view.is_some() { None } else { self.modal_approval_row().and_then(|r| r.approvals.first().map(|(id, prompt)| format!("approval {id}: {prompt}"))) };
        let search_bar = if self.ui.panel.is_some() { None } else { self.search_bar_line() };
        self.ui.header = header.clone();
        self.ui.bar = bar.clone();
        let status = self.status_line(panel_view.is_some());
        let rows_avail = area_rows.saturating_sub(4 + search_bar.is_some() as usize + status.is_some() as usize + show_telem as usize + n_prompt - 1);
        let is_tx = panel_view.is_none();
        let (lines, raw_end): (Vec<(usize, i64, String, u16)>, usize) = match &panel_view {
            Some((_, body)) => {
                let v: Vec<_> = body.iter().enumerate().map(|(i, l)| (i, i as i64, l.clone(), 0u16)).collect();
                let n = v.len();
                (v, n)
            }
            None => {
                let total_raw = self.view.as_ref().map_or(0, |v| v.lines.len());
                let re = total_raw.saturating_sub(self.ui.scroll.min(total_raw));
                (self.visible_transcript_lines(re), re)
            }
        };
        let fs = self.font_system.as_mut()?;
        let (mut first, mut end, mut used) = if is_tx { (lines.len(), lines.len(), 0) } else { (0, 0, 0) };
        if is_tx {
            while first > 0 {
                let rows = (display_cols(&lines[first - 1].2).max(1) + cols - 1) / cols;
                if used + rows > rows_avail { break; }
                (used, first) = (used + rows, first - 1);
            }
        } else {
            while end < lines.len() {
                let rows = (display_cols(&lines[end].2).max(1) + cols - 1) / cols;
                if used + rows > rows_avail { break; }
                (used, end) = (used + rows, end + 1);
            }
        }
        let mut text = header;
        if let Some(sb) = &search_bar {
            text.push('\n');
            text.push_str(&clean(sb));
        }
        let base = 1 + search_bar.is_some() as usize;
        // Shaping a mixed-script line is what a frame costs (about 37 of its 39 ms), so a line whose text and style are unchanged keeps its
        // shaped layout: the buffer's lines are taken apart and put back by (style_tag, text), and only new text is shaped.
        let mut prev_styles = std::mem::take(&mut self.ui.line_styles);
        let faces = &mut self.ui.faces;
        let mut build = |first: usize, end: usize, text: &mut String, buffer: &mut Buffer, fs: &mut FontSystem| {
            let mut pool: HashMap<(u8, String), Vec<BufferLine>> = HashMap::new();
            for (idx, line) in std::mem::take(&mut buffer.lines).into_iter().enumerate() {
                let tag = prev_styles.get(idx).copied().unwrap_or(0);
                pool.entry((tag, line.text().to_string())).or_default().push(line);
            }
            let mut wanted: Vec<(u8, u16, &str)> = text.split('\n').map(|s| (0u8, 0u16, s)).collect();
            wanted.extend(lines[first..end].iter().map(|l| {
                let tag = if is_tx { 1 + (l.3 & super::transcript::STYLE_MASK) as u8 } else { 0 };
                (tag, l.3, l.2.as_str())
            }));
            wanted.extend(status.as_deref().map(|s| (0u8, 0u16, s)));
            wanted.extend(pv.rows.iter().chain(&pv.note).map(|s| (0u8, 0u16, s.as_str())));
            prev_styles.clear();
            prev_styles.reserve(wanted.len());
            for (tag, role, line_text) in wanted {
                prev_styles.push(tag);
                let line = pool.get_mut(&(tag, line_text.to_string())).and_then(Vec::pop).unwrap_or_else(|| {
                    let attrs = faces.line(fs, line_text);
                    let attrs = if tag > 0 { super::transcript::tint_role(attrs, line_text, role) } else { attrs };
                    BufferLine::new(line_text, LineEnding::None, attrs, Shaping::Advanced)
                });
                buffer.lines.push(line);
            }
            buffer.set_scroll(Scroll::default());
            buffer.set_size(Some(text_w as f32), Some(height as f32 - chat_top));
            buffer.shape_until_scroll(fs, true);
            // lines put in by hand are not all laid out by the call above (new lines in front of a reused one stayed unshaped and
            // the whole text vanished): every line of the screenful is laid out here
            for i in 0..buffer.lines.len() {
                buffer.line_layout(fs, i);
            }
        };
        let view = self.view.get_or_insert_with(|| {
            // no session selected yet: a text buffer still carries the header and the draft
            let buffer = Buffer::new(fs, Metrics::new(fp * s, lh));
            super::View { lines: Default::default(), surface: None, buffer, swash: SwashCache::new(), session_rgb: 0xFF00_0000, seq_rgb: 0xFF00_0000 }
        });
        build(first, end, &mut text, &mut view.buffer, fs);
        // the estimate of wrapped rows can be short: if the layout is taller than the area, drop the trailing (panel) or oldest (transcript) line
        for _ in 0..6 {
            let runs = view.buffer.layout_runs().count();
            if runs <= rows_avail + base + n_prompt + status.is_some() as usize || first >= end {
                break;
            }
            if is_tx { first += 1; } else { end -= 1; }
            build(first, end, &mut text, &mut view.buffer, fs);
        }
        self.ui.line_styles = prev_styles;
        self.ui.window_span = (lines.get(first).map_or(raw_end, |l| l.0), if is_tx { raw_end } else { end });
        let laid_out = Instant::now();
        // click boxes of the lines on screen, from the layout that was just made
        self.ui.hit_lines.clear();
        let mut search_y: Option<(f32, f32)> = None;
        for run in view.buffer.layout_runs() {
            if search_bar.is_some() && run.line_i == 1 {
                let (y0, y1) = (run.line_top + chat_top, run.line_top + run.line_height + chat_top);
                search_y = Some(search_y.map_or((y0, y1), |(a, _)| (a, y1)));
            }
            if run.line_i >= base && first + (run.line_i - base) < end {
                let entry = &lines[first + (run.line_i - base)];
                let (raw_idx, role) = (entry.0, entry.3);
                match self.ui.hit_lines.last_mut() {
                    Some(last) if last.2 == raw_idx => last.1 = run.line_top + run.line_height + chat_top,
                    _ => self.ui.hit_lines.push((run.line_top + chat_top, run.line_top + run.line_height + chat_top, raw_idx, role)),
                }
            }
        }
        // the scroll step: what the frame is made of, and whether the last one can be kept in part
        let (ox, oy) = (list_w as i32 + (4.0 * s) as i32, chat_top as i32);
        let line_count = view.buffer.lines.len();
        self.ed.place(&view.buffer, (line_count - n_prompt, pv.rows.len(), pv.caret), [ox as f32, oy as f32, text_w as f32, s]);
        let mut recs: Vec<LineRec> = (0..line_count)
            .map(|i| {
                let (key, role) = if i + n_prompt >= line_count {
                    (scroll::Key::Prompt, 0)
                } else if status.is_some() && i + n_prompt + 1 == line_count {
                    (scroll::Key::Status, 0)
                } else if i >= base && first + (i - base) < end {
                    let entry = &lines[first + (i - base)];
                    (scroll::Key::Transcript(entry.1), entry.3)
                } else {
                    (scroll::Key::Header, 0)
                };
                LineRec { key, role, text: view.buffer.lines[i].text().to_string(), top: 0.0, bottom: 0.0, rows: 0, ext: Extent::EMPTY }
            })
            .collect();
        for run in view.buffer.layout_runs() {
            let rec = &mut recs[run.line_i];
            if rec.rows == 0 {
                rec.top = run.line_top + oy as f32;
            }
            rec.bottom = run.line_top + run.line_height + oy as f32;
            rec.rows += 1;
        }
        if let Some(last) = self.ui.composed.as_ref() {
            // the header's rows are needed to say which rows may be kept; it is the one line that is measured before the others are drawn
            recs[0].ext = match last.lines.first() {
                Some(l) if l.key == scroll::Key::Header && l.text == recs[0].text => l.ext,
                _ => line_extent(&mut view.buffer, fs, &mut view.swash, 0, ox, oy),
            };
        }
        let hint_y_f = height as f32 - lh;
        let telem_y_f = hint_y_f - lh;
        let hint_row = (if show_telem { telem_y_f } else { hint_y_f }) as i32;
        let mut frame = Composed {
            width,
            height,
            scale: s,
            fonts: self.ui.fonts_gen,
            list_w,
            top: oy,
            hint_y: hint_row,
            corner: if self.ui.markers { 16 } else { 0 },
            overlay: Overlay {
                menu: overlay_menu,
                selection: self.ui.sel.is_some() || !self.ed.sel_rects.is_empty(), search: self.ui.search.open || search_bar.is_some(),
                panel: panel_view.is_some(),
                approval: bar.is_some(),
                preedit: !self.preedit.is_empty(),
                scrollback: self.ui.scroll != 0,
            },
            lines: recs,
        };
        let plan = scroll::decide(self.ui.composed.as_ref(), &scroll::Now { allowed: !self.ui.no_scroll_step, age, row_height: lh, composed: &frame });
        let cause = plan.as_ref().map_or_else(|cause| *cause, |_| Cause::Shift);
        self.ui.stats.causes[Cause::ALL.iter().position(|c| *c == cause).unwrap_or(0)] += 1;
        let cleared = plan.as_ref().map_or_else(|_| Vec::new(), |shift| shift.cleared(height as i32));
        match &plan {
            Ok(shift) => {
                // the kept rows move, everything else is background again: from here on the frame is drawn as a full repaint is, over the kept rows
                let old = &self.ui.composed.as_ref().expect("a shift has a last frame").lines;
                for n in 0..shift.kept.len() {
                    frame.lines[shift.kept.start + n].ext = old[shift.old_first + n].ext.shifted(shift.dy);
                }
                keep_rows(buf, width, list_w, shift, &cleared);
                self.ui.stats.kept_rows += (shift.hi - shift.lo) as u64;
            }
            Err(_) => buf.fill(BACKGROUND),
        }
        if self.ui.markers {
            // two 16-pixel squares (session and sequence colour) that the pixel-latency harness reads; the product window has none
            fill_rect(&mut buf, width, 0, 0, 16, view.session_rgb);
            fill_rect(&mut buf, width, 16, 0, 16, view.seq_rgb);
        }
        if list_w > 0 {
            for y in (top as usize)..height {
                let idx = y * width + list_w - 1;
                if let Some(px) = buf.get_mut(idx) {
                    *px = 0xFF30_3840;
                }
            }
        }
        if is_tx {
            super::transcript::paint_transcript_decorations(&mut buf, width, height, list_w, s, oy, hint_row, &self.ui.hit_lines, plan.is_ok().then_some(cleared.as_slice()));
        }
        if let Some((sy0, sy1)) = search_y {
            rect(&mut buf, width, height, list_w as i32, sy0 as i32, main_w as i32, (sy1 - sy0) as i32, 0xFF1E_2836);
        }
        if is_tx && self.ui.search.open {
            for (y0, y1, idx, _) in &self.ui.hit_lines {
                if let Some(active) = view.lines.get(*idx).and_then(|(seq, _)| self.ui.search.line_kind(&session_sel, *seq)) {
                    let bg = if active { 0xFF5A_4314 } else { 0xFF2B_2412 };
                    rect(&mut buf, width, height, list_w as i32, *y0 as i32, main_w as i32, (*y1 - *y0) as i32, bg);
                }
            }
        }
        if let Some((a, b)) = self.ui.sel {
            let (lo, hi) = (a.min(b), a.max(b));
            for (y0, y1, idx, _) in &self.ui.hit_lines {
                if *idx >= lo && *idx <= hi {
                    rect(&mut buf, width, height, list_w as i32, *y0 as i32, main_w as i32, (*y1 - *y0) as i32, 0xFF2A_4A7A);
                }
            }
        }
        for &r in &self.ed.sel_rects { controls::fill(&mut buf, width, height, r, 0xFF2A_4A7A); }
        let ink = Color::rgb(0xE0, 0xE0, 0xE0);
        let hint_ink = Color::rgb(0x9A, 0xA4, 0xB0);
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        controls.begin(s, fp);
        let mut y = list_top;
        if !sb_mode {
            for item in &list_items {
                match item {
                    ListItem::Heading(text) => {
                        controls.text(fs, swash, &mut buf, width, height, 4.0 * s, y, list_w as i32 - 2, text, ink);
                        y += lh;
                    }
                    ListItem::Row(id, text, cell) => {
                        controls.row(fs, swash, &mut buf, width, height, [0.0, y, list_w as f32 - 1.0, y + lh], text, *id == session_sel, ink, Action::Session(id.clone()));
                        memrow::draw(controls, fs, swash, &mut buf, width, height, [0.0, y, list_w as f32 - 1.0, y + lh], cell);
                        y += lh;
                    }
                }
            }
        }
        let modes: Vec<Mode> = (0..frame.lines.len())
            .map(|i| match &plan {
                Ok(shift) if shift.kept.contains(&i) => {
                    if frame.lines[i].ext.touches(&cleared) {
                        Mode::Rows(&cleared)
                    } else {
                        Mode::Skip
                    }
                }
                _ => Mode::All,
            })
            .collect();
        let mut extents: Vec<Extent> = frame.lines.iter().zip(&modes).map(|(l, m)| if matches!(m, Mode::All) { Extent::EMPTY } else { l.ext }).collect();
        blit_with(&mut view.buffer, fs, &mut view.swash, &mut buf, width, height, ox, oy, i32::MAX, ink, Some(Lines { modes: &modes, extents: &mut extents }));
        for (line, extent) in frame.lines.iter_mut().zip(extents) {
            line.ext = extent;
        }
        if let Some([x, y, w, h]) = self.ed.caret {
            controls::fill(&mut buf, width, height, [x, y, x + w, y + h], 0xFFE0_E0E0); // the caret of the input box: a bar, never blinking
        }
        // the permanent hint line, over whatever reaches the bottom row
        let hint_y = hint_y_f;
        controls::fill(&mut buf, width, height, [0.0, hint_y, width as f32, height as f32], 0xFF18_1E26);
        let hint_w = controls.text(fs, swash, &mut buf, width, height, 4.0 * s, hint_y, i32::MAX, &hint, hint_ink);
        if !sb_mode {
            memrow::draw_total(controls, fs, swash, &mut buf, width, height, [4.0 * s, hint_y - lh, list_w as f32 - 2.0, y], &total, hint_ink);
        }
        super::realbar::draw(&real_entries, controls, fs, swash, &mut buf, width, height, bar_h);
        if sb_mode {
            self.draw_sidebar(&mut buf, width, height, list_w, top, chat_top, hint_y, &total);
        }
        self.draw_telemetry_bar(&mut buf, width, height, list_w, telem_y_f, hint_y);
        self.draw_global_quota_summary(&mut buf, width, height, hint_y, 4.0 * s + hint_w);
        self.draw_tab_bar(&mut buf, width, height, list_w, top, chat_top);
        self.draw_call_room_controls(&mut buf, width, height, list_w, chat_top);
        self.draw_mention_popup(&mut buf, width, height, list_w);
        self.draw_sidebar_host_popup(&mut buf, width, height);
        self.draw_sidebar_bookmarks_popup(&mut buf, width, height);
        self.draw_approval_modal(&mut buf, width, height, list_w, top, chat_top, hint_y);
        self.draw_new_chat(&mut buf, width, height, list_w, top, chat_top, hint_y);
        self.draw_native_prompt(&mut buf, width, height, top, chat_top, hint_y);
        self.draw_llm_find(&mut buf, width, height, top, chat_top, hint_y);
        self.draw_top_bars(&mut buf, width, height, menu_h, bar_h, menu_open, menu_sel, &menu_entries, &tool_entries);
        let painted = Instant::now();
        self.ui.composed = Some(frame);
        Some((laid_out, painted))
    }
}

/// The scroll step's move: the kept rows go to their new place, every other row is background, and the list column is background again
/// (it is drawn anew with its rows). What follows is the code of a full repaint.
fn keep_rows(buf: &mut [u32], width: usize, list_w: usize, shift: &scroll::Shift, cleared: &[(i32, i32)]) {
    if shift.dy != 0 {
        buf.copy_within(shift.lo as usize * width..shift.hi as usize * width, (shift.lo + shift.dy) as usize * width);
    }
    for &(a, b) in cleared {
        buf[a as usize * width..b as usize * width].fill(BACKGROUND);
    }
    for y in (shift.lo + shift.dy) as usize..(shift.hi + shift.dy) as usize {
        buf[y * width..y * width + list_w.min(width)].fill(BACKGROUND);
    }
}

/// The pixel rows that the glyph bitmaps of one buffer line reach, without drawing anything.
fn line_extent(buffer: &mut Buffer, fs: &mut FontSystem, swash: &mut SwashCache, line: usize, ox: i32, oy: i32) -> Extent {
    let mut extent = Extent::EMPTY;
    for run in buffer.layout_runs().filter(|run| run.line_i == line) {
        for glyph in run.glyphs {
            let physical = glyph.physical((0., run.line_y), 1.0);
            let Some(image) = swash.get_image(fs, physical.cache_key).as_ref() else { continue };
            if image.content == SwashContent::SubpixelMask {
                continue;
            }
            let y0 = physical.y - image.placement.top + oy;
            extent.add(physical.x + image.placement.left + ox, y0, y0 + image.placement.height as i32);
        }
    }
    extent
}

/// What `blit_with` does with the glyphs of one buffer line.
pub(super) enum Mode<'a> {
    All,
    Skip,
    Rows(&'a [(i32, i32)]), // only the pixel rows in these ranges
}

/// Per buffer line: what to draw of it, and where the extents of the lines that were drawn in full are written.
pub(super) struct Lines<'a> {
    pub modes: &'a [Mode<'a>],
    pub extents: &'a mut [Extent],
}

pub(super) fn rect(buf: &mut [u32], width: usize, height: usize, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for row in y.max(0)..(y + h).min(height as i32) {
        for col in x.max(0)..(x + w).min(width as i32) {
            if let Some(px) = buf.get_mut(row as usize * width + col as usize) {
                *px = color;
            }
        }
    }
}

pub(super) fn blit(buffer: &mut Buffer, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, ox: i32, oy: i32, clip_x: i32, ink: Color) {
    blit_with(buffer, fs, swash, buf, width, height, ox, oy, clip_x, ink, None);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn blit_with(buffer: &mut Buffer, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, ox: i32, oy: i32, clip_x: i32, ink: Color, mut lines: Option<Lines>) {
    // what `Buffer::draw` does, but a glyph's bitmap is blended row by row from its pixels (most are empty) instead of through a callback per pixel
    buffer.shape_until_scroll(fs, false);
    let clip = clip_x.min(width as i32);
    let modes: Option<&[Mode]> = lines.as_ref().map(|l| l.modes);
    for run in buffer.layout_runs() {
        let mode = modes.map_or(&Mode::All, |m| &m[run.line_i]);
        if matches!(mode, Mode::Skip) {
            continue;
        }
        for glyph in run.glyphs {
            let physical = glyph.physical((0., run.line_y), 1.0);
            let ink = glyph.color_opt.unwrap_or(ink);
            let Some(image) = swash.get_image(fs, physical.cache_key).as_ref() else { continue };
            let (gw, gh) = (image.placement.width as i32, image.placement.height as i32);
            let (x0, y0) = (physical.x + image.placement.left + ox, physical.y - image.placement.top + oy);
            let (bytes, colored) = match image.content {
                SwashContent::Mask => (1, false),
                SwashContent::Color => (4, true),
                SwashContent::SubpixelMask => continue, // cosmic-text draws nothing for it either
            };
            if let (Some(l), Mode::All) = (lines.as_mut(), mode) {
                l.extents[run.line_i].add(x0, y0, y0 + gh);
            }
            let (first, last) = ((-x0).max(0).min(gw), (clip - x0).clamp(0, gw)); // the columns that fall inside the picture
            if first >= last {
                continue;
            }
            let (r, g, b) = (ink.r() as u32, ink.g() as u32, ink.b() as u32);
            for row in 0..gh {
                let py = y0 + row;
                if py < 0 || py as usize >= height {
                    continue;
                }
                if let Mode::Rows(rows) = mode {
                    if !rows.iter().any(|&(a, b)| py >= a && py < b) {
                        continue;
                    }
                }
                let line = &mut buf[py as usize * width + (x0 + first) as usize..py as usize * width + (x0 + last) as usize];
                let from = (row * gw + first) as usize * bytes;
                let source = &image.data[from..from + (last - first) as usize * bytes];
                if colored {
                    for (dst, texel) in line.iter_mut().zip(source.chunks_exact(4)) {
                        blend(dst, texel[0] as u32, texel[1] as u32, texel[2] as u32, texel[3] as u32);
                    }
                } else {
                    for (dst, &alpha) in line.iter_mut().zip(source) {
                        blend(dst, r, g, b, alpha as u32);
                    }
                }
            }
        }
    }
}

/// One pixel of a glyph over what is there: a covered pixel is replaced, a partly covered one is mixed per channel.
#[inline(always)]
fn blend(dst: &mut u32, r: u32, g: u32, b: u32, alpha: u32) {
    if alpha == 0 {
        return;
    }
    *dst = if alpha == 255 {
        0xFF00_0000 | r << 16 | g << 8 | b
    } else {
        let mix = |s: u32, shift: u32| {
            let d = (*dst >> shift) & 0xFF;
            ((d as i32 + (s as i32 - d as i32) * alpha as i32 / 255) as u32) & 0xFF
        };
        0xFF00_0000 | mix(r, 16) << 16 | mix(g, 8) << 8 | mix(b, 0)
    };
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
