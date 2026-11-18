//! The unit tests of the terminal tab's pure parts (`screen.rs`): the colours, a host answer read with bounds, the pieces of a line, the grid, the selection and the words.

use super::screen::*;
use lico_vt::MouseMode;
use serde_json::{json, Value};

fn run(text: &str) -> Run {
    Run { text: text.into(), fg: -1, bg: -1, attrs: 0 }
}

fn answer() -> Value {
    json!({
        "tab": "t1", "seq": 7, "cols": 80, "rows": 3, "title": "vim", "cursor": [1, 4],
        "modes": {"app_cursor": true, "bracketed_paste": true, "alt_screen": false, "cursor_visible": true, "mouse": "drag", "mouse_sgr": true},
        "scroll": 0, "top": 10, "total": 13,
        "lines": [[["$ ls", -1, -1, 0]], [["a", 2, -1, 1], [" b", -1, 4, 0]], []],
        "wrapped": [false, false, false], "bells": 0, "running": true, "exit": null,
    })
}

// ---------------------------------------------------------------- colours

#[test]
fn the_sixteen_colours_the_cube_and_the_greys_are_the_standard_table() {
    assert_eq!(palette(0), 0xFF00_0000);
    assert_eq!(palette(1), 0xFFCD_3131);
    assert_eq!(palette(15), 0xFFFF_FFFF);
    assert_eq!(palette(16), 0xFF00_0000, "the cube starts at black");
    assert_eq!(palette(21), 0xFF00_00FF, "the blue corner of the cube");
    assert_eq!(palette(196), 0xFFFF_0000, "the red corner of the cube");
    assert_eq!(palette(231), 0xFFFF_FFFF);
    assert_eq!(palette(17), 0xFF00_005F, "the first step of a cube axis is 95, not 40");
    assert_eq!(palette(232), 0xFF08_0808);
    assert_eq!(palette(255), 0xFFEE_EEEE);
}

#[test]
fn a_colour_number_is_indexed_true_colour_or_none() {
    assert_eq!(colour(-1), None);
    assert_eq!(colour(0), Some(palette(0)));
    assert_eq!(colour(255), Some(palette(255)));
    assert_eq!(colour(256), Some(0xFF00_0000), "256 + black");
    assert_eq!(colour(256 + 0x12_34_56), Some(0xFF12_3456));
    assert_eq!(colour(256 + 0xFF_FF_FF), Some(0xFFFF_FFFF));
    assert_eq!(colour(256 + 0x100_0000), None, "one past true colour");
    assert_eq!(colour(-2), None);
    assert_eq!(colour(i64::MAX), None);
}

#[test]
fn a_run_with_no_colours_is_the_default_pair_and_paints_no_ground() {
    let l = look(-1, -1, 0);
    assert_eq!((l.fg, l.bg, l.paint_bg, l.underline, l.strike), (DEFAULT_FG, DEFAULT_BG, false, false, false));
    let c = look(2, 4, 0);
    assert_eq!((c.fg, c.bg, c.paint_bg), (palette(2), palette(4), true));
}

#[test]
fn bold_takes_the_bright_colour_of_an_indexed_zero_to_seven_and_no_other() {
    assert_eq!(look(1, -1, BOLD).fg, palette(9));
    assert_eq!(look(7, -1, BOLD).fg, palette(15));
    assert_eq!(look(8, -1, BOLD).fg, palette(8), "an indexed colour above 7 stays");
    assert_eq!(look(-1, -1, BOLD).fg, DEFAULT_FG, "the default does not change");
    assert_eq!(look(256 + 0x010203, -1, BOLD).fg, 0xFF01_0203, "true colour stays");
    assert_eq!(look(1, -1, 0).fg, palette(1));
}

#[test]
fn dim_is_half_way_to_the_ground_and_inverse_swaps_and_paints_the_ground() {
    let d = look(-1, -1, DIM);
    let want = 0xFF00_0000 | ((((DEFAULT_FG >> 16 & 0xFF) + (DEFAULT_BG >> 16 & 0xFF)) / 2) << 16) | ((((DEFAULT_FG >> 8 & 0xFF) + (DEFAULT_BG >> 8 & 0xFF)) / 2) << 8) | (((DEFAULT_FG & 0xFF) + (DEFAULT_BG & 0xFF)) / 2);
    assert_eq!(d.fg, want);
    let i = look(-1, -1, INVERSE);
    assert_eq!((i.fg, i.bg, i.paint_bg), (DEFAULT_BG, DEFAULT_FG, true), "an inverse cell on the default ground paints the foreground as its ground");
    let j = look(2, 4, INVERSE);
    assert_eq!((j.fg, j.bg), (palette(4), palette(2)));
}

