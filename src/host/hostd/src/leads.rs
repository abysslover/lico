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
/// Bracket texts that begin like this: `[turn started]`, `[turn completed]`, `[turn failed] …`, `[exit 0]`, `[exit unknown]`,
/// `[log recovered: 12 byte(s) of a damaged tail and 1 segment(s) set aside]` (written by `registry` when a log is recovered).
const NOTE_STEMS: &[&str] = &["turn ", "exit ", "log recovered:"];

/// Characters that draw nothing (Unicode's default-ignorable set, with the ones `sanitize::dropped` lists, and the two line separators).
pub(crate) fn invisible(c: char) -> bool {
    matches!(c as u32,
        0x00ad | 0x034f | 0x061c | 0x115f..=0x1160 | 0x17b4..=0x17b5 | 0x180b..=0x180f | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x206f
        | 0x3164 | 0xfe00..=0xfe0f | 0xfeff | 0xffa0 | 0xfff0..=0xfff8 | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a | 0xe0000..=0xe0fff)
}

/// Those, and the ones that draw nothing alone or only on the character before them: the format characters, the nonspacing and enclosing marks and U+2800 (`marks`).
pub(crate) fn ignorable(c: char) -> bool {
    crate::marks::covers(c) || invisible(c)
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

/// Whether a skeleton begins like the header the host writes in front of a text from another session: a bracket, then any white space or controls, then `from` in
/// any case, and then anything but a letter or a digit (or nothing). A reader takes `[ from x` and `[FROM` for it as it takes `[from x`.
fn header_like(skeleton: &str) -> bool {
    let rest = skeleton.strip_prefix('[').map(|rest| rest.trim_start_matches(|c: char| c.is_whitespace() || c.is_control()));
    rest.and_then(|rest| Some((rest.get(..4)?, &rest[4..]))).is_some_and(|(word, after)| word.eq_ignore_ascii_case("from") && after.chars().next().is_none_or(|c| !c.is_alphanumeric()))
}

/// A line of text from another session that begins like the header the host writes in front of it (`header_like`) gets one leading space, so that no line but the
/// host's own begins that way (S03a). Judged on the skeleton, like `reserved`: white space or a character that draws nothing in front does not hide it. The text is
/// kept as it is. A look-alike with a letter of another script inside `from` (a Cyrillic o) is not caught: the skeleton does not fold scripts.
pub fn defuse(line: &str) -> String {
    if header_like(&skeleton(line)) {
        format!(" {line}")
    } else {
        line.to_string()
    }
}

/// The record for one line of agent text: the bound is applied after the tag.
pub fn agent_record(line: &str, max: usize) -> String {
    clip(&agent_line(line), max)
}

/// A text an agent wrote, cleaned once; `removed` is how many characters it lost (escape sequences count by their characters, a line end that changes does not).
#[derive(Debug, PartialEq)]
pub struct Cleaned {
    pub text: String,
    pub removed: usize,
}

/// The characters of a text that are removed for what they hide, besides those `strip` drops: U+200C and U+200D stay (see `codex::dropped`) and so do the two
/// selectors that make a character text or emoji, which stay only alone (`agent_text`).
fn hidden(c: char) -> bool {
    let code = c as u32;
    !matches!(code, 0x200c | 0x200d | 0xfe0e | 0xfe0f) && (invisible(c) || matches!(code, 0xfff9..=0xfffb | 0x13430..=0x1343f) || crate::codex::dropped(code))
}

fn selector(c: char) -> bool {
    matches!(c, '\u{fe0e}' | '\u{fe0f}')
}

/// The text of an agent as it is shown to a person and handed to a session: line ends are line feeds; escape sequences, controls and every character that draws
/// nothing or moves what is drawn are gone; a tab, a line feed, the spaces, the marks that go on a letter and the joiners of a script or an emoji stay.
/// A selector stays when it is alone and stands directly after a character that is not a space (a run of them carries data and a lone one at a start or after a
/// space changes nothing). Cleaning what is clean changes nothing. The count is of characters, escape sequences included, after the line ends are set.
pub fn agent_text(text: &str) -> Cleaned {
    let lines = text.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = crate::codex::strip(&lines, true).chars().filter(|c| !hidden(*c)).collect();
    let mut clean = String::with_capacity(chars.len());
    for (k, &c) in chars.iter().enumerate() {
        if selector(c) {
            let alone = k > 0 && !selector(chars[k - 1]) && chars.get(k + 1).is_none_or(|next| !selector(*next));
            if !alone || chars[k - 1].is_whitespace() || matches!(chars[k - 1], '\u{200c}' | '\u{200d}') {
                continue;
            }
        }
        clean.push(c);
    }
    Cleaned { removed: lines.chars().count() - clean.chars().count(), text: clean }
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
            "[a long line is cut after 2048 bytes; 3 bytes not shown]", "[log recovered: 12 byte(s) of a damaged tail and 1 segment(s) set aside]",
            "\u{00ad}>> x", ">\u{00ad}> x", "\u{3164}>> x", "\u{00a0}$ x", "\u{2003}[turn completed]", "\u{fe0f}> x", "\u{200c}$ x", "\u{200d}> x", "\u{1160}$ x", "\u{ffa0}$ x",
            "\u{180b}> x", "\u{e0100}> x", "\u{2065}>> x", "\u{206a}$ x", "\u{3000}\u{00ad} $ x", "[tu\u{00ad}rn completed]", "[turn completed]\u{fe0f}",
        ] {
            assert!(reserved(line), "{line:?} must be reserved");
        }
        for line in [
            "", "plain", "a>> x", ">>x", "$HOME", "$x", ">x", ">>> x", "[1] list", "[x] done", "[turn]", "[turnx completed]", "[turn completed]x", "[exit]", "[error", "[log recovered]", "[log recovery]",
            "[agent] >> x", "[agent] $ x", "\u{00ad}[agent] > x", ">\u{00a0}x",
        ] {
            assert!(!reserved(line), "{line:?} must not be reserved");
        }
        // white space inside a lead is not removed: this is the `> ` lead with the text `> x`, not `>> x`; both are tagged, once
        assert_eq!(agent_line("> > x"), "[agent] > > x");
    }

    /// V4: the first and the last code point of every range of `marks` (format characters, nonspacing and enclosing marks, U+2800), in front of a lead, inside
    /// it, between it and its space and inside the word of a note; and where a mark does not make a lead (a letter in front, no space behind).
    #[test]
    fn a_mark_does_not_hide_a_lead_at_either_end_of_any_range_of_the_table() {
        for &(first, last) in crate::marks::RANGES {
            for c in [first, last].map(|code| char::from_u32(code).unwrap()) {
                for line in [format!("{c}>> x"), format!(">{c}> x"), format!("{c}$ x"), format!(">{c} x"), format!("{c}[turn completed]"), format!("[turn{c} completed]")] {
                    assert!(reserved(&line), "{line:?} must be reserved");
                }
                for line in [format!("a{c}>> x"), format!(">{c}x"), format!("${c}HOME")] {
                    assert!(!reserved(&line), "{line:?} must not be reserved");
                }
            }
        }
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
            "[account changed]", "[account issue] sign in", "[log recovered: 0 byte(s) of a damaged tail and 2 segment(s) set aside]", "$ ls", "> out", ">> hello",
        ] {
            assert!(reserved(note), "{note:?}");
        }
    }

    /// The table of lines of provider text and the records they become, that the reference mapping (`lico_protocol/codex_mapping.py`) and the built program
    /// (`tests/test_forged_leads.py`) are tested against as well.
    #[test]
    fn the_shared_table_of_provider_text_gives_the_records_it_lists() {
        let table: serde_json::Value = serde_json::from_str(include_str!("../../../../tests/fixtures/provider_text.json")).unwrap();
        let text = |v: &serde_json::Value| match v.as_str() {
            Some(s) => s.to_string(),
            None => v["prefix"].as_str().unwrap().to_string() + &v["unit"].as_str().unwrap().repeat(v["times"].as_u64().unwrap() as usize),
        };
        let rows: Vec<_> = table["rows"].as_array().unwrap().iter().chain(table["long"].as_array().unwrap()).collect();
        assert!(rows.len() > 90);
        let wrong: Vec<&str> = rows.into_iter().filter(|row| agent_record(&text(&row["line"]), 4096) != text(&row["record"])).map(|row| row["why"].as_str().unwrap()).collect();
        assert_eq!(wrong, Vec::<&str>::new());
    }

    #[test]
    fn a_line_that_begins_like_the_hosts_header_gets_a_space_wherever_the_invisible_characters_and_white_space_stand() {
        for line in [
            "[from the person | 10:00 | message | 3 B]", "[from session 'x' (s000000) | 10:00 | message | 3 B]", "[from x", "  [from x", "\u{200b}[from x",
            "\u{3000}\u{00ad}[from x", "\t[from x", "\u{fe0f}[from x", "\u{a0}[from x",
        ] {
            assert_eq!(defuse(line), format!(" {line}"), "{line:?}");
        }
        for line in ["", "plain", "[", "[ ]", "from x", " from [from x", "[turn completed]", "$ ls", "[agent] [from x", "[f rom x", "[ fro", "[ x from"] {
            assert_eq!(defuse(line), line, "{line:?}");
        }
    }

    /// The line may have white space between the bracket and the word, any case of the word, and anything but a letter or a digit after it, or nothing: each is a
    /// line that a reader can take for the host's header. A word that goes on, or another word, is not.
    #[test]
    fn a_line_that_has_white_space_between_the_bracket_and_from_or_another_case_of_it_is_defused_too_and_a_longer_word_is_not() {
        for line in [
            "[ from x", "[FROM x", "[From x", "[fRoM x", "[\tfrom x", "[\u{a0}from x", "[\u{3000}from x", "[\u{2003}\u{2009}from x", "[\u{b}from x", "[\u{c}from x",
            "[\u{85}from x", "[\u{1}from x", "[\u{200b} from x", "[ \u{200d}from x", "[from-", "[from]", "[from", "[ from", "[FROM", "[from:", "[from_x", "[from\u{a0}x", "[from\tx",
            "\u{3000}[ \u{200b}From|x", "  [\u{3000}\u{3000}from",
        ] {
            assert_eq!(defuse(line), format!(" {line}"), "{line:?}");
        }
        for line in [
            "[fromage] x", "[from9 x", "[FROM9", "[fromé x", "[from한 x", "[ fromage", "[\tfromage", "[fro", "[ fr om x", "[f\tr\to\tm x", "[the from x", "[fromx x",
        ] {
            assert_eq!(defuse(line), line, "{line:?}");
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

    fn cleaned(text: &str) -> (String, usize) {
        let got = agent_text(text);
        (got.text, got.removed)
    }

    /// One example of every class that is removed, with what is left and how many characters went: the first and the last of the range where it is one.
    #[test]
    fn an_agents_text_loses_every_escape_sequence_control_and_character_that_draws_nothing_or_moves_what_is_drawn_and_says_how_many() {
        for (class, text, left, removed) in [
            ("a colour sequence", "\u{1b}[31mred\u{1b}[0m", "red", 9),
            ("a title sequence", "a\u{1b}]0;title\u{7}b", "ab", 10),
            ("a title sequence ended by ESC and a backslash", "a\u{1b}]0;t\u{1b}\\b", "ab", 7),
            ("a two-character escape", "a\u{1b}Mb", "ab", 2),
            ("an escape that never ends is a two-character one: the rest is text", "a\u{1b}[31", "a31", 2),
            ("NUL and backspace", "a\u{0}\u{8}b", "ab", 2),
            ("vertical tab and form feed", "a\u{b}\u{c}b", "ab", 2),
            ("DEL", "a\u{7f}b", "ab", 1),
            ("C1, first and last", "a\u{80}\u{9b}\u{9d}\u{9f}b", "ab", 4),
            ("next line", "a\u{85}b", "ab", 1),
            ("tags, first and last", "a\u{e0000}\u{e0041}\u{e007f}b", "ab", 3),
            ("variation selectors of the supplement, first and last", "a\u{e0100}\u{e01ef}b", "ab", 2),
            ("the rest of the plane of the tags", "a\u{e0080}\u{e0fff}b", "ab", 2),
            ("zero width space", "a\u{200b}b", "ab", 1),
            ("left and right marks", "a\u{200e}\u{200f}b", "ab", 2),
            ("word joiner and the invisible operators", "a\u{2060}\u{2064}b", "ab", 2),
            ("the unassigned code point between", "a\u{2065}b", "ab", 1),
            ("deprecated format characters", "a\u{206a}\u{206f}b", "ab", 2),
            ("byte order mark", "a\u{feff}b", "ab", 1),
            ("direction overrides and isolates", "a\u{202a}\u{202e}\u{2066}\u{2069}b", "ab", 4),
            ("Arabic letter mark", "a\u{61c}b", "ab", 1),
            ("line and paragraph separators", "a\u{2028}\u{2029}b", "ab", 2),
            ("soft hyphen and grapheme joiner", "a\u{ad}\u{34f}b", "ab", 2),
            ("fillers that draw nothing", "a\u{115f}\u{1160}\u{3164}\u{ffa0}b", "ab", 4),
            ("Khmer inherent vowels", "a\u{17b4}\u{17b5}b", "ab", 2),
            ("Mongolian selectors and separator", "a\u{180b}\u{180e}\u{180f}b", "ab", 3),
            ("variation selectors 1 to 14", "a\u{fe00}\u{fe0d}b", "ab", 2),
            ("specials", "a\u{fff0}\u{fff8}\u{fff9}\u{fffb}b", "ab", 4),
            ("format controls of the hieroglyphs, the shorthand and the music", "a\u{13430}\u{1343f}\u{1bca0}\u{1bca3}\u{1d173}\u{1d17a}b", "ab", 6),
        ] {
            assert_eq!(cleaned(text), (left.to_string(), removed), "{class}");
        }
    }

    #[test]
    fn a_line_end_is_a_line_feed_and_a_tab_and_what_is_seen_stay() {
        assert_eq!(cleaned("a\r\nb\rc\n\td"), ("a\nb\nc\n\td".to_string(), 0), "CR LF and a lone CR are line feeds; nothing was hidden");
        for kept in [
            "plain text, a tab\tand a line\nend", "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}", "\u{2764}\u{fe0f} and \u{2764}\u{fe0e}", "1\u{fe0f}\u{20e3}",
            "\u{d55c}\u{ad6d}\u{c5b4} \u{d14d}\u{c2a4}\u{d2b8}", "\u{645}\u{6cc}\u{200c}\u{62e}\u{648}\u{627}\u{647}\u{645}", "e\u{301} \u{e01}\u{e49}", "\u{3000}\u{a0}\u{2003}\u{202f}\u{205f}x",
            "\u{2800}\u{600}1", "\u{1f1f0}\u{1f1f7}", "\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}",
        ] {
            assert_eq!(cleaned(kept), (kept.to_string(), 0), "{kept:?}");
        }
    }

    #[test]
    fn a_selector_stays_only_alone_directly_after_a_character_and_a_run_of_them_is_a_payload() {
        assert_eq!(cleaned("\u{2764}\u{fe0f}\u{fe0f}\u{fe0f}"), ("\u{2764}".to_string(), 3));
        assert_eq!(cleaned("\u{2764}\u{fe0f}\u{fe0e}"), ("\u{2764}".to_string(), 2));
        assert_eq!(cleaned("\u{fe0f}x"), ("x".to_string(), 1), "at the start there is no character for it to follow");
        assert_eq!(cleaned("a\n\u{fe0f}b"), ("a\nb".to_string(), 1), "after a line end");
        assert_eq!(cleaned("a \u{fe0f}b"), ("a b".to_string(), 1), "after a space");
        assert_eq!(cleaned("\u{2764}\u{200b}\u{fe0f}\u{200b}\u{fe0f}"), ("\u{2764}".to_string(), 4), "what is removed brings two together: a run all the same");
        assert_eq!(cleaned("\u{2764}\u{1b}[31m\u{fe0f}"), ("\u{2764}\u{fe0f}".to_string(), 5));
    }

    #[test]
    fn cleaning_is_done_once_what_is_clean_is_not_changed_and_an_all_hidden_text_is_nothing() {
        for text in [
            "\u{1b}[31mred\u{1b}[0m", "a\u{200b}\u{e0041}b\r\nc", "\u{2764}\u{fe0f}\u{fe0f}", "\u{2764}\u{200b}\u{fe0f}\u{200b}\u{fe0f}", "\u{fe0f}\u{2764}", "\u{2764}\u{200d}\u{fe0f}", "a\u{1b}", "\u{1b}]x\u{1b}[31m\u{7}y",
            "[from x | 10:00 | message | 1 B]", "\u{d55c}\u{200c}\u{200d}\u{2028}",
        ] {
            let once = agent_text(text);
            assert_eq!(agent_text(&once.text), Cleaned { text: once.text.clone(), removed: 0 }, "{text:?}");
        }
        assert_eq!(cleaned("\u{200b}\u{e0041}\u{1b}[0m"), (String::new(), 6));
        assert_eq!(cleaned(""), (String::new(), 0));
    }
}
