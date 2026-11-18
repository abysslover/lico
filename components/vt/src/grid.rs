//! The screen: writing a character, the cursor's line moves, scrolling, erasing and editing cells, the scrollback, a resize.

use crate::term::{default_tabs, Line, Saved, Screen, Term};
use crate::types::{Attrs, Cell};
use crate::width::char_width;
use crate::MAX_SCROLLBACK_CELLS;

/// Blanks every half of a wide character whose other half is missing: a head with no tail after it, a tail with no head before it.
pub(crate) fn fix_slice(cells: &mut [Cell]) {
    for i in 0..cells.len() {
        if cells[i].attrs.contains(Attrs::WIDE) {
            if i + 1 >= cells.len() || !cells[i + 1].attrs.contains(Attrs::TAIL) {
                cells[i] = Cell::BLANK;
            }
        } else if cells[i].attrs.contains(Attrs::TAIL) && (i == 0 || !cells[i - 1].attrs.contains(Attrs::WIDE)) {
            cells[i] = Cell::BLANK;
        }
    }
}

/// A stored line as exactly `cols` cells: padded with blanks, or cut (a wide character cut in two is blanked).
pub(crate) fn fit(cells: &[Cell], cols: usize) -> Vec<Cell> {
    let mut v = Vec::with_capacity(cols);
    v.extend_from_slice(&cells[..cells.len().min(cols)]);
    v.resize(cols, Cell::BLANK);
    fix_slice(&mut v);
    v
}

impl Term {
    pub(crate) fn scr(&mut self) -> &mut Screen {
        &mut self.screens[self.modes.alt_screen as usize]
    }

    /// An erased cell: the pen's background, no attributes.
    pub(crate) fn blank(&self) -> Cell {
        Cell { ch: ' ', fg: crate::types::Color::Default, bg: self.pen.bg, attrs: Attrs::NONE }
    }

    fn row_cells(&mut self, r: u16) -> &mut [Cell] {
        let cols = self.cols as usize;
        let r = r as usize;
        &mut self.scr().cells[r * cols..(r + 1) * cols]
    }

    pub(crate) fn fix_row(&mut self, r: u16) {
        fix_slice(self.row_cells(r));
    }

    // ---- the cursor ----

    /// Row and column, 1-based, as CUP says them: relative to the scroll region in origin mode, held inside it.
    pub(crate) fn goto(&mut self, row1: u16, col1: u16) {
        let (lo, hi) = if self.modes.origin { (self.top, self.bottom) } else { (0, self.rows - 1) };
        self.row = (lo as u32 + row1.max(1) as u32 - 1).min(hi as u32) as u16;
        self.col = (col1.max(1) - 1).min(self.cols - 1);
        self.pending = false;
    }

    pub(crate) fn save_cursor(&mut self) {
        self.saved = Some(Saved { row: self.row, col: self.col, pen: self.pen, origin: self.modes.origin, g: self.g, shift: self.shift, pending: self.pending });
    }

    pub(crate) fn restore_cursor(&mut self) {
        match self.saved {
            Some(s) => {
                self.row = s.row.min(self.rows - 1);
                self.col = s.col.min(self.cols - 1);
                self.pen = s.pen;
                self.modes.origin = s.origin;
                self.g = s.g;
                self.shift = s.shift;
                self.pending = s.pending;
            }
            None => {
                self.row = 0;
                self.col = 0;
                self.pen = crate::term::Pen::DEFAULT;
                self.pending = false;
            }
        }
    }

    pub(crate) fn next_tab(&self, col: u16) -> u16 {
        (col + 1..self.cols).find(|c| self.tabs[*c as usize]).unwrap_or(self.cols - 1)
    }

    pub(crate) fn prev_tab(&self, col: u16) -> u16 {
        (0..col).rev().find(|c| self.tabs[*c as usize]).unwrap_or(0)
    }

    // ---- writing ----

