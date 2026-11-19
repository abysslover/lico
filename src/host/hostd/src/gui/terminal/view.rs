//! The terminal tab in the window (`M23-P03`). A tab is one of the host's terminals (`terminal.*`, `terminal/` of the host): the window shows the newest screen the host told it, sends
//! the person's keys, text, paste and wheel, and follows its own size. The host keeps the shell and the emulator; the window keeps what it needs to paint (the last screen, the size
//! it told the host, the selection) and one blocking `terminal.watch` for the shown tab.
//!
//! Nothing polls: the next watch is sent after the paint of the screen the last one brought. A watch carries a **generation**: the host has no way to cancel one, so a scroll (which
//! asks at another offset) starts a new generation and the answer of the old one, when it comes, is dropped. While the view is scrolled back no live watch is sent (the view is a still
//! picture; typing or Shift+End returns to the live screen). The pure parts (colours, the screen, the pieces of a line, the grid, the selection, the keys) are in `screen.rs` and
//! `keys.rs`, with tests.

use std::collections::VecDeque;

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::{KeyEvent, MouseScrollDelta};
use winit::keyboard::Key;

use super::keys::{self, Held, Lico, Logical, Routed};
use super::screen::{self, Screen, Sel};
use super::super::browser_view::call_watch;
use super::super::controls::{self, Controls};
use super::super::jobs::{AsyncDone, AsyncJob};
use super::super::App;

/// What the host answered, for one request the window made about a terminal.
pub(crate) enum TermJob {
    Open,
    /// An open that then types a line into the new shell (`M23-P04b`): the command and the conversation of `agy` the tab is for, when it is one.
    OpenTyping { line: String, conversation: Option<String> },
    List,
    Watch { tab: String, gen: u64 },
    Resize { tab: String, size: (u16, u16) },
    Input,
    /// Nothing to do with the answer (a close).
    Done,
    /// The text of a selection, for the clipboard.
    Text,
}

pub(crate) struct TTab {
    pub id: String,
    pub cwd: String,
    /// The title the list said (the screen's own is newer once there is one).
    pub title: String,
    pub screen: Screen,
    /// The change number of the screen, and the one the next watch asks with (0: answered at once).
    pub seq: u64,
    pub ask: u64,
    pub gen: u64,
    pub watching: bool,
    pub rearm: bool,
    /// How many rows the view is above the bottom.
    pub scroll: usize,
    /// The size the host has (the open's answer, a resize's, the screen's).
    pub size: (u16, u16),
    pub resizing: bool,
    pub painted: u64,
    /// The conversation of `agy` the window opened this terminal for (`M23-P04b`), while the window lives; none for a terminal the person opened.
    pub conversation: Option<String>,
}

impl TTab {
    fn new(id: String, cwd: String, title: String, size: (u16, u16)) -> TTab {
        TTab { id, cwd, title, screen: Screen::default(), seq: 0, ask: 0, gen: 0, watching: false, rearm: false, scroll: 0, size, resizing: false, painted: 0, conversation: None }
    }
}

/// What the tab row drew for one terminal tab: its id, the tab's box, its close box and, when an agent runs in it, the picture's box.
pub(crate) type TabGeom = (String, [f32; 4], [f32; 4], Option<[f32; 4]>);

#[derive(Default)]
pub(crate) struct TermState {
    pub tabs: Vec<TTab>,
    pub active: Option<String>,
    /// The host's list was asked for (once, when the window first has a host to ask).
    pub listed: bool,
    /// Where the grid is painted in the last frame of the window: x, y, width, height; and the cell (width as the font's advance, height of a line) and the grid it holds.
    pub rect: [i32; 4],
    pub cell: (f32, f32),
    pub grid: (u16, u16),
    /// The person's input waits here, in order, with the tab it is for: one request is in flight, and what is queued behind it for one tab is joined.
    pub queue: VecDeque<(String, Vec<u8>)>,
    pub input_inflight: bool,
    pub sel: Option<Sel>,
    pub dragging: bool,
    /// The shaped text of the grid: its own cache, so that a full screen does not push the window's labels out.
    pub labels: Controls,
    pub first_pixel: u32,
    /// Where the tab row drew each terminal tab in the last frame: the id, the tab's box and its close box (the dump says them; a test clicks there).
    pub tab_geom: Vec<TabGeom>,
}

/// The most bytes of input joined into one request while one is in flight, and the most one paste request carries (the host takes 64 KiB).
const JOIN: usize = 4096;
const CHUNK: usize = 32 * 1024;

