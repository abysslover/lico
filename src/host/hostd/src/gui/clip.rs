//! The selection code of the window: the text of a selection fetched on a connection of its own, and the owner of the CLIPBOARD selection, which holds a text
//! or a PNG picture (plan M02-P04, S04b). The reader that also takes an image is `crate::clipboard`; the owner is asked to write by the host (`on_clip_line`).

use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};
use winit::event::{ElementState, MouseButton};
use x11rb::protocol::xproto::Atom;

/// The text of a selection (`CLIPBOARD` or `PRIMARY`) as UTF-8, asked for through our own helper window on a connection of its own, within 500 ms.
/// An incremental transfer or more than `limit` bytes is refused rather than cut. On `display` (`None`: `$DISPLAY`; the unit tests name a private one).
pub(in crate::gui) fn fetch_selection(display: Option<&str>, name: &[u8], limit: usize) -> Result<String, &'static str> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, WindowClass};
    use x11rb::protocol::Event;
    let (conn, screen) = x11rb::connect(display).map_err(|_| "empty")?;
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
                    let reply = conn.get_property(true, win, property, AtomEnum::ANY, 0, (limit / 4 + 1) as u32).map_err(|_| "empty")?.reply().map_err(|_| "empty")?;
                    if reply.type_ == incr || reply.bytes_after > 0 || reply.value.len() > limit {
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

// ---- the owner of the CLIPBOARD selection -----------------------------------------------------------------------------------

/// A value over this many bytes is served by the incremental transfer (INCR) from the owner side; below it, in one property. (It is also kept under
/// what one request to the X server may carry, which is what makes the limit real on a server without the big-requests extension.)
pub(super) const INCR_ABOVE: usize = 256 * 1024;
/// The most bytes of one chunk of an incremental transfer.
pub(super) const CHUNK: usize = 128 * 1024;
/// A requestor that does not delete the property for this long is dropped; the owner goes on serving the others.
pub(super) const STALL: Duration = Duration::from_secs(5);

/// Where the bytes of a job come from: a text in hand, or a picture file that the owner opens itself.
pub(in crate::gui) enum Source {
    Text(String),
    File(PathBuf),
}

/// A request to own X11 selection(s). `req` is the id the host gave a `lico clipboard write`: when it is there, the owner reports the outcome to the
/// window (which tells the host); a copy by the person's own key (`Ctrl+C`, `Ctrl+Shift+C`, toolbar `Copy`) owns `CLIPBOARD` without touching `PRIMARY`.
pub(in crate::gui) struct Job {
    pub req: Option<String>,
    pub source: Source,
    pub clipboard: bool,
    pub primary: bool,
}

impl Job {
    pub(super) fn text(text: String) -> Job {
        Job { req: None, source: Source::Text(text), clipboard: true, primary: false }
    }
}

/// What the owner holds: a text or a PNG picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Held {
    Text(Arc<Vec<u8>>),
    Png(Arc<Vec<u8>>),
}

/// The atoms the owner answers with.
#[derive(Clone, Copy, Debug)]
pub(super) struct Atoms {
    pub targets: Atom,
    pub utf8: Atom,
    pub text: Atom,
    pub png: Atom,
}

const STRING_ATOM: Atom = 31; // the predefined atom STRING

/// What to answer to a request for `target` while `held` is held.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Reply {
    Targets(Vec<Atom>),
    Data { ptype: Atom, data: Arc<Vec<u8>> },
    Refuse,
}

/// The pure part of the owner: `TARGETS` lists what is held (a text: the four forms it has always listed; a picture: `TARGETS` and `image/png`), a
/// form that is held is served, and a form that is not held is refused (`property = NONE` for the requestor).
pub(super) fn answer(held: &Held, target: Atom, atoms: &Atoms) -> Reply {
    if target == atoms.targets {
        return Reply::Targets(match held {
            Held::Text(_) => vec![atoms.targets, atoms.utf8, STRING_ATOM, atoms.text],
            Held::Png(_) => vec![atoms.targets, atoms.png],
        });
    }
    match held {
        Held::Text(data) if target == atoms.utf8 || target == atoms.text || target == STRING_ATOM => Reply::Data { ptype: atoms.utf8, data: data.clone() },
        Held::Png(data) if target == atoms.png => Reply::Data { ptype: atoms.png, data: data.clone() },
        _ => Reply::Refuse,
    }
}

