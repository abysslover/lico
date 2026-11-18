//! The input side: the keys, a paste and the mouse, as the bytes a program expects.

mod common;
use common::*;
use lico_vt::{encode_key, encode_mouse, encode_paste, Button, Key, Modes, MouseEvent, MouseKind, MouseMode, Mods};

const NONE: Mods = Mods { shift: false, ctrl: false, alt: false };
const SHIFT: Mods = Mods { shift: true, ctrl: false, alt: false };
const CTRL: Mods = Mods { shift: false, ctrl: true, alt: false };
const ALT: Mods = Mods { shift: false, ctrl: false, alt: true };

fn plain() -> Modes {
    term(10, 3).modes()
}

fn app() -> Modes {
    let mut t = term(10, 3);
    feed(&mut t, "\x1b[?1h");
    t.modes()
}

fn k(key: Key, mods: Mods) -> Vec<u8> {
    encode_key(key, mods, &plain())
}

#[test]
fn a_character_is_its_utf8_and_alt_puts_an_escape_before_it() {
    assert_eq!(k(Key::Char('a'), NONE), b"a");
    assert_eq!(k(Key::Char('한'), NONE), "한".as_bytes());
    assert_eq!(k(Key::Char('A'), SHIFT), b"A");
    assert_eq!(k(Key::Char('x'), ALT), b"\x1bx");
    assert_eq!(k(Key::Char('한'), ALT), [&b"\x1b"[..], "한".as_bytes()].concat());
}

#[test]
fn control_gives_the_control_codes() {
    assert_eq!(k(Key::Char('a'), CTRL), [1]);
    assert_eq!(k(Key::Char('c'), CTRL), [3]);
    assert_eq!(k(Key::Char('Z'), CTRL), [26]);
    assert_eq!(k(Key::Char('d'), CTRL), [4]);
    assert_eq!(k(Key::Char(' '), CTRL), [0]);
    assert_eq!(k(Key::Char('@'), CTRL), [0]);
    assert_eq!(k(Key::Char('['), CTRL), [27]);
    assert_eq!(k(Key::Char('\\'), CTRL), [28]);
    assert_eq!(k(Key::Char(']'), CTRL), [29]);
    assert_eq!(k(Key::Char('^'), CTRL), [30]);
    assert_eq!(k(Key::Char('_'), CTRL), [31]);
    assert_eq!(k(Key::Char('?'), CTRL), [127]);
    assert_eq!(k(Key::Char('1'), CTRL), b"1", "any other control character is the character");
    assert_eq!(k(Key::Char('c'), Mods { ctrl: true, alt: true, shift: false }), [27, 3]);
}

#[test]
fn enter_backspace_tab_and_escape() {
    assert_eq!(k(Key::Enter, NONE), b"\r");
    assert_eq!(k(Key::Backspace, NONE), [127]);
    assert_eq!(k(Key::Backspace, CTRL), [8]);
    assert_eq!(k(Key::Backspace, ALT), [27, 127]);
    assert_eq!(k(Key::Tab, NONE), b"\t");
    assert_eq!(k(Key::Tab, SHIFT), b"\x1b[Z");
    assert_eq!(k(Key::Escape, NONE), [27]);
    assert_eq!(k(Key::Escape, ALT), [27, 27]);
    assert_eq!(k(Key::Enter, ALT), b"\x1b\r");
}

#[test]
fn the_arrows_home_and_end_follow_the_cursor_keys_mode() {
    for (key, letter) in [(Key::Up, 'A'), (Key::Down, 'B'), (Key::Right, 'C'), (Key::Left, 'D'), (Key::Home, 'H'), (Key::End, 'F')] {
        assert_eq!(encode_key(key, NONE, &plain()), format!("\x1b[{letter}").into_bytes(), "{letter}");
        assert_eq!(encode_key(key, NONE, &app()), format!("\x1bO{letter}").into_bytes(), "{letter} in application mode");
    }
}