/// What the window says for an error of `terminal.open`.
fn refusal(error: &str) -> String {
    match error {
        "tab_limit" => "eight terminals are open: close one first".into(),
        "shell_missing" => "no shell was found to start".into(),
        e if e.starts_with("folder_") => format!("that folder cannot be used for a terminal ({e})"),
        "spawn_failed" | "pty_failed" => "the shell could not be started".into(),
        other => format!("terminal refused: {other}"),
    }
}

impl App {
    /// The terminal view is the one shown: a terminal tab, and no session, call room or panel in front of it.
    pub(crate) fn terminal_active(&self) -> bool {
        self.ui.panel.is_none() && self.session.is_empty() && self.ui.calls.active_call.is_none() && self.ui.terminal.active.is_some()
    }

    /// A browser tab or a terminal tab is shown: the main area is painted over, so every frame is a full repaint and the text rows are blank lines.
    pub(crate) fn page_active(&self) -> bool {
        self.browser_active() || self.terminal_active()
    }

    fn terminal_tab(&mut self, id: &str) -> Option<&mut TTab> {
        self.ui.terminal.tabs.iter_mut().find(|t| t.id == id)
    }

    fn shown_tab(&self) -> Option<&TTab> {
        if !self.terminal_active() {
            return None;
        }
        let id = self.ui.terminal.active.as_deref()?;
        self.ui.terminal.tabs.iter().find(|t| t.id == id)
    }

    fn terminal_call(&mut self, request: Value, seconds: u64, job: TermJob) {
        let _ = self.spawn_host_call(request, seconds, move |_| AsyncJob::Terminal(job));
    }

    pub(crate) fn terminal_list(&mut self) {
        if !self.socket.is_empty() {
            self.terminal_call(json!({"op": "terminal.list"}), 6, TermJob::List);
        }
    }

    /// A new terminal, on the person's own action only (the menu, the wizard): in `cwd`, or the person's home; the size of the grid as it was last painted.
    pub(crate) fn terminal_open(&mut self, cwd: Option<String>) {
        self.terminal_open_as(cwd, TermJob::Open);
    }

    /// The same, with the job that tells what the window does with the host's answer (`M23-P04b`: the open that types a line into the new shell).
    pub(super) fn terminal_open_as(&mut self, cwd: Option<String>, job: TermJob) {
        let mut request = json!({"op": "terminal.open"});
        let (cols, rows) = self.ui.terminal.grid;
        if cols > 0 && rows > 0 {
            (request["cols"], request["rows"]) = (json!(cols), json!(rows));
        }
        if let Some(dir) = cwd.filter(|d| d.starts_with('/')) {
            request["cwd"] = json!(dir);
        }
        self.terminal_call(request, 15, job);
    }

    /// The menu's `New terminal tab`: in the folder Directories shows, else the selected session's, else at home.
    pub(crate) fn terminal_open_here(&mut self) {
        let cwd = self.terminal_start_dir();
        self.terminal_open(cwd);
    }

    /// The folder a terminal starts in, from the menu and from the Terminal icon: the folder Directories shows now (`M23-P03d`, the owner's word: the left decides; `M23-P03c` made it
    /// count at all, since `saved_cwd` does not follow the person's moves), else the folder of the session in front (when the left shows none of this machine's: an ssh host, the Home scope,
    /// a failed listing), else the last folder the window kept (`M23-P03a`); the first of them that is a folder of this machine. None, the host's home, when there is none.
    pub(super) fn terminal_start_dir(&self) -> Option<String> {
        let sb = &self.ui.sidebar;
        let session = self.selected_row().filter(|r| r.ssh_host.is_empty()).map(|r| r.cwd.as_str());
        let shown = Some(sb.cwd.as_str()).filter(|_| sb.ssh_host.is_none() && !sb.global_scope && !sb.unreachable);
        [shown, session, Some(self.ui.saved_cwd.as_str())].into_iter().flatten().find(|d| !d.is_empty() && std::path::Path::new(d).is_dir()).map(str::to_string)
    }

    /// Shows a terminal: whatever was shown gives way (a panel, a session, a browser tab with its address draft parked), and the shown tab asks for its screen again.
    pub(crate) fn terminal_activate(&mut self, id: &str) {
        if self.terminal_tab(id).is_none() {
            return;
        }
        self.browser_enter("terminal"); // (clears the terminal that was shown, and parks a browser tab's address draft)
        self.ui.browser.active = None;
        self.ui.browser.prompt = None;
        let t = &mut self.ui.terminal;
        t.active = Some(id.to_string());
        (t.sel, t.dragging) = (None, false);
        t.queue.clear();
        if let Some(tab) = t.tabs.iter_mut().find(|x| x.id == id) {
            (tab.scroll, tab.ask, tab.watching, tab.rearm) = (0, 0, false, true);
            tab.gen += 1;
        }
        self.ui.flash = None;
        self.dirty = true;
    }

