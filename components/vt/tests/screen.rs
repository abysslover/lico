//! The screen: text, the cursor, the controls, erase and edit, the scroll region and the scrollback, SGR, wide characters, UTF-8, the character sets, the alternate screen and the strings.

mod common;
use common::*;
use lico_vt::{Attrs, Cell, Color, Term};

// ---- text and the right edge ----

#[test]
fn text_is_written_at_the_cursor_and_the_cursor_follows() {
    let t = run(10, 3, "abc");
    assert_eq!(rows(&t), ["abc", "", ""]);
    assert_eq!(t.cursor(), (0, 3));
}

#[test]
fn the_right_edge_wraps_only_when_the_next_character_comes() {
    let mut t = run(5, 3, "abcde");
    assert_eq!(t.cursor(), (0, 4), "the cursor waits on the last column");
    assert_eq!(rows(&t), ["abcde", "", ""]);
    assert!(!t.view(0).wrapped[0], "no wrap yet");
    feed(&mut t, "f");
    assert_eq!(rows(&t), ["abcde", "f", ""]);
    assert!(t.view(0).wrapped[0], "the first row goes on in the second");
    assert_eq!(t.cursor(), (1, 1));
}

#[test]
fn a_return_or_a_move_cancels_the_pending_wrap() {
    let t = run(5, 3, "abcde\rX");
    assert_eq!(rows(&t), ["Xbcde", "", ""]);
    let t = run(5, 3, "abcde\x1b[DX");
    assert_eq!(rows(&t), ["abcXe", "", ""]);
}

#[test]
fn with_autowrap_off_a_character_replaces_the_last_cell() {
    let t = run(5, 3, "\x1b[?7labcdefg");
    assert_eq!(rows(&t), ["abcdg", "", ""]);
    assert!(!t.modes().autowrap);
}

#[test]
fn insert_mode_pushes_the_rest_of_the_line_right() {
    let t = run(10, 2, "ab\r\x1b[4hX");
    assert_eq!(rows(&t)[0], "Xab");
    assert!(t.modes().insert);
}

// ---- the controls ----

#[test]
fn backspace_tab_return_and_line_feed() {
    assert_eq!(rows(&run(20, 3, "ab\x08c"))[0], "ac");
    let t = run(20, 3, "a\tb");
    assert_eq!(at(&t, 0, 8).ch, 'b');
    assert_eq!(rows(&run(20, 3, "ab\r\ncd"))[..2], ["ab", "cd"]);
    assert_eq!(rows(&run(20, 3, "ab\ncd"))[..2], ["ab", "  cd"], "a line feed does not return the cursor");
    let t = run(20, 3, "\x08\x08x");
    assert_eq!(t.cursor(), (0, 1), "backspace stops at column 0");
}

#[test]
fn a_tab_stops_at_the_last_column_and_the_stops_can_be_set_and_cleared() {
    let t = run(10, 2, "\t\t\t");
    assert_eq!(t.cursor(), (0, 9));
    let t = run(20, 2, "\x1b[5G\x1bH\r\t");
    assert_eq!(t.cursor(), (0, 4), "a stop set with HTS");
    let t = run(20, 2, "\x1b[3g\t");
    assert_eq!(t.cursor(), (0, 19), "all stops cleared");
    let t = run(30, 2, "\x1b[2I");
    assert_eq!(t.cursor(), (0, 16));
    let t = run(30, 2, "\x1b[11G\x1b[Z");
    assert_eq!(t.cursor(), (0, 8));
}

#[test]
fn the_bell_is_counted_and_nothing_is_drawn() {
    let t = run(10, 2, "\x07a\x07");
    assert_eq!(t.bells(), 2);
    assert_eq!(rows(&t)[0], "a");
}

#[test]
fn a_bell_that_ends_a_string_is_not_a_bell() {
    let t = run(10, 2, "\x1b]2;hi\x07");
    assert_eq!(t.bells(), 0);
}

// ---- the cursor ----

