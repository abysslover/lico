//! The input box of a session: the draft as text plus a cursor, the draft of every other session, and what each session was sent before.
//! A state machine with no window and no host in it: `mod.rs` and `ui.rs` hand it the keys and ask it for the rows to draw.
//!
//! Limits. One input is at most `MAX_TEXT` bytes (the host's own limit). The recall history keeps 20 inputs per session and 256 KiB in all
//! sessions together, the oldest dropped first (the transcript has what was sent). A draft is never dropped (a deleted session's goes with
//! it: `forget`): one per session, and the box refuses what would take it past the limit; only a refused input that comes back into it can leave it over, and then it is kept
//! and not sent, with the reason under the box.

use std::collections::{HashMap, VecDeque};

use cosmic_text::Buffer;
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::{emit_ime, App};

pub(crate) const MAX_TEXT: usize = crate::ipc::MAX_INPUT_TEXT;
pub(crate) const MAX_ROWS: usize = 8;
/// What the person's clipboard may hold before it is normalised (CRLF shrinks it; the check against `MAX_TEXT` comes after).
pub(crate) const PASTE_FETCH: usize = 4 * MAX_TEXT;
const HISTORY_PER_SESSION: usize = 20;
const HISTORY_BYTES: usize = 256 * 1024;
const TAB_COLS: usize = 4;

/// Marks that belong to the character before them: they take no column and go with it.
fn attached(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1160..=0x11FF | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x200D | 0x20D0..=0x20FF | 0x3099..=0x309A | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0xE0100..=0xE01EF)
}

/// Characters that take two columns.
pub(crate) fn wide(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE6F | 0xFF00..=0xFF60 | 0x1F300..=0x1FAFF)
}

fn cells(c: char) -> usize {
    match c {
        '\t' => TAB_COLS,
        c if attached(c) => 0,
        c if wide(c) => 2,
        _ => 1,
    }
}

fn width(text: &str) -> usize {
    text.chars().map(cells).sum()
}

/// Direction overrides and line separators: shown by `clean()` nowhere, so a paste does not bring them into what is about to be sent.
fn hidden(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{2028}' | '\u{2029}' | '\u{200E}' | '\u{200F}' | '\u{061C}')
}

#[derive(PartialEq, Clone, Copy)]
enum Class {
    Space,
    Word,
    Other,
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Other
    }
}

/// The start of the character (with what is attached to it) that ends at `at`.
fn prev_boundary(text: &str, at: usize) -> usize {
    let mut i = at;
    loop {
        let Some(c) = text[..i].chars().next_back() else { return 0 };
        i -= c.len_utf8();
        if !attached(c) && !text[..i].ends_with('\u{200D}') {
            return i; // a base joined to the one before it (an emoji sequence) goes with it
        }
    }
}

/// The end of the character (with what is attached to it) that starts at `at`.
fn next_boundary(text: &str, at: usize) -> usize {
    let Some(first) = text[at..].chars().next() else { return at };
    let (mut end, mut prev) = (at + first.len_utf8(), first);
    while let Some(c) = text[end..].chars().next().filter(|c| attached(*c) || (prev == '\u{200D}' && *c != '\n')) {
        end += c.len_utf8();
        prev = c;
    }
    end
}

fn word_left(text: &str, mut i: usize) -> usize {
    while let Some(c) = text[..i].chars().next_back().filter(|c| class(*c) == Class::Space) {
        i -= c.len_utf8();
    }
    let Some(last) = text[..i].chars().next_back() else { return i };
    let k = class(last);
    while let Some(c) = text[..i].chars().next_back().filter(|c| class(*c) == k || attached(*c)) {
        i -= c.len_utf8();
    }
    i
}

fn word_right(text: &str, mut i: usize) -> usize {
    while let Some(c) = text[i..].chars().next().filter(|c| class(*c) == Class::Space) {
        i += c.len_utf8();
    }
    let Some(first) = text[i..].chars().next() else { return i };
    let k = class(first);
    while let Some(c) = text[i..].chars().next().filter(|c| class(*c) == k || attached(*c)) {
        i += c.len_utf8();
    }
    i
}

