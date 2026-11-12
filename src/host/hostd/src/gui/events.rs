//! What the window says on standard output on its own. `ready` and `state` carry an id and counters. The `ime` events carry what a person typed, composed,
//! pasted-then-edited or sent, so they are printed only when a measurement rig asked for them with `LICO_GUI_EVENTS=1`: the output of the product window may
//! be kept by a launcher in a log. The other answers (`ui`, `lines`, the host status) are given to a command on standard input, so only to whoever holds it.
//! While a panel is open the input line may hold a sign-in code: nothing typed is printed then, switch or not.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

pub(super) static QUIET_INPUT: AtomicBool = AtomicBool::new(false);

fn switched_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LICO_GUI_EVENTS").is_some_and(|v| v == "1"))
}

pub(super) fn emit_ime(kind: &str, text: &str) {
    if QUIET_INPUT.load(Ordering::Relaxed) || !switched_on() {
        return;
    }
    println!("{{\"ev\":\"ime\",\"kind\":\"{}\",\"text\":\"{}\"}}", kind, super::json_escape(text));
    let _ = std::io::stdout().flush();
}

/// Returns the lowercase ASCII letter `'a'..='z'` for a key press, falling back to `physical_key` (`KeyCode::KeyA..=KeyZ`)
/// when an IME layout (such as Korean Hangul `한/영` mode) maps `logical_key` to a non-ASCII character (e.g. `"ㅊ"` on `KeyC`).
pub(crate) fn key_letter(event: &winit::event::KeyEvent) -> Option<char> {
    key_letter_parts(&event.logical_key, &event.physical_key)
}

pub(crate) fn key_letter_parts(logical: &winit::keyboard::Key, physical: &winit::keyboard::PhysicalKey) -> Option<char> {
    use winit::keyboard::{Key, KeyCode, PhysicalKey};
    if let Key::Character(c) = logical {
        let mut chars = c.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if ch.is_ascii_alphabetic() {
                return Some(ch.to_ascii_lowercase());
            }
        }
    }
    match physical {
        PhysicalKey::Code(code) => Some(match code {
            KeyCode::KeyA => 'a', KeyCode::KeyB => 'b', KeyCode::KeyC => 'c', KeyCode::KeyD => 'd', KeyCode::KeyE => 'e', KeyCode::KeyF => 'f',
            KeyCode::KeyG => 'g', KeyCode::KeyH => 'h', KeyCode::KeyI => 'i', KeyCode::KeyJ => 'j', KeyCode::KeyK => 'k', KeyCode::KeyL => 'l',
            KeyCode::KeyM => 'm', KeyCode::KeyN => 'n', KeyCode::KeyO => 'o', KeyCode::KeyP => 'p', KeyCode::KeyQ => 'q', KeyCode::KeyR => 'r',
            KeyCode::KeyS => 's', KeyCode::KeyT => 't', KeyCode::KeyU => 'u', KeyCode::KeyV => 'v', KeyCode::KeyW => 'w', KeyCode::KeyX => 'x',
            KeyCode::KeyY => 'y', KeyCode::KeyZ => 'z',
            _ => return None,
        }),
        _ => None,
    }
}

pub(crate) fn key_letter_cased(event: &winit::event::KeyEvent, shift: bool) -> Option<char> {
    let lo = key_letter(event)?;
    Some(if shift { lo.to_ascii_uppercase() } else { lo })
}
