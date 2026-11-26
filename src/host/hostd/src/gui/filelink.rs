//! `M24-P11e`: a file name in a conversation that is really there is a link. With the pointer over it the name is underlined and the pointer is a hand; a click opens the file in the
//! window's file viewer (a folder as a list), at the line when the name carries one (`src/a.rs:12`).
//!
//! The name is read from the text of the line under the pointer in the last frame's layout (nothing is sent anywhere); the machine is asked whether it is there (`std::fs::metadata`
//! on a short-lived thread that the window waits 30 ms for, so that a network mount that does not answer never freezes it); what the agent wrote is data: a click opens a read-only
//! view, nothing is run, and a file that is not text is shown as such in the viewer (it is not handed to the system's program, as a click in the list of folders does).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cosmic_text::{Buffer, Color};
use serde_json::{json, Value};

use super::super::App;

/// How long the window waits for the machine to say whether a name is there, how long an answer is kept, how many are kept and how many unanswered looks may be out at once.
pub(crate) const BUDGET: Duration = Duration::from_millis(30);
const KEEP: Duration = Duration::from_secs(5);
const TABLE: usize = 256;
const OUT: usize = 16;

/// The characters that end a name on both sides (a name is not found across them): a space and the marks of quotes, brackets, lists and options.
const MARKS: &[char] = &['"', '\'', '`', '<', '>', '(', ')', '[', ']', '{', '}', ',', ';', '|', '*', '='];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    File,
    Folder,
}

/// A name found in the text of a line: its bytes there (the position after the path is part of it), the path as written and the line and column after it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Name {
    pub start: usize,
    pub end: usize,
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// The names in a text, in order.
pub(crate) fn names(text: &str) -> Vec<Name> {
    let mut found = Vec::new();
    let mut from: Option<usize> = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        let cut = c.is_whitespace() || MARKS.contains(&c);
        match (from, cut) {
            (None, false) => from = Some(i),
            (Some(s), true) => {
                found.extend(name_in(&text[s..i], s));
                from = None;
            }
            _ => {}
        }
    }
    found
}

/// A name from one stretch of text without spaces or marks, at byte `base` of its line: the full stops, colons, exclamation and question marks at its end and a particle of
/// one to three Korean letters glued to it are not part of it; `:LINE` and `:LINE:COLUMN` are the position.
fn name_in(token: &str, base: usize) -> Option<Name> {
    let token = trim_end(token);
    let (path_len, line, column) = position(token);
    let path = &token[..path_len];
    is_path(path).then(|| Name { start: base, end: base + token.len(), path: path.to_string(), line, column })
}

/// (A particle is cut only after an ASCII letter or digit, which no mark follows: one pass takes the marks and then the particle, and there is nothing left for a second.)
fn trim_end(token: &str) -> &str {
    particle(token.trim_end_matches(['.', ':', '!', '?']))
}

/// `src/main.rs를`: one to three letters that are not ASCII after an ASCII letter or digit are the particle of the sentence, not of the name.
fn particle(s: &str) -> &str {
    let tail = s.chars().rev().take_while(|c| !c.is_ascii()).count();
    if (1..=3).contains(&tail) {
        let cut = s.len() - s.chars().rev().take(tail).map(char::len_utf8).sum::<usize>();
        if s[..cut].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric()) {
            return &s[..cut];
        }
    }
    s
}

/// The length of the path in `path:LINE` and `path:LINE:COLUMN`, and the line and the column.
fn position(s: &str) -> (usize, Option<u32>, Option<u32>) {
    let (mut end, mut numbers) = (s.len(), Vec::new());
    while numbers.len() < 2 {
        match s[..end].rsplit_once(':') {
            Some((head, tail)) if tail.bytes().all(|b| b.is_ascii_digit()) && tail.parse::<u32>().is_ok() => {
                numbers.push(tail.parse().unwrap_or(0));
                end = head.len();
            }
            _ => break,
        }
    }
    (end, numbers.last().copied(), (numbers.len() == 2).then(|| numbers[0]))
}

