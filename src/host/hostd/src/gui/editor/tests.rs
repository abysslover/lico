//! Unit tests of the input box: grapheme boundaries, wrapping, paste limits, recall, selection, hit-testing, and bounded per-session undo/redo.

use super::selection::{ Move, shown_offset, text_offset_from_shown, UNDO_BYTES, UNDO_PER_SESSION };
use super::*;

fn ed(text: &str) -> Editor {
    let mut e = Editor::default();
    e.cols = 20;
    e.set(text.to_string());
    e
}

fn at(e: &Editor) -> &str {
    &e.text[..e.cursor]
}

#[test]
fn one_press_removes_one_character_with_what_is_attached_to_it() {
    let after = |text: &str| {
        let mut e = ed(text);
        e.backspace();
        e.text
    };
    assert_eq!(after("ab"), "a");
    assert_eq!(after("a한"), "a");
    assert_eq!(after("ae\u{301}"), "a");
    assert_eq!(after("a\u{1112}\u{1161}\u{11AB}"), "a");
    assert_eq!(after("x\u{1F468}\u{200D}\u{1F469}"), "x");
    assert_eq!(after("a\u{2764}\u{FE0F}"), "a");
    assert_eq!(after(""), "");
    assert!(!ed("").backspace());
}

#[test]
fn delete_takes_the_character_after_the_cursor_with_its_marks() {
    let mut e = ed("ae\u{301}b");
    e.cursor = 1;
    assert!(e.delete());
    assert_eq!(e.text, "ab");
    e.cursor = 2;
    assert!(!e.delete());
    let mut e = ed("한글");
    e.cursor = 0;
    e.delete();
    assert_eq!(e.text, "글");
}

#[test]
fn the_cursor_moves_by_character_and_by_word() {
    let mut e = ed("one two  three");
    e.word_left();
    assert_eq!(at(&e), "one two  ");
    e.word_left();
    assert_eq!(at(&e), "one ");
    e.word_left();
    e.word_left();
    assert_eq!(at(&e), "");
    e.word_right();
    assert_eq!(at(&e), "one");
    e.word_right();
    assert_eq!(at(&e), "one two");
    e.right();
    e.left();
    assert_eq!(at(&e), "one two");
    let mut e = ed("a/b.rs");
    e.word_left();
    assert_eq!(at(&e), "a/b.");
    let mut e = ed("한글 입력\u{301}");
    e.left();
    assert_eq!(at(&e), "한글 입");
}

#[test]
fn ctrl_backspace_removes_a_word_and_ctrl_k_the_rest_of_the_line() {
    let mut e = ed("one two  ");
    assert!(e.delete_word_back());
    assert_eq!(e.text, "one ");
    let mut e = ed("alpha beta\ngamma");
    e.cursor = 3;
    assert!(e.kill_to_end());
    assert_eq!(e.text, "alp\ngamma");
    assert!(e.kill_to_end());
    assert_eq!(e.text, "alpgamma");
    e.cursor = e.text.len();
    assert!(!e.kill_to_end());
    let mut e = ed("a\nb");
    e.cursor = 0;
    assert!(e.clear() && e.text.is_empty() && e.cursor == 0);
    assert!(!e.clear());
}

#[test]
fn rows_break_at_the_width_and_at_a_newline_and_up_and_down_keep_the_column() {
    let mut e = ed("abcdef\n\nxy");
    e.cols = 4;
    e.fit();
    let rows: Vec<&str> = e.rows.iter().map(|r| &e.text[r.0..r.1]).collect();
    assert_eq!(rows, ["abcd", "ef", "", "xy"]);
    e.cursor = 3;
    assert!(e.vertical(true));
    assert_eq!(at(&e), "abcdef");
    assert!(e.vertical(true));
    assert_eq!(at(&e), "abcdef\n");
    assert!(e.vertical(true));
    assert_eq!(at(&e), "abcdef\n\nxy");
    assert!(!e.vertical(true));
    assert!(e.vertical(false) && e.vertical(false) && e.vertical(false));
    assert_eq!(at(&e), "abc");
    assert!(!e.vertical(false) && e.on_first_row());
}

#[test]
fn wide_characters_take_two_columns_and_marks_none() {
    assert_eq!(width("한a"), 3);
    assert_eq!(width("e\u{301}"), 1);
    let rows = wrap("한한한", 5);
    assert_eq!(rows, [(0, 6), (6, 9)]);
    assert_eq!(wrap("", 5), [(0, 0)]);
    assert_eq!(wrap("a\n", 5), [(0, 1), (2, 2)]);
    assert_eq!(wrap("abc", 1), [(0, 2), (2, 3)]);
}

