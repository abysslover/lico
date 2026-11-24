//! The browser tab in the window (M17-P07). A tab is one of the host's browser tabs (`browser.*`, adapter `browser/`): the window shows its newest frame, edits its address on the
//! window's own input line, and asks the host again only after it has painted. The host keeps the model; the window keeps what it needs to paint: the tab's row, the newest
//! compressed frame, the decoded picture of the SHOWN tab only (freed when the tab is hidden), and one blocking `browser.watch` at a time.
//!
//! Nothing polls: the next watch is sent after the paint of the frame that the last one brought, so a frame newer than the painter can use is never sent; at most one `show` or
//! `resize` is in flight and the newest size wins. A new tab is made on the person's own action only (the `+B` buttons, the Session menu): never from a page or a link.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use cosmic_text::{Buffer, Metrics, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;

#[path = "browser_bar.rs"]
pub(super) mod browser_bar;
#[path = "browser_pages.rs"]
mod browser_pages;
use super::controls::{self, Rect};
use super::jobs::{AsyncDone, AsyncJob};
use super::{emit_ime, App, View};

/// What the host answered, or what the window asked, for one tab.
pub(crate) enum BrowserJob {
    Open { local: bool },
    Op { tab: String, what: &'static str, area: (u32, u32) },
    Go { tab: String },
    /// Nothing to do with the answer (a close, a navigation key).
    Done,
    List,
    Watch { tab: String },
    /// The answer to one input of the person (the next one is sent after it).
    Input,
    /// The answer to `browser.forget` (the remembered pages cleared).
    Forget,
}

type Inbox = Arc<Mutex<Option<(u64, Vec<u8>)>>>;

pub(crate) struct BTab {
    pub id: String,
    pub local: bool,
    pub row: Value,
    pub seq: u64,
    pub frame_seq: u64,
    pub png: Vec<u8>,
    pub image: Option<lico_frame::Image>,
    pub decoded_seq: u64,
    pub watching: bool,
    pub rearm: bool,
    /// The size last told to the host with `show` or `resize` (none: hidden as far as the host knows).
    pub sent: Option<(u32, u32)>,
    pub inflight: bool,
    pub inbox: Inbox,
    pub painted: u64,
    /// The address last put in the address row for this tab: the row follows the page only when the page's address CHANGED (a row the person cleared stays clear).
    pub put_url: String,
    /// The page was not there (starting, absent, failed) at the last look: the focus goes to it when it comes up.
    pub waiting: bool,
}

#[derive(Default)]
pub(crate) struct BrowserState {
    pub tabs: Vec<BTab>,
    pub active: Option<String>,
    /// An address prompt for a tab that does not exist yet (`Some(local)`).
    pub prompt: Option<bool>,
    /// Where the frame is painted in the last frame of the window: x, y, width, height.
    pub rect: [i32; 4],
    pub listed: bool,
    pub help: bool,
    pub last_pixel: u32,
    /// The keys and the text go to the page (a click on the page, a new tab) or to the address bar (a click on it, Ctrl+L, a prompt).
    pub page_focus: bool,
    /// Where the bar was painted in the last frame (none before the first), and what its field showed.
    pub bar: Option<browser_bar::BarLayout>,
    pub bar_text: String,
    /// What the one row under the page showed in the last frame.
    pub row: String,
    /// The person's input waits here, in order: one request is in flight and a mouse move that is not yet sent is replaced by the newer one.
    pub queue: VecDeque<Value>,
    pub input_inflight: bool,
    /// The pointer is over the page and is a crosshair there.
    pub pointer_on_page: bool,
}

/// The most of the page the window asks for at scale 2 and more (pixels).
pub(crate) const SCALE2_CAP: (u32, u32) = (1400, 900);

/// What the header says about the engine: that it is being installed (the size, the source and the start of the pinned hash: no question), or, when the install failed or is
/// switched off, the manual way (the reason is the tab's notice).
pub(crate) fn engine_text(row: &Value) -> Option<String> {
    match row["phase"].as_str()? {
        "installing" => Some(format!("installing the browser engine (about {} MB from {}, SHA-256 starts {})", row["size_mb"].as_u64().unwrap_or(0), row["source"].as_str().unwrap_or("the release page"), row["sha256_start"].as_str().unwrap_or(""))),
        "absent" => Some(format!("the engine is not installed; by hand: download {} (SHA-256 {}) and put the file moli (taken out of the archive folder) in {}", row["download"].as_str().unwrap_or(""), row["archive_sha256"].as_str().or_else(|| row["expected_sha256"].as_str()).unwrap_or(""), row["directory"].as_str().unwrap_or("~/.local/share/lico/browser/moli-v1.1.25"))),
        _ => None,
    }
}

/// The caption of a browser tab in the tab bar.
pub(crate) fn caption(tab: &BTab) -> String {
    let title = tab.row["title"].as_str().filter(|t| !t.is_empty()).or_else(|| tab.row["url"].as_str()).unwrap_or(&tab.id);
    let host = title.trim_start_matches("https://").trim_start_matches("http://");
    let mut cut: String = host.chars().take(20).collect();
    if host.chars().count() > 20 {
        cut.push('\u{2026}');
    }
    let loading = if tab.row["loading"] == json!(true) { " \u{25cf}" } else { "" };
    let note = if tab.row["notice"].is_string() { " !" } else { "" };
    format!("{}{cut}{loading}{note}", if tab.local { "\u{1f310}\u{b7}L " } else { "\u{1f310} " })
}

/// Reads one reply line with no time limit (a watch waits for a change), freeing as it goes: the text is turned into bytes and dropped, the JSON is dropped before the bytes
/// are kept. Returns the reply without the frame and the frame's bytes.
pub(crate) fn call_watch(socket: &str, request: Value) -> (Value, Option<(u64, Vec<u8>)>) {
    let gone = || (json!({"ok": 0, "error": "host_unreachable"}), None);
    let Ok(mut conn) = UnixStream::connect(socket) else { return gone() };
    if conn.write_all(format!("{request}\n").as_bytes()).is_err() {
        return gone();
    }
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    if !matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
        return gone();
    }
    let Ok(mut reply) = serde_json::from_str::<Value>(&line) else { return gone() };
    drop(line);
    let text = reply.as_object_mut().and_then(|o| o.remove("png")).and_then(|v| if let Value::String(s) = v { Some(s) } else { None });
    let frame_seq = reply["frame_seq"].as_u64().unwrap_or(0);
    let bytes = text.and_then(|t| crate::base64::base64_decode(&t)).map(|b| (frame_seq, b));
    (reply, bytes)
}