/// A path is a name when it starts as a path does (`/`, `~/`, `./`, `../`, with something after it) or its last part has an extension; a web address is not one.
fn is_path(p: &str) -> bool {
    if p.contains("://") {
        return false;
    }
    let rooted = ["/", "~/", "./", "../"].iter().any(|r| p.starts_with(r) && p.len() > r.len());
    rooted || extension(p.rsplit('/').next().unwrap_or(p))
}

/// The last part of a path has an extension: a stem, a full stop and one to eight ASCII letters and digits, at least one of them a letter (`0.8.2` and `1.5` are numbers).
fn extension(last: &str) -> bool {
    last.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && (1..=8).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()) && ext.bytes().any(|b| b.is_ascii_alphabetic())
    })
}

/// Where a name is: `~/` is the home folder, a rooted path is itself and a relative one is the session's folder (none for a session with no folder).
pub(crate) fn resolve(path: &str, cwd: &str, home: &str) -> Option<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        return (!home.is_empty()).then(|| Path::new(home).join(rest));
    }
    if path.starts_with('/') {
        return Some(PathBuf::from(path));
    }
    (!cwd.is_empty()).then(|| Path::new(cwd).join(path))
}

/// What the machine says of a path: a file, a folder, or nothing (a socket, a pipe and a missing path are not links).
pub(crate) fn probe(path: &Path) -> Option<Kind> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        Some(Kind::Folder)
    } else {
        meta.is_file().then_some(Kind::File)
    }
}

/// Where names are looked for: the folder of the session (relative names) and the home folder (`~/`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Ctx {
    pub cwd: String,
    pub home: String,
}

/// The name under the pointer: where it is in the layout (the line of the buffer and its bytes), what was written, where it is on the machine and what it is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hover {
    pub line_i: usize,
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub line: Option<u32>,
}

#[derive(Default)]
pub(crate) struct State {
    pub hover: Option<Hover>,
    ctx: Option<Ctx>,
    pub(crate) origin: (f32, f32),
    known: HashMap<PathBuf, (Instant, Option<Kind>)>,
    waiting: HashMap<PathBuf, mpsc::Receiver<Option<Kind>>>,
    pub(crate) hand: bool,
    drawn: Vec<[f32; 4]>,
}