    /// Blanks both halves of a wide character that the cell (r, c) is a half of.
    fn clear_pair(&mut self, r: u16, c: u16) {
        let blank = self.blank();
        let cols = self.cols as usize;
        let row = self.row_cells(r);
        let c = c as usize;
        if c >= cols {
            return;
        }
        if row[c].attrs.contains(Attrs::TAIL) && c > 0 {
            row[c - 1] = blank;
            row[c] = blank;
        } else if row[c].attrs.contains(Attrs::WIDE) && c + 1 < cols {
            row[c] = blank;
            row[c + 1] = blank;
        }
    }

    /// The next row, at the left margin: the row that was left is marked as wrapped.
    fn wrap(&mut self) {
        let r = self.row as usize;
        self.scr().wrapped[r] = true;
        self.col = 0;
        self.pending = false;
        self.index();
    }

    /// Puts a character (already mapped through the character set) at the cursor.
    pub(crate) fn put_char(&mut self, c: char) {
        let w = char_width(c) as u16;
        if w == 0 || (w == 2 && self.cols < 2) {
            return;
        }
        if self.pending {
            self.wrap();
        }
        if w == 2 && self.col + 1 >= self.cols {
            if !self.modes.autowrap {
                return;
            }
            let (r, cc) = (self.row, self.col);
            self.clear_pair(r, cc);
            let blank = self.blank();
            self.row_cells(r)[cc as usize] = blank;
            self.wrap();
        }
        if self.modes.insert {
            self.insert_cells(w);
        }
        let (r, col) = (self.row, self.col);
        self.clear_pair(r, col);
        if w == 2 {
            self.clear_pair(r, col + 1);
        }
        let (fg, bg, attrs) = (self.pen.fg, self.pen.bg, self.pen.attrs);
        let row = self.row_cells(r);
        if w == 2 {
            row[col as usize] = Cell { ch: c, fg, bg, attrs: attrs.with(Attrs::WIDE) };
            row[col as usize + 1] = Cell { ch: ' ', fg, bg, attrs: attrs.with(Attrs::TAIL) };
        } else {
            row[col as usize] = Cell { ch: c, fg, bg, attrs };
        }
        self.last = Some(c);
        let next = col + w;
        if next >= self.cols {
            self.col = self.cols - 1;
            self.pending = self.modes.autowrap;
        } else {
            self.col = next;
        }
    }

    // ---- lines ----

    /// Line feed: down one row, scrolling at the bottom of the region; below the region nothing scrolls.
    pub(crate) fn index(&mut self) {
        if self.row == self.bottom {
            self.scroll_up(1);
        } else if self.row + 1 < self.rows {
            self.row += 1;
        }
    }

    pub(crate) fn reverse_index(&mut self) {
        if self.row == self.top {
            self.scroll_down(1);
        } else if self.row > 0 {
            self.row -= 1;
        }
    }

    fn push_scrollback(&mut self, r: usize) {
        let (alt, cols) = (self.modes.alt_screen as usize, self.cols as usize);
        self.push_scrollback_of(alt, r, cols);
    }

    /// Scrolls the region up by `n` lines. Only the whole screen, the main one, gives its lines to the scrollback.
    pub(crate) fn scroll_up(&mut self, n: usize) {
        let n = n.min(self.bottom as usize - self.top as usize + 1);
        if self.top == 0 && self.bottom + 1 == self.rows && !self.modes.alt_screen {
            for r in 0..n {
                self.push_scrollback(r);
            }
        }
        self.shift_up(n);
    }

    /// The lines of the region move up by `n` and `n` blank lines come in at its bottom; nothing goes to the scrollback.
    fn shift_up(&mut self, n: usize) {
        let (top, bottom, cols) = (self.top as usize, self.bottom as usize, self.cols as usize);
        let n = n.min(bottom - top + 1);
        if n == 0 {
            return;
        }
        let blank = self.blank();
        let scr = self.scr();
        scr.cells.copy_within((top + n) * cols..(bottom + 1) * cols, top * cols);
        scr.wrapped.copy_within(top + n..bottom + 1, top);
        for r in bottom + 1 - n..=bottom {
            scr.cells[r * cols..(r + 1) * cols].fill(blank);
            scr.wrapped[r] = false;
        }
    }