impl App {
    /// The browser view is the one shown: a tab or an address prompt, and no session or call room in front of it.
    pub(crate) fn browser_active(&self) -> bool {
        self.ui.panel.is_none() && self.session.is_empty() && self.ui.calls.active_call.is_none() && (self.ui.browser.active.is_some() || self.ui.browser.prompt.is_some())
    }

    fn browser_tab(&mut self, id: &str) -> Option<&mut BTab> {
        self.ui.browser.tabs.iter_mut().find(|t| t.id == id)
    }

    /// The address row's draft key: one draft for each tab and one for the prompt.
    fn browser_key(&self) -> String {
        format!("browser:{}", self.ui.browser.active.as_deref().unwrap_or("new"))
    }

    pub(super) fn browser_enter(&mut self, key: &str) {
        self.ui.terminal.active = None; // (a terminal tab gives way to whatever is entered)
        if self.ui.panel.is_some() {
            self.panel_close();
        }
        if !self.session.is_empty() {
            self.hide();
        } else {
            self.park_active_call_draft();
            if self.ui.browser.active.is_some() || self.ui.browser.prompt.is_some() {
                let pending = std::mem::take(&mut self.preedit);
                let old = self.browser_key();
                self.ed.park(&old, &pending);
            }
        }
        self.ui.scroll = 0;
        self.ui.sel = None;
        self.ui.composed = None;
        self.load_fonts();
        let (fp, lh) = (self.font_px(), self.line_h());
        let scale = self.ui.scale.max(1.0);
        if let Some(fs) = self.font_system.as_mut() {
            let buffer = Buffer::new(fs, Metrics::new(fp * scale, lh * scale));
            self.view = Some(View { lines: Default::default(), surface: None, buffer, swash: SwashCache::new(), session_rgb: 0xFF00_3848, seq_rgb: 0xFF00_0000 });
        }
        self.ed.resume(key);
        self.dirty = true;
    }

    /// An address prompt for a new tab (the person's `+B` or `+BL`).
    pub(crate) fn browser_prompt(&mut self, local: bool) {
        self.ui.browser.active = None;
        self.browser_enter("browser:new");
        self.ui.browser.prompt = Some(local);
        self.ui.browser.page_focus = false;
        self.ui.browser.help = false; // an empty page with the bar focused; F1 says the rest (M22-P05a)
        self.ui.flash = None;
        self.browser_list();
    }