#[test]
fn the_cursor_moves_and_stops_at_the_edges() {
    let t = run(10, 5, "\x1b[3;5H");
    assert_eq!(t.cursor(), (2, 4));
    let t = run(10, 5, "\x1b[3;5H\x1b[2A\x1b[3C");
    assert_eq!(t.cursor(), (0, 7));
    let t = run(10, 5, "\x1b[3;5H\x1b[99A\x1b[99D");
    assert_eq!(t.cursor(), (0, 0));
    let t = run(10, 5, "\x1b[99B\x1b[99C");
    assert_eq!(t.cursor(), (4, 9));
    let t = run(10, 5, "\x1b[3;5H\x1b[H");
    assert_eq!(t.cursor(), (0, 0));
    let t = run(10, 5, "\x1b[3;5f");
    assert_eq!(t.cursor(), (2, 4), "HVP is CUP");
    let t = run(10, 5, "\x1b[99;99H");
    assert_eq!(t.cursor(), (4, 9));
}

#[test]
fn a_missing_or_zero_parameter_is_the_default() {
    let t = run(10, 5, "\x1b[3;5H\x1b[A\x1b[0C");
    assert_eq!(t.cursor(), (1, 5), "a missing count and a zero count are 1");
}

#[test]
fn column_row_next_line_and_previous_line() {
    assert_eq!(run(20, 5, "\x1b[7G").cursor(), (0, 6));
    assert_eq!(run(20, 5, "\x1b[4d").cursor(), (3, 0));
    assert_eq!(run(20, 5, "\x1b[3;5H\x1b[2E").cursor(), (4, 0));
    assert_eq!(run(20, 5, "\x1b[3;5H\x1b[F").cursor(), (1, 0));
    assert_eq!(run(20, 5, "\x1b[2;3H\x1b[2a\x1b[1e").cursor(), (2, 4));
    assert_eq!(run(20, 5, "\x1b[3;5H\x1b[8`").cursor(), (2, 7));
}

#[test]
fn save_and_restore_keep_the_position_and_the_pen() {
    let mut t = run(10, 5, "\x1b[2;3H\x1b[1;31m\x1b7\x1b[5;5H\x1b[0m");
    feed(&mut t, "\x1b8x");
    let c = at(&t, 1, 2);
    assert_eq!(c.ch, 'x');
    assert!(c.attrs.contains(Attrs::BOLD));
    assert_eq!(c.fg, Color::Indexed(1));
    let mut t = run(10, 5, "\x1b[2;3H\x1b[s\x1b[5;5H");
    feed(&mut t, "\x1b[u");
    assert_eq!(t.cursor(), (1, 2));
}

#[test]
fn restoring_with_nothing_saved_goes_home_with_the_default_pen() {
    let mut t = run(10, 5, "\x1b[3;3H\x1b[1m");
    feed(&mut t, "\x1b8x");
    assert_eq!(t.cursor(), (0, 1));
    assert_eq!(at(&t, 0, 0).attrs, Attrs::NONE);
}

#[test]
fn index_next_line_and_reverse_index() {
    let t = run(10, 3, "a\x1bDb");
    assert_eq!(rows(&t)[..2], ["a", " b"]);
    let t = run(10, 3, "ab\x1bEc");
    assert_eq!(rows(&t)[..2], ["ab", "c"]);
    let t = run(10, 3, "a\r\nb\x1b[H\x1bMx");
    assert_eq!(rows(&t), ["x", "a", "b"], "reverse index at the top scrolls down");
}

#[test]
fn a_full_reset_clears_the_screen_the_modes_and_the_scrollback_and_keeps_the_title() {
    let mut t = run(10, 2, "\x1b]2;T\x07\x1b[?2004h\x1b[?25l1\r\n2\r\n3");
    feed(&mut t, "\x1bc");
    assert_eq!(rows(&t), ["", ""]);
    assert!(!t.modes().bracketed_paste && t.modes().cursor_visible);
    assert_eq!(t.view(0).total, 2);
    assert_eq!(t.title(), "T");
}

// ---- erase and edit ----