/// A paste (or a typed run) as text for the box: CR and CRLF become LF, a tab stays, any other control character and any direction override is
/// dropped. Returns the text and how many characters were dropped.
pub(crate) fn normalise(raw: &str) -> (String, usize) {
    let mut out = String::with_capacity(raw.len());
    let mut dropped = 0;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() || hidden(c) => dropped += 1,
            c => out.push(c),
        }
    }
    (out, dropped)
}

/// The rows of a text at `cols` columns, as byte ranges without the newline: a newline ends a row and so does a full row. At least one row.
fn wrap(text: &str, cols: usize) -> Vec<(usize, usize)> {
    let cols = cols.max(2);
    let (mut rows, mut start, mut used) = (Vec::new(), 0, 0);
    for (i, c) in text.char_indices() {
        if c == '\n' {
            rows.push((start, i));
            (start, used) = (i + 1, 0);
            continue;
        }
        let w = cells(c);
        if used + w > cols && i > start {
            rows.push((start, i));
            (start, used) = (i, 0);
        }
        used += w;
    }
    rows.push((start, text.len()));
    rows
}

fn expand(text: &str) -> String {
    text.replace('\t', &" ".repeat(TAB_COLS))
}

/// A walk through what a session was sent before, started by Up. `stash` is the draft that was in the box when it began.
#[derive(Clone)]
struct Recall {
    depth: usize,
    shown: String,
    stash: (String, usize),
}

/// A draft set aside: the text of the box, its cursor and the recall it was in.
#[derive(Clone, Default)]
pub(crate) struct Parked {
    text: String,
    cursor: usize,
    recall: Option<Recall>,
}

impl Parked {
    pub(crate) fn text(self) -> String {
        self.text
    }

    /// A text for the box to take up, with the cursor after it (the field of a form).
    pub(crate) fn typed(text: &str) -> Parked {
        Parked { text: text.to_string(), cursor: text.len(), recall: None }
    }
}

#[derive(Default)]
pub(crate) struct Editor {
    text: String,
    cursor: usize, // a byte index on a character boundary
    goal: Option<usize>, // the column that Up and Down keep
    first: usize, // the first row shown
    cols: usize,
    stamp: u64,
    rows: Vec<(usize, usize)>,
    rows_for: (u64, usize),
    note: Option<String>,
    parked: HashMap<String, Parked>,
    sent: VecDeque<(String, String)>,
    recall: Option<Recall>,
    pub(crate) rect: Option<[f32; 4]>, // the box on screen, as the last frame drew it
    pub(crate) caret: Option<[f32; 4]>,
}

/// What the prompt of the last frame is made of.
pub(crate) struct Prompt {
    pub rows: Vec<String>,
    pub note: Option<String>,
    pub caret: Option<(usize, usize)>, // (row, byte offset in that row's text)
}

impl Editor {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(crate) fn say(&mut self, note: impl Into<String>) {
        self.note = Some(note.into());
    }

    fn edited(&mut self) {
        self.stamp += 1;
        self.goal = None;
        self.note = None;
    }

    /// The text replaced, the cursor at its end (the unit tests and the scroll oracle set a draft this way).
    #[cfg(test)]
    pub(crate) fn set(&mut self, text: String) {
        self.text = text;
        self.cursor = self.text.len();
        self.recall = None;
        self.edited();
    }

    /// Everything the box holds, set aside for a panel.
    pub(crate) fn take(&mut self) -> Parked {
        let parked = Parked { text: std::mem::take(&mut self.text), cursor: self.cursor, recall: self.recall.take() };
        self.cursor = 0;
        self.edited();
        parked
    }

    pub(crate) fn restore(&mut self, parked: Parked) {
        self.text = parked.text;
        self.cursor = parked.cursor.min(self.text.len());
        self.recall = parked.recall;
        self.edited();
    }

    /// The box is left for another session (or for none): what is in it, and a composition still open, stays with this one.
    pub(crate) fn park(&mut self, session: &str, preedit: &str) {
        self.text.insert_str(self.cursor, preedit);
        self.cursor += preedit.len();
        let parked = self.take();
        self.parked.insert(session.to_string(), parked);
    }

