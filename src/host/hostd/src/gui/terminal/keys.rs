//! Which keys of the person are the shell's and which are Lico's (`M23-P03`, D4): terminal first. A pure function of the key, the text it makes, the modifiers held and what the program
//! switched on; the window turns a `KeyEvent` into these three and acts on the answer. The bytes of a key are `lico_vt::encode_key`'s (the sequences of xterm); nothing is encoded here.

use lico_vt::{encode_key, Key, Modes, Mods};
use winit::keyboard::NamedKey;

/// What the window does itself, not the shell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Lico {
    Copy,
    /// The clipboard.
    Paste,
    /// The primary selection (Shift+Insert).
    PastePrimary,
    Close,
    HistoryUp,
    HistoryDown,
    /// Back to the live screen.
    HistoryEnd,
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum Routed {
    Lico(Lico),
    /// For the shell (may be empty: a key that has no bytes is taken and nothing is sent).
    Send(Vec<u8>),
    /// Not the terminal's: the window's own handlers get it.
    Pass,
}

/// The modifier keys held.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct Held {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

/// The logical key of an event: a named key, the characters a key makes, or something else (a dead key, an unidentified one).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Logical<'a> {
    Named(NamedKey),
    Char(&'a str),
    Other,
}

/// The key of the emulator that a named key is; none for a key that is not a terminal's (a modifier, a media key).
pub(crate) fn named_key(k: NamedKey) -> Option<Key> {
    Some(match k {
        NamedKey::Enter => Key::Enter,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Tab => Key::Tab,
        NamedKey::Escape => Key::Escape,
        NamedKey::Space => Key::Char(' '),
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::Insert => Key::Insert,
        NamedKey::Delete => Key::Delete,
        NamedKey::F1 => Key::F(1),
        NamedKey::F2 => Key::F(2),
        NamedKey::F3 => Key::F(3),
        NamedKey::F4 => Key::F(4),
        NamedKey::F5 => Key::F(5),
        NamedKey::F6 => Key::F(6),
        NamedKey::F7 => Key::F(7),
        NamedKey::F8 => Key::F(8),
        NamedKey::F9 => Key::F(9),
        NamedKey::F10 => Key::F(10),
        NamedKey::F11 => Key::F(11),
        NamedKey::F12 => Key::F(12),
        _ => return None,
    })
}

/// Text the person typed that goes to the shell as it is: not empty and no control character (a control character is a key's, and the key is encoded as a key).
fn typed(text: Option<&str>) -> Option<&str> {
    text.filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
}

/// Where a key goes. `text` is what the system says the key typed (the shifted letter, a character a dead key made, an AltGr character).
pub(crate) fn route(key: Logical<'_>, text: Option<&str>, held: Held, modes: &Modes) -> Routed {
    if held.logo {
        return Routed::Pass;
    }
    let mods = Mods { shift: held.shift, ctrl: held.ctrl, alt: held.alt };
    match key {
        Logical::Char(s) if held.ctrl && held.shift => match s.chars().next().map(|c| c.to_ascii_lowercase()) {
            Some('c') => Routed::Lico(Lico::Copy),
            Some('v') => Routed::Lico(Lico::Paste),
            Some('w') => Routed::Lico(Lico::Close),
            _ => Routed::Pass, // the other Ctrl+Shift+letter chords are the window's own namespace
        },
        Logical::Named(k) => {
            let plain_shift = held.shift && !held.ctrl && !held.alt;
            match k {
                NamedKey::Insert if plain_shift => return Routed::Lico(Lico::PastePrimary),
                NamedKey::PageUp if plain_shift => return Routed::Lico(Lico::HistoryUp),
                NamedKey::PageDown if plain_shift => return Routed::Lico(Lico::HistoryDown),
                NamedKey::End if plain_shift => return Routed::Lico(Lico::HistoryEnd),
                NamedKey::Tab if held.ctrl => return Routed::Pass, // Ctrl+Tab goes round the window's tabs
                _ => {}
            }
            Routed::Send(named_key(k).map(|key| encode_key(key, mods, modes)).unwrap_or_default())
        }
        Logical::Char(s) => {
            let Some(c) = s.chars().next() else { return Routed::Pass };
            if held.ctrl || held.alt {
                return Routed::Send(encode_key(Key::Char(c), mods, modes));
            }
            Routed::Send(typed(text).unwrap_or(s).as_bytes().to_vec())
        }
        Logical::Other => Routed::Pass,
    }
}