#[test]
fn erase_in_display() {
    let base = "abc\r\ndef\r\nghi\x1b[2;2H";
    assert_eq!(rows(&run(5, 3, &format!("{base}\x1b[J"))), ["abc", "d", ""]);
    assert_eq!(rows(&run(5, 3, &format!("{base}\x1b[1J"))), ["", "  f", "ghi"]);
    assert_eq!(rows(&run(5, 3, &format!("{base}\x1b[2J"))), ["", "", ""]);
    assert_eq!(run(5, 3, &format!("{base}\x1b[2J")).cursor(), (1, 1), "the cursor stays");
}

#[test]
fn erase_in_line() {
    let base = "abcde\x1b[1;3H";
    assert_eq!(rows(&run(8, 2, &format!("{base}\x1b[K")))[0], "ab");
    assert_eq!(rows(&run(8, 2, &format!("{base}\x1b[1K")))[0], "   de");
    assert_eq!(rows(&run(8, 2, &format!("{base}\x1b[2K")))[0], "");
}

#[test]
fn erased_cells_take_the_current_background_and_no_attributes() {
    let t = run(5, 2, "\x1b[1;31;44m\x1b[K");
    let c = at(&t, 0, 3);
    assert_eq!((c.ch, c.fg, c.bg, c.attrs), (' ', Color::Default, Color::Indexed(4), Attrs::NONE));
}

#[test]
fn insert_delete_and_erase_characters() {
    assert_eq!(rows(&run(10, 2, "abcdef\x1b[3G\x1b[2@"))[0], "ab  cdef");
    assert_eq!(rows(&run(10, 2, "abcdef\x1b[3G\x1b[2P"))[0], "abef");
    assert_eq!(rows(&run(10, 2, "abcdef\x1b[3G\x1b[2X"))[0], "ab  ef");
    assert_eq!(rows(&run(6, 2, "abcdef\x1b[1G\x1b[99@"))[0], "", "inserting more than there is room for blanks the line");
    assert_eq!(rows(&run(6, 2, "abcdef\x1b[3G\x1b[99P"))[0], "ab");
}

#[test]
fn insert_and_delete_lines_stay_inside_the_scroll_region() {
    assert_eq!(rows(&run(5, 4, "a\r\nb\r\nc\r\nd\x1b[2;1H\x1b[L")), ["a", "", "b", "c"]);
    assert_eq!(rows(&run(5, 4, "a\r\nb\r\nc\r\nd\x1b[2;1H\x1b[M")), ["a", "c", "d", ""]);
    assert_eq!(rows(&run(5, 5, "a\r\nb\r\nc\r\nd\r\ne\x1b[2;4r\x1b[2;1H\x1b[L")), ["a", "", "b", "c", "e"]);
    assert_eq!(rows(&run(5, 5, "a\r\nb\r\nc\r\nd\r\ne\x1b[2;4r\x1b[5;1H\x1b[L")), ["a", "b", "c", "d", "e"], "outside the region nothing happens");
}

#[test]
fn repeat_the_last_character() {
    assert_eq!(rows(&run(10, 2, "a\x1b[3b"))[0], "aaaa");
    assert_eq!(rows(&run(5, 2, "\x1b[b"))[0], "", "nothing printed yet: nothing to repeat");
}

// ---- the scroll region and the scrollback ----

#[test]
fn a_line_feed_at_the_bottom_of_the_region_scrolls_only_the_region() {
    let mut t = run(5, 5, "a\r\nb\r\nc\r\nd\r\ne\x1b[2;4r\x1b[4;1H");
    feed(&mut t, "\n");
    assert_eq!(rows(&t), ["a", "c", "d", "", "e"]);
    assert_eq!(t.view(0).total, 5, "a smaller region gives nothing to the scrollback");
    feed(&mut t, "\x1b[2;1H\x1bM");
    assert_eq!(rows(&t), ["a", "", "c", "d", "e"]);
    let mut t = run(5, 5, "a\r\nb\r\nc\r\nd\r\ne\x1b[2;4r\x1b[5;1H");
    feed(&mut t, "\n");
    assert_eq!(rows(&t), ["a", "b", "c", "d", "e"], "below the region a line feed at the bottom row does nothing");
    assert_eq!(t.cursor().0, 4);
}