    /// The tab is gone (closed by the person, or the host no longer has it): another terminal is shown if one is, else the first screen.
    fn terminal_drop(&mut self, id: &str) {
        let shown = self.terminal_active() && self.ui.terminal.active.as_deref() == Some(id);
        self.ui.terminal.tabs.retain(|t| t.id != id);
        if self.ui.terminal.active.as_deref() == Some(id) {
            self.ui.terminal.active = None;
            (self.ui.terminal.sel, self.ui.terminal.dragging) = (None, false);
            self.ui.terminal.queue.retain(|(t, _)| t != id);
            if shown {
                match self.ui.terminal.tabs.last().map(|t| t.id.clone()) {
                    Some(next) => self.terminal_activate(&next),
                    None => self.view = None,
                }
            }
        }
        self.ui.composed = None;
        self.dirty = true;
    }

    pub(crate) fn terminal_close_ui(&mut self, id: &str) {
        if !self.socket.is_empty() {
            self.terminal_call(json!({"op": "terminal.close", "tab": id}), 8, TermJob::Done);
        }
        self.terminal_drop(id);
    }

    /// The shell of a tab has ended (`M23-P03a`): the tab goes away by itself, the host is told to give the terminal's place back, and a person who did not end it with `exit` (a failure, a
    /// signal) is told how it ended on the header's line.
    fn terminal_ended(&mut self, id: &str, exit: Option<i64>) {
        self.terminal_close_ui(id);
        match exit {
            Some(0) => {}
            Some(n) => self.ui.flash = Some(format!("The terminal's shell ended (exit {n}).")),
            None => self.ui.flash = Some("The terminal's shell ended.".into()),
        }
        self.dirty = true;
    }

    /// The host's answer to `terminal.open`: the tab is adopted and shown; its id, or none when the host refused (the person is told why).
    fn terminal_opened(&mut self, reply: &Value) -> Option<String> {
        if reply["ok"] != json!(1) {
            self.ui.flash = Some(refusal(reply["error"].as_str().unwrap_or("refused")));
            self.dirty = true;
            return None;
        }
        let id = reply["tab"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            return None;
        }
        let size = (reply["cols"].as_u64().unwrap_or(80) as u16, reply["rows"].as_u64().unwrap_or(24) as u16);
        if self.terminal_tab(&id).is_none() {
            self.ui.terminal.tabs.push(TTab::new(id.clone(), reply["cwd"].as_str().unwrap_or("").into(), String::new(), size));
        }
        self.terminal_activate(&id);
        Some(id)
    }

    /// The answers of the host to what this module asked.
    pub(crate) fn on_terminal_async(&mut self, job: TermJob, reply: Value) {
        let ok = reply["ok"] == json!(1);
        let error = reply["error"].as_str().unwrap_or("refused").to_string();
        match job {
            TermJob::Open => {
                self.terminal_opened(&reply);
            }
            TermJob::OpenTyping { line, conversation } => {
                if let Some(id) = self.terminal_opened(&reply) {
                    self.agy_started(&id, line, conversation);
                }
            }
            TermJob::List => {
                self.ui.terminal.listed = true;
                for row in reply["terminals"].as_array().filter(|_| ok).into_iter().flatten() {
                    let id = row["tab"].as_str().unwrap_or("").to_string();
                    if id.is_empty() || self.terminal_tab(&id).is_some() {
                        continue;
                    }
                    if row["running"] == json!(false) {
                        self.terminal_close_ui(&id); // a shell that ended while no window was looking: its table place is given back, and it gets no tab
                        continue;
                    }
                    let size = (row["cols"].as_u64().unwrap_or(80) as u16, row["rows"].as_u64().unwrap_or(24) as u16);
                    let mut tab = TTab::new(id, row["cwd"].as_str().unwrap_or("").into(), row["title"].as_str().unwrap_or("").into(), size);
                    tab.screen.agent = screen::agent_word(row.get("agent"));
                    self.ui.terminal.tabs.push(tab);
                }
                self.dirty = true;
            }
            TermJob::Watch { tab, gen } => {
                let Some(t) = self.terminal_tab(&tab) else { return };
                if gen != t.gen {
                    return; // the answer of a watch that was replaced (a scroll, or the tab shown again)
                }
                t.watching = false;
                let mut ended = None;
                match (ok, Screen::parse(&reply)) {
                    (true, Some(screen)) => {
                        t.size = (screen.cols, screen.rows);
                        (t.seq, t.ask, t.scroll) = (screen.seq, screen.seq, screen.scroll);
                        t.rearm = screen.scroll == 0;
                        ended = (!screen.running).then_some(screen.exit);
                        t.screen = screen;
                    }
                    _ if error == "unknown_tab" => self.terminal_drop(&tab),
                    _ => {}
                }
                if let Some(exit) = ended {
                    self.terminal_ended(&tab, exit);
                }
                self.dirty = true;
            }
            TermJob::Resize { tab, size } => {
                if let Some(t) = self.terminal_tab(&tab) {
                    t.resizing = false;
                    if ok {
                        t.size = size;
                    }
                }
                self.dirty = true; // the paint after the next screen may find another size to send
            }
            TermJob::Input => {
                self.ui.terminal.input_inflight = false;
                if !ok && matches!(error.as_str(), "terminal_ended" | "unknown_tab") {
                    self.ui.terminal.queue.clear();
                }
                self.terminal_pump();
            }
            TermJob::Done => {}
            TermJob::Text => {
                let text = reply["text"].as_str().filter(|_| ok).unwrap_or("").to_string();
                if text.is_empty() {
                    self.ui.flash = Some("Nothing to copy.".into());
                } else {
                    let (n, before) = (text.lines().count().max(1), self.ui.copied);
                    self.copy_text(text);
                    self.ui.flash = Some(if self.ui.copied > before { format!("Copied {n} {}.", if n == 1 { "line" } else { "lines" }) } else { "Copy refused: more than 64 KiB selected.".into() });
                }
                self.dirty = true;
            }
        }
    }