    pub(crate) fn browser_activate(&mut self, id: &str) {
        if self.browser_tab(id).is_none() {
            return;
        }
        let key = format!("browser:{id}");
        self.ui.browser.prompt = None;
        self.browser_enter(&key);
        self.ui.browser.active = Some(id.to_string());
        let url = self.browser_tab(id).and_then(|t| t.row["url"].as_str().map(str::to_string)).unwrap_or_default();
        if self.ed.is_empty() && !url.is_empty() {
            self.ed.put(&url);
            if let Some(t) = self.browser_tab(id) {
                t.put_url = url;
            }
        }
        self.ui.browser.help = false;
        self.ui.browser.queue.clear();
        self.ui.browser.page_focus = true;
        if let Some(t) = self.browser_tab(id) {
            t.rearm = true;
        }
    }

    pub(crate) fn browser_close_ui(&mut self, id: &str) {
        let socket_empty = self.socket.is_empty();
        if !socket_empty {
            self.browser_call(json!({"op": "browser.close", "tab": id}), 8, BrowserJob::Done);
        }
        self.ui.browser.tabs.retain(|t| t.id != id);
        if self.ui.browser.active.as_deref() == Some(id) {
            self.ui.browser.active = None;
            match self.ui.browser.tabs.last().map(|t| t.id.clone()) {
                Some(next) => self.browser_activate(&next),
                None => {
                    self.ed.park(&format!("browser:{id}"), "");
                    self.view = None;
                }
            }
        }
        self.dirty = true;
    }

    fn browser_call(&mut self, request: Value, seconds: u64, job: BrowserJob) {
        let _ = self.spawn_host_call(request, seconds, move |_| AsyncJob::Browser(job));
    }

    pub(crate) fn browser_list(&mut self) {
        if !self.socket.is_empty() {
            self.browser_call(json!({"op": "browser.list"}), 6, BrowserJob::List);
        }
    }

    /// Enter on the address row: a new tab from the prompt, or a new address for the tab.
    pub(crate) fn browser_submit(&mut self) {
        let key = self.browser_key();
        let Some(text) = self.ed.prepare_send(&key) else {
            self.dirty = true;
            return;
        };
        emit_ime("sent", &text);
        match (self.ui.browser.prompt, self.ui.browser.active.clone()) {
            (Some(local), _) => {
                let mut request = json!({"op": "browser.open", "url": text});
                if local {
                    request["local"] = json!(true);
                } else {
                    request["search"] = json!(true); // words are searched, by the host's setting (M22-P05b); a tab for this machine takes addresses only
                }
                self.browser_call(request, 30, BrowserJob::Open { local });
            }
            (None, Some(tab)) => {
                self.ui.browser.page_focus = true; // the page is where the person goes next
                let local = self.ui.browser.tabs.iter().any(|t| t.id == tab && t.local);
                let mut request = json!({"op": "browser.go", "tab": tab, "address": text});
                if !local {
                    request["search"] = json!(true);
                }
                self.browser_call(request, 30, BrowserJob::Go { tab })
            }
            _ => {}
        }
        self.dirty = true;
    }

    fn refuse_text(&mut self, error: &str) {
        let why = match error {
            "address_empty" => "type an address or words to search".to_string(),
            "no_page" => "no closed tab to reopen".to_string(),
            "search_setting" => "the browser_search setting is not an https address with one %s".to_string(),
            "address_scheme" => "only http and https addresses load".to_string(),
            "address_credentials" => "an address with a user name or password is not opened".to_string(),
            "address_too_long" => "that address is too long".to_string(),
            "address_invalid" | "bad_request" => "that is not an address".to_string(),
            "tab_limit" => "eight browser tabs are open: close one first".to_string(),
            "engine_unsafe" => "the engine's folder can be written by others: it is not run".to_string(),
            "engine_not_pinned" => "the engine file is not the pinned release: it is not run".to_string(),
            "engine_failed" | "engine_fence" => "the engine could not start".to_string(),
            other => format!("refused: {other}"),
        };
        self.ui.flash = Some(why);
        self.dirty = true;
    }