impl State {
    /// Whether `path` is there, from the table when the answer is younger than `KEEP`, else from the machine: `probe` runs on a thread and is waited for `budget`. An answer that
    /// does not come in time is not an answer now (the name is not a link at this moment) and the thread's late one is taken by the next look at the same path.
    pub(crate) fn ask(&mut self, path: &Path, now: Instant, budget: Duration, probe: impl FnOnce(&Path) -> Option<Kind> + Send + 'static) -> Option<Kind> {
        if let Some(&(at, kind)) = self.known.get(path) {
            if now.duration_since(at) < KEEP {
                return kind;
            }
        }
        if let Some(rx) = self.waiting.get(path) {
            return match rx.try_recv() {
                Ok(kind) => {
                    self.waiting.remove(path);
                    self.keep(path, now, kind)
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.waiting.remove(path);
                    None
                }
            };
        }
        if self.waiting.len() >= OUT {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        let owned = path.to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(probe(&owned));
        });
        match rx.recv_timeout(budget) {
            Ok(kind) => self.keep(path, now, kind),
            Err(_) => {
                self.waiting.insert(path.to_path_buf(), rx);
                None
            }
        }
    }

    fn keep(&mut self, path: &Path, now: Instant, kind: Option<Kind>) -> Option<Kind> {
        if self.known.len() >= TABLE {
            self.known.clear();
        }
        self.known.insert(path.to_path_buf(), (now, kind));
        kind
    }

    /// The names of a frame that was just laid out: where it is drawn from (`origin`), where names are looked for (`ctx`, none where there are no links), the lines of the
    /// conversation on the screen and the pointer. True when the name under the pointer is another one than before.
    pub(crate) fn frame(&mut self, ctx: Option<Ctx>, origin: (f32, f32), buffer: &Buffer, hit_lines: &[(f32, f32, usize, u16)], pointer: (f64, f64)) -> bool {
        self.ctx = ctx;
        self.origin = origin;
        self.refresh(buffer, hit_lines, pointer)
    }

    /// The name under the pointer, from the layout of the last frame. True when it changed.
    pub(crate) fn refresh(&mut self, buffer: &Buffer, hit_lines: &[(f32, f32, usize, u16)], pointer: (f64, f64)) -> bool {
        let found = self.find(buffer, hit_lines, pointer);
        let changed = found != self.hover;
        self.hover = found;
        changed
    }

    fn find(&mut self, buffer: &Buffer, hit_lines: &[(f32, f32, usize, u16)], pointer: (f64, f64)) -> Option<Hover> {
        let ctx = self.ctx.clone()?;
        let (x, y) = (pointer.0 as f32, pointer.1 as f32);
        if !hit_lines.iter().any(|l| y >= l.0 && y < l.1) {
            return None; // not a row of the conversation (the strip, the prompt, the bars)
        }
        let (line_i, at) = under(buffer, (x - self.origin.0, y - self.origin.1))?;
        let name = names(buffer.lines.get(line_i)?.text()).into_iter().find(|n| (n.start..n.end).contains(&at))?;
        let path = resolve(&name.path, &ctx.cwd, &ctx.home)?;
        let kind = self.ask(&path, Instant::now(), BUDGET, probe)?;
        Some(Hover { line_i, start: name.start, end: name.end, name: name.path, path, kind, line: name.line })
    }

    /// The underline of the name under the pointer: one pixel row (the scale) under its glyphs, on each row it is on, in the ink of the text (the colour the text is given: `rect` tones it
    /// for the light theme, as it does every colour it fills with, so it is not toned here).
    pub(crate) fn paint(&mut self, buffer: &Buffer, buf: &mut [u32], width: usize, height: usize, s: f32, ink: Color) {
        self.drawn.clear();
        let Some(h) = &self.hover else { return };
        let argb = 0xFF00_0000 | (ink.r() as u32) << 16 | (ink.g() as u32) << 8 | ink.b() as u32;
        for sp in spans(buffer, h.line_i, h.start, h.end) {
            let y = (sp.line_y + self.origin.1 + (2.0 * s).round()).round();
            let rect = [(sp.x0 + self.origin.0).floor(), y, (sp.x1 + self.origin.0).ceil(), y + s.round().max(1.0)]; // whole pixels: the stretch covers the glyphs' span, never less
            super::super::controls::fill(buf, width, height, rect, argb);
            self.drawn.push(rect);
        }
    }

    pub(crate) fn json(&self) -> Value {
        json!({
            "armed": self.ctx.is_some(),
            "hover": self.hover.as_ref().map(|h| json!({
                "name": h.name, "path": h.path.to_string_lossy(), "kind": if h.kind == Kind::File { "file" } else { "folder" }, "line": h.line, "underline": self.drawn,
            })),
        })
    }
}

/// Where the bytes `start..end` of a line of the buffer stand, one stretch for each row they are on: the span of the glyphs and the row's baseline, top and bottom.
pub(crate) struct Span {
    pub x0: f32,
    pub x1: f32,
    pub line_y: f32,
    pub top: f32,
    pub bottom: f32,
}

pub(crate) fn spans(buffer: &Buffer, line_i: usize, start: usize, end: usize) -> Vec<Span> {
    let mut out = Vec::new();
    for run in buffer.layout_runs().filter(|r| r.line_i == line_i) {
        let mut span: Option<(f32, f32)> = None;
        for g in run.glyphs.iter().filter(|g| g.start >= start && g.end <= end) {
            span = Some(span.map_or((g.x, g.x + g.w), |(a, b)| (a.min(g.x), b.max(g.x + g.w))));
        }
        if let Some((x0, x1)) = span {
            out.push(Span { x0, x1, line_y: run.line_y, top: run.line_top, bottom: run.line_top + run.line_height });
        }
    }
    out
}

