//! The public names: sizes, the view, the selection text, the modes, the title, the replies, the counters, a resize and the width of a character.

mod common;
use common::*;
use lico_vt::{char_width, Attrs, Cell, Color, MouseMode, Term, MAX_COLS, MAX_ROWS, MAX_TITLE};

#[test]
fn a_new_terminal_is_blank_and_its_size_is_clamped() {
    let t = Term::new(80, 24);
    assert_eq!(t.size(), (80, 24));
    assert_eq!(t.cursor(), (0, 0));
    assert_eq!(t.title(), "");
    assert_eq!(t.bells(), 0);
    let v = t.view(0);
    assert_eq!((v.lines.len(), v.lines[0].len(), v.total, v.top, v.scroll), (24, 80, 24, 0, 0));
    assert!(v.lines.iter().flatten().all(|c| *c == Cell::BLANK));
    assert_eq!(Term::new(0, 0).size(), (1, 1));
    assert_eq!(Term::new(60_000, 60_000).size(), (MAX_COLS, MAX_ROWS));
}

#[test]
fn a_blank_cell_is_a_space_in_default_colours_with_no_attributes() {
    let c = Cell::BLANK;
    assert_eq!((c.ch, c.fg, c.bg, c.attrs), (' ', Color::Default, Color::Default, Attrs::NONE));
}

#[test]
fn attributes_combine_and_are_told_apart() {
    let a = Attrs::BOLD | Attrs::ITALIC;
    assert!(a.contains(Attrs::BOLD) && a.contains(Attrs::ITALIC) && !a.contains(Attrs::DIM));
    assert_eq!(Attrs::NONE.bits(), 0);
    let all = [Attrs::BOLD, Attrs::DIM, Attrs::ITALIC, Attrs::UNDERLINE, Attrs::BLINK, Attrs::INVERSE, Attrs::HIDDEN, Attrs::STRIKE, Attrs::WIDE, Attrs::TAIL];
    for (i, x) in all.iter().enumerate() {
        assert_eq!(x.bits().count_ones(), 1);
        for y in &all[i + 1..] {
            assert_ne!(x.bits(), y.bits());
        }
    }
}

#[test]
fn the_cursor_is_hidden_in_the_view_while_the_program_hides_it_but_still_known() {
    let mut t = run(10, 3, "ab");
    assert_eq!(t.view(0).cursor, Some((0, 2)));
    feed(&mut t, "\x1b[?25l");
    assert_eq!(t.view(0).cursor, None);
    assert_eq!(t.cursor(), (0, 2));
    assert!(!t.modes().cursor_visible);
    feed(&mut t, "\x1b[?25h");
    assert_eq!(t.view(0).cursor, Some((0, 2)));
}

#[test]
fn the_modes_follow_what_the_program_switches() {
    let mut t = term(10, 3);
    let m = t.modes();
    assert!(m.autowrap && m.cursor_visible && !m.app_cursor && !m.bracketed_paste && !m.alt_screen && !m.origin && !m.insert && !m.focus_events && !m.app_keypad);
    assert_eq!((m.mouse, m.mouse_sgr), (MouseMode::Off, false));
    feed(&mut t, "\x1b[?1h\x1b[?2004h\x1b[?1004h\x1b=");
    let m = t.modes();
    assert!(m.app_cursor && m.bracketed_paste && m.focus_events && m.app_keypad);
    feed(&mut t, "\x1b[?1l\x1b[?2004l\x1b[?1004l\x1b>");
    let m = t.modes();
    assert!(!m.app_cursor && !m.bracketed_paste && !m.focus_events && !m.app_keypad);
    feed(&mut t, "\x1b[?1000h");
    assert_eq!(t.modes().mouse, MouseMode::Press);
    feed(&mut t, "\x1b[?1002h");
    assert_eq!(t.modes().mouse, MouseMode::Drag);
    feed(&mut t, "\x1b[?1003h");
    assert_eq!(t.modes().mouse, MouseMode::Any);
    feed(&mut t, "\x1b[?1006h");
    assert!(t.modes().mouse_sgr);
    feed(&mut t, "\x1b[?1003l");
    assert_eq!(t.modes().mouse, MouseMode::Off);
    feed(&mut t, "\x1b[?1006l");
    assert!(!t.modes().mouse_sgr);
    feed(&mut t, "\x1b[?1;2004;9999h\x1b[?25;7l");
    assert!(t.modes().app_cursor && t.modes().bracketed_paste && !t.modes().cursor_visible && !t.modes().autowrap, "several modes in one sequence, unknown ones ignored");
}

// ---- the title ----

#[test]
fn the_title_is_set_by_osc_zero_one_and_two_with_either_ending() {
    let mut t = term(10, 2);
    feed(&mut t, "\x1b]0;first\x07");
    assert_eq!(t.title(), "first");
    feed(&mut t, "\x1b]2;second\x1b\\");
    assert_eq!(t.title(), "second");
    feed(&mut t, "\x1b]1;third\x07");
    assert_eq!(t.title(), "third");
    feed(&mut t, "\x1b]2;with;semicolons\x07");
    assert_eq!(t.title(), "with;semicolons");
    feed(&mut t, "\x1b]2;\x07");
    assert_eq!(t.title(), "", "an empty title clears it");
    assert_eq!(rows(&t), ["", ""], "a title draws nothing");
}