    /// The answers of the host to what this module asked.
    pub(crate) fn on_browser_async(&mut self, job: BrowserJob, reply: Value) {
        let ok = reply["ok"] == json!(1);
        match job {
            BrowserJob::Open { local } => {
                if !ok {
                    let e = reply["error"].as_str().unwrap_or("refused").to_string();
                    self.refuse_text(&e);
                    return;
                }
                let id = reply["tab"].as_str().unwrap_or("").to_string();
                self.ui.browser.tabs.push(BTab { id: id.clone(), local, row: reply.clone(), seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: true, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new(), waiting: false });
                self.ui.browser.prompt = None;
                self.browser_activate(&id);
            }
            BrowserJob::Go { tab } => {
                if !ok {
                    let e = reply["error"].as_str().unwrap_or("refused").to_string();
                    self.refuse_text(&e);
                } else if let Some(t) = self.browser_tab(&tab) {
                    t.rearm = true;
                }
                self.dirty = true;
            }
            BrowserJob::Op { tab, what, area } => {
                if let Some(t) = self.browser_tab(&tab) {
                    t.inflight = false;
                    if ok {
                        t.sent = if what == "hide" { None } else { Some(area) };
                        t.rearm = what != "hide";
                    }
                }
                self.dirty = true; // the reconcile after the next frame may send a newer size
            }
            BrowserJob::Done => {}
            BrowserJob::Forget => self.browser_forgot(&reply),
            BrowserJob::Input => {
                self.ui.browser.input_inflight = false;
                self.browser_pump();
            }
            BrowserJob::List => {
                if !ok {
                    return;
                }
                let known: Vec<String> = self.ui.browser.tabs.iter().map(|t| t.id.clone()).collect();
                for row in reply["tabs"].as_array().into_iter().flatten() {
                    let id = row["tab"].as_str().unwrap_or("").to_string();
                    if !id.is_empty() && !known.contains(&id) {
                        let local = row["local"] == json!(true);
                        self.ui.browser.tabs.push(BTab { id, local, row: row.clone(), seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: false, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new(), waiting: false });
                    }
                }
                self.ui.browser.listed = true;
                self.dirty = true;
            }
            BrowserJob::Watch { tab } => {
                let inbox = match self.browser_tab(&tab) {
                    Some(t) => {
                        t.watching = false;
                        t.inbox.clone()
                    }
                    None => return,
                };
                if reply["gone"] == json!(true) {
                    self.ui.browser.tabs.retain(|t| t.id != tab);
                    if self.ui.browser.active.as_deref() == Some(tab.as_str()) {
                        self.ui.browser.active = None;
                    }
                } else if ok && reply["seq"].is_u64() {
                    let frame = inbox.lock().unwrap().take();
                    if let Some(t) = self.browser_tab(&tab) {
                        t.seq = reply["seq"].as_u64().unwrap_or(t.seq);
                        // the answer of `open` carried more (where to put the engine); the rows of the list do not
                        if let (Some(old), Some(new)) = (t.row.as_object_mut(), reply["tab"].as_object()) {
                            for (k, v) in new {
                                old.insert(k.clone(), v.clone());
                            }
                        } else {
                            t.row = reply["tab"].clone();
                        }
                        if let Some((n, bytes)) = frame {
                            t.frame_seq = n;
                            t.png = bytes;
                        }
                        t.rearm = true;
                    }
                    let url = reply["tab"]["url"].as_str().unwrap_or("").to_string();
                    // the address row follows the page when the page's address CHANGED and the person is not typing; a row the person cleared stays clear
                    let before = self.browser_tab(&tab).map(|t| std::mem::replace(&mut t.put_url, url.clone())).unwrap_or_default();
                    if self.browser_active() && self.ui.browser.active.as_deref() == Some(tab.as_str()) && lico_browser_view::follows_page(self.ed.text(), &before, &url) {
                        self.ed.clear(); // `put` inserts at the cursor: the old address must go first
                        self.ed.put(&url);
                    }
                    self.ui.composed = None;
                }
                self.dirty = true;
            }
        }
    }

