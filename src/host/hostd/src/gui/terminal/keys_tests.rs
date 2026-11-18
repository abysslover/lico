//! The unit tests of the terminal tab's key routing (`keys.rs`): the shell gets every key except the few that are Lico's.

use super::keys::*;
use super::screen::Wire;
use winit::keyboard::NamedKey;

fn none() -> Held {
    Held::default()
}

fn shift() -> Held {
    Held { shift: true, ..none() }
}

fn ctrl() -> Held {
    Held { ctrl: true, ..none() }
}

fn alt() -> Held {
    Held { alt: true, ..none() }
}

fn ctrl_shift() -> Held {
    Held { ctrl: true, shift: true, ..none() }
}

fn go(key: Logical<'_>, text: Option<&str>, held: Held) -> Routed {
    route(key, text, held, &Wire::default().modes())
}

fn sent(key: Logical<'_>, text: Option<&str>, held: Held) -> Vec<u8> {
    match go(key, text, held) {
        Routed::Send(b) => b,
        other => panic!("{key:?} {held:?} was not sent: {other:?}"),
    }
}

#[test]
fn plain_text_goes_to_the_shell_as_the_system_made_it() {
    assert_eq!(sent(Logical::Char("a"), Some("a"), none()), b"a");
    assert_eq!(sent(Logical::Char("a"), Some("A"), shift()), b"A", "the shifted letter is the text, not the key");
    assert_eq!(sent(Logical::Char("\u{d55c}"), Some("\u{d55c}"), none()), "\u{d55c}".as_bytes(), "a Hangul syllable");
    assert_eq!(sent(Logical::Char("a"), None, none()), b"a", "no text: the key's own characters");
    assert_eq!(sent(Logical::Char("e"), Some("\u{e9}"), none()), "\u{e9}".as_bytes(), "a dead key's character");
    assert_eq!(sent(Logical::Char("a"), Some("\u{1}"), none()), b"a", "a control character as the text is not typed: the key is");
    assert_eq!(sent(Logical::Char("a"), Some(""), none()), b"a");
}

#[test]
fn space_and_the_editing_keys_have_their_bytes() {
    assert_eq!(sent(Logical::Named(NamedKey::Space), Some(" "), none()), b" ");
    assert_eq!(sent(Logical::Named(NamedKey::Enter), Some("\r"), none()), b"\r");
    assert_eq!(sent(Logical::Named(NamedKey::Backspace), None, none()), b"\x7f");
    assert_eq!(sent(Logical::Named(NamedKey::Tab), None, none()), b"\t");
    assert_eq!(sent(Logical::Named(NamedKey::Tab), None, shift()), b"\x1b[Z");
    assert_eq!(sent(Logical::Named(NamedKey::Escape), None, none()), b"\x1b", "Esc is the shell's: it does not leave the terminal");
    assert_eq!(sent(Logical::Named(NamedKey::Delete), None, none()), b"\x1b[3~");
    assert_eq!(sent(Logical::Named(NamedKey::Insert), None, none()), b"\x1b[2~");
    assert_eq!(sent(Logical::Named(NamedKey::Home), None, none()), b"\x1b[H");
    assert_eq!(sent(Logical::Named(NamedKey::End), None, none()), b"\x1b[F");
    assert_eq!(sent(Logical::Named(NamedKey::PageUp), None, none()), b"\x1b[5~");
    assert_eq!(sent(Logical::Named(NamedKey::PageDown), None, none()), b"\x1b[6~");
}

#[test]
fn the_function_keys_are_the_shells() {
    assert_eq!(sent(Logical::Named(NamedKey::F1), None, none()), b"\x1bOP");
    assert_eq!(sent(Logical::Named(NamedKey::F4), None, none()), b"\x1bOS");
    assert_eq!(sent(Logical::Named(NamedKey::F5), None, none()), b"\x1b[15~");
    assert_eq!(sent(Logical::Named(NamedKey::F10), None, none()), b"\x1b[21~", "F10 does not open a menu of the window");
    assert_eq!(sent(Logical::Named(NamedKey::F12), None, none()), b"\x1b[24~");
}

#[test]
fn the_arrows_follow_the_cursor_mode_the_program_asked_for() {
    let normal = Wire::default().modes();
    let app = Wire { app_cursor: true, ..Wire::default() }.modes();
    let up = Logical::Named(NamedKey::ArrowUp);
    assert_eq!(route(up, None, none(), &normal), Routed::Send(b"\x1b[A".to_vec()));
    assert_eq!(route(up, None, none(), &app), Routed::Send(b"\x1bOA".to_vec()), "application cursor keys");
    assert_eq!(route(Logical::Named(NamedKey::ArrowLeft), None, none(), &app), Routed::Send(b"\x1bOD".to_vec()));
    assert_eq!(route(up, None, ctrl(), &app), Routed::Send(b"\x1b[1;5A".to_vec()), "a modifier makes the long form in both modes");
    assert_eq!(route(Logical::Named(NamedKey::ArrowRight), None, alt(), &normal), Routed::Send(b"\x1b[1;3C".to_vec()));
}