#[test]
fn a_title_is_cut_and_stripped_and_other_strings_do_nothing() {
    let mut t = term(10, 2);
    feed(&mut t, &format!("\x1b]2;{}\x07", "é".repeat(1000)));
    assert_eq!(t.title().chars().count(), MAX_TITLE);
    feed(&mut t, "\x1b]2;a\x01b\x7fc\x07");
    assert_eq!(t.title(), "abc", "control characters are removed");
    feed(&mut t, "\x1b]52;c;aGVsbG8=\x07\x1b]8;;http://x\x07\x1b]7;file:///tmp\x07\x1b]11;?\x07");
    assert_eq!(t.title(), "abc");
    assert!(t.take_reply().is_empty(), "no clipboard write, no colour answer");
}

#[test]
fn a_string_longer_than_the_bound_is_ignored_whole_and_text_goes_on_after_it() {
    let mut t = term(10, 2);
    feed(&mut t, "\x1b]2;kept\x07");
    t.feed(b"\x1b]2;");
    for _ in 0..1000 {
        t.feed(&[b'x'; 1000]);
    }
    feed(&mut t, "\x07ok");
    assert_eq!(t.title(), "kept");
    assert_eq!(rows(&t)[0], "ok");
}

// ---- the replies ----

#[test]
fn the_cursor_position_report_is_one_based() {
    let mut t = run(20, 10, "\x1b[3;5H\x1b[6n");
    assert_eq!(t.take_reply(), b"\x1b[3;5R");
    assert!(t.take_reply().is_empty(), "the buffer is empty after it is taken");
}

#[test]
fn in_origin_mode_the_report_is_relative_to_the_region() {
    let mut t = run(20, 10, "\x1b[3;6r\x1b[?6h\x1b[2;4H\x1b[6n");
    assert_eq!(t.take_reply(), b"\x1b[2;4R");
}

#[test]
fn status_and_device_attributes_are_answered_and_nothing_else_is() {
    let mut t = run(20, 10, "\x1b[5n\x1b[c\x1b[0c");
    assert_eq!(t.take_reply(), b"\x1b[0n\x1b[?62;22c\x1b[?62;22c");
    feed(&mut t, "\x1b[>c\x1b[>0q\x1b[?u\x1b[?1$p\x1b[18t\x1b[=c\x1b]11;?\x07\x1bZ");
    assert!(t.take_reply().is_empty());
}

#[test]
fn replies_come_in_the_order_asked_and_are_bounded() {
    let mut t = term(20, 10);
    feed(&mut t, "\x1b[5n\x1b[6n");
    assert_eq!(t.take_reply(), b"\x1b[0n\x1b[1;1R");
    for _ in 0..100_000 {
        feed(&mut t, "\x1b[5n");
    }
    assert_eq!(t.take_reply().len(), lico_vt::MAX_REPLY);
}

// ---- the counters ----

#[test]
fn the_generation_moves_with_input_and_with_a_real_resize_only() {
    let mut t = term(10, 3);
    let g0 = t.generation();
    t.feed(b"");
    assert_eq!(t.generation(), g0, "an empty call changes nothing");
    feed(&mut t, "a");
    let g1 = t.generation();
    assert!(g1 > g0);
    t.resize(10, 3);
    assert_eq!(t.generation(), g1, "the same size is not a change");
    t.resize(12, 3);
    assert!(t.generation() > g1);
}

// ---- resize ----

#[test]
fn growing_and_shrinking_pad_and_cut_without_reflow() {
    let mut t = run(5, 2, "abcde\r\nfg");
    t.resize(8, 3);
    assert_eq!((t.size(), rows(&t).len()), ((8, 3), 3));
    assert_eq!(rows(&t), ["abcde", "fg", ""]);
    t.resize(3, 3);
    assert_eq!(rows(&t), ["abc", "fg", ""]);
    assert!(t.view(0).lines.iter().all(|l| l.len() == 3));
}

#[test]
fn a_shorter_screen_keeps_the_cursor_row_and_sends_the_top_to_the_scrollback() {
    let mut t = run(5, 4, "1\r\n2\r\n3\r\n4");
    t.resize(5, 2);
    assert_eq!(rows(&t), ["3", "4"]);
    assert_eq!(t.cursor(), (1, 1));
    assert_eq!(t.view(0).total, 4, "two lines went to the scrollback");
    assert_eq!(t.view(99).lines.iter().map(|l| line_text(l)).collect::<Vec<_>>(), ["1", "2"]);
}