#[test]
fn setting_the_region_homes_the_cursor_and_a_bad_region_is_ignored() {
    assert_eq!(run(5, 5, "\x1b[3;3H\x1b[2;4r").cursor(), (0, 0));
    let mut t = run(5, 5, "a\r\nb\r\nc\r\nd\r\ne\x1b[4;2r\x1b[5;1H");
    feed(&mut t, "\nx");
    assert_eq!(rows(&t)[..4], ["b", "c", "d", "e"], "a reversed region leaves the whole screen as the region");
    assert_eq!(run(5, 5, "\x1b[3;3H\x1b[2;2r").cursor(), (2, 2), "a region of one row is ignored: the cursor stays");
    assert_eq!(run(5, 5, "\x1b[3;3H\x1b[2;3r").cursor(), (0, 0), "a region of two rows is set");
}

#[test]
fn scroll_up_and_scroll_down() {
    let t = run(5, 4, "a\r\nb\r\nc\r\nd\x1b[S");
    assert_eq!(rows(&t), ["b", "c", "d", ""]);
    let t = run(5, 4, "a\r\nb\r\nc\r\nd\x1b[T");
    assert_eq!(rows(&t), ["", "a", "b", "c"]);
    let t = run(5, 4, "a\r\nb\r\nc\r\nd\x1b[2S");
    assert_eq!(rows(&t), ["c", "d", "", ""]);
}

#[test]
fn origin_mode_makes_positions_relative_to_the_region() {
    let t = run(5, 5, "\x1b[2;4r\x1b[?6h\x1b[1;1H");
    assert_eq!(t.cursor(), (1, 0));
    let t = run(5, 5, "\x1b[2;4r\x1b[?6h\x1b[99;1H");
    assert_eq!(t.cursor(), (3, 0), "held inside the region");
    assert!(t.modes().origin);
}

#[test]
fn cursor_moves_stop_at_the_margins_of_the_region_they_start_in() {
    let t = run(5, 5, "\x1b[2;4r\x1b[3;1H\x1b[10A");
    assert_eq!(t.cursor().0, 1);
    let t = run(5, 5, "\x1b[2;4r\x1b[3;1H\x1b[10B");
    assert_eq!(t.cursor().0, 3);
    let t = run(5, 5, "\x1b[2;4r\x1b[1;1H\x1b[10A");
    assert_eq!(t.cursor().0, 0, "above the region it is not held");
}

#[test]
fn lines_that_scroll_off_the_top_go_to_the_scrollback() {
    let t = run(5, 3, "1\r\n2\r\n3\r\n4\r\n5");
    let v = t.view(0);
    assert_eq!((v.total, v.top, v.scroll), (5, 2, 0));
    assert_eq!(rows(&t), ["3", "4", "5"]);
    let v = t.view(1);
    assert_eq!((v.top, v.scroll, v.cursor), (1, 1, None));
    assert_eq!(v.lines.iter().map(|l| line_text(l)).collect::<Vec<_>>(), ["2", "3", "4"]);
    let v = t.view(99);
    assert_eq!((v.top, v.scroll), (0, 2), "the distance is clamped");
    assert_eq!(v.lines.iter().map(|l| line_text(l)).collect::<Vec<_>>(), ["1", "2", "3"]);
}

#[test]
fn the_scrollback_keeps_the_pen_of_its_lines_and_pads_them_to_the_width() {
    let t = run(8, 2, "\x1b[31mred\x1b[0m\r\n\r\nx");
    let v = t.view(1);
    assert_eq!(v.lines[0].len(), 8);
    assert_eq!(v.lines[0][0].fg, Color::Indexed(1));
    assert_eq!(v.lines[0][7], Cell::BLANK);
}