    /// After a frame is painted: ask for the host's list once, tell the host the size the grid has, and ask for the next change of the shown tab.
    pub(crate) fn terminal_after_paint(&mut self) {
        if self.socket.is_empty() {
            return;
        }
        if !self.ui.terminal.listed {
            self.ui.terminal.listed = true; // (a list that fails is not asked for again until the person acts: nothing here repeats on its own)
            self.terminal_list();
        }
        if !self.terminal_active() {
            return;
        }
        let (grid, active) = (self.ui.terminal.grid, self.ui.terminal.active.clone());
        let mut resize = None;
        let mut watch = None;
        if let Some(t) = self.ui.terminal.tabs.iter_mut().find(|t| Some(&t.id) == active.as_ref()) {
            if grid.0 > 0 && t.size != grid && !t.resizing {
                t.resizing = true;
                resize = Some((t.id.clone(), grid));
            }
            if t.rearm && !t.watching {
                (t.watching, t.rearm) = (true, false);
                watch = Some((t.id.clone(), t.ask, t.gen, t.scroll));
            }
        }
        if let Some((tab, size)) = resize {
            self.terminal_call(json!({"op": "terminal.resize", "tab": tab, "cols": size.0, "rows": size.1}), 8, TermJob::Resize { tab: tab.clone(), size });
        }
        if let Some((tab, ask, gen, scroll)) = watch {
            let (socket, proxy) = (self.socket.clone(), self.ui.proxy.clone());
            let request = json!({"op": "terminal.watch", "tab": tab, "seq": ask, "scroll": scroll});
            std::thread::spawn(move || {
                let (reply, _) = call_watch(&socket, request);
                if let Some(proxy) = proxy {
                    let _ = proxy.send_event(super::super::Ev::Async(AsyncDone { job: AsyncJob::Terminal(TermJob::Watch { tab, gen }), reply }));
                }
            });
        }
    }

    // ------------------------------------------------------------------ input

    /// Something is drawn over the window that the keys belong to first: a menu, a popup, the dialog of a new chat, or an approval waiting for its answer.
    fn terminal_overlay_open(&self) -> bool {
        self.ui.menu_open.is_some()
            || self.files_host_popup_open()
            || self.sidebar_host_popup_open()
            || self.sidebar_bookmarks_popup_open()
            || self.ui.sidebar.newchat.is_some()
            || self.modal_approval_row().is_some()
    }

    /// What the program switched on in the shown tab.
    fn terminal_modes(&self) -> lico_vt::Modes {
        self.shown_tab().map(|t| t.screen.modes).unwrap_or_default().modes()
    }

    /// Bytes for the shell of the shown tab: typing returns to the live screen and ends a selection; joined with what already waits for the same tab, in order.
    pub(super) fn terminal_send(&mut self, bytes: Vec<u8>) {
        let Some(t) = self.shown_tab() else { return };
        let (id, scrolled) = (t.id.clone(), t.scroll > 0);
        if bytes.is_empty() {
            return;
        }
        if scrolled {
            self.terminal_scroll_to(0);
        }
        let term = &mut self.ui.terminal;
        (term.sel, term.dragging) = (None, false);
        let joined = match term.queue.back_mut() {
            Some((tab, last)) if *tab == id && last.len() + bytes.len() <= JOIN => {
                last.extend_from_slice(&bytes);
                true
            }
            _ => false,
        };
        if !joined && term.queue.len() < 256 {
            term.queue.push_back((id, bytes));
        }
        self.terminal_pump();
        self.dirty = true;
    }