#[test]
fn a_modifier_gives_the_one_semicolon_form_in_either_mode() {
    assert_eq!(k(Key::Up, SHIFT), b"\x1b[1;2A");
    assert_eq!(k(Key::Left, ALT), b"\x1b[1;3D");
    assert_eq!(k(Key::Right, CTRL), b"\x1b[1;5C");
    assert_eq!(k(Key::Home, Mods { shift: true, ctrl: true, alt: false }), b"\x1b[1;6H");
    assert_eq!(k(Key::End, Mods { shift: true, ctrl: true, alt: true }), b"\x1b[1;8F");
    assert_eq!(encode_key(Key::Up, CTRL, &app()), b"\x1b[1;5A");
}

#[test]
fn insert_delete_and_the_page_keys() {
    assert_eq!(k(Key::Insert, NONE), b"\x1b[2~");
    assert_eq!(k(Key::Delete, NONE), b"\x1b[3~");
    assert_eq!(k(Key::PageUp, NONE), b"\x1b[5~");
    assert_eq!(k(Key::PageDown, NONE), b"\x1b[6~");
    assert_eq!(k(Key::Delete, CTRL), b"\x1b[3;5~");
    assert_eq!(k(Key::PageUp, SHIFT), b"\x1b[5;2~");
    assert_eq!(encode_key(Key::Delete, NONE, &app()), b"\x1b[3~", "the cursor-keys mode does not change them");
}

#[test]
fn the_function_keys() {
    let ss3 = [(1, 'P'), (2, 'Q'), (3, 'R'), (4, 'S')];
    for (n, l) in ss3 {
        assert_eq!(k(Key::F(n), NONE), format!("\x1bO{l}").into_bytes(), "F{n}");
        assert_eq!(k(Key::F(n), CTRL), format!("\x1b[1;5{l}").into_bytes(), "Ctrl+F{n}");
    }
    let tilde = [(5, 15), (6, 17), (7, 18), (8, 19), (9, 20), (10, 21), (11, 23), (12, 24)];
    for (n, code) in tilde {
        assert_eq!(k(Key::F(n), NONE), format!("\x1b[{code}~").into_bytes(), "F{n}");
        assert_eq!(k(Key::F(n), SHIFT), format!("\x1b[{code};2~").into_bytes(), "Shift+F{n}");
    }
    assert!(k(Key::F(0), NONE).is_empty());
    assert!(k(Key::F(13), NONE).is_empty());
}

// ---- paste ----

#[test]
fn a_bracketed_paste_is_wrapped_and_keeps_its_line_breaks() {
    let mut t = term(10, 3);
    feed(&mut t, "\x1b[?2004h");
    assert_eq!(encode_paste("a\nb", &t.modes()), b"\x1b[200~a\nb\x1b[201~");
    assert_eq!(encode_paste("", &t.modes()), b"\x1b[200~\x1b[201~");
}

#[test]
fn a_plain_paste_turns_line_breaks_into_returns() {
    assert_eq!(encode_paste("a\nb\r\nc", &plain()), b"a\rb\rc");
    assert_eq!(encode_paste("한글", &plain()), "한글".as_bytes());
}

#[test]
fn a_pasted_text_cannot_carry_an_escape_or_end_the_bracketed_paste() {
    let mut t = term(10, 3);
    feed(&mut t, "\x1b[?2004h");
    let out = encode_paste("x\x1b[201~rm -rf ~\n", &t.modes());
    assert_eq!(out, b"\x1b[200~x[201~rm -rf ~\n\x1b[201~");
    assert_eq!(out.windows(6).filter(|w| *w == b"\x1b[201~").count(), 1, "the end marker is there once");
    assert_eq!(encode_paste("a\x1bb", &plain()), b"ab");
}

// ---- the mouse ----

fn mouse(kind: MouseKind, col: u16, row: u16, mods: Mods) -> MouseEvent {
    MouseEvent { kind, col, row, mods }
}

fn modes_with(seq: &str) -> Modes {
    let mut t = term(10, 3);
    feed(&mut t, seq);
    t.modes()
}

#[test]
fn nothing_is_reported_while_the_mouse_mode_is_off() {
    for kind in [MouseKind::Press(Button::Left), MouseKind::Release(Button::Left), MouseKind::Move(None), MouseKind::WheelUp] {
        assert!(encode_mouse(mouse(kind, 1, 1, NONE), &plain()).is_empty());
    }
}