    /// After a frame is painted: tell the host the size or the hide, and ask for the next change. One `show`/`resize` is in flight and the newest size wins; the watch is sent
    /// only now, so a frame that the painter could not use was never asked for.
    pub(crate) fn browser_after_paint(&mut self) {
        if self.socket.is_empty() {
            return;
        }
        let visible = self.browser_active();
        let active = self.ui.browser.active.clone();
        let rect = self.ui.browser.rect;
        // at scale 2 and more the page is asked for no more than 1400 x 900 pixels: a frame is decoded with its scratch beside it (two pictures of memory at the moment), and a full
        // area would take the window over 50 MiB at that moment; the tab says the size it is drawn at
        let cap = if self.ui.scale >= 2.0 { SCALE2_CAP } else { (lico_browser_view::MAX_VIEWPORT_W, lico_browser_view::MAX_VIEWPORT_H) };
        let area = ((rect[2].max(64) as u32).min(cap.0), (rect[3].max(64) as u32).min(cap.1));
        let socket = self.socket.clone();
        let proxy = self.ui.proxy.clone();
        // a page that is not there (starting, absent, failed) has no focus: the keys go to the address row, where the next address is typed; when it comes up the page has them
        let mut focus = self.ui.browser.page_focus;
        if let Some(t) = active.as_ref().and_then(|id| self.ui.browser.tabs.iter_mut().find(|t| &t.id == id)) {
            if matches!(t.row["phase"].as_str(), Some("ready" | "loading")) {
                if std::mem::take(&mut t.waiting) {
                    focus = true;
                }
            } else {
                t.waiting = true;
                focus = false;
            }
        }
        self.ui.browser.page_focus = focus;
        let mut calls: Vec<(Value, BrowserJob)> = Vec::new();
        let mut watches: Vec<(String, u64, u64)> = Vec::new();
        for t in &mut self.ui.browser.tabs {
            let want = visible && active.as_deref() == Some(t.id.as_str());
            if t.inflight {
                continue;
            }
            match (want, t.sent) {
                (true, None) if rect[2] > 0 => {
                    t.inflight = true;
                    calls.push((json!({"op": "browser.show", "tab": t.id, "width": area.0, "height": area.1}), BrowserJob::Op { tab: t.id.clone(), what: "show", area }));
                }
                (true, Some(a)) if a != area && rect[2] > 0 => {
                    t.inflight = true;
                    calls.push((json!({"op": "browser.resize", "tab": t.id, "width": area.0, "height": area.1}), BrowserJob::Op { tab: t.id.clone(), what: "resize", area }));
                }
                (false, Some(_)) => {
                    t.inflight = true;
                    t.image = None; // a hidden tab keeps its compressed frame only
                    t.decoded_seq = 0;
                    calls.push((json!({"op": "browser.hide", "tab": t.id}), BrowserJob::Op { tab: t.id.clone(), what: "hide", area }));
                }
                (true, Some(_)) if t.rearm && !t.watching => {
                    t.watching = true;
                    t.rearm = false;
                    watches.push((t.id.clone(), t.seq, t.frame_seq));
                }
                _ => {}
            }
        }
        for (request, job) in calls {
            self.browser_call(request, 8, job);
        }
        for (tab, seq, frame_seq) in watches {
            let inbox = self.browser_tab(&tab).map(|t| t.inbox.clone()).unwrap_or_default();
            let (socket, proxy) = (socket.clone(), proxy.clone());
            let request = json!({"op": "browser.watch", "tab": tab, "seq": seq, "frame_seq": frame_seq, "frame": true});
            std::thread::spawn(move || {
                let (reply, frame) = call_watch(&socket, request);
                if let Some(f) = frame {
                    *inbox.lock().unwrap() = Some(f);
                }
                if let Some(proxy) = proxy {
                    let _ = proxy.send_event(super::Ev::Async(AsyncDone { job: AsyncJob::Browser(BrowserJob::Watch { tab }), reply }));
                }
            });
        }
    }

    /// The header line of the browser view.
    pub(crate) fn browser_header(&self) -> Option<String> {
        if !self.browser_active() {
            return None;
        }
        let Some(id) = self.ui.browser.active.as_ref() else {
            let local = self.ui.browser.prompt == Some(true);
            let ask = if local { "type an address and press Enter (http or https)" } else { "search or type an address and press Enter" };
            return Some(format!("browser{} | {ask} | F1 help", if local { " (this machine)" } else { "" }));
        };
        let t = self.ui.browser.tabs.iter().find(|t| &t.id == id)?;
        let phase = t.row["phase"].as_str().unwrap_or("starting");
        let mut h = format!("browser | {} | {}", t.row["title"].as_str().filter(|s| !s.is_empty()).unwrap_or(id), phase);
        if matches!(phase, "ready" | "loading") {
            let focus = if self.ui.browser.page_focus { lico_browser_view::Focus::Page } else { lico_browser_view::Focus::Address };
            h = format!("{h} | {}", lico_browser_view::keys_hint(focus));
            if self.ui.browser.page_focus && self.preedit_active() {
                h = format!("{h} | composing: {}", self.preedit);
            }
        }
        if let Some(n) = t.row["notice"].as_str() {
            h = format!("{h} | {n}");
        }
        let drawn = (t.row["viewport"][0].as_i64().unwrap_or(0), t.row["viewport"][1].as_i64().unwrap_or(0));
        let [_, _, rw, rh] = self.ui.browser.rect;
        if drawn.0 > 0 && (drawn.0 < rw as i64 || drawn.1 < rh as i64) {
            h = format!("{h} | the page is drawn at {} x {}", drawn.0, drawn.1);
        }
        if let Some(text) = engine_text(&t.row) {
            h = format!("{h} | {text}");
        }
        if phase == "failed" {
            h = format!("{h} | {} (open an address to start the engine again)", t.row["reason"].as_str().unwrap_or("the browser engine stopped"));
        }
        if t.row["no_font"] == json!(true) {
            h = format!("{h} | no Korean font found");
        }
        if t.row["unpinned"] == json!(true) {
            h = format!("{h} | running an engine that is not the pinned release");
        }
        if t.local {
            h = format!("{h} | {}", lico_browser_view::LOCAL_NOTE);
        }
        Some(self.with_pane_cue(h, id))
    }

