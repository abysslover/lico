//! The CSI sequences: the cursor, erasing, editing, the scroll region, the modes, SGR and the reports.

use crate::parser::Param;
use crate::term::Term;
use crate::types::{Attrs, Color, MouseMode};
use crate::MAX_REPLY;

/// The count a parameter means: a missing or zero one is `default`.
fn count(params: &[Param], i: usize, default: u16) -> u16 {
    match params.get(i) {
        Some(p) if p.v > 0 => p.v,
        _ => default,
    }
}

fn value(params: &[Param], i: usize) -> u16 {
    params.get(i).map_or(0, |p| p.v)
}

impl Term {
    pub(crate) fn csi_dispatch(&mut self, f: u8) {
        let params = std::mem::take(&mut self.p.params);
        let (prefix, ninter) = (self.p.prefix, self.p.ninter);
        if ninter == 0 {
            match prefix {
                0 => self.csi_plain(f, &params),
                b'?' => match f {
                    b'h' => self.private_modes(&params, true),
                    b'l' => self.private_modes(&params, false),
                    _ => {}
                },
                _ => {}
            }
        }
        self.p.params = params;
        self.p.params.clear();
    }

    fn csi_plain(&mut self, f: u8, p: &[Param]) {
        let n = count(p, 0, 1);
        let (rows, cols) = (self.rows, self.cols);
        match f {
            b'@' => self.insert_cells(n),
            b'A' | b'F' => {
                let floor = if self.row >= self.top { self.top } else { 0 };
                self.row = self.row.saturating_sub(n).max(floor);
                self.pending = false;
                if f == b'F' {
                    self.col = 0;
                }
            }
            b'B' | b'E' | b'e' => {
                let ceil = if self.row <= self.bottom { self.bottom } else { rows - 1 };
                self.row = (self.row as u32 + n as u32).min(ceil as u32) as u16;
                self.pending = false;
                if f == b'E' {
                    self.col = 0;
                }
            }
            b'C' | b'a' => {
                self.col = (self.col as u32 + n as u32).min(cols as u32 - 1) as u16;
                self.pending = false;
            }
            b'D' => {
                self.col = self.col.saturating_sub(n);
                self.pending = false;
            }
            b'G' | b'`' => {
                self.col = (n - 1).min(cols - 1);
                self.pending = false;
            }
            b'H' | b'f' => self.goto(count(p, 0, 1), count(p, 1, 1)),
            b'I' => {
                for _ in 0..n.min(cols) {
                    self.col = self.next_tab(self.col);
                }
                self.pending = false;
            }
            b'Z' => {
                for _ in 0..n.min(cols) {
                    self.col = self.prev_tab(self.col);
                }
                self.pending = false;
            }
            b'J' => self.erase_display(value(p, 0)),
            b'K' => {
                let (r, c) = (self.row, self.col);
                match value(p, 0) {
                    0 => self.erase_cells(r, c, cols),
                    1 => self.erase_cells(r, 0, c + 1),
                    2 => self.erase_cells(r, 0, cols),
                    _ => {}
                }
            }
            b'L' => self.insert_lines(n as usize),
            b'M' => self.delete_lines(n as usize),
            b'P' => self.delete_cells(n),
            b'S' => self.scroll_up(n as usize),
            b'T' => self.scroll_down(n as usize),
            b'X' => {
                let (r, c) = (self.row, self.col);
                self.erase_cells(r, c, c.saturating_add(n).min(cols));
                self.pending = false;
            }
            b'b' => {
                if let Some(c) = self.last {
                    for _ in 0..(n as usize).min(cols as usize * rows as usize) {
                        self.put_char(c);
                    }
                }
            }
            b'c' if p.len() == 1 && p[0].v == 0 => self.push_reply(b"\x1b[?62;22c"),
            b'd' => {
                let line = count(p, 0, 1);
                let col = self.col + 1;
                self.goto(line, col);
            }
            b'g' => match value(p, 0) {
                0 => self.tabs[self.col as usize] = false,
                3 => self.tabs.iter_mut().for_each(|t| *t = false),
                _ => {}
            },
            b'h' | b'l' if p.iter().any(|x| x.v == 4) => self.modes.insert = f == b'h',
            b'm' => self.sgr(p),
            b'n' => match value(p, 0) {
                5 => self.push_reply(b"\x1b[0n"),
                6 => {
                    let base = if self.modes.origin { self.top } else { 0 };
                    let report = format!("\x1b[{};{}R", self.row - base + 1, self.col + 1);
                    self.push_reply(report.as_bytes());
                }
                _ => {}
            },
            b'r' => {
                let top = count(p, 0, 1);
                let bottom = count(p, 1, rows).min(rows);
                if top < bottom {
                    self.top = top - 1;
                    self.bottom = bottom - 1;
                    self.goto(1, 1);
                }
            }
            b's' if p.len() == 1 && p[0].v == 0 => self.save_cursor(),
            b'u' if p.len() == 1 && p[0].v == 0 => self.restore_cursor(),
            _ => {}
        }
    }

    fn push_reply(&mut self, bytes: &[u8]) {
        if self.reply.len() + bytes.len() <= MAX_REPLY {
            self.reply.extend_from_slice(bytes);
        }
    }

    fn erase_display(&mut self, mode: u16) {
        let (r, c, rows, cols) = (self.row, self.col, self.rows, self.cols);
        match mode {
            0 => {
                self.erase_cells(r, c, cols);
                self.erase_rows(r + 1, rows);
            }
            1 => {
                self.erase_rows(0, r);
                self.erase_cells(r, 0, c + 1);
            }
            2 => self.erase_rows(0, rows),
            3 => {
                self.sb.clear();
                self.sb_cells = 0;
            }
            _ => {}
        }
    }

