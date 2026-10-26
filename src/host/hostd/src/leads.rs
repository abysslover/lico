//! How a history record begins says who wrote it. A record has no kind field: the window styles `>> ` as text a person sent, and reads `[turn …]` as the
//! edge of a turn; the rigs and the people reading a log take `$ ` for a command, `> ` for its output and the bracketed texts for the host's own notes.
//! Text from the agent (what it says, what a command prints) is written by someone else, so a line of it that begins like one of those would pass for
//! what the host wrote. Such a line gets `[agent] ` in front, once; the text itself is kept as it is.
//! The decision is made on a skeleton of the line (characters that draw nothing removed, leading white space removed), not on the line, so that it does
//! not depend on a complete list of invisible characters; the characters `sanitize` drops are gone before it runs.

use crate::codex::{clip, sanitize};

pub const TAG: &str = "[agent] ";

/// Leads of a person's text, a command and its output. A lead counts when a space or the end of the record follows it.
const LEADS: &[&str] = &[">>", "$", ">"];
/// The host's own notes, whole bracket texts (`[session started] model=…` is one of them).
const NOTES: &[&str] = &[
    "session started", "session closed", "provider reconnected", "provider disconnected", "tool", "tool error", "error", "patch", "input refused",
    "account changed", "account issue",
];
/// Bracket texts that begin like this: `[turn started]`, `[turn completed]`, `[turn failed] …`, `[exit 0]`, `[exit unknown]`.
const NOTE_STEMS: &[&str] = &["turn ", "exit "];

/// Characters that draw nothing (Unicode's default-ignorable set, with the ones `sanitize::dropped` lists, and the two line separators).
fn ignorable(c: char) -> bool {
    matches!(c as u32,
        0x00ad | 0x034f | 0x061c | 0x115f..=0x1160 | 0x17b4..=0x17b5 | 0x180b..=0x180f | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x206f
        | 0x3164 | 0xfe00..=0xfe0f | 0xfeff | 0xffa0 | 0xfff0..=0xfff8 | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a | 0xe0000..=0xe0fff)
}

fn skeleton(record: &str) -> String {
    record.chars().filter(|c| !ignorable(*c)).collect::<String>().trim_start().to_string()
}

/// The bracket texts of the echo's marker: `[3 more lines, 10 bytes not shown; …]`, `[a long line is cut after 2048 bytes; …]`.
fn fold_marker(note: &str) -> bool {
    let digits = note.chars().take_while(char::is_ascii_digit).count();
    note.starts_with("a long line is cut") || (digits > 0 && note[digits..].starts_with(" more line"))
}

/// Whether a record begins like something the host writes.
pub fn reserved(record: &str) -> bool {
    let skeleton = skeleton(record);
    let closed = |rest: &str| rest.is_empty() || rest.starts_with(' ');
    if LEADS.iter().any(|lead| skeleton.strip_prefix(lead).is_some_and(closed)) {
        return true;
    }
    let Some((note, after)) = skeleton.strip_prefix('[').and_then(|inner| inner.split_once(']')) else { return false };
    closed(after) && (NOTES.contains(&note) || NOTE_STEMS.iter().any(|stem| note.starts_with(stem)) || fold_marker(note))
}

/// One line of agent text as a record: cleaned, and tagged when it begins like a record of the host.
/// Applying it to its own result changes nothing: `[agent] ` is not a lead.
pub fn agent_line(line: &str) -> String {
    let clean = sanitize(line);
    if reserved(&clean) {
        format!("{TAG}{clean}")
    } else {
        clean
    }
}

