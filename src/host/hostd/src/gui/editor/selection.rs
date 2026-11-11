//! Selection, mouse hit-testing, highlight geometry, and bounded per-session undo/redo for `Editor`.

use cosmic_text::Buffer;

use super::{attached, prev_boundary, Editor, TAB_COLS};

pub(super) const UNDO_PER_SESSION: usize = 32;
pub(super) const UNDO_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snap {
    pub text: String,
    pub cursor: usize,
}

#[derive(Clone, Debug)]
pub(super) struct RowHit {
    pub y0: f32,
    pub y1: f32,
    pub row: usize,
    pub glyphs: Vec<(usize, usize, f32, f32)>, // (shown_start, shown_end, x0, x1)
}

/// Maps a relative byte offset in `row_slice` (`self.text[start..end]`) to the byte offset in `shown` (`"> "` / `"  "` + expanded tabs).
pub(super) fn shown_offset(row_slice: &str, rel_byte: usize) -> usize {
    let rel = rel_byte.min(row_slice.len());
    let tabs = row_slice.as_bytes()[..rel].iter().filter(|&&b| b == b'\t').count();
    2 + rel + tabs * (TAB_COLS - 1)
}

/// Maps a byte offset in `shown` back to a grapheme-aligned relative byte offset in `row_slice`.
pub(super) fn text_offset_from_shown(row_slice: &str, shown_byte: usize) -> usize {
    if shown_byte <= 2 {
        return 0;
    }
    let mut cur_shown = 2usize;
    let mut out = row_slice.len();
    for (idx, ch) in row_slice.char_indices() {
        let len_shown = if ch == '\t' { TAB_COLS } else { ch.len_utf8() };
        if shown_byte < cur_shown + len_shown {
            out = if ch == '\t' && shown_byte - cur_shown >= TAB_COLS / 2 { idx + 1 } else { idx };
            break;
        }
        cur_shown += len_shown;
    }
    if out > 0 && out < row_slice.len() {
        if let Some(c) = row_slice[out..].chars().next() {
            if attached(c) || row_slice[..out].ends_with('\u{200D}') {
                out = prev_boundary(row_slice, out);
            }
        }
    }
    out
}

#[derive(Clone, Copy)]
pub(super) enum Move {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
    Up,
    Down,
}

impl Editor {
    pub(crate) fn selection_range(&self) -> Option<(usize, usize)> {
        match self.anchor {
            Some(a) if a != self.cursor && a <= self.text.len() && self.cursor <= self.text.len() => Some((a.min(self.cursor), a.max(self.cursor))),
            _ => None,
        }
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    pub(crate) fn selected_text(&self) -> Option<String> {
        self.selection_range().map(|(lo, hi)| self.text[lo..hi].to_string())
    }

    pub(crate) fn clear_selection(&mut self) -> bool {
        let had = self.has_selection();
        self.anchor = None;
        self.selecting = false;
        self.typing_run = false;
        had
    }

    /// Removes the active selection and returns its text, recording an undo step.
    pub(crate) fn cut_selection(&mut self) -> Option<String> {
        let (lo, hi) = self.selection_range()?;
        let out = self.text[lo..hi].to_string();
        self.push_undo();
        self.text.replace_range(lo..hi, "");
        self.cursor = lo;
        self.edited();
        Some(out)
    }

    /// Moves the cursor by `mv`; extends selection from `anchor` when `shift` is held, or collapses/clears selection when not.
    pub(super) fn move_cursor(&mut self, shift: bool, mv: Move) {
        self.typing_run = false;
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
            self.step_move(mv);
            if self.anchor == Some(self.cursor) {
                self.anchor = None;
            }
            return;
        }
        if let Some((lo, hi)) = self.selection_range() {
            self.anchor = None;
            match mv {
                Move::Left => {
                    self.cursor = lo;
                    self.goal = None;
                    return;
                }
                Move::Right => {
                    self.cursor = hi;
                    self.goal = None;
                    return;
                }
                _ => {}
            }
        } else {
            self.anchor = None;
        }
        self.step_move(mv);
    }