#[test]
fn the_scrollback_holds_at_most_the_lines_it_was_given() {
    let mut t = Term::with_scrollback(5, 2, 3);
    for i in 0..20 {
        feed(&mut t, &format!("{i}\r\n"));
    }
    assert_eq!(t.view(0).total, 2 + 3);
    assert_eq!(t.view(99).lines.iter().map(|l| line_text(l)).collect::<Vec<_>>(), ["16", "17"]);
}

#[test]
fn the_scrollback_is_bounded_in_cells_too() {
    let mut t = Term::with_scrollback(500, 2, 20_000);
    let line = "x".repeat(500);
    for _ in 0..3000 {
        feed(&mut t, &format!("{line}\r\n"));
    }
    let kept = t.view(0).total - 2;
    assert!((1000..=1200).contains(&kept), "600,000 cells of 500 is 1,200 lines, kept {kept}");
}

#[test]
fn a_request_for_more_scrollback_than_the_limit_is_cut_to_it() {
    let t = Term::with_scrollback(5, 2, 1_000_000);
    let mut t = t;
    for _ in 0..25_000 {
        feed(&mut t, "x\r\n");
    }
    assert_eq!(t.view(0).total, 2 + 20_000);
}

#[test]
fn erase_display_three_empties_the_scrollback() {
    let mut t = run(5, 2, "1\r\n2\r\n3\r\n4");
    assert!(t.view(0).total > 2);
    feed(&mut t, "\x1b[3J");
    assert_eq!(t.view(0).total, 2);
    assert_eq!(rows(&t), ["3", "4"], "the screen stays");
}

// ---- SGR ----

fn pen(seq: &str) -> Cell {
    at(&run(10, 2, &format!("\x1b[{seq}mx")), 0, 0)
}

#[test]
fn sgr_attributes_turn_on_and_off() {
    for (on, off, flag) in [("1", "22", Attrs::BOLD), ("2", "22", Attrs::DIM), ("3", "23", Attrs::ITALIC), ("4", "24", Attrs::UNDERLINE), ("5", "25", Attrs::BLINK), ("7", "27", Attrs::INVERSE), ("8", "28", Attrs::HIDDEN), ("9", "29", Attrs::STRIKE)] {
        assert!(pen(on).attrs.contains(flag), "{on}");
        assert_eq!(pen(&format!("{on};{off}")).attrs, Attrs::NONE, "{off}");
    }
    assert!(pen("21").attrs.contains(Attrs::UNDERLINE));
    assert!(pen("6").attrs.contains(Attrs::BLINK));
    assert_eq!(pen("1;3;4;0").attrs, Attrs::NONE);
    let t = run(10, 2, "\x1b[1m\x1b[mx");
    assert_eq!(at(&t, 0, 0).attrs, Attrs::NONE, "an empty SGR is a reset");
    assert_eq!(pen("1;2;22").attrs, Attrs::NONE, "22 turns off both bold and dim");
}

#[test]
fn sgr_sixteen_colours() {
    assert_eq!(pen("31").fg, Color::Indexed(1));
    assert_eq!(pen("37").fg, Color::Indexed(7));
    assert_eq!(pen("90").fg, Color::Indexed(8));
    assert_eq!(pen("97").fg, Color::Indexed(15));
    assert_eq!(pen("41").bg, Color::Indexed(1));
    assert_eq!(pen("47").bg, Color::Indexed(7));
    assert_eq!(pen("100").bg, Color::Indexed(8));
    assert_eq!(pen("107").bg, Color::Indexed(15));
    assert_eq!(pen("31;39").fg, Color::Default);
    assert_eq!(pen("41;49").bg, Color::Default);
    assert_eq!(pen("1;31;4").fg, Color::Indexed(1));
}