    fn terminal_pump(&mut self) {
        if self.ui.terminal.input_inflight || self.socket.is_empty() {
            return;
        }
        let Some((tab, bytes)) = self.ui.terminal.queue.pop_front() else { return };
        self.ui.terminal.input_inflight = true;
        let mut request = json!({"op": "terminal.input", "tab": tab});
        match String::from_utf8(bytes) {
            Ok(text) => request["text"] = json!(text),
            Err(e) => request["data"] = json!(crate::base64::base64_encode(e.as_bytes())),
        }
        self.terminal_call(request, 8, TermJob::Input);
    }

    /// Moves the view `by` rows (positive: back in the history); the host answers at once at the new offset, and the live watch is not sent while the view is back.
    pub(super) fn terminal_scroll_by(&mut self, by: isize) {
        let Some(t) = self.shown_tab() else { return };
        let most = t.screen.total.saturating_sub(usize::from(t.screen.rows));
        let to = (t.scroll as isize + by).clamp(0, most as isize) as usize;
        self.terminal_scroll_to(to);
    }

    fn terminal_scroll_to(&mut self, to: usize) {
        let Some(id) = self.ui.terminal.active.clone() else { return };
        if let Some(t) = self.terminal_tab(&id).filter(|t| t.scroll != to) {
            (t.scroll, t.ask, t.watching, t.rearm) = (to, 0, false, true);
            t.gen += 1;
            self.dirty = true;
        }
    }

    /// The keys of the terminal view (D4): the shell gets every key but Lico's few. A composition in progress owns the keys.
    pub(crate) fn terminal_hotkey(&mut self, event: &KeyEvent) -> bool {
        let m = &self.ui.mods;
        let held = Held { ctrl: m.control_key(), shift: m.shift_key(), alt: m.alt_key(), logo: m.super_key() };
        let logical = match &event.logical_key {
            Key::Named(n) => Logical::Named(*n),
            Key::Character(c) => Logical::Char(c.as_str()),
            _ => Logical::Other,
        };
        let text = event.text.as_ref().map(|t| t.to_string());
        self.terminal_key(logical, text.as_deref(), held, event.repeat)
    }

    /// `terminal_hotkey` with what it takes from the event (a test cannot make a `KeyEvent`): whether the key is the terminal's, and the effect.
    pub(super) fn terminal_key(&mut self, logical: Logical<'_>, text: Option<&str>, held: Held, repeat: bool) -> bool {
        let Some(tab) = self.shown_tab().map(|t| t.id.clone()) else { return false };
        if self.terminal_overlay_open() {
            return false; // a dropdown, a popup or an approval has the keys first
        }
        match keys::route(logical, text, held, &self.terminal_modes()) {
            Routed::Pass => false,
            Routed::Lico(action) => {
                if !repeat || matches!(action, Lico::HistoryUp | Lico::HistoryDown) {
                    self.terminal_lico(action, &tab);
                }
                true
            }
            Routed::Send(bytes) => {
                if !self.preedit.is_empty() {
                    return true;
                }
                self.terminal_send(bytes);
                true
            }
        }
    }

    pub(super) fn terminal_lico(&mut self, action: Lico, tab: &str) {
        let page = self.shown_tab().map_or(1, |t| usize::from(t.screen.rows).saturating_sub(1).max(1)) as isize;
        match action {
            Lico::Copy => self.terminal_copy(),
            Lico::Paste => self.start_paste(b"CLIPBOARD"),
            Lico::PastePrimary => self.start_paste(b"PRIMARY"),
            Lico::Close => self.terminal_close_ui(tab),
            Lico::HistoryUp => self.terminal_scroll_by(page),
            Lico::HistoryDown => self.terminal_scroll_by(-page),
            Lico::HistoryEnd => self.terminal_scroll_to(0),
        }
    }

    /// Ctrl+Shift+C: the selected text, asked of the host by absolute lines, goes to the clipboard when it comes.
    fn terminal_copy(&mut self) {
        let sel = self.ui.terminal.sel.filter(|s| !s.is_point());
        let (Some(sel), Some(id)) = (sel, self.shown_tab().map(|t| t.id.clone())) else {
            self.ui.flash = Some("Nothing selected: drag over the text first.".into());
            self.dirty = true;
            return;
        };
        self.terminal_call(text_request(&id, sel), 8, TermJob::Text);
    }