    pub(crate) fn resume(&mut self, session: &str) {
        let parked = self.parked.remove(session).unwrap_or_default();
        self.restore(parked);
        self.first = 0;
    }

    /// Text at the cursor; false (and a line under the box) when it would take the input past the limit.
    pub(crate) fn put(&mut self, s: &str) -> bool {
        if s.is_empty() {
            return false;
        }
        if self.text.len() + s.len() > MAX_TEXT {
            self.say(format!("The box is full: one input holds at most {} KiB. That was not added.", MAX_TEXT / 1024));
            return false;
        }
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.edited();
        true
    }

    /// A paste: normalised, then checked as a whole; a paste that would pass the limit is refused whole, with the reason.
    pub(crate) fn paste(&mut self, raw: &str) {
        let (clean, dropped) = normalise(raw);
        let left_out = |n: usize| if n == 1 { "1 control character was".to_string() } else { format!("{n} control characters were") };
        if clean.is_empty() {
            self.say(if dropped > 0 { format!("Nothing to paste: {} left out.", left_out(dropped)) } else { "Nothing to paste.".to_string() });
            return;
        }
        if self.text.len() + clean.len() > MAX_TEXT {
            self.say(format!(
                "Paste refused: {} bytes added to the {} in the box would pass the limit of {} bytes ({} KiB) for one input. Nothing was added.",
                clean.len(), self.text.len(), MAX_TEXT, MAX_TEXT / 1024
            ));
            return;
        }
        if self.put(&clean) && dropped > 0 {
            self.say(format!("{} left out of the paste.", left_out(dropped)));
        }
    }