#[test]
fn a_shorter_alternate_screen_loses_its_top_rows() {
    let mut t = run(5, 4, "main\x1b[?1049h1\r\n2\r\n3\r\n4");
    t.resize(5, 2);
    assert_eq!(rows(&t), ["3", "4"]);
    assert_eq!(t.view(0).total, 2, "no scrollback on the alternate screen");
    feed(&mut t, "\x1b[?1049l");
    assert_eq!(rows(&t)[..2], ["main", ""]);
}

#[test]
fn a_resize_clamps_the_cursor_resets_the_region_and_cancels_the_pending_wrap() {
    let mut t = run(10, 6, "\x1b[2;4r\x1b[6;10H");
    t.resize(4, 3);
    let (r, c) = t.cursor();
    assert!(r < 3 && c < 4);
    feed(&mut t, "\x1b[H\r\n\r\n\r\nx");
    assert_eq!(rows(&t)[2], "x", "the region is the whole screen again: a line feed scrolls everything");
    let mut t = run(5, 2, "abcde");
    t.resize(5, 2);
    t.resize(6, 2);
    feed(&mut t, "f");
    assert_eq!(rows(&t)[0], "abcdef", "the pending wrap does not survive a change of width");
}

#[test]
fn a_wide_character_cut_by_a_new_width_is_blanked() {
    let mut t = run(6, 2, "abc한");
    t.resize(4, 2);
    assert_eq!(rows(&t)[0], "abc");
    let v = t.view(0);
    assert!(!v.lines[0][3].attrs.contains(Attrs::WIDE));
}

#[test]
fn tab_stops_beyond_the_old_width_are_every_eight() {
    let mut t = term(10, 2);
    t.resize(30, 2);
    feed(&mut t, "\t\t\t");
    assert_eq!(t.cursor(), (0, 24));
}

// ---- the text of a selection ----

#[test]
fn the_text_of_a_selection_cuts_trailing_blanks_and_joins_rows_with_a_break() {
    let t = run(10, 4, "hello  \r\nworld\r\nabc");
    assert_eq!(t.text_between((0, 1), (0, 3)), "ell");
    assert_eq!(t.text_between((0, 0), (1, 4)), "hello\nworld");
    assert_eq!(t.text_between((1, 2), (0, 3)), "lo\nwor", "either order; blanks at a line end are cut");
    assert_eq!(t.text_between((0, 0), (2, 9)), "hello\nworld\nabc");
}

#[test]
fn a_wrapped_line_joins_the_next_without_a_break_and_a_wide_character_is_once() {
    let t = run(5, 3, "abcdefgh\r\nxy");
    assert_eq!(t.text_between((0, 0), (2, 4)), "abcdefgh\nxy");
    let t = run(6, 2, "한글a");
    assert_eq!(t.text_between((0, 0), (0, 5)), "한글a");
    assert_eq!(t.text_between((0, 1), (0, 1)), "한", "a click on the tail takes the character");
}

#[test]
fn the_selection_may_reach_into_the_scrollback_and_is_clamped() {
    let t = run(5, 2, "1\r\n2\r\n3\r\n4");
    assert_eq!(t.text_between((0, 0), (3, 4)), "1\n2\n3\n4");
    assert_eq!(t.text_between((0, 0), (999, 999)), "1\n2\n3\n4");
    assert_eq!(t.text_between((999, 0), (999, 3)), "");
}

// ---- the width of a character ----

#[test]
fn the_width_of_a_character() {
    for c in ['a', 'Z', ' ', 'é', 'ｱ', '─', '┌', '·'] {
        assert_eq!(char_width(c), 1, "{c}");
    }
    for c in ['한', '글', 'あ', 'ア', '漢', 'Ａ', '😀', '🚀', '가', 'ㄱ', '、'] {
        assert_eq!(char_width(c), 2, "{c}");
    }
    for c in ['\u{301}', '\u{200d}', '\u{200b}', '\u{fe0f}', '\u{0}', '\u{7}', '\u{7f}'] {
        assert_eq!(char_width(c), 0, "{:x}", c as u32);
    }
}

#[test]
fn the_width_at_the_borders_of_its_ranges() {
    for c in ['\u{300}', '\u{36f}', '\u{80}', '\u{9f}'] {
        assert_eq!(char_width(c), 0, "{:x}", c as u32);
    }
    for c in ['\u{a0}', '\u{2ff}', '\u{370}', '\u{ff61}', '\u{ff9f}', '\u{ffe7}'] {
        assert_eq!(char_width(c), 1, "{:x}", c as u32);
    }
    for c in ['\u{ac00}', '\u{d7a3}', '\u{ff01}', '\u{ff60}', '\u{ffe0}', '\u{1100}', '\u{115f}'] {
        assert_eq!(char_width(c), 2, "{:x}", c as u32);
    }
}

#[test]
fn the_scrollback_keeps_a_short_line_as_a_short_line() {
    // 3000 lines of one letter on a 500-wide screen: kept as 1 cell each, so the line limit (2000) is what bounds them, not the cell limit (600000 / 500 = 1200 lines)
    let mut t = term(500, 5);
    for _ in 0..3000 {
        t.feed(b"x\r\n");
    }
    assert_eq!(t.view(0).total, 2000 + 5);
}