#[test]
fn sgr_the_palette_and_true_colour_in_both_forms() {
    assert_eq!(pen("38;5;200").fg, Color::Indexed(200));
    assert_eq!(pen("48;5;17").bg, Color::Indexed(17));
    assert_eq!(pen("38;2;1;2;3").fg, Color::Rgb(1, 2, 3));
    assert_eq!(pen("48;2;4;5;6").bg, Color::Rgb(4, 5, 6));
    assert_eq!(pen("38:5:200").fg, Color::Indexed(200));
    assert_eq!(pen("38:2::10:20:30").fg, Color::Rgb(10, 20, 30));
    assert_eq!(pen("38:2:10:20:30").fg, Color::Rgb(10, 20, 30));
    assert_eq!(pen("48:2::1:2:3").bg, Color::Rgb(1, 2, 3));
    let c = pen("1;38;5;9;4");
    assert_eq!((c.fg, c.attrs), (Color::Indexed(9), Attrs::BOLD | Attrs::UNDERLINE), "parameters after a colour are still read");
}

#[test]
fn a_malformed_extended_colour_changes_nothing() {
    assert_eq!(pen("38;5").fg, Color::Default);
    assert_eq!(pen("38;2;1;2").fg, Color::Default);
    assert_eq!(pen("38;5;300").fg, Color::Default, "an index over 255");
    assert_eq!(pen("38;5;256").fg, Color::Default, "the first index over 255");
    assert_eq!(pen("38;5;255").fg, Color::Indexed(255), "the last index");
    assert_eq!(pen("38;2;1;2;999").fg, Color::Default, "a component over 255");
    let c = pen("4;38;5");
    assert_eq!((c.fg, c.attrs), (Color::Default, Attrs::UNDERLINE));
}

#[test]
fn the_pen_lasts_until_it_is_changed() {
    let t = run(10, 2, "\x1b[1;32mab\x1b[0mc");
    assert_eq!(at(&t, 0, 1).fg, Color::Indexed(2));
    assert_eq!(at(&t, 0, 2).fg, Color::Default);
}

// ---- wide characters ----

#[test]
fn a_wide_character_is_two_cells_the_second_a_tail() {
    let t = run(6, 2, "한글a");
    assert_eq!((at(&t, 0, 0).ch, at(&t, 0, 2).ch, at(&t, 0, 4).ch), ('한', '글', 'a'));
    assert!(at(&t, 0, 0).attrs.contains(Attrs::WIDE) && at(&t, 0, 1).attrs.contains(Attrs::TAIL));
    assert!(at(&t, 0, 2).attrs.contains(Attrs::WIDE) && at(&t, 0, 3).attrs.contains(Attrs::TAIL));
    assert_eq!(at(&t, 0, 4).attrs, Attrs::NONE);
    assert_eq!(t.cursor(), (0, 5));
    assert_eq!(rows(&t)[0], "한글a");
}

#[test]
fn a_wide_character_that_does_not_fit_wraps_first() {
    let t = run(5, 2, "abcd한");
    assert_eq!(rows(&t), ["abcd", "한"]);
    assert!(t.view(0).wrapped[0]);
    let t = run(5, 2, "\x1b[?7labcd한");
    assert_eq!(rows(&t), ["abcd", ""], "with auto-wrap off it is dropped");
}

#[test]
fn writing_over_half_of_a_wide_character_blanks_the_other_half() {
    let t = run(6, 2, "한\ra");
    assert_eq!((at(&t, 0, 0).ch, at(&t, 0, 1).ch), ('a', ' '));
    assert_eq!(at(&t, 0, 1).attrs, Attrs::NONE, "not a tail any more");
    let t = run(6, 2, "한\x1b[2Gb");
    assert_eq!((at(&t, 0, 0).ch, at(&t, 0, 0).attrs, at(&t, 0, 1).ch), (' ', Attrs::NONE, 'b'));
    let t = run(6, 2, "한한\x1b[2G\x1b[P");
    assert_eq!(rows(&t)[0], " 한", "deleting the tail of one blanks its head");
}

#[test]
fn a_zero_width_character_is_dropped() {
    let t = run(6, 2, "e\u{301}x");
    assert_eq!(rows(&t)[0], "ex");
    assert_eq!(t.cursor(), (0, 2));
}

// ---- UTF-8 ----

