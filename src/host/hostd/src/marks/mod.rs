//! Characters that draw nothing on their own, or only on the character before them: the format characters (Cf), the nonspacing marks (Mn) and the enclosing
//! marks (Me) of Unicode 15.0.0, and U+2800 (the blank braille pattern, a symbol that draws an empty cell). `leads` leaves them out of the skeleton on which it
//! decides whether a line begins like a record of the host, so that a lead cannot be hidden behind one.
//! The table is generated (`python -m lico_protocol.gen_unicode_marks`, which also writes the Python mirror `lico_protocol/unicode_marks.py`) and pinned to one
//! version of Unicode: it does not follow the compiler's or the system's.
//! The second table is of the opening brackets (Ps) and the initial quotation marks (Pi) that are not ASCII (`python -m lico_protocol.gen_unicode_openers`, the
//! mirror is `lico_protocol/unicode_openers.py`), pinned the same way: `leads` gives a space to a line that begins with one of them, or with an ASCII `[`.

mod openers;
mod table;

pub(crate) use table::RANGES;

/// Whether `c` is one of them.
pub fn covers(c: char) -> bool {
    if c.is_ascii() {
        return false; // the table starts at U+00AD
    }
    let code = c as u32;
    let at = RANGES.partition_point(|&(_, last)| last < code);
    RANGES.get(at).is_some_and(|&(first, _)| first <= code)
}

/// Whether `c` is an opening bracket or an initial quotation mark that is not ASCII (a reader takes it for the `[` of a header, as it takes `[` itself).
pub fn opens(c: char) -> bool {
    if c.is_ascii() {
        return false; // the table starts at U+00AB; the ASCII bracket is the rule's own word
    }
    let code = c as u32;
    let at = openers::RANGES.partition_point(|&(_, last)| last < code);
    openers::RANGES.get(at).is_some_and(|&(first, _)| first <= code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(code: u32) -> bool {
        char::from_u32(code).is_some_and(covers)
    }

    #[test]
    fn the_table_is_sorted_apart_and_inside_unicode() {
        assert_eq!(table::UNICODE, "15.0.0");
        let mut before = None;
        for &(first, last) in RANGES {
            assert!(first <= last && last <= 0x10ffff, "{first:x}..{last:x}");
            assert!(!(0xd800..=0xdfff).contains(&first) && !(0xd800..=0xdfff).contains(&last), "a surrogate in {first:x}..{last:x}");
            if let Some(end) = before {
                assert!(first > end + 1, "{first:x} follows {end:x} without a gap: neighbours are joined");
            }
            before = Some(last);
        }
    }

    #[test]
    fn the_lookup_finds_every_code_point_of_the_table_and_no_other() {
        // 1,985 Mn + 170 Cf + 13 Me of Unicode 15.0.0 and U+2800: the number is written here, so that a table that was edited or generated from another
        // version of Unicode is noticed without the generator
        let wanted: u32 = RANGES.iter().map(|&(first, last)| last - first + 1).sum();
        assert_eq!(wanted, 1985 + 170 + 13 + 1);
        assert_eq!((0..=0x10ffff).filter(|&code| covered(code)).count() as u32, wanted);
        for &(first, last) in RANGES {
            assert!(covered(first) && covered(last), "{first:x}..{last:x}");
            assert!(!covered(first - 1) && !covered(last + 1), "{first:x}..{last:x}");
        }
    }

    #[test]
    fn the_gap_the_second_reviewer_found_is_in_it_and_ordinary_text_is_not() {
        // Cf that are not default-ignorable (32 code points), then Mn, Me and U+2800
        for code in [0x600..=0x605, 0x6dd..=0x6dd, 0x70f..=0x70f, 0x890..=0x891, 0x8e2..=0x8e2, 0xfff9..=0xfffb, 0x110bd..=0x110bd, 0x110cd..=0x110cd, 0x13430..=0x1343f]
            .into_iter()
            .flatten()
            .chain([0x300, 0x301, 0x94d, 0xe31, 0x1e94a, 0x488, 0x20dd, 0x20e3, 0x2800])
        {
            assert!(covered(code), "U+{code:04X}");
        }
        for c in ['a', 'Z', '0', ' ', '>', '$', '[', ']', '\u{a0}', '\u{e9}', '\u{d55c}', '\u{e01}', '\u{3042}', '\u{1f600}', '\u{3000}', '\u{2029}', '\u{ff1e}'] {
            assert!(!covers(c), "U+{:04X}", c as u32);
        }
    }

    fn opening(code: u32) -> bool {
        char::from_u32(code).is_some_and(opens)
    }

    #[test]
    fn the_table_of_openers_is_sorted_apart_inside_unicode_and_holds_nothing_ascii() {
        assert_eq!(openers::UNICODE, "15.0.0");
        let mut before = None;
        for &(first, last) in openers::RANGES {
            assert!(0x80 <= first && first <= last && last <= 0x10ffff, "{first:x}..{last:x}");
            if let Some(end) = before {
                assert!(first > end + 1, "{first:x} follows {end:x} without a gap: neighbours are joined");
            }
            before = Some(last);
        }
    }

    #[test]
    fn the_lookup_finds_every_code_point_of_the_table_of_openers_and_no_other() {
        // 76 Ps and 12 Pi that are not ASCII in Unicode 15.0.0: written here, so that a table that was edited or generated from another version is noticed
        let wanted: u32 = openers::RANGES.iter().map(|&(first, last)| last - first + 1).sum();
        assert_eq!(wanted, 76 + 12);
        assert_eq!((0..=0x10ffff).filter(|&code| opening(code)).count() as u32, wanted);
        for &(first, last) in openers::RANGES {
            assert!(opening(first) && opening(last), "{first:x}..{last:x}");
            assert!(!opening(first - 1) && !opening(last + 1), "{first:x}..{last:x}");
        }
    }

    #[test]
    fn a_bracket_or_a_quote_of_another_script_that_opens_is_one_and_what_closes_or_is_ascii_or_a_letter_is_not() {
        // the fullwidth bracket, the corner, lenticular and tortoise-shell brackets, a guillemet (Pi), the typographic quotes (Pi: the low-9 and the double high-reversed too)
        for c in ['\u{ff3b}', '\u{300c}', '\u{3010}', '\u{3014}', '\u{ff08}', '\u{ab}', '\u{2018}', '\u{201c}', '\u{201a}', '\u{201f}', '\u{2039}'] {
            assert!(opens(c), "U+{:04X}", c as u32);
        }
        for c in ['[', '(', '{', '<', '"', '\'', '-', '*', '1', 'a', ']', '\u{ff3d}', '\u{300d}', '\u{bb}', '\u{2019}', '\u{201d}', '\u{ff46}', '\u{43e}', '\u{3000}', '\u{e9}'] {
            assert!(!opens(c), "U+{:04X}", c as u32);
        }
    }
}