    pub(crate) fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let from = prev_boundary(&self.text, self.cursor);
        self.text.replace_range(from..self.cursor, "");
        self.cursor = from;
        self.edited();
        true
    }

    pub(crate) fn delete(&mut self) -> bool {
        let to = next_boundary(&self.text, self.cursor);
        self.cut(self.cursor, to)
    }

    pub(crate) fn delete_word_back(&mut self) -> bool {
        let from = word_left(&self.text, self.cursor);
        let to = self.cursor;
        self.cursor = from;
        self.cut(from, to)
    }

    /// Ctrl+K: from the cursor to the end of its line; at the end of a line, the line break.
    pub(crate) fn kill_to_end(&mut self) -> bool {
        let end = self.text[self.cursor..].find('\n').map_or(self.text.len(), |i| self.cursor + i);
        let to = if end == self.cursor && end < self.text.len() { end + 1 } else { end };
        self.cut(self.cursor, to)
    }

    fn cut(&mut self, from: usize, to: usize) -> bool {
        if from >= to {
            return false;
        }
        self.text.replace_range(from..to, "");
        self.edited();
        true
    }

    /// Ctrl+U: the whole text.
    pub(crate) fn clear(&mut self) -> bool {
        let had = !self.text.is_empty();
        self.text.clear();
        self.cursor = 0;
        self.edited();
        had
    }

    pub(crate) fn left(&mut self) {
        self.cursor = prev_boundary(&self.text, self.cursor);
        self.goal = None;
    }

    pub(crate) fn right(&mut self) {
        self.cursor = next_boundary(&self.text, self.cursor);
        self.goal = None;
    }

    pub(crate) fn word_left(&mut self) {
        self.cursor = word_left(&self.text, self.cursor);
        self.goal = None;
    }

    pub(crate) fn word_right(&mut self) {
        self.cursor = word_right(&self.text, self.cursor);
        self.goal = None;
    }

    fn fit(&mut self) {
        if self.rows_for != (self.stamp, self.cols) || self.rows.is_empty() {
            self.rows = wrap(&self.text, if self.cols == 0 { 80 } else { self.cols });
            self.rows_for = (self.stamp, self.cols);
        }
    }

    fn row_of(&self, at: usize) -> usize {
        self.rows.iter().rposition(|r| r.0 <= at).unwrap_or(0)
    }

    /// The last place of a row that still belongs to it (the end of a row that was cut by width is the start of the next).
    fn row_end(&self, row: usize) -> usize {
        let (start, end) = self.rows[row];
        match self.rows.get(row + 1) {
            Some(next) if next.0 == end && end > start => prev_boundary(&self.text, end),
            _ => end,
        }
    }

    fn at_col(&self, row: usize, goal: usize) -> usize {
        let (start, end) = self.rows[row];
        let mut used = 0;
        for (i, c) in self.text[start..end].char_indices() {
            if used + cells(c) > goal {
                return start + i;
            }
            used += cells(c);
        }
        self.row_end(row)
    }

    /// Home and End go to the ends of the row that shows the cursor.
    pub(crate) fn home(&mut self) {
        self.fit();
        self.cursor = self.rows[self.row_of(self.cursor)].0;
        self.goal = None;
    }

    pub(crate) fn end(&mut self) {
        self.fit();
        self.cursor = self.row_end(self.row_of(self.cursor));
        self.goal = None;
    }

    /// One row up or down, keeping the column; false when there is no such row.
    pub(crate) fn vertical(&mut self, down: bool) -> bool {
        self.fit();
        let row = self.row_of(self.cursor);
        let target = if down { row + 1 } else { row.wrapping_sub(1) };
        if target >= self.rows.len() {
            return false;
        }
        let goal = *self.goal.get_or_insert_with(|| width(&self.text[self.rows[row].0..self.cursor]));
        self.cursor = self.at_col(target, goal);
        true
    }

    pub(crate) fn on_first_row(&mut self) -> bool {
        self.fit();
        self.row_of(self.cursor) == 0
    }

    pub(crate) fn on_last_row(&mut self) -> bool {
        self.fit();
        self.row_of(self.cursor) + 1 == self.rows.len()
    }

    // ------------------------------------------------------------------ what was sent before
    /// The text of the box, ready to send: none when there is nothing to send, or when it is over the limit (kept, and a line says why).
    pub(crate) fn prepare_send(&mut self, session: &str) -> Option<String> {
        if self.text.trim().is_empty() {
            return None;
        }
        if self.text.len() > MAX_TEXT {
            self.say(format!("Not sent: {} bytes is over the limit of {} bytes ({} KiB) for one input. The text is kept; shorten it.", self.text.len(), MAX_TEXT, MAX_TEXT / 1024));
            return None;
        }
        let text = std::mem::take(&mut self.text);
        self.cursor = 0;
        let back = self.recall.take().map(|r| r.stash);
        self.edited();
        self.remember(session, &text);
        if let Some((stash, cursor)) = back {
            self.text = stash; // the draft that a recall had set aside comes back
            self.cursor = cursor.min(self.text.len());
            self.edited();
        }
        Some(text)
    }

    fn remember(&mut self, session: &str, text: &str) {
        self.sent.push_back((session.to_string(), text.to_string()));
        if self.sent.iter().filter(|e| e.0 == session).count() > HISTORY_PER_SESSION {
            if let Some(i) = self.sent.iter().position(|e| e.0 == session) {
                self.sent.remove(i);
            }
        }
        while self.sent.len() > 1 && self.sent.iter().map(|e| e.1.len()).sum::<usize>() > HISTORY_BYTES {
            self.sent.pop_front();
        }
    }

    /// The host refused an input: it leaves the history and goes back in front of the draft, wherever that is (kept, never dropped).
    pub(crate) fn give_back(&mut self, session: &str, current: bool, text: &str) {
        if let Some(i) = self.sent.iter().rposition(|e| e.0 == session && e.1 == text) {
            self.sent.remove(i);
        }
        if current {
            self.text.insert_str(0, text);
            self.cursor += text.len();
            self.edited();
        } else {
            let draft = self.parked.entry(session.to_string()).or_default();
            draft.text.insert_str(0, text);
            draft.cursor += text.len();
        }
    }

    pub(crate) fn forget(&mut self, session: &str) {
        self.parked.remove(session);
        self.sent.retain(|e| e.0 != session);
    }

    pub(crate) fn sent_count(&self, session: &str) -> usize {
        self.sent.iter().filter(|e| e.0 == session).count()
    }

    fn entry(&self, session: &str, depth: usize) -> Option<String> {
        self.sent.iter().rev().filter(|e| e.0 == session).nth(depth).map(|e| e.1.clone())
    }

    /// Up: on the first row, the input sent before; false when Up has nothing to do here. Text that was typed or edited is not replaced
    /// (the cursor only goes to its start), an empty box or an unchanged recalled input is.
    pub(crate) fn recall_older(&mut self, session: &str) -> bool {
        let depth = match &self.recall {
            Some(r) if r.shown == self.text => r.depth + 1,
            Some(_) if self.text.is_empty() => 0,
            None if self.text.is_empty() => 0,
            None => {
                let draft = (std::mem::take(&mut self.text), self.cursor);
                return self.start_recall(session, draft);
            }
            Some(_) => return false,
        };
        let Some(entry) = self.entry(session, depth) else { return self.recall.is_some() };
        let stash = self.recall.take().map_or((String::new(), 0), |r| r.stash);
        self.show_recalled(depth, entry, stash);
        true
    }

    fn start_recall(&mut self, session: &str, draft: (String, usize)) -> bool {
        match self.entry(session, 0) {
            Some(entry) => {
                self.show_recalled(0, entry, draft);
                true
            }
            None => {
                (self.text, self.cursor) = draft; // nothing was sent yet: the draft stays as it is
                false
            }
        }
    }

    fn show_recalled(&mut self, depth: usize, entry: String, stash: (String, usize)) {
        self.text = entry.clone();
        self.cursor = self.text.len();
        self.recall = Some(Recall { depth, shown: entry, stash });
        self.edited();
    }

    /// Down on the last row of a recalled input: the next newer one, and after the newest the draft that was set aside.
    pub(crate) fn recall_newer(&mut self, session: &str) -> bool {
        let Some(r) = self.recall.clone().filter(|r| r.shown == self.text) else { return false };
        if r.depth == 0 {
            self.text = r.stash.0;
            self.cursor = r.stash.1.min(self.text.len());
            self.recall = None;
            self.edited();
        } else if let Some(entry) = self.entry(session, r.depth - 1) {
            self.show_recalled(r.depth - 1, entry, r.stash);
        }
        true
    }

    // ------------------------------------------------------------------ drawing
    /// The rows of the box for a frame: at most `cap` of them, the cursor's row always among them, the first with the prompt "> ".
    pub(crate) fn prompt(&mut self, cols: usize, cap: usize, preedit: &str) -> Prompt {
        self.cols = cols.saturating_sub(2).max(2);
        self.fit();
        let total = self.rows.len();
        let cap = cap.clamp(1, MAX_ROWS);
        let at = self.row_of(self.cursor);
        if at < self.first {
            self.first = at;
        }
        if at >= self.first + cap {
            self.first = at + 1 - cap;
        }
        self.first = self.first.min(total.saturating_sub(cap)); // no empty rows below the last
        let last = (self.first + cap).min(total);
        let mut rows = Vec::new();
        let mut caret = None;
        for row in self.first..last {
            let (start, end) = self.rows[row];
            let mut shown = if row == self.first && self.first == 0 { "> ".to_string() } else { "  ".to_string() };
            if row == at {
                shown.push_str(&expand(&self.text[start..self.cursor]));
                if preedit.is_empty() {
                    caret = Some((row - self.first, shown.len()));
                } else {
                    shown.push_str(&format!("[{preedit}]"));
                }
                shown.push_str(&expand(&self.text[self.cursor..end]));
            } else {
                shown.push_str(&expand(&self.text[start..end]));
            }
            rows.push(shown);
        }
        let note = self.note.clone().or_else(|| {
            (total > cap).then(|| format!("  rows {}-{} of {} (Up and Down move; the box shows {} at a time)", self.first + 1, last, total, cap))
        });
        Prompt { rows, note, caret }
    }

    /// Where the box and its caret are on screen after a frame was laid out: `lines` is (the first buffer line of the prompt, the rows of
    /// the box, the caret as (row, byte offset)), `at` is (x, y, width of the text, scale).
    pub(crate) fn place(&mut self, buffer: &Buffer, lines: (usize, usize, Option<(usize, usize)>), at: [f32; 4]) {
        let (first_line, rows, caret) = lines;
        let [ox, oy, text_w, s] = at;
        let (mut top, mut bottom) = (f32::MAX, f32::MIN);
        self.caret = None;
        for run in buffer.layout_runs().filter(|r| r.line_i >= first_line && r.line_i < first_line + rows) {
            (top, bottom) = (top.min(run.line_top), bottom.max(run.line_top + run.line_height));
            if let Some((row, offset)) = caret.filter(|c| first_line + c.0 == run.line_i) {
                let x = run.glyphs.iter().find(|g| g.start >= offset).map(|g| g.x).or_else(|| run.glyphs.last().map(|g| g.x + g.w)).unwrap_or(0.0);
                let _ = row;
                self.caret = Some([ox + x, oy + run.line_top, (1.5 * s).ceil(), run.line_height]);
            }
        }
        self.rect = (top < bottom).then_some([ox, oy + top, ox + text_w, oy + bottom]);
    }

    pub(crate) fn dump(&self, session: &str) -> Value {
        json!({
            "text": self.text, "cursor": self.cursor, "rows": self.rows.len(), "first": self.first, "note": self.note, "sent": self.sent_count(session),
            "recall": self.recall.as_ref().map(|r| r.depth), "parked": self.parked.len(), "rect": self.rect, "caret": self.caret,
        })
    }
}