/// The line of the buffer and the byte of the glyph under a point given from the buffer's own origin.
pub(crate) fn under(buffer: &Buffer, (x, y): (f32, f32)) -> Option<(usize, usize)> {
    let run = buffer.layout_runs().find(|r| y >= r.line_top && y < r.line_top + r.line_height)?;
    let glyph = run.glyphs.iter().find(|g| x >= g.x && x < g.x + g.w)?;
    Some((run.line_i, glyph.start))
}

impl App {
    /// Where names are looked for in this frame: a conversation of the workbench with a session on this machine; none for a panel, the browser, a terminal tab, the Call Room or an
    /// ssh session.
    pub(crate) fn file_link_ctx(&self, panel: bool) -> Option<Ctx> {
        // (a browser tab and a terminal tab are shown only with no session selected: there is no row for them, so they have no names either)
        if !self.ui.workbench || panel || self.active_call_header().is_some() {
            return None;
        }
        let row = self.selected_row().filter(|r| r.ssh_host.is_empty())?;
        Some(Ctx { cwd: row.cwd.clone(), home: std::env::var("HOME").unwrap_or_default() })
    }

    /// The dump's `links`: the hover and, for the names in the rows on the screen (looked at or not), where each stands in the window.
    pub(crate) fn file_link_json(&self) -> Value {
        let mut json = self.ui.links.json();
        let mut found = Vec::new();
        if let (Some(view), true) = (self.view.as_ref(), self.ui.links.ctx.is_some()) {
            let (ox, oy) = self.ui.links.origin;
            for (i, line) in view.buffer.lines.iter().enumerate() {
                for n in names(line.text()) {
                    if let Some(sp) = spans(&view.buffer, i, n.start, n.end).first() {
                        found.push(json!({"name": n.path, "line": n.line, "rect": [sp.x0 + ox, sp.top + oy, sp.x1 + ox, sp.bottom + oy]}));
                    }
                }
            }
        }
        json["names"] = Value::Array(found);
        json
    }

    /// The pointer moved: the name under it from the last frame, the pointer a hand over one. True when the picture must be drawn again.
    pub(crate) fn file_link_moved(&mut self) -> bool {
        let Some(view) = self.view.as_ref() else { return false };
        let changed = self.ui.links.refresh(&view.buffer, &self.ui.hit_lines, self.ui.mouse);
        self.file_link_pointer();
        changed
    }

    /// The pointer is a hand while a name is under it and the window's own elsewhere (it is told to the window when it changes).
    pub(crate) fn file_link_pointer(&mut self) {
        let on = self.ui.links.hover.is_some();
        if on != self.ui.links.hand {
            self.ui.links.hand = on;
            if let Some(window) = &self.window {
                window.set_cursor(if on { winit::window::CursorIcon::Pointer } else { winit::window::CursorIcon::Default });
            }
        }
    }

    /// A stationary click with no Shift on a name that is there: the file opens in the viewer (at its line), a folder as a list; the selected line is dropped. False for a click
    /// elsewhere, which keeps its other meanings.
    pub(crate) fn file_link_click(&mut self) -> bool {
        let _ = self.file_link_moved();
        let Some(h) = self.ui.links.hover.clone() else { return false };
        let path = h.path.to_string_lossy().into_owned();
        match h.kind {
            Kind::Folder => self.files_open_at(&path),
            Kind::File => {
                let dir = h.path.parent().map_or_else(|| "/".to_string(), |d| d.to_string_lossy().into_owned());
                self.files_open_preview_from(None, &dir, &path, false);
                if let Some(n) = h.line {
                    self.files_preview_to_line(n);
                }
            }
        }
        self.ui.links.hover = None; // (opening the viewer dropped the selection of the line that was pressed and has the window drawn again)
        self.file_link_pointer();
        true
    }
}

#[cfg(test)]
#[path = "filelink_tests.rs"]
mod filelink_tests;