#[test]
fn a_character_cut_between_two_calls_is_completed() {
    for text in ["한", "é", "😀", "a한b"] {
        let bytes = text.as_bytes();
        for cut in 0..=bytes.len() {
            let mut t = term(10, 2);
            t.feed(&bytes[..cut]);
            t.feed(&bytes[cut..]);
            assert_eq!(rows(&t)[0], text, "{text} cut at {cut}");
        }
    }
}

#[test]
fn bad_utf8_gives_one_replacement_for_each_bad_part() {
    let r = |b: &[u8]| {
        let mut t = term(10, 2);
        t.feed(b);
        rows(&t)[0].clone()
    };
    assert_eq!(r(b"\xff"), "\u{fffd}");
    assert_eq!(r(b"\xe4\xbda"), "\u{fffd}a", "a sequence cut short by a plain byte");
    assert_eq!(r(b"\xc0\x80"), "\u{fffd}\u{fffd}", "an overlong form");
    assert_eq!(r(b"\xed\xa0\x80"), "\u{fffd}\u{fffd}\u{fffd}", "a surrogate");
    assert_eq!(r(b"\xf4\x90\x80\x80"), "\u{fffd}\u{fffd}\u{fffd}\u{fffd}", "over U+10FFFF");
    assert_eq!(r(b"\x80a"), "\u{fffd}a", "a stray continuation byte");
    assert_eq!(r(b"\xe0\x80\x80"), "\u{fffd}\u{fffd}\u{fffd}", "an overlong three-byte form");
    assert_eq!(r(b"\xe0\x9f\xbf"), "\u{fffd}\u{fffd}\u{fffd}", "the last overlong three-byte form");
    assert_eq!(r(b"\xf0\x8f\xbf\xbf"), "\u{fffd}\u{fffd}\u{fffd}\u{fffd}", "an overlong four-byte form");
    assert_eq!(r("é".as_bytes()), "é");
}

#[test]
fn the_eight_bit_controls_are_not_commands() {
    let mut t = term(10, 2);
    t.feed(b"\x9b2Jx"); // 0x9b is a continuation byte, not CSI
    assert_eq!(rows(&t)[0], "\u{fffd}2Jx");
}

// ---- the character sets ----

#[test]
fn the_line_drawing_set_maps_its_range_and_ascii_restores_it() {
    let t = run(10, 2, "\x1b(0lqk\x1b(Blqk");
    assert_eq!(rows(&t)[0], "┌─┐lqk");
    let t = run(10, 2, "\x1b)0\x0eq\x0fq");
    assert_eq!(rows(&t)[0], "─q", "shift out uses G1, shift in G0");
    let t = run(10, 2, "\x1b(0jmntuvwx\x1b(B");
    assert_eq!(rows(&t)[0], "┘└┼├┤┴┬│");
}

// ---- the alternate screen ----

#[test]
fn the_alternate_screen_is_blank_and_the_main_screen_comes_back_as_it_was() {
    let mut t = run(8, 3, "main\x1b[?1049h");
    assert_eq!(rows(&t), ["", "", ""]);
    assert!(t.modes().alt_screen);
    feed(&mut t, "alt");
    assert_eq!(rows(&t)[0], "    alt", "the cursor stays where it was: the alternate screen does not home it");
    feed(&mut t, "\x1b[?1049l");
    assert_eq!(rows(&t)[0], "main");
    assert_eq!(t.cursor(), (0, 4), "the cursor is restored");
    assert!(!t.modes().alt_screen);
}

#[test]
fn the_alternate_screen_has_no_scrollback_and_is_cleared_each_time() {
    let mut t = run(5, 3, "1\r\n2\r\n3\r\n4");
    let main_total = t.view(0).total;
    assert_eq!(main_total, 4, "one line went to the scrollback");
    feed(&mut t, "\x1b[?1049h");
    feed(&mut t, "1\r\n2\r\n3\r\n4\r\n5");
    assert_eq!(t.view(0).total, 3);
    assert_eq!(t.view(5).scroll, 0, "no scrolling back on the alternate screen");
    feed(&mut t, "\x1b[?1049l");
    assert_eq!(t.view(0).total, main_total);
    feed(&mut t, "\x1b[?1049h");
    assert_eq!(rows(&t), ["", "", ""]);
}