    pub(crate) fn scroll_down(&mut self, n: usize) {
        let (top, bottom, cols) = (self.top as usize, self.bottom as usize, self.cols as usize);
        let n = n.min(bottom - top + 1);
        if n == 0 {
            return;
        }
        let blank = self.blank();
        let scr = self.scr();
        scr.cells.copy_within(top * cols..(bottom + 1 - n) * cols, (top + n) * cols);
        scr.wrapped.copy_within(top..bottom + 1 - n, top + n);
        for r in top..top + n {
            scr.cells[r * cols..(r + 1) * cols].fill(blank);
            scr.wrapped[r] = false;
        }
    }

    /// Inserts `n` blank lines at the cursor's row, inside the region only; the cursor goes to the left margin.
    pub(crate) fn insert_lines(&mut self, n: usize) {
        if self.row < self.top || self.row > self.bottom {
            return;
        }
        let saved_top = self.top;
        self.top = self.row;
        self.scroll_down(n);
        self.top = saved_top;
        self.col = 0;
        self.pending = false;
    }

    /// Deletes `n` lines at the cursor's row, inside the region only: they are gone, not scrolled into the history.
    pub(crate) fn delete_lines(&mut self, n: usize) {
        if self.row < self.top || self.row > self.bottom {
            return;
        }
        let saved_top = self.top;
        self.top = self.row;
        self.shift_up(n);
        self.top = saved_top;
        self.col = 0;
        self.pending = false;
    }

    // ---- cells ----

    pub(crate) fn insert_cells(&mut self, n: u16) {
        let (r, c, cols) = (self.row, self.col as usize, self.cols as usize);
        let n = (n as usize).min(cols - c);
        let blank = self.blank();
        let row = self.row_cells(r);
        row.copy_within(c..cols - n, c + n);
        row[c..c + n].fill(blank);
        self.fix_row(r);
        self.pending = false;
    }

    pub(crate) fn delete_cells(&mut self, n: u16) {
        let (r, c, cols) = (self.row, self.col as usize, self.cols as usize);
        let n = (n as usize).min(cols - c);
        let blank = self.blank();
        let row = self.row_cells(r);
        row.copy_within(c + n..cols, c);
        row[cols - n..].fill(blank);
        self.fix_row(r);
        self.pending = false;
    }

    /// Erases the cells `[c0, c1)` of row `r`.
    pub(crate) fn erase_cells(&mut self, r: u16, c0: u16, c1: u16) {
        let blank = self.blank();
        let c1 = c1.min(self.cols) as usize;
        let c0 = (c0 as usize).min(c1);
        self.row_cells(r)[c0..c1].fill(blank);
        self.fix_row(r);
        if c1 == self.cols as usize {
            self.scr().wrapped[r as usize] = false;
        }
        self.pending = false;
    }

    pub(crate) fn erase_rows(&mut self, r0: u16, r1: u16) {
        for r in r0..r1.min(self.rows) {
            self.erase_cells(r, 0, self.cols);
        }
    }

    // ---- screens ----

    pub(crate) fn enter_alt(&mut self, save_cursor: bool) {
        if self.modes.alt_screen {
            return;
        }
        if save_cursor {
            self.save_cursor();
        }
        self.modes.alt_screen = true;
        let (cols, rows) = (self.cols, self.rows);
        self.screens[1] = Screen::blank(cols, rows);
        self.top = 0;
        self.bottom = rows - 1;
    }

    pub(crate) fn leave_alt(&mut self, restore_cursor: bool) {
        if !self.modes.alt_screen {
            return;
        }
        self.modes.alt_screen = false;
        self.top = 0;
        self.bottom = self.rows - 1;
        if restore_cursor {
            self.restore_cursor();
        }
    }

    // ---- the size ----