impl App {
    /// The editor's part of the dump. While a panel owns the line (a sign-in code is typed or pasted into it) the dump has the length and not the text.
    pub(crate) fn editor_json(&self) -> Value {
        let mut dump = self.ed.dump(&self.session);
        if self.ui.panel.is_some() {
            (dump["text"], dump["cursor"], dump["len"]) = (Value::Null, Value::Null, json!(self.ed.text().len()));
        }
        dump
    }

    /// What the prompt of this frame is: the rows of the input box with the line under it, or the one line of a panel (a sign-in code is never drawn).
    pub(crate) fn prompt_view(&mut self, cols: usize, area_rows: usize) -> Prompt {
        let line = |text: String| Prompt { rows: vec![text], note: None, caret: None };
        if self.code_entry() {
            line(format!("> {}", self.shown_input()))
        } else if self.ui.panel.is_some() || self.session.is_empty() {
            line(format!("> {}[{}]", self.ed.text(), self.preedit))
        } else {
            self.ed.prompt(cols, (area_rows.saturating_sub(6) / 2).clamp(1, MAX_ROWS), &self.preedit)
        }
    }

    /// What the selection's owner sent, for the input box of a session: all of it, normalised, or the reason it was not taken.
    pub(crate) fn box_paste(&mut self, result: Result<String, &'static str>) {
        match result {
            Ok(text) => self.ed.paste(&text),
            Err("too_large") => self.ed.say(format!("Paste refused: more than {} KiB, or sent in pieces.", PASTE_FETCH / 1024)),
            Err("timeout") => self.ed.say("Paste: the owner of the selection did not answer."),
            Err(_) => self.ed.say("Nothing to paste."),
        }
        self.dirty = true;
    }