#[test]
fn hidden_is_drawn_in_the_colour_of_its_ground_and_the_lines_are_flags() {
    let h = look(2, 4, HIDDEN);
    assert_eq!((h.fg, h.bg), (palette(4), palette(4)));
    let hi = look(2, 4, HIDDEN | INVERSE);
    assert_eq!(hi.fg, hi.bg, "hidden after the swap: the text cannot be read");
    let u = look(-1, -1, UNDERLINE | STRIKE);
    assert!(u.underline && u.strike);
    let n = look(-1, -1, BOLD);
    assert!(!n.underline && !n.strike);
}

// ---------------------------------------------------------------- the answer of the host

#[test]
fn an_answer_is_read_in_full() {
    let s = Screen::parse(&answer()).expect("a good answer");
    assert_eq!((s.seq, s.cols, s.rows, s.title.as_str()), (7, 80, 3, "vim"));
    assert_eq!(s.cursor, Some((1, 4)));
    assert_eq!((s.scroll, s.top, s.total), (0, 10, 13));
    assert!(s.running && s.exit.is_none());
    assert_eq!(s.lines.len(), 3);
    assert_eq!(s.lines[1], vec![Run { text: "a".into(), fg: 2, bg: -1, attrs: 1 }, Run { text: " b".into(), fg: -1, bg: 4, attrs: 0 }]);
    assert!(s.lines[2].is_empty());
    assert!(s.modes.app_cursor && s.modes.bracketed_paste && s.modes.mouse_sgr && !s.modes.alt_screen);
    assert_eq!(s.modes.mouse, MouseMode::Drag);
    assert_eq!(s.plain_lines(), vec!["$ ls", "a b", ""]);
}

#[test]
fn an_answer_without_a_change_number_or_with_a_size_the_host_never_gives_is_refused() {
    for (k, v) in [("seq", Value::Null), ("cols", json!(0)), ("cols", json!(501)), ("rows", json!(0)), ("rows", json!(201)), ("cols", json!("80")), ("rows", json!(-1))] {
        let mut a = answer();
        a[k] = v.clone();
        if v.is_null() {
            a.as_object_mut().unwrap().remove(k);
        }
        assert!(Screen::parse(&a).is_none(), "{k} = {v}");
    }
    assert!(Screen::parse(&json!({"ok": 0, "error": "unknown_tab"})).is_none());
    assert!(Screen::parse(&Value::Null).is_none());
    for edge in [(1, 1), (500, 200)] {
        let mut a = answer();
        (a["cols"], a["rows"]) = (json!(edge.0), json!(edge.1));
        assert!(Screen::parse(&a).is_some(), "{edge:?} are the host's own bounds");
    }
}

#[test]
fn a_damaged_answer_costs_the_part_that_is_damaged_and_nothing_else() {
    let mut a = answer();
    a["lines"] = json!([[["ok", -1, -1, 0], ["short", -1], "text", [5, -1, -1, 0], ["x", "red", -1, 0]], "not a line", [["z", 1, 1, 1]], [["extra", -1, -1, 0]]]);
    let s = Screen::parse(&a).unwrap();
    assert_eq!(s.lines.len(), 3, "the lines beyond the rows are cut");
    assert_eq!(s.lines[0], vec![run("ok")], "only the run that has its four fields right is kept");
    assert!(s.lines[1].is_empty(), "a line that is not a list is an empty one: the rows below stay where they are");
    assert_eq!(s.lines[2][0].text, "z");
    let mut b = answer();
    b["cursor"] = json!([3, 0]);
    assert_eq!(Screen::parse(&b).unwrap().cursor, None, "a cursor below the screen");
    b["cursor"] = json!([0, 80]);
    assert_eq!(Screen::parse(&b).unwrap().cursor, None, "a cursor past the width");
    b["cursor"] = json!([-1, 0]);
    assert_eq!(Screen::parse(&b).unwrap().cursor, None);
    b["cursor"] = Value::Null;
    assert_eq!(Screen::parse(&b).unwrap().cursor, None);
    let mut c = answer();
    c["modes"] = json!("nonsense");
    assert_eq!(Screen::parse(&c).unwrap().modes, Wire::default());
    c.as_object_mut().unwrap().remove("lines");
    assert!(Screen::parse(&c).unwrap().lines.is_empty());
    c["running"] = json!("yes");
    assert!(Screen::parse(&c).unwrap().running, "a running that is not a flag is the default: running");
}