    /// The lines of the help (a new tab and F1), painted over the page area.
    fn browser_help_lines(&self) -> Vec<String> {
        let mut tab = lico_browser_view::Tab::new();
        if self.ui.browser.prompt == Some(true) || self.ui.browser.active.as_ref().and_then(|id| self.ui.browser.tabs.iter().find(|t| &t.id == id)).is_some_and(|t| t.local) {
            tab.engine_event(lico_browser_view::EngineEvent::LocalTab);
        }
        tab.help_lines()
    }

    /// Places the page area (`x`, the header's bottom, the prompt's top) and paints the frame in it.
    pub(crate) fn browser_paint(&mut self, buf: &mut [u32], width: usize, height: usize, place: (f32, f32, f32)) {
        let r = page_rect(place.0, place.1, place.2, width);
        self.ui.browser.rect = [r[0] as i32, r[1] as i32, (r[2] - r[0]) as i32, (r[3] - r[1]) as i32];
        let lh = self.line_h() * self.ui.scale.max(1.0);
        self.browser_paint_bar(buf, width, height, place.0, place.1 - lh, place.1);
        if !self.browser_has_frame() {
            controls::fill(buf, width, height, [r[0], r[1], r[2], r[3]], 0xFF10_151C); // the empty page
        }
        self.draw_browser_frame(buf, width, height);
        if self.ui.browser.help {
            self.draw_browser_help(buf, width, height);
        }
    }

    /// The shown tab has a picture (decoded or not yet) to paint.
    fn browser_has_frame(&self) -> bool {
        self.ui.browser.active.as_ref().and_then(|id| self.ui.browser.tabs.iter().find(|t| &t.id == id)).is_some_and(|t| t.image.is_some() || !t.png.is_empty())
    }

    /// The help lines over the page area (a new tab and F1).
    fn draw_browser_help(&mut self, buf: &mut [u32], width: usize, height: usize) {
        let [x0, y0, w, h] = self.ui.browser.rect;
        let lines = self.browser_help_lines();
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        super::controls::fill(buf, width, height, [x0 as f32, y0 as f32, (x0 + w) as f32, (y0 + h) as f32], 0xFF10_151C);
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let ink = cosmic_text::Color::rgb(0xD8, 0xDE, 0xE8);
        for (i, line) in lines.iter().enumerate() {
            controls.text(fs, swash, buf, width, height, x0 as f32 + 8.0 * s, y0 as f32 + 8.0 * s + i as f32 * lh, x0 + w, line, ink);
        }
    }

    /// Paints the shown tab's frame 1:1 at the top left of `rect` (or the help, when there is no frame yet or F1 is pressed), decoding it first when a newer one came.
    pub(crate) fn draw_browser_frame(&mut self, buf: &mut [u32], width: usize, height: usize) {
        if !self.browser_active() {
            return;
        }
        let [x0, y0, w, h] = self.ui.browser.rect;
        let Some(id) = self.ui.browser.active.clone() else { return };
        let Some(t) = self.ui.browser.tabs.iter_mut().find(|t| t.id == id) else { return };
        if t.frame_seq != t.decoded_seq && !t.png.is_empty() {
            let limits = lico_frame::Limits { max_input: lico_browser_view::MAX_FRAME_BYTES, max_pixels: (lico_browser_view::MAX_VIEWPORT_W * lico_browser_view::MAX_VIEWPORT_H) as usize };
            // the old picture is let go BEFORE the new one is decoded (the peak is one picture and the decoder's scratch, not two pictures and the scratch); a frame that does not
            // decode (too large for the window, or damaged) leaves the picture as it was
            let old = t.image.take();
            t.image = lico_frame::decode(&t.png, &limits).ok().or(old);
            t.decoded_seq = t.frame_seq;
        }
        let Some(img) = &t.image else { return };
        if self.ui.browser.help {
            return;
        }
        let (iw, ih) = (img.width as i32, img.height as i32);
        for row in 0..ih.min(h) {
            let y = y0 + row;
            if y < 0 || y as usize >= height {
                continue;
            }
            let cols = iw.min(w).min(width as i32 - x0).max(0) as usize;
            let (dst, src) = (y as usize * width + x0.max(0) as usize, row as usize * iw as usize);
            if dst + cols <= buf.len() && src + cols <= img.pixels.len() {
                buf[dst..dst + cols].copy_from_slice(&img.pixels[src..src + cols]);
            }
        }
        t.painted += 1;
        let first = buf.get(y0.max(0) as usize * width + x0.max(0) as usize).copied().unwrap_or(0);
        self.ui.browser.last_pixel = first;
    }