#[test]
fn the_press_mode_reports_presses_releases_and_the_wheel_only() {
    let m = modes_with("\x1b[?1000h\x1b[?1006h");
    assert_eq!(m.mouse, MouseMode::Press);
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 0, 0, NONE), &m), b"\x1b[<0;1;1M");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Middle), 4, 2, NONE), &m), b"\x1b[<1;5;3M");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Right), 4, 2, NONE), &m), b"\x1b[<2;5;3M");
    assert_eq!(encode_mouse(mouse(MouseKind::Release(Button::Left), 9, 9, NONE), &m), b"\x1b[<0;10;10m");
    assert_eq!(encode_mouse(mouse(MouseKind::WheelUp, 1, 1, NONE), &m), b"\x1b[<64;2;2M");
    assert_eq!(encode_mouse(mouse(MouseKind::WheelDown, 1, 1, NONE), &m), b"\x1b[<65;2;2M");
    assert!(encode_mouse(mouse(MouseKind::Move(None), 1, 1, NONE), &m).is_empty());
    assert!(encode_mouse(mouse(MouseKind::Move(Some(Button::Left)), 1, 1, NONE), &m).is_empty());
}

#[test]
fn the_drag_mode_adds_moves_with_a_button_and_the_any_mode_all_moves() {
    let d = modes_with("\x1b[?1002h\x1b[?1006h");
    assert_eq!(encode_mouse(mouse(MouseKind::Move(Some(Button::Left)), 2, 3, NONE), &d), b"\x1b[<32;3;4M");
    assert!(encode_mouse(mouse(MouseKind::Move(None), 2, 3, NONE), &d).is_empty());
    let a = modes_with("\x1b[?1003h\x1b[?1006h");
    assert_eq!(encode_mouse(mouse(MouseKind::Move(None), 2, 3, NONE), &a), b"\x1b[<35;3;4M");
    assert_eq!(encode_mouse(mouse(MouseKind::Move(Some(Button::Right)), 2, 3, NONE), &a), b"\x1b[<34;3;4M");
}

#[test]
fn the_modifiers_add_four_eight_and_sixteen() {
    let m = modes_with("\x1b[?1000h\x1b[?1006h");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 0, 0, SHIFT), &m), b"\x1b[<4;1;1M");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 0, 0, ALT), &m), b"\x1b[<8;1;1M");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 0, 0, CTRL), &m), b"\x1b[<16;1;1M");
}

#[test]
fn the_plain_form_is_three_bytes_and_a_release_is_button_three() {
    let m = modes_with("\x1b[?1000h");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 0, 0, NONE), &m), [27, b'[', b'M', 32, 33, 33]);
    assert_eq!(encode_mouse(mouse(MouseKind::Release(Button::Right), 4, 2, NONE), &m), [27, b'[', b'M', 35, 37, 35]);
    assert_eq!(encode_mouse(mouse(MouseKind::WheelUp, 0, 0, NONE), &m), [27, b'[', b'M', 96, 33, 33]);
    assert!(encode_mouse(mouse(MouseKind::Press(Button::Left), 300, 0, NONE), &m).is_empty(), "a coordinate that does not fit a byte");
    assert!(!encode_mouse(mouse(MouseKind::Press(Button::Left), 222, 0, NONE), &m).is_empty());
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 222, 222, NONE), &m), [27, b'[', b'M', 32, 255, 255], "the last cell that fits a byte");
    assert!(encode_mouse(mouse(MouseKind::Press(Button::Left), 223, 0, NONE), &m).is_empty(), "x = 224 would be 256");
    assert!(encode_mouse(mouse(MouseKind::Press(Button::Left), 0, 223, NONE), &m).is_empty(), "y = 224 would be 256");
    let sgr = modes_with("\x1b[?1000h\x1b[?1006h");
    assert_eq!(encode_mouse(mouse(MouseKind::Press(Button::Left), 300, 500, NONE), &sgr), b"\x1b[<0;301;501M", "the SGR form has no limit of 222");
}