#[test]
fn home_and_end_go_to_the_ends_of_the_row() {
    let mut e = ed("abcdefgh");
    e.cols = 4;
    e.cursor = 6;
    e.home();
    assert_eq!(e.cursor, 4);
    e.end();
    assert_eq!(e.cursor, 8);
    assert_eq!(e.row_of(e.cursor), 1);
    e.cursor = 1;
    e.end();
    assert_eq!(e.cursor, 3);
}

#[test]
fn a_paste_is_normalised_and_checked_as_a_whole() {
    assert_eq!(normalise("a\r\nb\rc\td\u{1b}[31me\u{7}\u{202E}f"), ("a\nb\nc\td[31mef".to_string(), 3));
    let mut e = ed("");
    e.paste("l1\r\nl2\r\n");
    assert_eq!(e.text, "l1\nl2\n");
    e.paste("\u{1b}\u{7}");
    assert!(e.note.as_deref().unwrap().contains("Nothing to paste"), "{:?}", e.note);
    assert_eq!(e.text, "l1\nl2\n");
    let mut e = ed("");
    e.paste("x\u{1b}y");
    assert_eq!(e.text, "xy");
    assert!(e.note.as_deref().unwrap().starts_with("1 control character was left out"));
    let mut e = ed("");
    e.paste(&"a\r\n".repeat(30_000));
    assert_eq!(e.text.len(), 60_000);
    let mut e = ed("");
    e.paste(&"a".repeat(MAX_TEXT));
    assert_eq!(e.text.len(), MAX_TEXT);
    e.paste("b");
    assert_eq!(e.text.len(), MAX_TEXT);
    assert!(e.note.as_deref().unwrap().starts_with("Paste refused"), "{:?}", e.note);
}

#[test]
fn typing_past_the_limit_is_refused_with_the_reason_and_a_returned_input_may_pass_it() {
    let mut e = ed(&"a".repeat(MAX_TEXT));
    assert!(!e.put("b"));
    assert!(e.note.as_deref().unwrap().contains("box is full"));
    assert_eq!(e.prepare_send("s").map(|t| t.len()), Some(MAX_TEXT));
    e.text = "b".repeat(10);
    e.give_back("s", true, &"a".repeat(MAX_TEXT));
    assert_eq!(e.text.len(), MAX_TEXT + 10);
    assert!(e.prepare_send("s").is_none());
    assert!(e.note.as_deref().unwrap().starts_with("Not sent"), "{:?}", e.note);
    assert_eq!(e.text.len(), MAX_TEXT + 10);
    assert!(e.delete_word_back());
    assert!(e.text.len() <= MAX_TEXT);
}

#[test]
fn an_empty_or_blank_draft_is_not_sent_and_nothing_is_said() {
    let mut e = ed("  \n\t");
    assert!(e.prepare_send("s").is_none() && e.note.is_none());
    assert_eq!(e.text, "  \n\t");
}

#[test]
fn drafts_are_kept_per_session_with_their_cursor_and_a_composition_goes_with_the_draft() {
    let mut e = ed("first draft");
    e.cursor = 5;
    e.park("a", "한");
    assert!(e.text.is_empty());
    e.set("other".to_string());
    e.park("b", "");
    e.resume("a");
    assert_eq!(e.text, "first한 draft");
    assert_eq!(at(&e), "first한");
    e.park("a", "");
    e.resume("b");
    assert_eq!(e.text, "other");
    e.resume("c");
    assert!(e.text.is_empty());
}

#[test]
fn up_recalls_what_the_session_was_sent_and_down_gives_the_draft_back() {
    let mut e = ed("");
    for t in ["one", "two", "three"] {
        e.set(t.to_string());
        assert_eq!(e.prepare_send("s").as_deref(), Some(t));
    }
    e.set("other session".to_string());
    e.prepare_send("t");
    e.set("my draft".to_string());
    assert!(e.recall_older("s"));
    assert_eq!(e.text, "three");
    assert!(e.recall_older("s") && e.text == "two");
    assert!(e.recall_older("s") && e.text == "one");
    assert!(e.recall_older("s") && e.text == "one");
    assert!(e.recall_newer("s") && e.text == "two");
    assert!(e.recall_newer("s") && e.text == "three");
    assert!(e.recall_newer("s"));
    assert_eq!((e.text.as_str(), e.cursor), ("my draft", 8));
    assert!(!e.recall_newer("s"));
    assert!(e.recall_older("s"));
    e.put("!");
    assert!(!e.recall_older("s"));
    assert_eq!(e.prepare_send("s").as_deref(), Some("three!"));
    assert_eq!(e.text, "my draft");
    e.clear();
    assert!(e.recall_older("s") && e.text == "three!");
    let mut n = ed("keep");
    assert!(!n.recall_older("nothing sent"));
    assert_eq!(n.text, "keep");
}