#[test]
fn modes_47_and_1047_switch_the_screen_without_saving_the_cursor() {
    let mut t = run(8, 3, "abc\x1b[?47h");
    assert!(t.modes().alt_screen);
    feed(&mut t, "\x1b[3;3Hx\x1b[?47l");
    assert_eq!(rows(&t)[0], "abc");
    assert_eq!(t.cursor(), (2, 3), "the cursor is where the alternate screen left it");
    let mut t = run(8, 3, "abc\x1b[?1047h");
    feed(&mut t, "\x1b[?1047l");
    assert_eq!(rows(&t)[0], "abc");
}

// ---- strings and cancelling ----

#[test]
fn a_device_control_string_is_skipped_to_its_end() {
    assert_eq!(rows(&run(10, 2, "\x1bPq#0;2;0;0;0\x1b\\ab"))[0], "ab");
    assert_eq!(rows(&run(10, 2, "\x1b_Gi=1;AAAA\x1b\\ab"))[0], "ab");
    assert_eq!(rows(&run(10, 2, "\x1b^private\x1b\\ab"))[0], "ab");
    assert_eq!(rows(&run(10, 2, "\x1bXsos\x1b\\ab"))[0], "ab");
}

#[test]
fn cancel_and_substitute_end_a_sequence_and_an_escape_starts_a_new_one() {
    assert_eq!(rows(&run(10, 2, "\x1b[12\x18ab"))[0], "ab");
    assert_eq!(rows(&run(10, 2, "\x1b[12\x1aab"))[0], "ab");
    assert_eq!(run(10, 5, "\x1b[3\x1b[2;2H").cursor(), (1, 1));
    assert_eq!(rows(&run(10, 2, "\x1b]2;title\x18ab"))[0], "ab");
}

#[test]
fn sequences_that_are_not_understood_are_skipped_to_their_end() {
    let t = run(10, 2, "a\x1b[?9999hb\x1b[>4;2mc\x1b[=1;2uc\x1b[<uc\x1b[ qd\x1b[1;2;3;4;5;6;7$zd\x1b(Xe");
    assert_eq!(rows(&t)[0], "abcccdde");
}

// ---- found by the mutation checks of M23-P01 ----

#[test]
fn a_delete_character_is_dropped_not_printed() {
    assert_eq!(rows(&run(10, 2, "a\x7fb"))[0], "ab");
}

#[test]
fn a_parameter_over_65535_saturates() {
    assert_eq!(run(10, 5, "\x1b[99999C").cursor(), (0, 9), "not wrapped round to 3 (and then to the default 1)");
    assert_eq!(run(10, 5, "\x1b[65536C").cursor(), (0, 9));
    assert_eq!(run(10, 5, "\x1b[70000;70000H").cursor(), (4, 9));
}

#[test]
fn only_the_first_32_parameters_are_kept_and_the_digits_of_the_rest_are_dropped() {
    let thirty_one_zeros = "0;".repeat(31);
    assert!(pen(&format!("{thirty_one_zeros}1")).attrs.contains(Attrs::BOLD), "the 32nd parameter is kept");
    assert_eq!(pen(&format!("{thirty_one_zeros}0;1")).attrs, Attrs::NONE, "the 33rd is dropped and does not join the 32nd");
    assert_eq!(pen(&format!("{thirty_one_zeros}0;1;1;1")).attrs, Attrs::NONE);
}

#[test]
fn erasing_to_the_end_of_a_row_ends_its_wrap() {
    let mut t = run(5, 3, "abcdefg");
    assert!(t.view(0).wrapped[0], "the first row goes on in the second");
    feed(&mut t, "\x1b[1;3H\x1b[K");
    assert!(!t.view(0).wrapped[0], "erased to its end: it no longer goes on");
    assert_eq!(rows(&t)[..2], ["ab", "fg"]);
}
