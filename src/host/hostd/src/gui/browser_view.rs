//! The browser tab in the window (M17-P07). A tab is one of the host's browser tabs (`browser.*`, adapter `browser/`): the window shows its newest frame, edits its address on the
//! window's own input line, and asks the host again only after it has painted. The host keeps the model; the window keeps what it needs to paint: the tab's row, the newest
//! compressed frame, the decoded picture of the SHOWN tab only (freed when the tab is hidden), and one blocking `browser.watch` at a time.
//!
//! Nothing polls: the next watch is sent after the paint of the frame that the last one brought, so a frame newer than the painter can use is never sent; at most one `show` or
//! `resize` is in flight and the newest size wins. A new tab is made on the person's own action only (the `+B` buttons, the Session menu): never from a page or a link.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};

use cosmic_text::{Buffer, Metrics, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;

use super::controls::Rect;
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
fn call_watch(socket: &str, request: Value) -> (Value, Option<(u64, Vec<u8>)>) {
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

    fn browser_enter(&mut self, key: &str) {
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
        self.ui.browser.help = true;
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
                }
                self.browser_call(request, 30, BrowserJob::Open { local });
            }
            (None, Some(tab)) => self.browser_call(json!({"op": "browser.go", "tab": tab, "address": text}), 30, BrowserJob::Go { tab }),
            _ => {}
        }
        self.dirty = true;
    }

    fn refuse_text(&mut self, error: &str) {
        let why = match error {
            "address_empty" => "type an address".to_string(),
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
                self.ui.browser.tabs.push(BTab { id: id.clone(), local, row: reply.clone(), seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: true, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new() });
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
            BrowserJob::List => {
                if !ok {
                    return;
                }
                let known: Vec<String> = self.ui.browser.tabs.iter().map(|t| t.id.clone()).collect();
                for row in reply["tabs"].as_array().into_iter().flatten() {
                    let id = row["tab"].as_str().unwrap_or("").to_string();
                    if !id.is_empty() && !known.contains(&id) {
                        let local = row["local"] == json!(true);
                        self.ui.browser.tabs.push(BTab { id, local, row: row.clone(), seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: false, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new() });
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
                    let changed = self.browser_tab(&tab).is_some_and(|t| std::mem::replace(&mut t.put_url, url.clone()) != url);
                    if changed && self.browser_active() && self.ui.browser.active.as_deref() == Some(tab.as_str()) && self.ed.is_empty() && !url.is_empty() {
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
        let area = ((rect[2].max(64)) as u32, (rect[3].max(64)) as u32);
        let socket = self.socket.clone();
        let proxy = self.ui.proxy.clone();
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
            return Some(format!("browser{} | type an address and press Enter (http or https)", if local { " (this machine)" } else { "" }));
        };
        let t = self.ui.browser.tabs.iter().find(|t| &t.id == id)?;
        let phase = t.row["phase"].as_str().unwrap_or("starting");
        let mut h = format!("browser | {} | {}", t.row["title"].as_str().filter(|s| !s.is_empty()).unwrap_or(id), phase);
        if let Some(n) = t.row["notice"].as_str() {
            h = format!("{h} | {n}");
        }
        if t.row["viewport_capped"] == json!(true) {
            let v = &t.row["viewport"];
            h = format!("{h} | the page is drawn at {} x {}", v[0], v[1]);
        }
        if phase == "absent" {
            h = format!("{h} | the engine is not installed: {} (SHA-256 {}), put it in {}", t.row["download"].as_str().unwrap_or(""), t.row["expected_sha256"].as_str().or_else(|| t.row["engine_sha256"].as_str()).unwrap_or(""), t.row["directory"].as_str().unwrap_or("the engine folder"));
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
        Some(h)
    }

    /// The lines of the help (a new tab and F1), painted over the page area.
    fn browser_help_lines(&self) -> Vec<String> {
        let mut tab = lico_browser_view::Tab::new();
        if self.ui.browser.prompt == Some(true) || self.ui.browser.active.as_ref().and_then(|id| self.ui.browser.tabs.iter().find(|t| &t.id == id)).is_some_and(|t| t.local) {
            tab.engine_event(lico_browser_view::EngineEvent::LocalTab);
        }
        tab.help_lines()
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
            if let Ok(img) = lico_frame::decode(&t.png, &limits) {
                t.image = Some(img); // a frame that does not decode (too large for the window, or damaged) leaves the picture as it was
            }
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

    /// The keys of the browser view that are Lico's own: they are never sent to the page.
    pub(crate) fn browser_hotkey(&mut self, event: &KeyEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if !self.browser_active() || event.repeat || self.ui.browser.active.is_none() {
            return false;
        }
        let tab = self.ui.browser.active.clone().unwrap_or_default();
        let alt = self.ui.mods.alt_key();
        let nav = match &event.logical_key {
            Key::Named(NamedKey::ArrowLeft) if alt => Some("back"),
            Key::Named(NamedKey::ArrowRight) if alt => Some("forward"),
            Key::Named(NamedKey::F5) => Some("reload"),
            _ => None,
        };
        if let Some(action) = nav {
            self.browser_call(json!({"op": "browser.nav", "tab": tab, "action": action}), 8, BrowserJob::Done);
            return true;
        }
        if matches!(&event.logical_key, Key::Named(NamedKey::F1)) {
            self.ui.browser.help = !self.ui.browser.help;
            self.dirty = true;
            return true;
        }
        if self.ui.mods.control_key() && matches!(&event.logical_key, Key::Character(c) if c.as_str().eq_ignore_ascii_case("w")) {
            self.browser_close_ui(&tab);
            return true;
        }
        false
    }

    /// For the `ui` command on standard input: what a test reads.
    pub(crate) fn browser_json(&self) -> Value {
        let b = &self.ui.browser;
        json!({
            "active": b.active, "prompt": b.prompt, "rect": b.rect, "shown": self.browser_active(), "help": b.help, "last_pixel": b.last_pixel, "listed": b.listed,
            "header": self.browser_header(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixListener;

    fn tab(row: Value, local: bool) -> BTab {
        BTab { id: "b1".into(), local, row, seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: false, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new() }
    }

    #[test]
    fn a_caption_is_the_title_or_the_host_marks_the_local_tab_loading_and_notices_and_is_short() {
        assert_eq!(caption(&tab(json!({"title": "A page", "url": "https://a.test/x"}), false)), "\u{1f310} A page");
        assert_eq!(caption(&tab(json!({"title": "", "url": "https://a.test/x"}), false)), "\u{1f310} a.test/x");
        assert!(caption(&tab(json!({"title": "T", "loading": true}), false)).ends_with(" \u{25cf}"));
        assert!(caption(&tab(json!({"title": "T", "notice": "n"}), false)).ends_with(" !"));
        assert!(caption(&tab(json!({"title": "T"}), true)).contains("\u{b7}L "), "a local tab is marked");
        let long = caption(&tab(json!({"title": "x".repeat(80)}), false));
        assert!(long.chars().count() <= 26 && long.contains('\u{2026}'), "{long}");
        assert!(caption(&tab(json!({}), false)).contains("b1"), "no title and no address: the tab's id");
    }

    #[test]
    fn the_page_area_runs_from_the_header_to_the_prompt_and_is_never_negative() {
        assert_eq!(page_rect(10.0, 88.0, 700.0, 1200), [10.0, 88.0, 1200.0, 700.0]);
        assert_eq!(page_rect(10.0, 88.0, 50.0, 1200), [10.0, 88.0, 1200.0, 88.0], "a prompt above the header leaves no area");
    }

    fn serve_one(reply: String) -> (String, std::thread::JoinHandle<String>) {
        let dir = std::env::temp_dir().join(format!("lico_bv_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let handle = std::thread::spawn(move || {
            let (mut c, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let n = c.read(&mut buf).unwrap();
            let _ = writeln!(c, "{reply}");
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        (path.to_string_lossy().into_owned(), handle)
    }

    #[test]
    fn a_watch_answer_gives_the_frame_as_bytes_and_the_reply_without_the_text_of_it() {
        let png = vec![137u8, 80, 78, 71, 1, 2, 3, 250];
        let (socket, server) = serve_one(json!({"ok": 1, "seq": 7, "frame_seq": 4, "png": crate::base64::base64_encode(&png), "tab": {"url": "u"}}).to_string());
        let (reply, frame) = call_watch(&socket, json!({"op": "browser.watch", "tab": "b1", "seq": 0}));
        assert!(server.join().unwrap().contains("browser.watch"));
        assert_eq!((reply["seq"].clone(), reply.get("png").is_none()), (json!(7), true), "the text of the frame is not kept in the reply: {reply}");
        assert_eq!(frame, Some((4, png)));
        let (socket, server) = serve_one(json!({"ok": 1, "hidden": true}).to_string());
        let (reply, frame) = call_watch(&socket, json!({"op": "browser.watch"}));
        server.join().unwrap();
        assert_eq!((reply["hidden"].clone(), frame), (json!(true), None));
        let (reply, frame) = call_watch("/nonexistent/socket", json!({}));
        assert_eq!((reply["ok"].clone(), reply["error"].clone(), frame), (json!(0), json!("host_unreachable"), None));
        let (socket, server) = serve_one("not json".into());
        let (reply, _) = call_watch(&socket, json!({}));
        server.join().unwrap();
        assert_eq!(reply["error"], json!("host_unreachable"), "a garbled line is a failure, not a frame");
    }
}