#[test]
fn the_history_keeps_twenty_per_session_and_256_kib_in_all_the_oldest_dropped_first() {
    let mut e = ed("");
    for i in 0..25 {
        e.set(format!("m{i}"));
        e.prepare_send("s");
    }
    assert_eq!(e.sent_count("s"), 20);
    assert_eq!(e.entry("s", 0).as_deref(), Some("m24"));
    assert_eq!(e.entry("s", 19).as_deref(), Some("m5"));
    let mut e = ed("");
    for i in 0..6 {
        e.set(format!("{i}{}", "x".repeat(MAX_TEXT - 1)));
        e.prepare_send(&format!("s{}", i % 3));
    }
    let kept: usize = e.sent.iter().map(|x| x.1.len()).sum();
    assert!(kept <= HISTORY_BYTES && e.sent.len() == 4, "{kept} bytes in {} entries", e.sent.len());
    assert!(e.sent.front().unwrap().1.starts_with('2'));
    let mut e = ed("");
    e.set("bad".into());
    e.prepare_send("s");
    e.give_back("s", true, "bad");
    assert_eq!((e.sent_count("s"), e.text.as_str()), (0, "bad"));
    e.give_back("other", false, "text");
    e.resume("other");
    assert_eq!(e.text, "text");
}

#[test]
fn the_box_shows_at_most_the_cap_and_keeps_the_cursor_row_in_view() {
    let mut e = ed(&(0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));
    let p = e.prompt(30, 8, "");
    assert_eq!(p.rows.len(), 8);
    assert_eq!(p.rows[7], "  line 29");
    assert_eq!(p.caret, Some((7, "  line 29".len())));
    assert!(p.note.as_deref().unwrap().contains("rows 23-30 of 30"), "{:?}", p.note);
    e.cursor = 0;
    let p = e.prompt(30, 8, "");
    assert_eq!(p.rows[0], "> line 0");
    assert_eq!(p.caret, Some((0, 2)));
    let mut e = ed("a\tb\nc");
    let p = e.prompt(30, 8, "");
    assert_eq!(p.rows, ["> a    b", "  c"]);
    assert!(p.note.is_none());
    e.cursor = 1;
    let p = e.prompt(30, 8, "한");
    assert_eq!(p.rows[0], "> a[한]    b");
    assert!(p.caret.is_none());
    assert_eq!(e.prompt(30, 1, "").rows.len(), 1);
}

#[test]
fn a_deleted_session_leaves_neither_a_draft_nor_a_history_and_the_others_keep_theirs() {
    let mut e = ed("");
    for s in ["gone", "kept"] {
        e.set(format!("sent in {s}"));
        e.prepare_send(s);
        e.set(format!("draft of {s}"));
        e.park(s, "");
    }
    e.forget("gone");
    assert_eq!((e.sent_count("gone"), e.sent_count("kept")), (0, 1));
    e.resume("gone");
    assert_eq!(e.text, "");
    e.resume("kept");
    assert_eq!(e.text, "draft of kept");
}

#[test]
fn shift_movements_select_graphemes_words_and_lines_and_edits_replace_or_cut_selection() {
    let mut e = ed("alpha 한ae\u{301} beta\ngamma");
    e.cursor = 6; // right before "한"
    e.move_cursor(true, Move::Right);
    assert_eq!(e.selected_text().as_deref(), Some("한"));
    e.move_cursor(true, Move::Right);
    assert_eq!(e.selected_text().as_deref(), Some("한a"));
    e.move_cursor(true, Move::Right);
    assert_eq!(e.selected_text().as_deref(), Some("한ae\u{301}"));
    // Unshifted Left collapses to the left edge of the selection without deleting anything.
    e.move_cursor(false, Move::Left);
    assert_eq!((e.cursor, e.has_selection()), (6, false));
    // Shift+WordRight selects the word, and typing replaces the selection in one step.
    e.move_cursor(true, Move::WordRight);
    assert_eq!(e.selected_text().as_deref(), Some("한ae\u{301}"));
    assert!(e.put("X"));
    assert_eq!(e.text, "alpha X beta\ngamma");
    assert!(!e.has_selection() && e.cursor == 7);
    // Shift+End selects to end of row; cut_selection returns and removes it.
    e.move_cursor(true, Move::End);
    assert_eq!(e.selected_text().as_deref(), Some(" beta"));
    assert_eq!(e.cut_selection().as_deref(), Some(" beta"));
    assert_eq!(e.text, "alpha X\ngamma");
    // Shift+Down selects across the newline into the next row; Backspace removes only the selection.
    e.cursor = 0;
    e.move_cursor(true, Move::Down);
    assert_eq!(e.selected_text().as_deref(), Some("alpha X\n"));
    assert!(e.backspace());
    assert_eq!((e.text.as_str(), e.cursor), ("gamma", 0));
}