#[test]
fn what_the_host_tells_of_the_text_is_cleaned_and_bounded() {
    let mut a = answer();
    a["title"] = json!(format!("a\u{1b}[31mb\n{}", "x".repeat(400)));
    let s = Screen::parse(&a).unwrap();
    assert!(!s.title.chars().any(char::is_control) && s.title.chars().count() <= 256, "{:?}", s.title);
    assert!(s.title.starts_with("a[31mb"));
    a["lines"] = json!([[["x\u{7}y\tz".to_string() + &"w".repeat(5000), -1, -1, 0]]]);
    let r = &Screen::parse(&a).unwrap().lines[0][0];
    assert!(r.text.starts_with("xyzw") && r.text.chars().count() == 1000, "controls are dropped and the run is bounded: {}", r.text.chars().count());
    a["scroll"] = json!(u64::MAX);
    assert_eq!(Screen::parse(&a).unwrap().scroll, 1_000_000_000, "a count that no screen has is bounded");
}

#[test]
fn a_shell_that_ended_says_so_with_its_code() {
    let mut a = answer();
    (a["running"], a["exit"]) = (json!(false), json!(3));
    let s = Screen::parse(&a).unwrap();
    assert!(!s.running);
    assert_eq!(s.exit, Some(3));
}

#[test]
fn the_wire_modes_become_the_emulators_and_the_ones_not_told_are_its_start_values() {
    let m = Screen::parse(&answer()).unwrap().modes.modes();
    assert!(m.app_cursor && m.bracketed_paste && m.cursor_visible && m.mouse_sgr);
    assert_eq!(m.mouse, MouseMode::Drag);
    assert!(!m.alt_screen && !m.app_keypad && !m.focus_events);
    let d = Wire::default().modes();
    assert!(d.cursor_visible && d.autowrap, "the start values of the emulator for what the wire does not say");
    assert_eq!(d.mouse, MouseMode::Off);
}

// ---------------------------------------------------------------- the line

#[test]
fn the_spans_of_a_line_are_the_columns_of_its_runs_wide_characters_counted_twice() {
    let runs = [run("ab"), run("\u{d55c}\u{ae00}"), run("c")];
    assert_eq!(spans(&runs, 80), vec![Span { run: 0, col: 0, cols: 2 }, Span { run: 1, col: 2, cols: 4 }, Span { run: 2, col: 6, cols: 1 }]);
    assert_eq!(spans(&runs, 4), vec![Span { run: 0, col: 0, cols: 2 }, Span { run: 1, col: 2, cols: 2 }], "cut at the width: the run that is over is cut, the one after it is gone");
    assert!(spans(&runs, 0).is_empty());
    assert!(spans(&[], 80).is_empty());
}

#[test]
fn narrow_text_is_one_piece_and_each_wide_character_is_a_piece_at_its_own_column() {
    let runs = [run("ab\u{d55c}\u{ae00}cd")];
    let p = pieces(&runs, 80);
    assert_eq!(
        p,
        vec![
            Piece { run: 0, col: 0, cols: 2, text: "ab".into() },
            Piece { run: 0, col: 2, cols: 2, text: "\u{d55c}".into() },
            Piece { run: 0, col: 4, cols: 2, text: "\u{ae00}".into() },
            Piece { run: 0, col: 6, cols: 2, text: "cd".into() },
        ]
    );
}

#[test]
fn pieces_follow_the_runs_in_columns_and_spaces_alone_are_not_drawn() {
    let runs = [run("ab"), run("   "), run(" x"), Run { text: "\u{d55c}".into(), fg: 1, bg: -1, attrs: 0 }];
    let p = pieces(&runs, 80);
    assert_eq!(p[0], Piece { run: 0, col: 0, cols: 2, text: "ab".into() });
    assert_eq!(p[1], Piece { run: 2, col: 5, cols: 2, text: " x".into() }, "the run of spaces is skipped and the next one starts where it should");
    assert_eq!(p[2], Piece { run: 3, col: 7, cols: 2, text: "\u{d55c}".into() });
    assert_eq!(p.len(), 3);
}