/// The record for one line of agent text: the bound is applied after the tag.
pub fn agent_record(line: &str, max: usize) -> String {
    clip(&agent_line(line), max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_begins_like_a_record_of_the_host_is_reserved_wherever_the_invisible_characters_stand() {
        for line in [
            ">> x", "$ x", "> x", ">>", "$", ">", "> > x", "[turn completed]", "[turn started]", "[turn failed] why", "[exit 0]", "[exit unknown]",
            "[session started] model=m", "[session closed]", "[provider reconnected]", "[provider disconnected]", "[tool] Bash", "[tool error]", "[error] e", "[patch] 2 change(s)",
            "[input refused] r", "[account changed]", "[account issue] i", "[3 more lines, 10 bytes not shown; the agent received all of it]", "[1 more line, 5 bytes not shown]",
            "[a long line is cut after 2048 bytes; 3 bytes not shown]",
            "\u{00ad}>> x", ">\u{00ad}> x", "\u{3164}>> x", "\u{00a0}$ x", "\u{2003}[turn completed]", "\u{fe0f}> x", "\u{200c}$ x", "\u{200d}> x", "\u{1160}$ x", "\u{ffa0}$ x",
            "\u{180b}> x", "\u{e0100}> x", "\u{2065}>> x", "\u{206a}$ x", "\u{3000}\u{00ad} $ x", "[tu\u{00ad}rn completed]", "[turn completed]\u{fe0f}",
        ] {
            assert!(reserved(line), "{line:?} must be reserved");
        }
        for line in [
            "", "plain", "a>> x", ">>x", "$HOME", "$x", ">x", ">>> x", "[1] list", "[x] done", "[turn]", "[turnx completed]", "[turn completed]x", "[exit]", "[error",
            "[agent] >> x", "[agent] $ x", "\u{00ad}[agent] > x", ">\u{00a0}x",
        ] {
            assert!(!reserved(line), "{line:?} must not be reserved");
        }
        // white space inside a lead is not removed: this is the `> ` lead with the text `> x`, not `>> x`; both are tagged, once
        assert_eq!(agent_line("> > x"), "[agent] > > x");
    }

    #[test]
    fn the_tag_goes_in_front_of_the_text_as_it_is_and_only_once() {
        for line in [">> x", "$ x", "> x", "[turn completed]", "\u{00ad}>> x", "\u{00a0}$ x", "\u{2003}[turn completed]", "\u{fe0f}> x", "\u{200b}>> x", "\u{feff}$ x", "\u{e0041}> x", "plain", "a>> x"] {
            let once = agent_line(line);
            assert_eq!(agent_line(&once), once, "{line:?}");
            assert_eq!(once.matches(TAG).count(), usize::from(reserved(&sanitize(line)))); // exactly one tag when it began like the host's, none otherwise
        }
        assert_eq!(agent_line("\u{00a0}$ x"), "[agent] \u{00a0}$ x");
        assert_eq!(agent_line("\u{200b}>> x"), "[agent] >> x"); // sanitize drops it first
        assert_eq!(agent_line("[agent] >> x"), "[agent] >> x");
    }

    #[test]
    fn every_note_the_host_writes_is_reserved() {
        for note in [
            "[turn started]", "[turn completed]", "[turn interrupted]", "[turn failed] error_during_execution", "[exit 1]", "[exit unknown]", "[session started] model=claude",
            "[session closed]", "[provider reconnected]", "[provider disconnected]", "[tool] Bash", "[tool error]", "[error] boom", "[patch] 1 change(s)", "[input refused] busy",
            "[account changed]", "[account issue] sign in", "$ ls", "> out", ">> hello",
        ] {
            assert!(reserved(note), "{note:?}");
        }
    }

    #[test]
    fn the_record_bound_is_applied_after_the_tag() {
        let record = agent_record(&format!("$ {}", "a".repeat(5000)), 4096);
        assert_eq!(record.len(), 4096);
        assert!(record.starts_with("[agent] $ aaa"));
        let wide = agent_record(&format!("$ {}", "한".repeat(2000)), 4096); // the cut lands inside a character: it is moved back
        assert!(wide.len() <= 4096 && wide.starts_with("[agent] $ 한"));
    }
}