#[test]
fn shown_and_text_offsets_round_trip_across_tabs_hangul_and_combining_marks() {
    let slice = "a\t한e\u{301}b";
    assert_eq!(shown_offset(slice, 0), 2);
    assert_eq!(shown_offset(slice, 1), 3); // after 'a'
    assert_eq!(shown_offset(slice, 2), 7); // after '\t' (4 spaces)
    assert_eq!(shown_offset(slice, 5), 10); // after "한" (3 UTF-8 bytes)
    assert_eq!(text_offset_from_shown(slice, 0), 0);
    assert_eq!(text_offset_from_shown(slice, 2), 0);
    assert_eq!(text_offset_from_shown(slice, 3), 1); // first half of '\t' -> before '\t'
    assert_eq!(text_offset_from_shown(slice, 5), 2); // second half of '\t' -> after '\t'
    assert_eq!(text_offset_from_shown(slice, 7), 2); // start of "한"
    assert_eq!(text_offset_from_shown(slice, 10), 5); // start of "e\u{301}"
    assert_eq!(text_offset_from_shown(slice, 11), 5); // interior combining mark snaps back to base 'e'
    assert_eq!(text_offset_from_shown(slice, 13), 8); // 'b'
    assert_eq!(text_offset_from_shown(slice, 99), slice.len());
}

#[test]
fn undo_and_redo_are_bounded_kept_per_session_across_panels_and_dropped_on_forget() {
    let mut e = ed("");
    for ch in "hello world".chars() {
        e.put(&ch.to_string());
    }
    assert_eq!(e.text, "hello world");
    // Word-coalesced typing: one undo removes "world", second removes "hello ".
    assert!(e.undo() && e.text == "hello ");
    assert!(e.undo() && e.text.is_empty());
    assert!(!e.undo());
    assert!(e.redo() && e.text == "hello ");
    assert!(e.redo() && e.text == "hello world");
    assert!(!e.redo());

    // Panel take/restore preserves the session's undo/redo stack.
    assert!(e.undo() && e.text == "hello ");
    let stash = e.take();
    e.put("panel input");
    assert_eq!(e.text, "panel input");
    e.restore(stash);
    assert_eq!(e.text, "hello ");
    assert!(e.redo() && e.text == "hello world");

    // Per-session isolation across park/resume and cleanup on forget.
    e.park("s1", "");
    e.resume("s2");
    e.paste("second");
    e.paste(" draft");
    assert!(e.undo() && e.text == "second");
    e.park("s2", "");
    e.resume("s1");
    assert!(e.undo() && e.text == "hello ");
    e.park("s1", "");
    e.forget("s1");
    e.resume("s1");
    assert!(e.text.is_empty() && !e.undo());

    // Cap checks: <= 32 undo snapshots per session and <= 128 KiB total across sessions.
    let mut cap_ed = ed("");
    for i in 0..45 {
        cap_ed.paste(&format!("step-{i} "));
    }
    assert!(cap_ed.undo.len() <= UNDO_PER_SESSION);
    let mut mem_ed = ed("");
    for s in ["a", "b", "c", "d"] {
        mem_ed.resume(s);
        mem_ed.paste(&"x".repeat(48 * 1024));
        mem_ed.clear();
        mem_ed.park(s, "");
    }
    let total_bytes: usize = mem_ed
        .parked
        .values()
        .map(|p| p.undo.iter().chain(p.redo.iter()).map(|sn| sn.text.len()).sum::<usize>())
        .sum();
    assert!(total_bytes <= UNDO_BYTES, "undo bytes exceeded cap: {total_bytes}");
}
