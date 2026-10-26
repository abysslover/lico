//! Unit tests of the input box: this is where its keys, limits and caps are checked exhaustively; one window test covers the wiring.

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
    assert_eq!(after("ae\u{301}"), "a"); // an accented letter in two code points
    assert_eq!(after("a\u{1112}\u{1161}\u{11AB}"), "a"); // a decomposed Hangul syllable
    assert_eq!(after("x\u{1F468}\u{200D}\u{1F469}"), "x"); // a joined emoji pair
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
    assert!(!e.delete()); // nothing after the cursor
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
    e.word_left(); // already at the start
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
    assert_eq!(at(&e), "한글 입"); // the mark goes with its letter
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
    assert!(e.kill_to_end()); // at the end of a line it joins the next one
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
    e.cursor = 3; // "abc|d"
    assert!(e.vertical(true));
    assert_eq!(at(&e), "abcdef"); // the row "ef" is shorter: the end of it
    assert!(e.vertical(true));
    assert_eq!(at(&e), "abcdef\n"); // the empty row
    assert!(e.vertical(true));
    assert_eq!(at(&e), "abcdef\n\nxy"); // the goal column 3 is past "xy"
    assert!(!e.vertical(true));
    assert!(e.vertical(false) && e.vertical(false) && e.vertical(false));
    assert_eq!(at(&e), "abc"); // the column kept all the way up
    assert!(!e.vertical(false) && e.on_first_row());
}

#[test]
fn wide_characters_take_two_columns_and_marks_none() {
    assert_eq!(width("한a"), 3);
    assert_eq!(width("e\u{301}"), 1);
    let rows = wrap("한한한", 5);
    assert_eq!(rows, [(0, 6), (6, 9)]); // two fit in 5 columns, the third wraps
    assert_eq!(wrap("", 5), [(0, 0)]);
    assert_eq!(wrap("a\n", 5), [(0, 1), (2, 2)]); // a trailing newline leaves an empty row to type in
    assert_eq!(wrap("abc", 1), [(0, 2), (2, 3)]); // the width never goes below 2, and a row is never empty for lack of room
}

#[test]
fn home_and_end_go_to_the_ends_of_the_row() {
    let mut e = ed("abcdefgh");
    e.cols = 4;
    e.cursor = 6;
    e.home();
    assert_eq!(e.cursor, 4);
    e.end();
    assert_eq!(e.cursor, 8); // the last row ends at the end of the text
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
    // the limit applies to the normalised text: 30,000 CRLF pairs are 90,000 bytes as they come and 60,000 once they are LF
    let mut e = ed("");
    e.paste(&"a\r\n".repeat(30_000));
    assert_eq!(e.text.len(), 60_000);
    let mut e = ed("");
    e.paste(&"a".repeat(MAX_TEXT));
    assert_eq!(e.text.len(), MAX_TEXT);
    e.paste("b"); // one byte more: refused whole, the reason said
    assert_eq!(e.text.len(), MAX_TEXT);
    assert!(e.note.as_deref().unwrap().starts_with("Paste refused"), "{:?}", e.note);
}

#[test]
fn typing_past_the_limit_is_refused_with_the_reason_and_a_returned_input_may_pass_it() {
    let mut e = ed(&"a".repeat(MAX_TEXT));
    assert!(!e.put("b"));
    assert!(e.note.as_deref().unwrap().contains("box is full"));
    assert_eq!(e.prepare_send("s").map(|t| t.len()), Some(MAX_TEXT));
    // a refused input comes back in front of a new draft: over the limit, kept, not sent
    e.text = "b".repeat(10);
    e.give_back("s", true, &"a".repeat(MAX_TEXT));
    assert_eq!(e.text.len(), MAX_TEXT + 10);
    assert!(e.prepare_send("s").is_none());
    assert!(e.note.as_deref().unwrap().starts_with("Not sent"), "{:?}", e.note);
    assert_eq!(e.text.len(), MAX_TEXT + 10); // still there
    assert!(e.delete_word_back()); // the person shortens it
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
    assert!(e.recall_older("s") && e.text == "one"); // the oldest: stays
    assert!(e.recall_newer("s") && e.text == "two");
    assert!(e.recall_newer("s") && e.text == "three");
    assert!(e.recall_newer("s"));
    assert_eq!((e.text.as_str(), e.cursor), ("my draft", 8)); // the draft is back
    assert!(!e.recall_newer("s"));
    // an edited recalled input is not replaced by a further Up; sending it brings the draft back
    assert!(e.recall_older("s"));
    e.put("!");
    assert!(!e.recall_older("s"));
    assert_eq!(e.prepare_send("s").as_deref(), Some("three!"));
    assert_eq!(e.text, "my draft");
    // an empty box recalls; a session with nothing sent has nothing to recall and the draft stays
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
    assert_eq!(e.entry("s", 19).as_deref(), Some("m5")); // m0..m4 are gone
    let mut e = ed("");
    for i in 0..6 {
        e.set(format!("{i}{}", "x".repeat(MAX_TEXT - 1)));
        e.prepare_send(&format!("s{}", i % 3));
    }
    let kept: usize = e.sent.iter().map(|x| x.1.len()).sum();
    assert!(kept <= HISTORY_BYTES && e.sent.len() == 4, "{kept} bytes in {} entries", e.sent.len());
    assert!(e.sent.front().unwrap().1.starts_with('2')); // the first two went
    // a refused input leaves the history
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
    assert_eq!(p.rows[7], "  line 29"); // the cursor is at the end
    assert_eq!(p.caret, Some((7, "  line 29".len())));
    assert!(p.note.as_deref().unwrap().contains("rows 23-30 of 30"), "{:?}", p.note);
    e.cursor = 0;
    let p = e.prompt(30, 8, "");
    assert_eq!(p.rows[0], "> line 0");
    assert_eq!(p.caret, Some((0, 2)));
    // few rows: as many as there are, with no note
    let mut e = ed("a\tb\nc");
    let p = e.prompt(30, 8, "");
    assert_eq!(p.rows, ["> a    b", "  c"]); // the tab is shown as four spaces
    assert!(p.note.is_none());
    // a composition is drawn in brackets at the cursor, and there is no caret then
    e.cursor = 1;
    let p = e.prompt(30, 8, "한");
    assert_eq!(p.rows[0], "> a[한]    b");
    assert!(p.caret.is_none());
    // a cap of one row is honoured on a small window
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