    /// The keys of the input box, for a session that is open and no composition in progress (the input method owns the keys then).
    pub(crate) fn edit_key(&mut self, event: &KeyEvent) -> bool {
        if self.ui.panel.is_some() || self.session.is_empty() || !self.preedit.is_empty() || self.ui.mods.alt_key() {
            return false;
        }
        let (ctrl, shift) = (self.ui.mods.control_key(), self.ui.mods.shift_key());
        let session = self.session.clone();
        let ed = &mut self.ed;
        let (stamp, empty, note) = (ed.stamp, ed.is_empty(), ed.note.take());
        let handled = match &event.logical_key {
            Key::Named(NamedKey::Enter) if shift && !ctrl => ed.put("\n") || true,
            Key::Named(NamedKey::ArrowLeft) => {
                if ctrl { ed.word_left() } else { ed.left() }
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                if ctrl { ed.word_right() } else { ed.right() }
                true
            }
            Key::Named(NamedKey::Home) if !ctrl && !empty => {
                ed.home();
                true
            }
            Key::Named(NamedKey::End) if !ctrl && !empty => {
                ed.end();
                true
            }
            Key::Named(NamedKey::ArrowUp) if !ctrl => {
                if !(ed.on_first_row() && ed.recall_older(&session)) && !ed.vertical(false) {
                    (ed.cursor, ed.goal) = (0, None); // the first row and nothing to recall: the start of the text
                }
                true
            }
            Key::Named(NamedKey::ArrowDown) if !ctrl => {
                if !(ed.on_last_row() && ed.recall_newer(&session)) && !ed.vertical(true) {
                    (ed.cursor, ed.goal) = (ed.text.len(), None);
                }
                true
            }
            Key::Named(NamedKey::Delete) if !ctrl => ed.delete() || true,
            Key::Named(NamedKey::Backspace) if ctrl => ed.delete_word_back() || true,
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("k") => ed.kill_to_end() || true,
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("u") => ed.clear() || true,
            _ => false,
        };
        if !handled {
            ed.note = note;
            return false;
        }
        if self.ed.stamp != stamp {
            emit_ime("text", self.ed.text());
        }
        self.dirty = true;
        true
    }