    /// The person's input for the page waits in a queue and goes one request at a time, in order; a mouse move that has not been sent yet is replaced by the newer one.
    fn browser_push(&mut self, request: Value) {
        let q = &mut self.ui.browser.queue;
        if request["kind"] == json!("move") && q.back().is_some_and(|r| r["kind"] == json!("move")) {
            q.pop_back();
        }
        if q.len() < 64 {
            q.push_back(request);
        }
        self.browser_pump();
    }

    fn browser_pump(&mut self) {
        if self.ui.browser.input_inflight || self.socket.is_empty() {
            return;
        }
        if let Some(request) = self.ui.browser.queue.pop_front() {
            self.ui.browser.input_inflight = true;
            self.browser_call(request, 8, BrowserJob::Input);
        }
    }

    /// Whether the person's keys and text go to the page now: the page has the focus and is the shown tab.
    pub(super) fn browser_to_page(&self) -> Option<String> {
        if self.browser_active() && self.ui.browser.page_focus { self.ui.browser.active.clone() } else { None }
    }

    /// The pointer as a point of the page, when it is inside the page area.
    pub(super) fn browser_point(&self) -> Option<(i32, i32)> {
        let [x0, y0, w, h] = self.ui.browser.rect;
        let (x, y) = (self.ui.mouse.0 as i32, self.ui.mouse.1 as i32);
        self.browser_active().then_some(()).and(self.ui.browser.active.as_ref())?;
        (x >= x0 && y >= y0 && x < x0 + w && y < y0 + h).then_some((x - x0, y - y0))
    }

