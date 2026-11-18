//! The byte stream: UTF-8 as it arrives and the state machine of the escape sequences (the shape of the DEC parser: ground, escape, CSI, strings).

use crate::term::{Charset, Term};
use crate::{MAX_OSC, MAX_PARAMS, MAX_TITLE};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum State {
    Ground,
    Escape,
    EscInter,
    Csi,
    CsiIgnore,
    Osc,
    /// DCS, SOS, PM and APC: skipped to their end.
    Str,
}

/// A parameter of a CSI sequence; `colon`: it was separated from the one before by `:` (a sub-parameter), not by `;`.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Param {
    pub v: u16,
    pub colon: bool,
}

pub(crate) struct Parser {
    pub state: State,
    // UTF-8 in progress
    need: u8,
    acc: u32,
    lo: u8,
    hi: u8,
    // a CSI or an ESC with intermediates
    pub params: Vec<Param>,
    pub prefix: u8,
    pub inter: [u8; 2],
    pub ninter: u8,
    seen: bool,
    /// A separator was dropped (`MAX_PARAMS` reached): the digits that follow belong to a parameter that is not kept.
    full: bool,
    // an OSC string
    osc: Vec<u8>,
    osc_over: bool,
}

impl Parser {
    pub fn new() -> Parser {
        Parser { state: State::Ground, need: 0, acc: 0, lo: 0x80, hi: 0xBF, params: Vec::new(), prefix: 0, inter: [0; 2], ninter: 0, seen: false, full: false, osc: Vec::new(), osc_over: false }
    }

    fn start_csi(&mut self) {
        self.params.clear();
        self.params.push(Param::default());
        self.prefix = 0;
        self.ninter = 0;
        self.seen = false;
        self.full = false;
        self.state = State::Csi;
    }

    fn add_inter(&mut self, b: u8) -> bool {
        if self.ninter < 2 {
            self.inter[self.ninter as usize] = b;
            self.ninter += 1;
            true
        } else {
            false
        }
    }

    fn separator(&mut self, colon: bool) {
        self.seen = true;
        if self.params.len() < MAX_PARAMS {
            self.params.push(Param { v: 0, colon });
        } else {
            self.full = true;
        }
    }

    fn digit(&mut self, d: u8) {
        self.seen = true;
        if self.full {
            return;
        }
        if let Some(p) = self.params.last_mut() {
            p.v = (p.v as u32 * 10 + d as u32).min(65_535) as u16;
        }
    }
}

impl Term {
    pub(crate) fn step(&mut self, b: u8) {
        match self.p.state {
            State::Ground => self.ground(b),
            State::Escape => self.escape(b),
            State::EscInter => self.esc_inter(b),
            State::Csi => self.csi_byte(b),
            State::CsiIgnore => self.csi_ignore(b),
            State::Osc => self.osc_byte(b),
            State::Str => self.str_byte(b),
        }
    }

    // ---- the printable text and UTF-8 ----

    fn ground(&mut self, b: u8) {
        if self.p.need > 0 {
            if b >= self.p.lo && b <= self.p.hi {
                self.p.acc = (self.p.acc << 6) | (b & 0x3F) as u32;
                self.p.need -= 1;
                self.p.lo = 0x80;
                self.p.hi = 0xBF;
                if self.p.need == 0 {
                    let c = char::from_u32(self.p.acc).unwrap_or('\u{FFFD}');
                    self.print(c);
                }
                return;
            }
            // a sequence cut short: one replacement for the part, and the byte is read again
            self.p.need = 0;
            self.print('\u{FFFD}');
        }
        match b {
            0x1B => self.p.state = State::Escape,
            0x18 | 0x1A | 0x7F => {}
            0x00..=0x1F => self.control(b),
            0x20..=0x7E => self.print(b as char),
            0xC2..=0xDF => self.utf8_lead(1, b & 0x1F, 0x80, 0xBF),
            0xE0 => self.utf8_lead(2, b & 0x0F, 0xA0, 0xBF),
            0xE1..=0xEC | 0xEE..=0xEF => self.utf8_lead(2, b & 0x0F, 0x80, 0xBF),
            0xED => self.utf8_lead(2, b & 0x0F, 0x80, 0x9F),
            0xF0 => self.utf8_lead(3, b & 0x07, 0x90, 0xBF),
            0xF1..=0xF3 => self.utf8_lead(3, b & 0x07, 0x80, 0xBF),
            0xF4 => self.utf8_lead(3, b & 0x07, 0x80, 0x8F),
            _ => self.print('\u{FFFD}'),
        }
    }

    fn utf8_lead(&mut self, need: u8, bits: u8, lo: u8, hi: u8) {
        self.p.need = need;
        self.p.acc = bits as u32;
        self.p.lo = lo;
        self.p.hi = hi;
    }

    fn print(&mut self, c: char) {
        let line_drawing = self.g[self.shift] == Charset::Line;
        let c = if line_drawing { line_char(c) } else { c };
        self.put_char(c);
    }

    /// The C0 controls that do something.
    fn control(&mut self, b: u8) {
        match b {
            0x07 => self.bells += 1,
            0x08 => {
                self.col = self.col.saturating_sub(1);
                self.pending = false;
            }
            0x09 => {
                self.col = self.next_tab(self.col);
                self.pending = false;
            }
            0x0A..=0x0C => {
                self.pending = false;
                self.index();
            }
            0x0D => {
                self.col = 0;
                self.pending = false;
            }
            0x0E => self.shift = 1,
            0x0F => self.shift = 0,
            _ => {}
        }
    }

