//! The terminal: its state and the public reading side. The writing side is `parser.rs` (bytes), `csi.rs` (sequences) and `grid.rs` (the screen).

use std::collections::VecDeque;

use crate::parser::Parser;
use crate::types::{Attrs, Cell, Color, Modes, View};
use crate::{DEFAULT_SCROLLBACK, MAX_COLS, MAX_ROWS, MAX_SCROLLBACK};

/// The pen: what the next character is written with.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Pen {
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Pen {
    pub const DEFAULT: Pen = Pen { fg: Color::Default, bg: Color::Default, attrs: Attrs::NONE };
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Charset {
    Ascii,
    Line,
}

/// What `ESC 7` keeps.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Saved {
    pub row: u16,
    pub col: u16,
    pub pen: Pen,
    pub origin: bool,
    pub g: [Charset; 2],
    pub shift: usize,
    pub pending: bool,
}

/// One screen: `rows * cols` cells, row after row, and whether each row goes on in the next.
#[derive(Clone)]
pub(crate) struct Screen {
    pub cells: Vec<Cell>,
    pub wrapped: Vec<bool>,
}

impl Screen {
    pub fn blank(cols: u16, rows: u16) -> Screen {
        Screen { cells: vec![Cell::BLANK; cols as usize * rows as usize], wrapped: vec![false; rows as usize] }
    }
}

/// A line of the scrollback, kept without its trailing blank cells.
pub(crate) struct Line {
    pub cells: Vec<Cell>,
    pub wrapped: bool,
}

pub struct Term {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    /// 0 the main screen, 1 the alternate screen.
    pub(crate) screens: [Screen; 2],
    pub(crate) row: u16,
    pub(crate) col: u16,
    pub(crate) pending: bool,
    pub(crate) pen: Pen,
    pub(crate) g: [Charset; 2],
    pub(crate) shift: usize,
    pub(crate) saved: Option<Saved>,
    pub(crate) top: u16,
    pub(crate) bottom: u16,
    pub(crate) tabs: Vec<bool>,
    pub(crate) sb: VecDeque<Line>,
    pub(crate) sb_cells: usize,
    pub(crate) sb_max: usize,
    pub(crate) modes: Modes,
    pub(crate) title: String,
    pub(crate) bells: u64,
    pub(crate) generation: u64,
    pub(crate) reply: Vec<u8>,
    pub(crate) last: Option<char>,
    pub(crate) p: Parser,
}

pub(crate) fn default_tabs(cols: u16) -> Vec<bool> {
    (0..cols).map(|c| c % 8 == 0 && c > 0).collect()
}

impl Term {
    pub fn new(cols: u16, rows: u16) -> Term {
        Term::with_scrollback(cols, rows, DEFAULT_SCROLLBACK)
    }

    pub fn with_scrollback(cols: u16, rows: u16, lines: usize) -> Term {
        let cols = cols.clamp(1, MAX_COLS);
        let rows = rows.clamp(1, MAX_ROWS);
        Term {
            cols,
            rows,
            screens: [Screen::blank(cols, rows), Screen::blank(cols, rows)],
            row: 0,
            col: 0,
            pending: false,
            pen: Pen::DEFAULT,
            g: [Charset::Ascii; 2],
            shift: 0,
            saved: None,
            top: 0,
            bottom: rows - 1,
            tabs: default_tabs(cols),
            sb: VecDeque::new(),
            sb_cells: 0,
            sb_max: lines.min(MAX_SCROLLBACK),
            modes: Modes::INITIAL,
            title: String::new(),
            bells: 0,
            generation: 0,
            reply: Vec::new(),
            last: None,
            p: Parser::new(),
        }
    }

    /// What the program wrote. Any cutting of the stream into calls gives the same state.
    pub fn feed(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.generation += 1;
        for &b in bytes {
            self.step(b);
        }
    }

    /// The bytes the program asked for, oldest first; the buffer is empty after the call.
    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.reply)
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// The cursor's (row, col) on the screen shown, hidden or not.
    pub fn cursor(&self) -> (u16, u16) {
        (self.row, self.col)
    }

    pub fn modes(&self) -> Modes {
        self.modes
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn bells(&self) -> u64 {
        self.bells
    }

    fn sb_len(&self) -> usize {
        if self.modes.alt_screen {
            0
        } else {
            self.sb.len()
        }
    }

    /// The line at an absolute index (scrollback first, then the screen) as `cols` cells.
    fn abs_line(&self, i: usize) -> (Vec<Cell>, bool) {
        let cols = self.cols as usize;
        let sb = self.sb_len();
        if i < sb {
            (crate::grid::fit(&self.sb[i].cells, cols), self.sb[i].wrapped)
        } else {
            let r = i - sb;
            let scr = &self.screens[self.modes.alt_screen as usize];
            (scr.cells[r * cols..(r + 1) * cols].to_vec(), scr.wrapped[r])
        }
    }

    /// The rows to show, `scroll` rows above the bottom (clamped to what the scrollback holds, and to 0 on the alternate screen).
    pub fn view(&self, scroll: usize) -> View {
        let rows = self.rows as usize;
        let sb = self.sb_len();
        let scroll = scroll.min(sb);
        let top = sb - scroll;
        let mut lines = Vec::with_capacity(rows);
        let mut wrapped = Vec::with_capacity(rows);
        for i in top..top + rows {
            let (l, w) = self.abs_line(i);
            lines.push(l);
            wrapped.push(w);
        }
        let cursor = if scroll == 0 && self.modes.cursor_visible { Some((self.row, self.col)) } else { None };
        View { lines, wrapped, top, total: sb + rows, scroll, cursor }
    }

    /// The text of a selection from `(line, col)` to `(line, col)`, either order, both ends included; `line` is absolute (0 the oldest of the scrollback). Valid until the next
    /// `feed` or `resize`.
    pub fn text_between(&self, a: (usize, u16), b: (usize, u16)) -> String {
        let total = self.sb_len() + self.rows as usize;
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        if from.0 >= total {
            return String::new();
        }
        let to = (to.0.min(total - 1), if to.0 >= total { u16::MAX } else { to.1 });
        let last_col = self.cols as usize - 1;
        let mut out = String::new();
        for line in from.0..=to.0 {
            let (cells, wrapped) = self.abs_line(line);
            let mut c0 = if line == from.0 { (from.1 as usize).min(last_col) } else { 0 };
            let c1 = if line == to.0 { (to.1 as usize).min(last_col) } else { last_col };
            if cells[c0].attrs.contains(Attrs::TAIL) && c0 > 0 {
                c0 -= 1;
            }
            let mut text: String = cells[c0..=c1].iter().filter(|c| !c.attrs.contains(Attrs::TAIL)).map(|c| c.ch).collect();
            let joins = wrapped && line != to.0 && c1 == last_col;
            if !joins {
                text.truncate(text.trim_end_matches(' ').len());
            }
            out.push_str(&text);
            if line != to.0 && !joins {
                out.push('\n');
            }
        }
        out
    }
}
