//! The input side: the keys, a paste and the mouse, as the bytes a program expects (the sequences of xterm).

use crate::types::{Modes, MouseMode};

/// The keys the window gives; a key the person types as text is `Char`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// `F(1)` to `F(12)`; any other number has no bytes.
    F(u8),
}

/// The modifier keys held.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Mods {
    fn any(self) -> bool {
        self.shift || self.ctrl || self.alt
    }

    /// The `m` of the sequences: 1 plus 1 for Shift, 2 for Alt, 4 for Ctrl.
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }

    /// The bits the mouse adds to its button code.
    fn mouse_bits(self) -> u16 {
        4 * self.shift as u16 + 8 * self.alt as u16 + 16 * self.ctrl as u16
    }
}

/// The bytes for the program; empty when the key has none.
pub fn encode_key(key: Key, mods: Mods, modes: &Modes) -> Vec<u8> {
    let mut out = Vec::new();
    match key {
        Key::Char(c) => {
            if mods.alt {
                out.push(0x1B);
            }
            match control_code(c).filter(|_| mods.ctrl) {
                Some(b) => out.push(b),
                None => {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
        Key::Enter => alt_then(&mut out, mods, b"\r"),
        Key::Backspace => alt_then(&mut out, mods, if mods.ctrl { &[8] } else { &[127] }),
        Key::Tab => {
            if mods.shift {
                alt_then(&mut out, mods, b"\x1b[Z");
            } else {
                alt_then(&mut out, mods, b"\t");
            }
        }
        Key::Escape => alt_then(&mut out, mods, &[0x1B]),
        Key::Up => cursor_key(&mut out, b'A', mods, modes),
        Key::Down => cursor_key(&mut out, b'B', mods, modes),
        Key::Right => cursor_key(&mut out, b'C', mods, modes),
        Key::Left => cursor_key(&mut out, b'D', mods, modes),
        Key::Home => cursor_key(&mut out, b'H', mods, modes),
        Key::End => cursor_key(&mut out, b'F', mods, modes),
        Key::Insert => tilde_key(&mut out, 2, mods),
        Key::Delete => tilde_key(&mut out, 3, mods),
        Key::PageUp => tilde_key(&mut out, 5, mods),
        Key::PageDown => tilde_key(&mut out, 6, mods),
        Key::F(n @ 1..=4) => {
            let letter = b"PQRS"[n as usize - 1];
            if mods.any() {
                out.extend_from_slice(format!("\x1b[1;{}{}", mods.param(), letter as char).as_bytes());
            } else {
                out.extend_from_slice(&[0x1B, b'O', letter]);
            }
        }
        Key::F(n @ 5..=12) => tilde_key(&mut out, [15, 17, 18, 19, 20, 21, 23, 24][n as usize - 5], mods),
        Key::F(_) => {}
    }
    out
}

fn alt_then(out: &mut Vec<u8>, mods: Mods, bytes: &[u8]) {
    if mods.alt {
        out.push(0x1B);
    }
    out.extend_from_slice(bytes);
}

/// What Ctrl turns a character into, if it turns it into anything.
fn control_code(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        ' ' | '@' => Some(0),
        '[' => Some(27),
        '\\' => Some(28),
        ']' => Some(29),
        '^' => Some(30),
        '_' => Some(31),
        '?' => Some(127),
        _ => None,
    }
}

fn cursor_key(out: &mut Vec<u8>, letter: u8, mods: Mods, modes: &Modes) {
    if mods.any() {
        out.extend_from_slice(format!("\x1b[1;{}{}", mods.param(), letter as char).as_bytes());
    } else if modes.app_cursor {
        out.extend_from_slice(&[0x1B, b'O', letter]);
    } else {
        out.extend_from_slice(&[0x1B, b'[', letter]);
    }
}

fn tilde_key(out: &mut Vec<u8>, code: u8, mods: Mods) {
    if mods.any() {
        out.extend_from_slice(format!("\x1b[{code};{}~", mods.param()).as_bytes());
    } else {
        out.extend_from_slice(format!("\x1b[{code}~").as_bytes());
    }
}

/// The bytes of a paste: every `ESC` removed; wrapped as the program asked (bracketed paste), or with the line breaks turned into returns.
pub fn encode_paste(text: &str, modes: &Modes) -> Vec<u8> {
    let clean: String = text.chars().filter(|c| *c != '\x1b').collect();
    if modes.bracketed_paste {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(clean.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        clean.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Middle,
    Right,
}

impl Button {
    fn code(self) -> u16 {
        match self {
            Button::Left => 0,
            Button::Middle => 1,
            Button::Right => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseKind {
    Press(Button),
    Release(Button),
    /// A move, with the button that is held if there is one.
    Move(Option<Button>),
    WheelUp,
    WheelDown,
}

/// A mouse event at a cell (`col` and `row` are 0-based).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub col: u16,
    pub row: u16,
    pub mods: Mods,
}

/// The mouse as the program asked for it; empty when the mode does not report the event.
pub fn encode_mouse(ev: MouseEvent, modes: &Modes) -> Vec<u8> {
    let reported = match (modes.mouse, ev.kind) {
        (MouseMode::Off, _) => false,
        (_, MouseKind::Move(Some(_))) => modes.mouse != MouseMode::Press,
        (_, MouseKind::Move(None)) => modes.mouse == MouseMode::Any,
        _ => true,
    };
    if !reported {
        return Vec::new();
    }
    let (code, release) = match ev.kind {
        MouseKind::Press(b) => (b.code(), false),
        MouseKind::Release(b) => (if modes.mouse_sgr { b.code() } else { 3 }, true),
        MouseKind::Move(Some(b)) => (32 + b.code(), false),
        MouseKind::Move(None) => (32 + 3, false),
        MouseKind::WheelUp => (64, false),
        MouseKind::WheelDown => (65, false),
    };
    let code = code + ev.mods.mouse_bits();
    let (x, y) = (ev.col as u32 + 1, ev.row as u32 + 1);
    if modes.mouse_sgr {
        let end = if release { 'm' } else { 'M' };
        format!("\x1b[<{code};{x};{y}{end}").into_bytes()
    } else if 32 + x > 255 || 32 + y > 255 {
        Vec::new()
    } else {
        vec![0x1B, b'[', b'M', 32 + code as u8, 32 + x as u8, 32 + y as u8]
    }
}