/// What one step of an incremental transfer does, after the requestor deleted the property.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// Write this range of the value as the next chunk (an empty range is the last chunk, which ends the transfer).
    Chunk(std::ops::Range<usize>),
    /// The last chunk was deleted: the transfer is over.
    Done,
}

/// One incremental transfer to one requestor, as a state machine on the number of bytes sent. The first `step` is called when the requestor deletes the
/// property that announced INCR.
#[derive(Debug)]
pub(super) struct Transfer {
    pub requestor: u32,
    pub property: Atom,
    pub ptype: Atom,
    pub data: Arc<Vec<u8>>,
    sent: usize,
    ended: bool,
    pub last: Instant,
}

impl Transfer {
    pub(super) fn new(requestor: u32, property: Atom, ptype: Atom, data: Arc<Vec<u8>>, now: Instant) -> Transfer {
        Transfer { requestor, property, ptype, data, sent: 0, ended: false, last: now }
    }

    pub(super) fn step(&mut self, now: Instant) -> Step {
        self.last = now;
        if self.ended {
            return Step::Done;
        }
        let from = self.sent;
        let to = (from + CHUNK).min(self.data.len());
        self.sent = to;
        if from == to {
            self.ended = true; // the empty chunk
        }
        Step::Chunk(from..to)
    }

    pub(super) fn stalled(&self, now: Instant) -> bool {
        now.duration_since(self.last) >= STALL
    }
}

/// The value of a job, and what to tell the host about it.
struct Taken {
    held: Held,
    kind: &'static str,
    bytes: usize,
    size: Option<(u32, u32)>,
}

fn resolve(source: Source) -> Result<Taken, (&'static str, String)> {
    use crate::clipboard::open::{open_regular, read_checked, MAX_IMAGE_FILE};
    match source {
        Source::Text(text) => {
            let bytes = text.into_bytes();
            Ok(Taken { bytes: bytes.len(), held: Held::Text(Arc::new(bytes)), kind: "text", size: None })
        }
        Source::File(path) => {
            let fault = |f: crate::clipboard::open::Fault| (f.reason(), f.detail());
            let opened = open_regular(&path, 1, MAX_IMAGE_FILE).map_err(fault)?;
            let bytes = read_checked(opened, MAX_IMAGE_FILE).map_err(fault)?;
            let size = crate::clipboard::png_size(&bytes).map_err(|e| (e.reason, e.detail))?;
            Ok(Taken { bytes: bytes.len(), held: Held::Png(Arc::new(bytes)), kind: "image", size: Some(size) })
        }
    }
}