#[test]
fn a_line_is_cut_at_the_width_and_a_wide_character_that_does_not_fit_is_not_drawn() {
    let runs = [run("abc\u{d55c}d")];
    let p = pieces(&runs, 4);
    assert_eq!(p, vec![Piece { run: 0, col: 0, cols: 3, text: "abc".into() }], "the wide character would need columns 3 and 4");
    let q = pieces(&runs, 5);
    assert_eq!(q.last().unwrap(), &Piece { run: 0, col: 3, cols: 2, text: "\u{d55c}".into() });
    assert_eq!(pieces(&runs, 3), vec![Piece { run: 0, col: 0, cols: 3, text: "abc".into() }]);
    assert!(pieces(&runs, 0).is_empty());
    let two = [run("abcd"), run("efgh")];
    assert_eq!(pieces(&two, 6).iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), vec!["abcd", "ef"], "the second run is cut and nothing comes after");
}

#[test]
fn a_character_with_no_width_takes_no_column() {
    let runs = [run("a\u{200b}b")];
    let p = pieces(&runs, 80);
    assert_eq!(p, vec![Piece { run: 0, col: 0, cols: 2, text: "ab".into() }]);
    assert_eq!(spans(&runs, 80), vec![Span { run: 0, col: 0, cols: 2 }]);
}

#[test]
fn the_character_at_a_column_is_found_across_runs_and_wide_characters() {
    let runs = [run("ab"), run("\u{d55c}c")];
    assert_eq!(char_at(&runs, 0), Some(('a', 1)));
    assert_eq!(char_at(&runs, 1), Some(('b', 1)));
    assert_eq!(char_at(&runs, 2), Some(('\u{d55c}', 2)), "the first half of a wide character");
    assert_eq!(char_at(&runs, 3), Some(('\u{d55c}', 2)), "and its second half");
    assert_eq!(char_at(&runs, 4), Some(('c', 1)));
    assert_eq!(char_at(&runs, 5), None, "past the text");
    assert_eq!(char_at(&[], 0), None);
    assert_eq!(char_at(&[run("a\u{200b}b")], 1), Some(('b', 1)), "a character with no width takes no column");
}

// ---------------------------------------------------------------- the grid

#[test]
fn the_grid_is_whole_cells_within_the_hosts_bounds() {
    assert_eq!(grid_of(800.0, 400.0, 8.0, 16.0), (100, 25));
    assert_eq!(grid_of(807.9, 415.9, 8.0, 16.0), (100, 25), "a part of a cell is not a cell");
    assert_eq!(grid_of(7_000.0, 9_000.0, 8.0, 16.0), (500, 200), "the host's most");
    assert_eq!(grid_of(3.0, 3.0, 8.0, 16.0), (2, 1), "the least");
    assert_eq!(grid_of(-50.0, -50.0, 8.0, 16.0), (2, 1));
    for bad in [(0.0, 16.0), (8.0, 0.0), (-8.0, 16.0), (f32::NAN, 16.0), (8.0, f32::INFINITY)] {
        assert_eq!(grid_of(800.0, 400.0, bad.0, bad.1), (80, 24), "a cell of {bad:?} gives no grid: the host's default");
    }
    assert_eq!(grid_of(f32::NAN, 400.0, 8.0, 16.0), (80, 24));
}

#[test]
fn the_cell_under_a_point_is_none_outside_the_grid_and_the_near_one_is_always_inside() {
    let (origin, cell, grid) = ((100.0, 50.0), (8.0, 16.0), (10, 5));
    assert_eq!(cell_at((100.0, 50.0), origin, cell, grid), Some((0, 0)));
    assert_eq!(cell_at((107.9, 65.9), origin, cell, grid), Some((0, 0)));
    assert_eq!(cell_at((108.0, 66.0), origin, cell, grid), Some((1, 1)));
    assert_eq!(cell_at((179.9, 129.9), origin, cell, grid), Some((9, 4)), "the last cell");
    assert_eq!(cell_at((180.0, 60.0), origin, cell, grid), None, "right of the grid");
    assert_eq!(cell_at((150.0, 130.0), origin, cell, grid), None, "below it");
    assert_eq!(cell_at((99.9, 60.0), origin, cell, grid), None, "left of it");
    assert_eq!(cell_at((120.0, 49.9), origin, cell, grid), None, "above it");
    assert_eq!(cell_near((5.0, 5.0), origin, cell, grid), (0, 0));
    assert_eq!(cell_near((900.0, 900.0), origin, cell, grid), (9, 4));
    assert_eq!(cell_near((140.0, 900.0), origin, cell, grid), (5, 4));
    assert_eq!(cell_near((900.0, 60.0), origin, cell, grid), (9, 0));
}

// ---------------------------------------------------------------- the selection