    // ---- escape ----

    fn escape(&mut self, b: u8) {
        match b {
            0x18 | 0x1A => self.p.state = State::Ground,
            0x1B | 0x7F => {}
            0x00..=0x1F => self.control(b),
            0x20..=0x2F => {
                self.p.ninter = 0;
                self.p.add_inter(b);
                self.p.state = State::EscInter;
            }
            b'[' => self.p.start_csi(),
            b']' => {
                self.p.osc.clear();
                self.p.osc_over = false;
                self.p.state = State::Osc;
            }
            b'P' | b'X' | b'^' | b'_' => self.p.state = State::Str,
            0x80..=0xFF => {
                self.p.state = State::Ground;
                self.ground(b);
            }
            _ => {
                self.p.state = State::Ground;
                match b {
                    b'7' => self.save_cursor(),
                    b'8' => self.restore_cursor(),
                    b'D' => self.index(),
                    b'E' => {
                        self.col = 0;
                        self.pending = false;
                        self.index();
                    }
                    b'M' => {
                        self.pending = false;
                        self.reverse_index();
                    }
                    b'H' => self.tabs[self.col as usize] = true,
                    b'c' => self.reset(),
                    b'=' => self.modes.app_keypad = true,
                    b'>' => self.modes.app_keypad = false,
                    _ => {}
                }
            }
        }
    }

    fn esc_inter(&mut self, b: u8) {
        match b {
            0x18 | 0x1A => self.p.state = State::Ground,
            0x1B => self.p.state = State::Escape,
            0x7F => {}
            0x00..=0x1F => self.control(b),
            0x20..=0x2F => {
                self.p.add_inter(b);
            }
            0x30..=0x7E => {
                let set = if b == b'0' { Charset::Line } else { Charset::Ascii };
                match self.p.inter[0] {
                    b'(' => self.g[0] = set,
                    b')' => self.g[1] = set,
                    _ => {}
                }
                self.p.state = State::Ground;
            }
            _ => {
                self.p.state = State::Ground;
                self.ground(b);
            }
        }
    }

    // ---- CSI ----

    fn csi_byte(&mut self, b: u8) {
        match b {
            0x18 | 0x1A => self.p.state = State::Ground,
            0x1B => self.p.state = State::Escape,
            0x7F => {}
            0x00..=0x1F => self.control(b),
            b'0'..=b'9' => {
                if self.p.ninter > 0 {
                    self.p.state = State::CsiIgnore;
                } else {
                    self.p.digit(b - b'0');
                }
            }
            b':' | b';' => {
                if self.p.ninter > 0 {
                    self.p.state = State::CsiIgnore;
                } else {
                    self.p.separator(b == b':');
                }
            }
            0x3C..=0x3F => {
                if self.p.seen || self.p.prefix != 0 || self.p.ninter > 0 {
                    self.p.state = State::CsiIgnore;
                } else {
                    self.p.prefix = b;
                }
            }
            0x20..=0x2F => {
                if !self.p.add_inter(b) {
                    self.p.state = State::CsiIgnore;
                }
            }
            0x40..=0x7E => {
                self.p.state = State::Ground;
                self.csi_dispatch(b);
            }
            _ => {
                self.p.state = State::Ground;
                self.ground(b);
            }
        }
    }

    fn csi_ignore(&mut self, b: u8) {
        match b {
            0x18 | 0x1A => self.p.state = State::Ground,
            0x1B => self.p.state = State::Escape,
            0x00..=0x1F => self.control(b),
            0x40..=0x7E => self.p.state = State::Ground,
            0x80..=0xFF => {
                self.p.state = State::Ground;
                self.ground(b);
            }
            _ => {}
        }
    }

    // ---- strings ----

    fn osc_byte(&mut self, b: u8) {
        match b {
            0x07 => {
                self.finish_osc();
                self.p.state = State::Ground;
            }
            0x18 | 0x1A => {
                self.p.osc.clear();
                self.p.state = State::Ground;
            }
            0x1B => {
                self.finish_osc();
                self.p.state = State::Escape;
            }
            0x00..=0x1F | 0x7F => {}
            _ => {
                if self.p.osc.len() < MAX_OSC {
                    self.p.osc.push(b);
                } else {
                    self.p.osc_over = true;
                }
            }
        }
    }

    fn str_byte(&mut self, b: u8) {
        match b {
            0x18 | 0x1A => self.p.state = State::Ground,
            0x1B => self.p.state = State::Escape,
            _ => {}
        }
    }

    /// OSC 0, 1 and 2 set the title; every other one is ignored (OSC 52, the clipboard, on purpose).
    fn finish_osc(&mut self) {
        let over = std::mem::replace(&mut self.p.osc_over, false);
        let data = std::mem::take(&mut self.p.osc);
        if over {
            return;
        }
        let Some(semi) = data.iter().position(|b| *b == b';') else { return };
        if !matches!(&data[..semi], b"0" | b"1" | b"2") {
            return;
        }
        self.title = String::from_utf8_lossy(&data[semi + 1..]).chars().filter(|c| !c.is_control()).take(MAX_TITLE).collect();
    }
}

/// The DEC special graphics set: `` ` `` to `~` are the line-drawing characters.
fn line_char(c: char) -> char {
    match c {
        '`' => '◆',
        'a' => '▒',
        'f' => '°',
        'g' => '±',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => c,
    }
}