    /// Text that the input method committed (Korean and the like): the shell's when a terminal is shown.
    pub(crate) fn terminal_commit(&mut self, text: &str) -> bool {
        if !self.terminal_active() {
            return false;
        }
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        self.terminal_send(clean.into_bytes());
        true
    }

    /// The text of the person's own paste key, as the program asked for it (bracketed or not), in pieces the host takes.
    pub(crate) fn terminal_paste(&mut self, result: &Result<String, &'static str>) -> bool {
        if !self.terminal_active() {
            return false;
        }
        match result {
            Ok(text) => {
                let bytes = lico_vt::encode_paste(text, &self.terminal_modes());
                for piece in bytes.chunks(CHUNK) {
                    self.terminal_send(piece.to_vec());
                }
            }
            Err(_) => self.ui.flash = Some("Nothing to paste.".into()),
        }
        self.dirty = true;
        true
    }

    /// The cell of the shown screen under the pointer.
    fn terminal_point(&self) -> Option<(u16, u16)> {
        let t = self.shown_tab()?;
        let [x0, y0, ..] = self.ui.terminal.rect;
        let size = (t.screen.cols.min(self.ui.terminal.grid.0), t.screen.rows.min(self.ui.terminal.grid.1));
        screen::cell_at((self.ui.mouse.0 as f32, self.ui.mouse.1 as f32), (x0 as f32, y0 as f32), self.ui.terminal.cell, size)
    }

