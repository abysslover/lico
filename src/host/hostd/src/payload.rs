//! Deterministic synthetic record payloads: a pure function of the sequence number.

#[allow(dead_code)]
pub fn payload(seq: i64, size: usize) -> Vec<u8> {
    let mut state = (seq as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0xD1B5_4A32_D192_ED03);
    let mut out = Vec::with_capacity(size + 8);
    while out.len() < size {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    out.truncate(size);
    out
}

/// The words of the text workload. The standard one is mixed-script on purpose (a right-to-left word and a combining accent among them); the realistic one is
/// the same generator with those two words replaced by ordinary ones (an arrow and a check mark), so every line has the same shape and length.
const WORDS: [&str; 16] = [
    "the", "session", "reads", "파일을", "읽습니다", "안녕하세요", "漢字", "日本語", "שלום", "🙂", "e\u{301}", "fn main() {}", "let x = 42;", "--flag=value", "src/lib.rs:12", "완료",
];
const REALISTIC_WORDS: [&str; 16] = [
    "the", "session", "reads", "파일을", "읽습니다", "안녕하세요", "漢字", "日本語", "→", "🙂", "✓", "fn main() {}", "let x = 42;", "--flag=value", "src/lib.rs:12", "완료",
];

/// A line of text shaped like agent output: English, Korean, ideographs, Hebrew, an emoji, a
/// combining accent and some code, deterministic in the sequence number and about `size` bytes.
#[allow(dead_code)]
pub fn text_line(seq: i64, size: usize) -> String {
    line_of(&WORDS, seq, size)
}

/// The same lines with ordinary words where the standard ones have a right-to-left word and a combining accent: English, Korean, ideographs, symbols, emoji, code.
#[allow(dead_code)]
pub fn realistic_line(seq: i64, size: usize) -> String {
    line_of(&REALISTIC_WORDS, seq, size)
}

fn line_of(words: &[&str; 16], seq: i64, size: usize) -> String {
    let mut state = (seq as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0xD1B5_4A32_D192_ED03);
    let mut out = format!("{seq:06} ");
    while out.len() < size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        out.push_str(words[(state >> 60) as usize]);
        out.push(' ');
    }
    let mut end = out.len().min(size);
    while !out.is_char_boundary(end) {
        end -= 1;
    }
    out.truncate(end);
    out
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

#[allow(dead_code)]
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    bytes.chunks(2).map(|p| Some(nibble(p[0])? << 4 | nibble(p[1])?)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_text_workload_is_unchanged() {
        // the pixel test and every earlier measurement use these lines: they must stay what they were
        assert_eq!(text_line(0, 110), "000000 the reads session 읽습니다 파일을 🙂 🙂 src/lib.rs:12 漢字 the e\u{301} src/lib.rs:12 e\u{301} 안");
        assert_eq!(text_line(7, 110), "000007 fn main() {} session let x = 42; 🙂 the 漢字 let x = 42; --flag=value 漢字 let x = 42; --flag=val");
        assert_eq!(text_line(123456, 110), "123456 읽습니다 안녕하세요 안녕하세요 the session reads let x = 42; 🙂 안녕하세요 --flag=");
    }

    #[test]
    fn the_realistic_workload_is_the_same_lines_without_right_to_left_or_combining_characters() {
        let mut changed = 0;
        for seq in 0..500 {
            let (standard, realistic) = (text_line(seq, 110), realistic_line(seq, 110));
            assert!(!realistic.contains('ש') && !realistic.contains('\u{301}'), "{realistic:?}");
            let expected = standard.replace("שלום", "→").replace("e\u{301}", "✓");
            // the same words in the same places; the end of the line may be cut at another word because the replaced ones have other lengths
            assert_eq!(expected.chars().take(25).collect::<String>(), realistic.chars().take(25).collect::<String>());
            if standard != realistic {
                changed += 1;
            }
        }
        assert!(changed > 300, "the two words are on most lines of the standard workload ({changed} of 500 differ)");
    }
}
