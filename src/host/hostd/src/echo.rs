//! What the history shows of an input the host accepted: the first lines of the text, folded.
//!
//! The provider keeps the whole text in its own record; the host keeps a short, marked copy so that a
//! prompt does not vanish after Enter, in any window, after a restart, and for every kind of session.
//! One input costs at most `ECHO_LINES + 1` records. Each shown line passes the same `sanitize` as every
//! other record (a bidi override or a line separator in a person's text must not reach the log raw);
//! the counts in the marker are those of the original text.

use crate::codex::{clip, sanitize};
use crate::session::Session;

pub const ECHO_LINES: usize = 12;
pub const ECHO_BYTES: usize = 2048; // of line text, newlines not counted
/// History records have no kind field: this prefix is the marker. No other kind of record may begin
/// with it (`> ` is a tool result, `$ ` a command, `[` a note): the window styles `>> ` as text a person
/// (later also another session) sent. Agent text that begins like this is tagged by both adapters (`leads`).
pub const PREFIX: &str = ">> ";

/// The records for one input: at most `ECHO_LINES` lines within `ECHO_BYTES`, then one marker with
/// exact counts when anything was left out. A first line over the byte cap is cut at the cap.
pub fn records(text: &str) -> Vec<String> {
    let total = text.split_inclusive('\n').count();
    let mut out: Vec<String> = Vec::new();
    let (mut used, mut shown_len, mut cut) = (0, 0, None);
    for piece in text.split_inclusive('\n') {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        if out.len() == ECHO_LINES {
            break;
        }
        if used + line.len() <= ECHO_BYTES {
            out.push(format!("{PREFIX}{}", sanitize(line)));
            used += line.len();
            shown_len += piece.len();
        } else {
            if out.is_empty() {
                let head = clip(line, ECHO_BYTES);
                shown_len = head.len();
                cut = Some(head.len());
                out.push(format!("{PREFIX}{}", sanitize(&head)));
            }
            break;
        }
    }
    if text.is_empty() {
        out.push(PREFIX.to_string());
    }
    if shown_len < text.len() {
        let more = total - out.len();
        let lines = match more {
            0 => String::new(),
            1 => "1 more line, ".to_string(),
            _ => format!("{more} more lines, "),
        };
        let cut = cut.map(|k| format!("a long line is cut after {k} bytes; ")).unwrap_or_default();
        out.push(format!("{PREFIX}[{cut}{lines}{} bytes not shown; the agent received all of it]", text.len() - shown_len));
    }
    out
}

/// Called once for each input the host accepts (and again for none of its retries).
pub fn append(s: &mut Session, text: &str) {
    for record in records(text) {
        s.append(record.into_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_input_is_one_record_per_line_and_never_marked() {
        assert_eq!(records("hello"), [">> hello"]);
        assert_eq!(records("a\n\nb\n"), [">> a", ">> ", ">> b"]);
        let twelve: String = (0..ECHO_LINES).map(|n| format!("l{n}\n")).collect();
        assert_eq!(records(&twelve).len(), ECHO_LINES);
    }

    #[test]
    fn more_lines_than_the_cap_end_in_a_marker_with_exact_counts() {
        let text: String = (0..30).map(|n| format!("l{n}\n")).collect();
        let out = records(&text);
        assert_eq!(out.len(), ECHO_LINES + 1);
        assert_eq!(out[ECHO_LINES - 1], ">> l11");
        let shown: usize = out[..ECHO_LINES].iter().map(|r| r.len() - PREFIX.len() + 1).sum();
        assert_eq!(out[ECHO_LINES], format!(">> [18 more lines, {} bytes not shown; the agent received all of it]", text.len() - shown));
        assert!(records(&format!("{}x", &text[..text.len() - 1])).last().unwrap().contains("18 more lines"));
        assert!(records("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13").last().unwrap().starts_with(">> [1 more line, 2 bytes not shown;"));
    }

    #[test]
    fn the_byte_cap_stops_at_a_line_and_a_long_first_line_is_cut_on_a_character() {
        let line = "x".repeat(1000);
        let out = records(&format!("{line}\n{line}\n{line}\n"));
        assert_eq!(out.len(), 3);
        assert_eq!(out[2], ">> [1 more line, 1001 bytes not shown; the agent received all of it]");
        let long = "한".repeat(1000); // 3000 bytes
        let out = records(&format!("{long}\nnext"));
        assert_eq!(out[0].len() - PREFIX.len(), 2046);
        assert_eq!(out[1], ">> [a long line is cut after 2046 bytes; 1 more line, 959 bytes not shown; the agent received all of it]");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn a_shown_line_loses_what_sanitize_removes_but_the_counts_are_those_of_the_original() {
        assert_eq!(records("a\tb\u{202e}c\u{2028}d"), [">> a bcd"]);
        let out = records(&format!("{}\u{202e}", "x".repeat(ECHO_BYTES)));          // cut on the original bytes: 2051 > 2048
        assert_eq!(out, [format!(">> {}", "x".repeat(ECHO_BYTES)), ">> [a long line is cut after 2048 bytes; 3 bytes not shown; the agent received all of it]".to_string()]);
    }

    #[test]
    fn a_cut_line_with_nothing_after_it_has_no_zero_clause() {
        let out = records(&"é".repeat(ECHO_BYTES));
        assert_eq!(out[1], ">> [a long line is cut after 2048 bytes; 2048 bytes not shown; the agent received all of it]");
    }
}