    /// The wheel over the grid: back and forward in the history; on the alternate screen (a pager, an editor) it is the arrow keys, as the usual terminals do.
    pub(crate) fn terminal_wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        if self.terminal_point().is_none() {
            return false;
        }
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => (*y * 3.0) as isize,
            MouseScrollDelta::PixelDelta(p) => (p.y / f64::from(self.ui.terminal.cell.1.max(1.0))) as isize,
        };
        let alt = self.shown_tab().is_some_and(|t| t.screen.modes.alt_screen);
        if alt && lines != 0 {
            let key = if lines > 0 { lico_vt::Key::Up } else { lico_vt::Key::Down };
            let one = lico_vt::encode_key(key, lico_vt::Mods::default(), &self.terminal_modes());
            self.terminal_send(one.repeat(lines.unsigned_abs().min(10)));
        } else if lines != 0 {
            self.terminal_scroll_by(lines);
        }
        true
    }

    /// A press on the grid starts a selection (at the absolute line, so it stays on its text when the screen scrolls); the release ends it, and a click that did not move selects nothing.
    pub(crate) fn terminal_mouse_button(&mut self, pressed: bool) -> bool {
        if pressed {
            let (Some((col, row)), Some(top)) = (self.terminal_point(), self.shown_tab().map(|t| t.screen.top)) else { return false };
            self.ui.terminal.sel = Some(Sel { anchor: (top + usize::from(row), col), focus: (top + usize::from(row), col) });
            self.ui.terminal.dragging = true;
            self.dirty = true;
            return true;
        }
        if !self.ui.terminal.dragging {
            return false;
        }
        self.ui.terminal.dragging = false;
        if self.ui.terminal.sel.is_some_and(|s| s.is_point()) {
            self.ui.terminal.sel = None;
        }
        self.dirty = true;
        true
    }

    /// The pointer moved with the button held: the selection's far end follows it, and stays in the grid.
    pub(crate) fn terminal_drag(&mut self) {
        if !self.ui.terminal.dragging {
            return;
        }
        let Some(t) = self.shown_tab() else { return };
        let [x0, y0, ..] = self.ui.terminal.rect;
        let size = (t.screen.cols.min(self.ui.terminal.grid.0), t.screen.rows.min(self.ui.terminal.grid.1));
        let (col, row) = screen::cell_near((self.ui.mouse.0 as f32, self.ui.mouse.1 as f32), (x0 as f32, y0 as f32), self.ui.terminal.cell, size);
        let line = t.screen.top + usize::from(row);
        if let Some(sel) = self.ui.terminal.sel.as_mut() {
            sel.focus = (line, col);
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------------ what the window says

    /// The header line of the terminal view.
    pub(crate) fn terminal_header(&self) -> Option<String> {
        let t = self.shown_tab()?;
        let title = if t.screen.title.is_empty() { t.title.as_str() } else { t.screen.title.as_str() };
        Some(screen::header(title, &t.cwd))
    }

    /// The one row under the grid.
    pub(crate) fn terminal_status_row(&self) -> String {
        self.shown_tab().map_or_else(String::new, |t| screen::status_row(t.scroll, &self.preedit))
    }

    /// The items of the hint line of the window while a terminal is shown (the window's own Ctrl+N and Ctrl+A are the shell's here).
    pub(crate) fn terminal_hint_items(&self) -> Vec<&'static str> {
        vec!["Terminal: your keys go to the shell", "Ctrl+Shift+C copy", "Ctrl+Shift+V paste", "Ctrl+Tab next session", "Ctrl+Shift+W close"]
    }

    /// The agent the host last named for the tab `id` (its list's row, then each watch of it), if any. A tab that is not shown is not watched, so it keeps the word it had.
    pub(crate) fn terminal_tab_agent(&self, id: &str) -> Option<&'static str> {
        self.ui.terminal.tabs.iter().find(|t| t.id == id)?.screen.agent
    }

    /// The tabs of the strip (`M23-P04a`): the id, the caption, whether it is the one shown, and the picture of the agent that runs in it; with a picture the caption lacks the text
    /// `>_` that says terminal (the picture is said in its place).
    pub(crate) fn terminal_strip(&self) -> Vec<(String, String, bool, Option<super::super::ui::icons::Icon>)> {
        self.terminal_tab_entries()
            .into_iter()
            .map(|(id, caption, shown)| {
                let picture = self.terminal_tab_agent(&id).and_then(super::super::ui::icons::Icon::of_agent);
                let caption = if picture.is_some() { screen::without_prompt(&caption).to_string() } else { caption };
                (id, caption, shown, picture)
            })
            .collect()
    }

    /// The tabs of the row: the id, the caption, whether it is the one shown.
    pub(crate) fn terminal_tab_entries(&self) -> Vec<(String, String, bool)> {
        let shown = self.terminal_active().then(|| self.ui.terminal.active.clone()).flatten();
        self.ui
            .terminal
            .tabs
            .iter()
            .map(|t| {
                let title = if t.screen.title.is_empty() { t.title.as_str() } else { t.screen.title.as_str() };
                (t.id.clone(), screen::caption(title, &t.cwd, &t.id), shown.as_deref() == Some(t.id.as_str()))
            })
            .collect()
    }

    // ------------------------------------------------------------------ painting

    /// Places the grid (from `x`, the header's bottom to the bottom row's top) and paints the shown tab's screen in it: the grounds, the selection, the text a piece at its own cell, the
    /// lines, and the cursor as a block with its character in the ground's colour.
    pub(crate) fn terminal_paint(&mut self, buf: &mut [u32], width: usize, height: usize, place: (f32, f32, f32)) {
        let s = self.ui.scale.max(1.0);
        let (x0, top, bottom) = place;
        let rect = [x0, top, width as f32 - 4.0 * s, bottom.max(top)];
        self.ui.terminal.rect = [x0 as i32, top as i32, (rect[2] - rect[0]) as i32, (rect[3] - rect[1]) as i32];
        controls::fill(buf, width, height, rect, screen::DEFAULT_BG);
        let (fp, cell_h) = (self.font_px(), self.line_h() * s);
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let term = &mut self.ui.terminal;
        term.labels.begin(s, fp);
        let cell_w = (term.labels.width(fs, &"M".repeat(16)) / 16.0).max(1.0); // the font's own advance, so that the columns are the font's
        term.cell = (cell_w, cell_h);
        term.grid = screen::grid_of(rect[2] - rect[0], rect[3] - rect[1] + 0.5, cell_w, cell_h);
        let grid = term.grid;
        let (sel, labels, first_pixel) = (term.sel, &mut term.labels, &mut term.first_pixel);
        let Some(t) = term.active.as_ref().and_then(|id| term.tabs.iter_mut().find(|t| &t.id == id)) else { return };
        t.painted += 1;
        *first_pixel = buf.get(top.max(0.0) as usize * width + x0.max(0.0) as usize).copied().unwrap_or(0);
        let sc = &t.screen;
        let ink = |rgb: u32| Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        let (cols, rows) = (usize::from(grid.0), usize::from(grid.1));
        let clip = (x0 + cols as f32 * cell_w) as i32;
        let x_of = |col: usize| x0 + col as f32 * cell_w;
        for (r, runs) in sc.lines.iter().enumerate().take(rows) {
            let y = top + r as f32 * cell_h;
            let spans = screen::spans(runs, cols);
            for sp in &spans {
                let lk = screen::look(runs[sp.run].fg, runs[sp.run].bg, runs[sp.run].attrs);
                if lk.paint_bg {
                    controls::fill(buf, width, height, [x_of(sp.col), y, x_of(sp.col + sp.cols), y + cell_h], lk.bg);
                }
            }
            if let Some((a, b)) = sel.and_then(|sl| sl.span_on(sc.top + r, sc.cols.min(grid.0))) {
                controls::fill(buf, width, height, [x_of(usize::from(a)), y, x_of(usize::from(b) + 1), y + cell_h], screen::SELECTION_BG);
            }
            for p in screen::pieces(runs, cols) {
                let lk = screen::look(runs[p.run].fg, runs[p.run].bg, runs[p.run].attrs);
                labels.text(fs, swash, buf, width, height, x_of(p.col), y, clip, &p.text, ink(lk.fg));
            }
            for sp in &spans {
                let lk = screen::look(runs[sp.run].fg, runs[sp.run].bg, runs[sp.run].attrs);
                let thick = s.round().max(1.0);
                if lk.underline {
                    controls::fill(buf, width, height, [x_of(sp.col), y + cell_h - thick - 1.0, x_of(sp.col + sp.cols), y + cell_h - 1.0], lk.fg);
                }
                if lk.strike {
                    controls::fill(buf, width, height, [x_of(sp.col), y + cell_h * 0.5, x_of(sp.col + sp.cols), y + cell_h * 0.5 + thick], lk.fg);
                }
            }
        }
        if let Some((cr, cc)) = sc.cursor.filter(|_| sc.modes.cursor_visible && t.scroll == 0) {
            let (cr, cc) = (usize::from(cr), usize::from(cc));
            if cr < rows && cc < cols {
                let y = top + cr as f32 * cell_h;
                let under = sc.lines.get(cr).and_then(|l| screen::char_at(l, cc));
                let w = under.map_or(1, |(_, w)| w);
                controls::fill(buf, width, height, [x_of(cc), y, x_of(cc + w), y + cell_h], screen::DEFAULT_FG);
                if let Some((c, _)) = under.filter(|(c, _)| *c != ' ') {
                    labels.text(fs, swash, buf, width, height, x_of(cc), y, clip, &c.to_string(), ink(screen::DEFAULT_BG));
                }
            }
        }
    }

    /// For the `ui` command on standard input: what a test reads.
    pub(crate) fn terminal_json(&self) -> Value {
        let t = &self.ui.terminal;
        let shown = self.shown_tab();
        json!({
            "active": t.active, "shown": self.terminal_active(), "rect": t.rect, "grid": [t.grid.0, t.grid.1], "cell": [t.cell.0, t.cell.1], "listed": t.listed,
            "header": self.terminal_header(), "status": shown.map(|_| self.terminal_status_row()), "queued": t.queue.len(), "inflight": t.input_inflight,
            "selection": t.sel.map(|s| json!({"anchor": [s.anchor.0, s.anchor.1], "focus": [s.focus.0, s.focus.1], "point": s.is_point()})), "dragging": t.dragging, "first_pixel": t.first_pixel,
            "lines": shown.map(|x| x.screen.plain_lines()),
            "tabs": t.tabs.iter().map(|x| { let mut v = json!({
                "rect": t.tab_geom.iter().find(|g| g.0 == x.id).map(|g| g.1), "close_rect": t.tab_geom.iter().find(|g| g.0 == x.id).map(|g| g.2),
                "agent": x.screen.agent, "picture_rect": t.tab_geom.iter().find(|g| g.0 == x.id).and_then(|g| g.3),
                "id": x.id, "cwd": x.cwd, "caption": screen::caption(if x.screen.title.is_empty() { &x.title } else { &x.screen.title }, &x.cwd, &x.id), "seq": x.seq, "gen": x.gen, "watching": x.watching,
                "scroll": x.scroll, "size": [x.size.0, x.size.1], "cols": x.screen.cols, "rows": x.screen.rows, "title": x.screen.title, "running": x.screen.running, "exit": x.screen.exit,
                "cursor": x.screen.cursor.map(|(r, c)| json!([r, c])), "top": x.screen.top, "total": x.screen.total, "painted": x.painted,
                "modes": {"app_cursor": x.screen.modes.app_cursor, "bracketed_paste": x.screen.modes.bracketed_paste, "alt_screen": x.screen.modes.alt_screen},
            }); v["conversation"] = json!(x.conversation); v }).collect::<Vec<_>>(),
        })
    }
}

/// The request for the text of a selection: its ends in the order of the text (a drag from the right to the left has its focus first).
pub(super) fn text_request(tab: &str, sel: Sel) -> Value {
    let (a, b) = sel.ordered();
    json!({"op": "terminal.text", "tab": tab, "from": [a.0, a.1], "to": [b.0, b.1]})
}