    fn step_move(&mut self, mv: Move) {
        match mv {
            Move::Left => self.left(),
            Move::Right => self.right(),
            Move::WordLeft => self.word_left(),
            Move::WordRight => self.word_right(),
            Move::Home => self.home(),
            Move::End => self.end(),
            Move::Up => {
                if !self.vertical(false) {
                    (self.cursor, self.goal) = (0, None);
                }
            }
            Move::Down => {
                if !self.vertical(true) {
                    (self.cursor, self.goal) = (self.text.len(), None);
                }
            }
        }
    }

    pub(crate) fn contains(&self, x: f32, y: f32) -> bool {
        self.rect.map_or(false, |[x0, y0, x1, y1]| x >= x0 && x <= x1 && y >= y0 && y <= y1)
    }

    pub(crate) fn byte_at_point(&self, x: f32, y: f32) -> Option<usize> {
        if self.row_hits.is_empty() {
            return None;
        }
        let hit = self
            .row_hits
            .iter()
            .find(|h| y >= h.y0 && y < h.y1)
            .or_else(|| if y < self.row_hits[0].y0 { self.row_hits.first() } else { self.row_hits.last() })?;
        let &(start, end) = self.rows.get(hit.row)?;
        let slice = &self.text[start..end];
        let mut last_end_shown = None;
        for &(gs, ge, gx0, gx1) in &hit.glyphs {
            if ge <= 2 {
                continue;
            }
            if x < (gx0 + gx1) * 0.5 {
                return Some(start + text_offset_from_shown(slice, gs.max(2)));
            }
            last_end_shown = Some(ge);
        }
        match last_end_shown {
            Some(ge) => Some(start + text_offset_from_shown(slice, ge)),
            None => Some(start),
        }
    }

    pub(crate) fn mouse_press(&mut self, x: f32, y: f32, shift: bool) -> bool {
        let Some(idx) = self.byte_at_point(x, y) else { return false };
        self.typing_run = false;
        self.goal = None;
        self.note = None;
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
            self.cursor = idx;
            if self.anchor == Some(self.cursor) {
                self.anchor = None;
            }
        } else {
            self.anchor = Some(idx);
            self.cursor = idx;
        }
        self.selecting = true;
        true
    }

    pub(crate) fn mouse_drag(&mut self, x: f32, y: f32) -> bool {
        if !self.selecting {
            return false;
        }
        let Some(idx) = self.byte_at_point(x, y) else { return false };
        if self.anchor.is_none() {
            self.anchor = Some(self.cursor);
        }
        let changed = self.cursor != idx;
        self.cursor = idx;
        self.goal = None;
        changed
    }

    pub(crate) fn finish_drag(&mut self) {
        self.selecting = false;
        if self.anchor == Some(self.cursor) {
            self.anchor = None;
        }
    }

    pub(super) fn push_undo(&mut self) {
        self.redo.clear();
        if self.text.len() > UNDO_BYTES {
            return;
        }
        if self.undo.back().map_or(false, |s| s.text == self.text && s.cursor == self.cursor) {
            return;
        }
        self.undo.push_back(Snap { text: self.text.clone(), cursor: self.cursor });
        while self.undo.len() > UNDO_PER_SESSION {
            self.undo.pop_front();
        }
        self.trim_undo_bytes();
    }

    pub(super) fn trim_undo_bytes(&mut self) {
        while self.undo.len() > UNDO_PER_SESSION {
            self.undo.pop_front();
        }
        while self.redo.len() > UNDO_PER_SESSION {
            self.redo.remove(0);
        }
        loop {
            let active: usize = self.undo.iter().chain(self.redo.iter()).map(|s| s.text.len()).sum();
            let parked: usize = self
                .parked
                .values()
                .map(|p| p.undo.iter().chain(p.redo.iter()).map(|s| s.text.len()).sum::<usize>())
                .sum();
            if active + parked <= UNDO_BYTES {
                break;
            }
            if let Some(p) = self.parked.values_mut().find(|p| !p.redo.is_empty()) {
                p.redo.remove(0);
            } else if let Some(p) = self.parked.values_mut().find(|p| !p.undo.is_empty()) {
                p.undo.pop_front();
            } else if !self.redo.is_empty() {
                self.redo.remove(0);
            } else if self.undo.pop_front().is_none() {
                break;
            }
        }
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop_back() else { return false };
        if self.text.len() <= UNDO_BYTES {
            self.redo.push(Snap { text: std::mem::take(&mut self.text), cursor: self.cursor });
        }
        self.text = prev.text;
        self.cursor = prev.cursor.min(self.text.len());
        self.recall = None;
        self.edited();
        self.trim_undo_bytes();
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        if self.text.len() <= UNDO_BYTES {
            self.undo.push_back(Snap { text: std::mem::take(&mut self.text), cursor: self.cursor });
        }
        self.text = next.text;
        self.cursor = next.cursor.min(self.text.len());
        self.recall = None;
        self.edited();
        self.trim_undo_bytes();
        true
    }
}