/// The line the window's loop takes for an outcome (`Ev::Sock`, handled by `App::on_clip_line`): the window sends the host's `clipboard.done`.
fn outcome_line(req: &str, outcome: &Result<Taken, (&'static str, String)>) -> String {
    let value = match outcome {
        Ok(t) => {
            let mut v = serde_json::json!({"ev": "clip.local", "req": req, "ok": true, "kind": t.kind, "bytes": t.bytes});
            if let Some((w, h)) = t.size {
                v["width"] = w.into();
                v["height"] = h.into();
            }
            v
        }
        Err((reason, _)) => serde_json::json!({"ev": "clip.local", "req": req, "ok": false, "error": reason}),
    };
    value.to_string()
}

/// Owns the CLIPBOARD selection with what it was last given (a text, or a PNG picture), until another client takes it. Never blocks: a requestor of a
/// large value is served chunk by chunk (INCR) as it deletes the property, one that stalls for `STALL` is dropped, and the others are served meanwhile.
/// A job with a request id is confirmed to the window only after the server says this window's helper window owns the selection.
pub(super) fn clipboard_server(rx: mpsc::Receiver<Job>, proxy: Option<winit::event_loop::EventLoopProxy<super::super::Ev>>) {
    let report = move |line: String| {
        if let Some(proxy) = &proxy {
            let _ = proxy.send_event(super::super::Ev::Sock(line));
        }
    };
    serve(None, rx, &report);
}

/// The owner on the display `display` (`None`: `$DISPLAY`); `report` takes the line of each outcome of a job that has a request id.
fn serve(display: Option<&str>, rx: mpsc::Receiver<Job>, report: &dyn Fn(String)) {
    serve_with(display, rx, report, &|| {});
}

/// `serve`, with `between` called after the window asked the server for the selection and before it asks who owns it (the tests make another client take
/// the selection exactly there: the race the check of ownership is for).
fn serve_with(display: Option<&str>, rx: mpsc::Receiver<Job>, report: &dyn Fn(String), between: &dyn Fn()) {
    use x11rb::connection::{Connection, RequestConnection};
    use x11rb::protocol::xproto::{AtomEnum, ChangeWindowAttributesAux, ConnectionExt, CreateWindowAux, EventMask, PropMode, Property, SelectionNotifyEvent, WindowClass, SELECTION_NOTIFY_EVENT};
    use x11rb::protocol::Event;
    use x11rb::wrapper::ConnectionExt as _;
    let Ok((conn, screen)) = x11rb::connect(display) else { return };
    let root = conn.setup().roots[screen].root;
    let Ok(win) = conn.generate_id() else { return };
    if conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).is_err() {
        return;
    }
    let atom = |name: &[u8]| conn.intern_atom(false, name).ok().and_then(|c| c.reply().ok()).map(|r| r.atom);
    let (Some(clipboard), Some(utf8), Some(targets), Some(text_atom), Some(png), Some(incr)) =
        (atom(b"CLIPBOARD"), atom(b"UTF8_STRING"), atom(b"TARGETS"), atom(b"TEXT"), atom(b"image/png"), atom(b"INCR"))
    else {
        return;
    };
    let primary: Atom = AtomEnum::PRIMARY.into();
    let atoms = Atoms { targets, utf8, text: text_atom, png };
    let incr_above = INCR_ABOVE.min(conn.maximum_request_bytes().saturating_sub(1024));
    let mut held_clip: Option<Held> = None;
    let mut held_primary: Option<Held> = None;
    let mut transfers: Vec<Transfer> = Vec::new();
    let mut owned_once = false;
    let tell = |req: &Option<String>, outcome: &Result<Taken, (&'static str, String)>| {
        if let Some(req) = req {
            report(outcome_line(req, outcome));
        }
    };
    let release = |conn: &x11rb::rust_connection::RustConnection, t: &Transfer| {
        let _ = conn.change_window_attributes(t.requestor, &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT));
    };
    loop {
        // a new value: owned first, confirmed after the server says so
        // before the first value there is nothing to serve: wait for it (afterwards the loop goes on, so that a request that races with a release is answered)
        let job = if held_clip.is_none() && held_primary.is_none() && transfers.is_empty() && !owned_once {
            match rx.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            }
        } else {
            rx.try_recv().ok()
        };
        let mut next = job;
        while let Some(Job { req, source, clipboard: want_clip, primary: want_prim }) = next.take() {
            let outcome = resolve(source);
            let outcome = match outcome {
                Ok(taken) => {
                    let mut clip_ok = true;
                    if want_clip {
                        let _ = conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME);
                        let _ = conn.flush();
                        between();
                        let owner = conn.get_selection_owner(clipboard).ok().and_then(|c| c.reply().ok()).map(|r| r.owner);
                        if owner == Some(win) {
                            held_clip = Some(taken.held.clone());
                            owned_once = true;
                        } else {
                            clip_ok = false;
                        }
                    }
                    if want_prim {
                        let _ = conn.set_selection_owner(win, primary, x11rb::CURRENT_TIME);
                        let _ = conn.flush();
                        let owner = conn.get_selection_owner(primary).ok().and_then(|c| c.reply().ok()).map(|r| r.owner);
                        if owner == Some(win) {
                            held_primary = Some(taken.held.clone());
                            owned_once = true;
                        }
                    }
                    if clip_ok {
                        Ok(taken)
                    } else {
                        Err(("x_error", "the window could not take the selection".to_string()))
                    }
                }
                Err(e) => Err(e),
            };
            tell(&req, &outcome);
            next = rx.try_recv().ok();
        }
        let mut busy = false;
        match conn.poll_for_event() {
            Ok(Some(Event::SelectionRequest(r))) => {
                busy = true;
                let mut property = if r.property == x11rb::NONE { r.target } else { r.property };
                let active = if r.selection == clipboard {
                    held_clip.as_ref()
                } else if r.selection == primary {
                    held_primary.as_ref()
                } else {
                    None
                };
                let reply = active.map_or(Reply::Refuse, |h| answer(h, r.target, &atoms));
                match reply {
                    Reply::Targets(list) => {
                        let _ = conn.change_property32(PropMode::REPLACE, r.requestor, property, AtomEnum::ATOM, &list);
                    }
                    Reply::Data { ptype, data } if data.len() <= incr_above => {
                        let _ = conn.change_property8(PropMode::REPLACE, r.requestor, property, ptype, &data);
                    }
                    Reply::Data { ptype, data } => {
                        // the incremental transfer: announce INCR with the size, then one chunk each time the requestor deletes the property
                        if let Some(old) = transfers.iter().position(|t| t.requestor == r.requestor && t.property == property) {
                            release(&conn, &transfers.remove(old));
                        }
                        let _ = conn.change_window_attributes(r.requestor, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE));
                        let _ = conn.change_property32(PropMode::REPLACE, r.requestor, property, incr, &[data.len().min(u32::MAX as usize) as u32]);
                        transfers.push(Transfer::new(r.requestor, property, ptype, data, Instant::now()));
                    }
                    Reply::Refuse => property = x11rb::NONE,
                }
                let notify = SelectionNotifyEvent { response_type: SELECTION_NOTIFY_EVENT, sequence: 0, time: r.time, requestor: r.requestor, selection: r.selection, target: r.target, property };
                let _ = conn.send_event(false, r.requestor, EventMask::NO_EVENT, notify);
                let _ = conn.flush();
            }
            Ok(Some(Event::PropertyNotify(e))) if e.state == Property::DELETE => {
                busy = true;
                if let Some(at) = transfers.iter().position(|t| t.requestor == e.window && t.property == e.atom) {
                    match transfers[at].step(Instant::now()) {
                        Step::Chunk(range) => {
                            let t = &transfers[at];
                            let _ = conn.change_property8(PropMode::REPLACE, t.requestor, t.property, t.ptype, &t.data[range]);
                            let _ = conn.flush();
                        }
                        Step::Done => release(&conn, &transfers.remove(at)),
                    }
                }
            }
            Ok(Some(Event::SelectionClear(e))) => {
                busy = true;
                if e.selection == clipboard {
                    held_clip = None; // another client owns CLIPBOARD now
                } else if e.selection == primary {
                    held_primary = None; // another client owns PRIMARY now
                }
            }
            Ok(Some(_)) => busy = true,
            Ok(None) => {}
            Err(_) => return,
        }
        let now = Instant::now();
        let mut at = 0;
        while at < transfers.len() {
            if transfers[at].stalled(now) {
                release(&conn, &transfers.remove(at)); // dropped; whatever it holds of the value is freed
            } else {
                at += 1;
            }
        }
        if !busy {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

/// Is the point inside the box as the last frame drew it? Both are physical pixels (the box is placed from the scaled layout, the mouse comes from the
/// window system), so no scale is needed. The left and top edges belong to the box, the right and bottom ones do not (as for every control).
pub(in crate::gui) fn in_box(rect: Option<[f32; 4]>, x: f32, y: f32) -> bool {
    rect.map_or(false, |r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
}

/// Does this mouse event ask for the primary selection to be pasted? Only the PRESS of the MIDDLE button, over the box, with no menu open.
pub(in crate::gui) fn wants_primary(state: ElementState, button: MouseButton, menu_open: bool, rect: Option<[f32; 4]>, at: (f64, f64)) -> bool {
    button == MouseButton::Middle && state == ElementState::Pressed && !menu_open && in_box(rect, at.0 as f32, at.1 as f32)
}

impl super::super::App {
    /// A press of the middle button over the input box: the same paste as Shift+Insert (the primary selection, text only, at the caret, never sent).
    /// Anywhere else, on the release and for the other buttons nothing happens here (false: as before this existed). It does not touch the selected lines.
    pub(super) fn middle_press(&mut self, state: ElementState, button: MouseButton) -> bool {
        if state == ElementState::Pressed && !self.ui.menu_open.is_some() && self.sidebar_shown() && self.ui.sidebar.tab == 0 {
            match button {
                MouseButton::Back => { self.sidebar_nav_step(-1); return true; }
                MouseButton::Forward => { self.sidebar_nav_step(1); return true; }
                _ => {}
            }
        }
        if !wants_primary(state, button, self.ui.menu_open.is_some(), self.ed.rect, self.ui.mouse) {
            return false;
        }
        self.start_paste(b"PRIMARY");
        true
    }

    pub(crate) fn paste_notice(&mut self, text: &str) {
        match self.ui.panel.as_mut() {
            Some(p) => p.notice = text.to_string(),
            None => self.ui.flash = Some(text.to_string()),
        }
    }

    /// Selected lines in either an open right-pane panel (`Send`, `Files`, `Accounts`, `Start`, etc.) or the active conversation transcript.
    pub(crate) fn selection_text(&self) -> Option<String> {
        let (a, b) = self.ui.sel?;
        let (lo, hi) = (a.min(b), a.max(b));
        if let Some((_, body)) = self.panel_view() {
            let lines: Vec<&str> = (lo..=hi).filter_map(|i| body.get(i).map(String::as_str)).collect();
            return if lines.is_empty() { None } else { Some(lines.join("\n")) };
        }
        let view = self.view.as_ref()?;
        let vis = crate::gui::transcript::compute_visible_transcript(&view.lines, view.lines.len(), self.ui.show_events);
        let lines: Vec<&str> = vis
            .iter()
            .filter(|(raw_idx, _, _, _)| *raw_idx >= lo && *raw_idx <= hi)
            .map(|(_, _, text, _)| text.as_str())
            .collect();
        if lines.is_empty() { None } else { Some(lines.join("\n")) }
    }

    /// Left-button press inside the right-hand main area (`x >= list_w`, outside controls and editor).
    pub(crate) fn main_mouse_press(&mut self, x: f32, y: f32) {
        let idx = self.line_at(y);
        let in_panel = self.panel_view().is_some();
        let shift = self.ui.mods.shift_key();
        if in_panel {
            if shift {
                if let Some(i) = idx {
                    let a = self.ui.sel.map_or(i, |(a, _)| a);
                    self.ui.sel = Some((a, i));
                    self.ui.dragging = true;
                    self.ui.drag_moved = true;
                }
                self.ui.drag_origin = None;
            } else {
                self.ui.drag_origin = Some((x, y, idx, true));
                self.ui.drag_moved = false;
                self.ui.dragging = false;
            }
        } else {
            self.ui.drag_origin = Some((x, y, idx, false));
            self.ui.drag_moved = shift;
            match idx {
                Some(i) => {
                    let a = if shift { self.ui.sel.map_or(i, |(a, _)| a) } else { i };
                    self.ui.sel = Some((a, i));
                    self.ui.dragging = true;
                }
                None => {
                    self.ui.sel = None;
                    self.ui.dragging = false;
                }
            }
        }
    }

    /// Cursor movement during a potential or active right-pane drag selection.
    pub(crate) fn main_mouse_drag(&mut self, x: f32, y: f32) {
        let thresh = 4.0 * self.ui.scale.max(1.0);
        if let Some((ox, oy, orig_idx, in_panel)) = self.ui.drag_origin {
            if !self.ui.drag_moved && (x - ox).hypot(y - oy) >= thresh {
                self.ui.drag_moved = true;
                if in_panel && !self.ui.dragging {
                    if let Some(start) = orig_idx.or_else(|| self.line_at(y)) {
                        self.ui.sel = Some((start, start));
                        self.ui.dragging = true;
                        self.dirty = true;
                    }
                }
            }
        }
        if self.ui.dragging {
            if let Some(idx) = self.line_at(y) {
                if let Some((a, cur_b)) = self.ui.sel {
                    if cur_b != idx {
                        self.ui.sel = Some((a, idx));
                        self.dirty = true;
                    }
                }
            }
        }
    }

    /// Left-button release: activates a stationary single click in panels, or auto-copies a finished mouse selection onto `CLIPBOARD`.
    pub(crate) fn main_mouse_release(&mut self) {
        let origin = self.ui.drag_origin.take();
        let moved = std::mem::take(&mut self.ui.drag_moved);
        let was_line_drag = std::mem::take(&mut self.ui.dragging);
        let was_ed_drag = self.ed.selecting;
        self.ed.finish_drag();
        if let Some((_, _, orig_idx, true)) = origin {
            if !moved {
                self.ui.sel = None;
                if let Some(idx) = orig_idx {
                    let _ = self.form_click(idx) || self.panel_click(idx);
                }
                self.dirty = true;
                return;
            }
        } else if let Some((_, _, Some(idx), false)) = origin {
            if !moved && !self.ui.mods.shift_key() && self.try_jump_call_from_line(idx) {
                return;
            }
        }
        let multi_line = self.ui.sel.is_some_and(|(a, b)| a != b);
        if was_line_drag && (moved || multi_line) && self.selection_text().is_some() {
            self.copy_selection();
        } else if was_ed_drag && self.ed.has_selection() {
            self.copy_selection();
        } else if was_line_drag || was_ed_drag {
            self.dirty = true;
        }
    }

    /// The sender of the owner thread, started on first use. (It lives in `ui.clip_tx`; the thread gets a proxy of the event loop to report to.)
    pub(super) fn clip_sender(&mut self) -> &mpsc::Sender<Job> {
        let proxy = self.ui.proxy.clone();
        self.ui.clip_tx.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<Job>();
            thread::spawn(move || clipboard_server(rx, proxy));
            tx
        })
    }

    /// A line of the host that is about the clipboard (`true`: it was, and is handled). `clip.write` is the host forwarding a `lico clipboard write` that
    /// a person ran: the value is given to the owner thread, which takes the selection and reports. `clip.local` is that thread's report, which comes
    /// back through the event loop (so that the host connection is written by one thread only) and is sent to the host as `clipboard.done`.
    pub(in crate::gui) fn on_clip_line(&mut self, line: &str) -> bool {
        if !line.contains("\"ev\":\"clip.") {
            return false;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return false };
        let req = v["req"].as_str().filter(|r| !r.is_empty() && r.len() <= 64 && r.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')).map(str::to_string);
        match (v["ev"].as_str(), req) {
            (Some("clip.write"), Some(req)) => {
                let source = match (v["kind"].as_str(), v["text"].as_str(), v["path"].as_str()) {
                    (Some("text"), Some(text), None) => Some(Source::Text(text.to_string())),
                    (Some("image"), None, Some(path)) if path.starts_with('/') => Some(Source::File(PathBuf::from(path))),
                    _ => None,
                };
                let sent = source
                    .map(|source| self.clip_sender().send(Job { req: Some(req.clone()), source, clipboard: true, primary: false }).is_ok())
                    .unwrap_or(false);
                if !sent {
                    let failed = serde_json::json!({"op": "clipboard.done", "req": req, "ok": false, "error": "x_error"});
                    self.send(&failed.to_string());
                }
                true
            }
            (Some("clip.local"), Some(req)) => {
                let mut done = serde_json::json!({"op": "clipboard.done", "req": req, "ok": v["ok"].as_bool() == Some(true)});
                for key in ["kind", "bytes", "width", "height", "error"] {
                    if !v[key].is_null() {
                        done[key] = v[key].clone();
                    }
                }
                self.send(&done.to_string());
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "clip_tests.rs"]
mod tests;