#[test]
fn a_selection_has_an_earlier_and_a_later_end_whichever_way_it_was_dragged() {
    let s = Sel { anchor: (5, 10), focus: (3, 2) };
    assert_eq!(s.ordered(), ((3, 2), (5, 10)));
    assert_eq!(Sel { anchor: (3, 2), focus: (5, 10) }.ordered(), ((3, 2), (5, 10)));
    assert_eq!(Sel { anchor: (4, 9), focus: (4, 1) }.ordered(), ((4, 1), (4, 9)), "on one line, by the column");
    assert!(Sel { anchor: (4, 9), focus: (4, 9) }.is_point());
    assert!(!Sel { anchor: (4, 9), focus: (4, 10) }.is_point());
}

#[test]
fn the_columns_selected_on_a_line_depend_on_where_the_line_is_in_the_selection() {
    let s = Sel { anchor: (3, 5), focus: (6, 2) };
    assert_eq!(s.span_on(2, 20), None, "above it");
    assert_eq!(s.span_on(3, 20), Some((5, 19)), "the first line: from the column to the end");
    assert_eq!(s.span_on(4, 20), Some((0, 19)), "a line in the middle: all of it");
    assert_eq!(s.span_on(6, 20), Some((0, 2)), "the last line: from the start to the column");
    assert_eq!(s.span_on(7, 20), None, "below it");
    let one = Sel { anchor: (4, 8), focus: (4, 3) };
    assert_eq!(one.span_on(4, 20), Some((3, 8)), "one line, dragged to the left");
    assert_eq!(Sel { anchor: (4, 8), focus: (4, 8) }.span_on(4, 20), None, "a point selects nothing");
    assert_eq!(Sel { anchor: (3, 30), focus: (4, 30) }.span_on(3, 20), Some((19, 19)), "a column past a narrower screen is clamped");
    assert_eq!(s.span_on(4, 0), None, "no columns");
}

// ---------------------------------------------------------------- the words

#[test]
fn the_caption_is_the_title_else_the_folders_name_else_the_id_and_is_short() {
    assert_eq!(caption("vim notes.txt", "/work/a", "t1"), ">_ vim notes.txt");
    assert_eq!(caption("", "/work/project", "t1"), ">_ project");
    assert_eq!(caption("  ", "/work/project/", "t1"), ">_ project", "a trailing slash and a blank title");
    assert_eq!(caption("", "/", "t4"), ">_ t4", "the root has no name: the id");
    assert_eq!(caption("", "", "t2"), ">_ t2");
    let long = caption(&"x".repeat(60), "/a", "t1");
    assert_eq!(long.chars().count(), 3 + 20 + 1);
    assert!(long.ends_with('\u{2026}'));
    assert_eq!(caption(&"y".repeat(20), "/a", "t1").chars().count(), 3 + 20, "exactly 20 is not cut");
}

#[test]
fn the_header_says_the_terminal_is_not_fenced_and_when_it_has_ended() {
    let h = header("bash", "/work/a", true);
    assert!(h.starts_with("terminal | bash | /work/a") && h.contains("not fenced: it runs as you"), "{h}");
    assert!(!h.contains("ended"));
    assert!(header("bash", "/work/a", false).contains("| ended"));
    assert!(header("", "/w", true).starts_with("terminal | shell | /w"), "no title: the word shell");
}

#[test]
fn the_bottom_row_says_what_matters_most_now() {
    let keys = status_row(true, None, 0, "");
    for needle in ["Ctrl+Shift+C copy", "Ctrl+Shift+V paste", "Shift+PgUp/PgDn history", "Ctrl+Shift+W close"] {
        assert!(keys.contains(needle), "{needle}: {keys}");
    }
    assert!(keys.chars().count() < 100, "one row of the default window, not two: {keys}");
    assert!(status_row(true, None, 12, "").contains("history: 12 lines up"));
    assert!(status_row(true, None, 1, "").contains("history: 1 line up"));
    assert!(status_row(true, None, 12, "").contains("Shift+End returns"));
    assert_eq!(status_row(true, None, 0, "\u{d55c}"), "composing: \u{d55c}");
    assert!(status_row(true, None, 5, "\u{d55c}").starts_with("composing"), "what is being composed is said before the history");
    assert_eq!(status_row(false, Some(0), 0, "x"), "the shell ended (exit 0): Enter closes this tab", "an ended shell says so before anything");
    assert_eq!(status_row(false, Some(137), 9, ""), "the shell ended (exit 137): Enter closes this tab");
    assert_eq!(status_row(false, None, 0, ""), "the shell ended: Enter closes this tab");
}