pub(super) fn place_layout(ed: &mut Editor, buffer: &Buffer, lines: (usize, usize, Option<(usize, usize)>), at: [f32; 4]) {
    let (first_line, rows, caret) = lines;
    let [ox, oy, text_w, s] = at;
    let (mut top, mut bottom) = (f32::MAX, f32::MIN);
    ed.caret = None;
    ed.row_hits.clear();
    ed.sel_rects.clear();
    let live_box = caret.is_some() && ed.first + rows <= ed.rows.len();
    let sel = if live_box { ed.selection_range() } else { None };
    for run in buffer.layout_runs().filter(|r| r.line_i >= first_line && r.line_i < first_line + rows) {
        let (y0, y1) = (oy + run.line_top, oy + run.line_top + run.line_height);
        (top, bottom) = (top.min(run.line_top), bottom.max(run.line_top + run.line_height));
        if ed.caret.is_none() {
            if let Some((_, offset)) = caret.filter(|c| first_line + c.0 == run.line_i) {
                let x = run.glyphs.iter().find(|g| g.start >= offset).map(|g| g.x).or_else(|| run.glyphs.last().map(|g| g.x + g.w)).unwrap_or(0.0);
                ed.caret = Some([ox + x, y0, (1.5 * s).ceil(), run.line_height]);
            }
        }
        if !live_box {
            continue;
        }
        let row = ed.first + (run.line_i - first_line);
        let (start, end) = ed.rows[row];
        let slice = &ed.text[start..end];
        let glyphs: Vec<(usize, usize, f32, f32)> = run.glyphs.iter().map(|g| (g.start, g.end, ox + g.x, ox + g.x + g.w)).collect();
        if let Some((sel_lo, sel_hi)) = sel {
            let (r_lo, r_hi) = (sel_lo.max(start), sel_hi.min(end));
            let spans_nl = sel_lo <= end && sel_hi > end && ed.text.as_bytes().get(end) == Some(&b'\n');
            if r_lo < r_hi {
                let (s_lo, s_hi) = (shown_offset(slice, r_lo - start), shown_offset(slice, r_hi - start));
                let (mut x0, mut x1) = (f32::MAX, f32::MIN);
                for &(gs, ge, gx0, gx1) in &glyphs {
                    if ge > s_lo && gs < s_hi {
                        x0 = x0.min(gx0);
                        x1 = x1.max(gx1);
                    }
                }
                if spans_nl && x1 > x0 {
                    x1 = (x1 + 4.0 * s).min(ox + text_w);
                }
                if x1 > x0 {
                    ed.sel_rects.push([x0, y0, x1, y1]);
                }
            } else if spans_nl && start == end {
                let px = glyphs.last().map_or(ox + 16.0 * s, |g| g.3);
                ed.sel_rects.push([px, y0, (px + 6.0 * s).min(ox + text_w), y1]);
            }
        }
        ed.row_hits.push(RowHit { y0, y1, row, glyphs });
    }
    ed.rect = (top < bottom).then_some([ox, oy + top, ox + text_w, oy + bottom]);
}