#[test]
fn ctrl_and_alt_with_a_letter_are_the_shells_not_the_windows() {
    assert_eq!(sent(Logical::Char("c"), Some("\u{3}"), ctrl()), vec![3], "Ctrl+C interrupts");
    assert_eq!(sent(Logical::Char("a"), Some("\u{1}"), ctrl()), vec![1], "Ctrl+A is the start of the line, not the window's Accounts");
    assert_eq!(sent(Logical::Char("n"), Some("\u{e}"), ctrl()), vec![14], "Ctrl+N is the next history line, not a new session");
    assert_eq!(sent(Logical::Char("r"), None, ctrl()), vec![18]);
    assert_eq!(sent(Logical::Char("d"), None, ctrl()), vec![4]);
    assert_eq!(sent(Logical::Char("["), None, ctrl()), vec![27]);
    assert_eq!(sent(Logical::Char("b"), Some("b"), alt()), b"\x1bb", "Alt+B is the word back");
    assert_eq!(sent(Logical::Char("1"), Some("1"), alt()), b"\x1b1", "Alt+digit is not the window's session by number");
    assert_eq!(sent(Logical::Char("F"), Some("F"), Held { alt: true, shift: true, ..none() }), b"\x1bF");
    assert_eq!(sent(Logical::Named(NamedKey::Backspace), None, ctrl()), vec![8]);
    assert_eq!(sent(Logical::Named(NamedKey::Space), None, ctrl()), vec![0], "Ctrl+Space");
}

#[test]
fn lico_has_a_few_keys_of_its_own() {
    assert_eq!(go(Logical::Char("C"), Some("\u{3}"), ctrl_shift()), Routed::Lico(Lico::Copy));
    assert_eq!(go(Logical::Char("c"), None, ctrl_shift()), Routed::Lico(Lico::Copy), "either case of the letter");
    assert_eq!(go(Logical::Char("V"), None, ctrl_shift()), Routed::Lico(Lico::Paste));
    assert_eq!(go(Logical::Char("W"), None, ctrl_shift()), Routed::Lico(Lico::Close));
    assert_eq!(go(Logical::Named(NamedKey::Insert), None, shift()), Routed::Lico(Lico::PastePrimary));
    assert_eq!(go(Logical::Named(NamedKey::PageUp), None, shift()), Routed::Lico(Lico::HistoryUp));
    assert_eq!(go(Logical::Named(NamedKey::PageDown), None, shift()), Routed::Lico(Lico::HistoryDown));
    assert_eq!(go(Logical::Named(NamedKey::End), None, shift()), Routed::Lico(Lico::HistoryEnd));
}

#[test]
fn the_keys_that_are_lico_only_with_shift_alone_are_the_shells_with_another_modifier() {
    assert_eq!(sent(Logical::Named(NamedKey::PageUp), None, ctrl()), b"\x1b[5;5~");
    assert_eq!(sent(Logical::Named(NamedKey::PageUp), None, Held { shift: true, alt: true, ..none() }), b"\x1b[5;4~");
    assert_eq!(sent(Logical::Named(NamedKey::Insert), None, ctrl_shift()), b"\x1b[2;6~");
    assert_eq!(sent(Logical::Named(NamedKey::End), None, Held { shift: true, ctrl: true, ..none() }), b"\x1b[1;6F");
    assert_eq!(sent(Logical::Char("c"), Some("c"), shift()), b"c", "Shift alone is a letter");
    assert_eq!(sent(Logical::Char("c"), Some("\u{3}"), ctrl()), vec![3], "Ctrl+C without Shift interrupts: it never copies");
}

#[test]
fn other_ctrl_shift_letters_and_ctrl_tab_and_the_logo_key_go_on_to_the_window() {
    assert_eq!(go(Logical::Char("N"), None, ctrl_shift()), Routed::Pass);
    assert_eq!(go(Logical::Char("S"), None, ctrl_shift()), Routed::Pass, "send to session");
    assert_eq!(go(Logical::Named(NamedKey::Tab), None, ctrl()), Routed::Pass);
    assert_eq!(go(Logical::Named(NamedKey::Tab), None, ctrl_shift()), Routed::Pass);
    assert_eq!(go(Logical::Char("c"), None, Held { logo: true, ..none() }), Routed::Pass);
    assert_eq!(go(Logical::Named(NamedKey::Enter), None, Held { logo: true, ..none() }), Routed::Pass);
    assert_eq!(go(Logical::Char("c"), None, Held { logo: true, ctrl: true, shift: true, ..none() }), Routed::Pass, "the logo key wins over the copy chord");
    assert_eq!(go(Logical::Other, None, none()), Routed::Pass);
    assert_eq!(go(Logical::Char("", ), None, none()), Routed::Pass, "no characters, no key");
}

#[test]
fn a_named_key_that_is_not_a_terminals_is_taken_and_sends_nothing() {
    assert_eq!(go(Logical::Named(NamedKey::Shift), None, shift()), Routed::Send(Vec::new()), "a modifier pressed alone");
    assert_eq!(go(Logical::Named(NamedKey::Control), None, ctrl()), Routed::Send(Vec::new()));
    assert_eq!(go(Logical::Named(NamedKey::CapsLock), None, none()), Routed::Send(Vec::new()));
    assert_eq!(go(Logical::Named(NamedKey::F20), None, none()), Routed::Send(Vec::new()));
}

#[test]
fn the_named_keys_table_has_the_keys_of_a_terminal_and_not_the_modifiers() {
    use lico_vt::Key;
    assert_eq!(named_key(NamedKey::Enter), Some(Key::Enter));
    assert_eq!(named_key(NamedKey::Space), Some(Key::Char(' ')));
    assert_eq!(named_key(NamedKey::F7), Some(Key::F(7)));
    assert_eq!(named_key(NamedKey::ArrowDown), Some(Key::Down));
    assert_eq!(named_key(NamedKey::Shift), None);
    assert_eq!(named_key(NamedKey::Alt), None);
}