    // ---- modes ----

    fn private_modes(&mut self, p: &[Param], set: bool) {
        for x in p {
            match x.v {
                1 => self.modes.app_cursor = set,
                6 => {
                    self.modes.origin = set;
                    self.goto(1, 1);
                }
                7 => {
                    self.modes.autowrap = set;
                    if !set {
                        self.pending = false;
                    }
                }
                25 => self.modes.cursor_visible = set,
                47 | 1047 => {
                    if set {
                        self.enter_alt(false)
                    } else {
                        self.leave_alt(false)
                    }
                }
                1049 => {
                    if set {
                        self.enter_alt(true)
                    } else {
                        self.leave_alt(true)
                    }
                }
                1000 => self.modes.mouse = if set { MouseMode::Press } else { MouseMode::Off },
                1002 => self.modes.mouse = if set { MouseMode::Drag } else { MouseMode::Off },
                1003 => self.modes.mouse = if set { MouseMode::Any } else { MouseMode::Off },
                1004 => self.modes.focus_events = set,
                1006 => self.modes.mouse_sgr = set,
                2004 => self.modes.bracketed_paste = set,
                _ => {}
            }
        }
    }

    // ---- SGR ----

    fn sgr(&mut self, p: &[Param]) {
        let mut i = 0;
        while i < p.len() {
            let v = p[i].v;
            i += 1;
            match v {
                0 => self.pen = crate::term::Pen::DEFAULT,
                1 => self.pen.attrs = self.pen.attrs.with(Attrs::BOLD),
                2 => self.pen.attrs = self.pen.attrs.with(Attrs::DIM),
                3 => self.pen.attrs = self.pen.attrs.with(Attrs::ITALIC),
                4 | 21 => self.pen.attrs = self.pen.attrs.with(Attrs::UNDERLINE),
                5 | 6 => self.pen.attrs = self.pen.attrs.with(Attrs::BLINK),
                7 => self.pen.attrs = self.pen.attrs.with(Attrs::INVERSE),
                8 => self.pen.attrs = self.pen.attrs.with(Attrs::HIDDEN),
                9 => self.pen.attrs = self.pen.attrs.with(Attrs::STRIKE),
                22 => self.pen.attrs = self.pen.attrs.without(Attrs::BOLD).without(Attrs::DIM),
                23 => self.pen.attrs = self.pen.attrs.without(Attrs::ITALIC),
                24 => self.pen.attrs = self.pen.attrs.without(Attrs::UNDERLINE),
                25 => self.pen.attrs = self.pen.attrs.without(Attrs::BLINK),
                27 => self.pen.attrs = self.pen.attrs.without(Attrs::INVERSE),
                28 => self.pen.attrs = self.pen.attrs.without(Attrs::HIDDEN),
                29 => self.pen.attrs = self.pen.attrs.without(Attrs::STRIKE),
                30..=37 => self.pen.fg = Color::Indexed((v - 30) as u8),
                39 => self.pen.fg = Color::Default,
                40..=47 => self.pen.bg = Color::Indexed((v - 40) as u8),
                49 => self.pen.bg = Color::Default,
                90..=97 => self.pen.fg = Color::Indexed((v - 90 + 8) as u8),
                100..=107 => self.pen.bg = Color::Indexed((v - 100 + 8) as u8),
                38 | 48 => {
                    let (color, used) = extended_colour(&p[i..]);
                    i += used;
                    if let Some(c) = color {
                        if v == 38 {
                            self.pen.fg = c;
                        } else {
                            self.pen.bg = c;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// The colour that follows a 38 or a 48, and how many parameters it used. The colon form takes the sub-parameters that follow; the semicolon form takes the selector and its
/// arguments. A malformed one gives no colour (and uses what it was made of).
fn extended_colour(rest: &[Param]) -> (Option<Color>, usize) {
    if rest.first().is_some_and(|x| x.colon) {
        let subs: Vec<u16> = rest.iter().take_while(|x| x.colon).map(|x| x.v).collect();
        let used = subs.len();
        let c = match subs.as_slice() {
            [5, n, ..] if *n <= 255 => Some(Color::Indexed(*n as u8)),
            [2, _, r, g, b, ..] if *r <= 255 && *g <= 255 && *b <= 255 => Some(Color::Rgb(*r as u8, *g as u8, *b as u8)),
            [2, r, g, b] if *r <= 255 && *g <= 255 && *b <= 255 => Some(Color::Rgb(*r as u8, *g as u8, *b as u8)),
            _ => None,
        };
        return (c, used);
    }
    match rest.first().map(|x| x.v) {
        Some(5) => match rest.get(1) {
            Some(n) if n.v <= 255 && !n.colon => (Some(Color::Indexed(n.v as u8)), 2),
            Some(_) => (None, 2),
            None => (None, rest.len()),
        },
        Some(2) => {
            if rest.len() >= 4 {
                let (r, g, b) = (rest[1].v, rest[2].v, rest[3].v);
                if r <= 255 && g <= 255 && b <= 255 {
                    (Some(Color::Rgb(r as u8, g as u8, b as u8)), 4)
                } else {
                    (None, 4)
                }
            } else {
                (None, rest.len())
            }
        }
        Some(_) => (None, 1),
        None => (None, 0),
    }
}