    /// The pointer moved (not a consumer: the window's own hover goes on).
    pub(crate) fn browser_move(&mut self) {
        if let (Some(tab), Some((x, y))) = (self.browser_active().then(|| self.ui.browser.active.clone()).flatten(), self.browser_point()) {
            self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "move", "x": x, "y": y}));
        }
    }

    fn browser_button_event(&mut self, pressed: bool, button: &str) -> bool {
        let (Some(tab), Some((x, y))) = (self.ui.browser.active.clone(), self.browser_point()) else { return false };
        if pressed {
            self.ui.browser.page_focus = true;
            self.dirty = true;
        }
        self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": if pressed { "down" } else { "up" }, "x": x, "y": y, "button": button, "clicks": 1}));
        true
    }

    pub(crate) fn browser_mouse_button(&mut self, pressed: bool) -> bool {
        self.browser_bar_button(pressed) || self.browser_button_event(pressed, "left")
    }

    pub(crate) fn browser_other_button(&mut self, state: winit::event::ElementState, button: winit::event::MouseButton) -> bool {
        let name = match button {
            winit::event::MouseButton::Right => "right",
            winit::event::MouseButton::Middle => "middle",
            _ => return false,
        };
        self.browser_button_event(state == winit::event::ElementState::Pressed, name)
    }

    pub(crate) fn browser_wheel(&mut self, delta: &winit::event::MouseScrollDelta) -> bool {
        let (Some(tab), Some((x, y))) = (self.ui.browser.active.clone(), self.browser_point()) else { return false };
        let dy = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => -(*y as f64) * 100.0,
            winit::event::MouseScrollDelta::PixelDelta(p) => -p.y,
        };
        if dy != 0.0 {
            self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "wheel", "x": x, "y": y, "delta": dy.clamp(-10_000.0, 10_000.0)}));
        }
        true
    }

    /// Text that the input method committed: for the page when the page has the focus (the composition itself is shown in the address row only).
    pub(crate) fn browser_commit(&mut self, text: &str) -> bool {
        let Some(tab) = self.browser_to_page() else { return false };
        if !text.is_empty() {
            self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "text", "text": text}));
        }
        true
    }

    /// The text of the person's own Ctrl+V, for the page: as committed text, at most one insert long.
    pub(crate) fn browser_paste(&mut self, result: &Result<String, &'static str>) -> bool {
        let Some(tab) = self.browser_to_page() else { return false };
        match result {
            Ok(text) => {
                let t: String = text.chars().filter(|c| !c.is_control()).take(lico_browser_view::MAX_INSERT).collect();
                if !t.is_empty() {
                    self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "text", "text": t}));
                }
            }
            Err(_) => self.ui.flash = Some("Nothing to paste.".into()),
        }
        self.dirty = true;
        true
    }

    /// The keys of the browser view. Lico's own never reach the page: anything with Ctrl, Alt or the logo key, the function keys, Escape, and the address row's keys; the plain keys
    /// of the person go to the page when it has the focus (printable text as text, the editing and moving keys as key events).
    pub(crate) fn browser_hotkey(&mut self, event: &KeyEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if !self.browser_active() {
            return false;
        }
        if !event.repeat && matches!(&event.logical_key, Key::Named(NamedKey::F1)) {
            self.ui.browser.help = !self.ui.browser.help;
            self.dirty = true;
            return true;
        }
        if self.ui.browser.active.is_none() {
            return false;
        }
        let tab = self.ui.browser.active.clone().unwrap_or_default();
        let (alt, ctrl) = (self.ui.mods.alt_key(), self.ui.mods.control_key());
        if !event.repeat {
            let nav = match &event.logical_key {
                Key::Named(NamedKey::ArrowLeft) if alt => Some("back"),
                Key::Named(NamedKey::ArrowRight) if alt => Some("forward"),
                Key::Named(NamedKey::F5) => Some("reload"),
                _ => None,
            };
            if let Some(action) = nav {
                self.browser_nav(&tab, action);
                return true;
            }
            if ctrl && matches!(&event.logical_key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("w")) {
                self.browser_close_ui(&tab);
                return true;
            }
            if ctrl && matches!(&event.logical_key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("l")) {
                self.browser_focus_address(); // the bar takes the keys, with the address selected
                return true;
            }
        }
        if !self.ui.browser.page_focus || ctrl || alt || self.ui.mods.super_key() || self.preedit_active() {
            return false;
        }
        let named = match &event.logical_key {
            Key::Named(NamedKey::Enter) => Some(("Enter", "Enter")),
            Key::Named(NamedKey::Backspace) => Some(("Backspace", "Backspace")),
            Key::Named(NamedKey::Delete) => Some(("Delete", "Delete")),
            Key::Named(NamedKey::Tab) => Some(("Tab", "Tab")),
            Key::Named(NamedKey::ArrowLeft) => Some(("ArrowLeft", "ArrowLeft")),
            Key::Named(NamedKey::ArrowRight) => Some(("ArrowRight", "ArrowRight")),
            Key::Named(NamedKey::ArrowUp) => Some(("ArrowUp", "ArrowUp")),
            Key::Named(NamedKey::ArrowDown) => Some(("ArrowDown", "ArrowDown")),
            Key::Named(NamedKey::Home) => Some(("Home", "Home")),
            Key::Named(NamedKey::End) => Some(("End", "End")),
            Key::Named(NamedKey::PageUp) => Some(("PageUp", "PageUp")),
            Key::Named(NamedKey::PageDown) => Some(("PageDown", "PageDown")),
            _ => None,
        };
        if let Some((key, code)) = named {
            self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "key_down", "key": key, "code": code}));
            self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "key_up", "key": key, "code": code}));
            return true;
        }
        if let Some(text) = event.text.as_ref().filter(|t| !t.is_empty() && !t.chars().any(char::is_control)) {
            let text = text.to_string();
            self.browser_push(json!({"op": "browser.input", "tab": tab, "kind": "text", "text": text}));
            return true;
        }
        false
    }

    fn preedit_active(&self) -> bool {
        !self.preedit.is_empty()
    }

    /// For the `ui` command on standard input: what a test reads.
    pub(crate) fn browser_json(&self) -> Value {
        let b = &self.ui.browser;
        json!({
            "active": b.active, "prompt": b.prompt, "page_focus": b.page_focus, "queued": b.queue.len(), "rect": b.rect, "shown": self.browser_active(), "help": b.help, "last_pixel": b.last_pixel, "bar": self.browser_bar_json(), "page": if b.help { "help" } else if self.browser_has_frame() { "frame" } else { "blank" }, "listed": b.listed,
            "header": self.browser_header(), "row": b.row, "pointer": if b.pointer_on_page { "crosshair" } else { "default" },
            "help_lines": if b.help && self.browser_active() { self.browser_help_lines() } else { Vec::new() },
            "tabs": b.tabs.iter().map(|t| json!({
                "id": t.id, "local": t.local, "caption": caption(t), "seq": t.seq, "frame_seq": t.frame_seq, "decoded": t.image.is_some(), "decoded_seq": t.decoded_seq,
                "watching": t.watching, "sent": t.sent, "painted": t.painted, "png_bytes": t.png.len(), "row": t.row,
            })).collect::<Vec<_>>(),
        })
    }
}

/// The painted rectangle of the page, from the header's bottom to the prompt's top; the frame is never larger than the host's viewport cap.
pub(crate) fn page_rect(x0: f32, header_bottom: f32, prompt_top: f32, width: usize) -> Rect {
    [x0, header_bottom, width as f32, prompt_top.max(header_bottom)]
}