    /// Ctrl+U clears the whole input line (and only that); true when it did.
    pub(crate) fn ui_ctrl_u(&mut self, key: &Key) -> bool {
        match key {
            Key::Character(c) if self.ui.mods.control_key() && c.eq_ignore_ascii_case("u") && self.preedit.is_empty() => {
                self.ed.clear();
                true
            }
            _ => false,
        }
    }

    /// The keys that reach the box last: Enter sends, Backspace removes a character, Ctrl+U clears, anything else that is text goes in.
    pub(crate) fn typed_key(&mut self, event: &KeyEvent) {
        if event.state != winit::event::ElementState::Pressed {
            return;
        }
        let key = &event.logical_key;
        if *key == Key::Named(NamedKey::Enter) {
            self.send_draft();
        } else if *key == Key::Named(NamedKey::Backspace) {
            // an open composition belongs to the input method (its own Backspace edits it); otherwise one press removes one character
            if self.preedit.is_empty() && self.ed.backspace() {
                emit_ime("text", self.ed.text());
                self.dirty = true;
            }
        } else if self.ui_ctrl_u(key) {
            emit_ime("text", self.ed.text());
            self.dirty = true;
        } else if let Some(text) = &event.text {
            if !text.chars().any(char::is_control) {
                self.ed.put(text);
                emit_ime("text", self.ed.text());
                self.dirty = true;
            }
        }
    }

    /// Send the draft of the visible session as one input with its own request id; over the limit it stays, with the reason under the box.
    pub(crate) fn send_draft(&mut self) {
        if self.session.is_empty() {
            return;
        }
        let Some(text) = self.ed.prepare_send(&self.session) else {
            self.dirty = true;
            return;
        };
        self.sent_count += 1;
        let request_id = format!("gui-{}-{}", std::process::id(), self.sent_count);
        let msg = format!(
            "{{\"op\":\"input\",\"id\":\"{}\",\"session\":\"{}\",\"generation\":{},\"request_id\":\"{}\",\"text\":\"{}\"}}",
            request_id, self.session, self.generation, request_id, super::json_escape(&text)
        );
        self.pending_inputs.insert(request_id.clone(), (self.session.clone(), text.clone()));
        self.send(&msg);
        emit_ime("sent", &text);
        emit_ime("text", self.ed.text());
        self.dirty = true;
    }

    /// A reply to one of our inputs carries its request id: a refused input returns to its draft, and a line says what the host said.
    pub(crate) fn on_input_reply(&mut self, line: &str) -> bool {
        let Some(id) = super::jstr(line, "id").filter(|id| id.starts_with("gui-")) else { return false };
        let id = id.to_string();
        if line.contains("\"ok\":0") {
            if let Some((session, text)) = self.pending_inputs.remove(&id) {
                let current = self.session == session;
                self.ed.give_back(&session, current, &text);
                if current {
                    let reason = super::jstr(line, "error").unwrap_or("refused");
                    self.ed.say(format!("Not sent: the host answered {reason}. The text is back in the box."));
                }
                emit_ime("send_failed", &text);
                self.dirty = true;
            }
        } else {
            self.pending_inputs.remove(&id);
        }
        true
    }
}

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;