    /// The new size. Rows and columns are cut or padded with no reflow; a shorter screen sends its top rows to the scrollback (main screen) so that the cursor row stays.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.clamp(1, crate::MAX_COLS);
        let rows = rows.clamp(1, crate::MAX_ROWS);
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        self.generation += 1;
        let (old_cols, old_rows) = (self.cols, self.rows);
        if self.pending && cols > old_cols {
            self.col = old_cols;
        }
        self.pending = false;
        let alt = self.modes.alt_screen;
        // the screen in view follows the cursor; the one behind it follows the saved cursor
        let cursor_row = self.row;
        let behind_row = if alt { self.saved.map_or(0, |s| s.row) } else { 0 };
        let (main_row, alt_row) = if alt { (behind_row, cursor_row) } else { (cursor_row, 0) };
        let main_shift = self.shrink(0, old_cols, old_rows, rows, main_row, true);
        let alt_shift = self.shrink(1, old_cols, old_rows, rows, alt_row, false);
        let shift = if alt { alt_shift } else { main_shift };
        self.row = self.row.saturating_sub(shift);
        if alt {
            if let Some(s) = self.saved.as_mut() {
                s.row = s.row.saturating_sub(main_shift);
            }
        }
        // the columns and the growing of the rows
        for s in 0..2 {
            let scr = &mut self.screens[s];
            let have = scr.wrapped.len();
            let mut cells = Vec::with_capacity(cols as usize * rows as usize);
            for r in 0..rows as usize {
                if r < have {
                    let line = &scr.cells[r * old_cols as usize..(r + 1) * old_cols as usize];
                    cells.extend_from_slice(&fit(line, cols as usize));
                } else {
                    cells.extend(std::iter::repeat_n(Cell::BLANK, cols as usize));
                }
            }
            scr.cells = cells;
            scr.wrapped.resize(rows as usize, false);
        }
        self.cols = cols;
        self.rows = rows;
        self.row = self.row.min(rows - 1);
        self.col = self.col.min(cols - 1);
        if let Some(s) = self.saved.as_mut() {
            s.row = s.row.min(rows - 1);
            s.col = s.col.min(cols - 1);
            s.pending = false;
        }
        self.top = 0;
        self.bottom = rows - 1;
        let mut tabs = default_tabs(cols);
        let keep = old_cols.min(cols) as usize;
        tabs[..keep].copy_from_slice(&self.tabs[..keep]);
        self.tabs = tabs;
    }

    /// Makes screen `s` at most `rows` tall by cutting its top (when the cursor's row would fall off) and then its bottom; the top rows go to the scrollback when asked.
    /// Returns how many rows were taken from the top.
    fn shrink(&mut self, s: usize, cols: u16, old_rows: u16, rows: u16, cursor_row: u16, history: bool) -> u16 {
        if rows >= old_rows {
            return 0;
        }
        let shift = if cursor_row >= rows { cursor_row + 1 - rows } else { 0 };
        let cols_n = cols as usize;
        if history {
            for r in 0..shift as usize {
                self.push_scrollback_of(s, r, cols_n);
            }
        }
        let scr = &mut self.screens[s];
        scr.cells.drain(..shift as usize * cols_n);
        scr.wrapped.drain(..shift as usize);
        scr.cells.truncate(rows as usize * cols_n);
        scr.wrapped.truncate(rows as usize);
        shift
    }

    fn push_scrollback_of(&mut self, s: usize, r: usize, cols: usize) {
        let wrapped = self.screens[s].wrapped[r];
        let mut cells = self.screens[s].cells[r * cols..(r + 1) * cols].to_vec();
        while cells.last() == Some(&Cell::BLANK) {
            cells.pop();
        }
        self.sb_cells += cells.len().max(1);
        self.sb.push_back(Line { cells, wrapped });
        while self.sb.len() > self.sb_max || self.sb_cells > MAX_SCROLLBACK_CELLS {
            match self.sb.pop_front() {
                Some(l) => self.sb_cells -= l.cells.len().max(1),
                None => break,
            }
        }
    }

    /// `ESC c`: everything but the title, the bell count and the generation.
    pub(crate) fn reset(&mut self) {
        let (cols, rows) = (self.cols, self.rows);
        self.screens = [Screen::blank(cols, rows), Screen::blank(cols, rows)];
        self.row = 0;
        self.col = 0;
        self.pending = false;
        self.pen = crate::term::Pen::DEFAULT;
        self.g = [crate::term::Charset::Ascii; 2];
        self.shift = 0;
        self.saved = None;
        self.top = 0;
        self.bottom = rows - 1;
        self.tabs = default_tabs(cols);
        self.sb.clear();
        self.sb_cells = 0;
        self.modes = crate::types::Modes::INITIAL;
        self.last = None;
    }
}
