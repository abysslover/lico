//! The lines a program writes, read as a stream with a bound on what is kept.
//!
//! A provider line is JSON and can be very long: a tool result that holds a screenshot is the same picture twice, a few hundred KiB each.
//! A line that fits the limit comes back byte for byte. When a string would take the line over the limit, that string is cut to its first
//! `HEAD` bytes and the number of bytes left out, written as `"<head>\u0001<omitted>"`: the line stays JSON, every other field is read as
//! before and the reader never holds more than the limit and one string in flight. A line that cannot be kept even so comes back as
//! `Over` with its size, so a caller can say that it was there. `plain` is the older reader for a program that writes plain text.

use crate::transfer::size_text;
use serde_json::Value;
use std::borrow::Cow;
use std::io::{BufRead, BufReader, Read};

/// Between what is kept of a cut string and the number of bytes that were left out.
pub const CUT: char = '\u{1}';
/// What stays of a cut string, and the length of the longest string that is never cut.
pub const HEAD: usize = 4096;

pub enum Next {
    /// A line, without its line end: `kept` is what is held (a cut string is a head and a mark), `bytes` the length it had.
    Whole { kept: Vec<u8>, bytes: usize },
    /// A line that could not be held, with its length.
    Over { bytes: usize },
    /// The stream ended inside a line: what was held of it (a cut string is closed, a line that is cut short stays cut short).
    Unended { kept: Vec<u8> },
    End,
}

pub enum Line {
    Text { text: String, bytes: usize },
    /// A line that was too long to hold even with its long strings cut, or is not UTF-8 text.
    Skipped { bytes: usize },
}

/// The record of a line that was not held: `[line 3.0 MiB omitted]`.
pub fn omitted_line(bytes: usize) -> String {
    format!("[line {} omitted]", size_text(bytes))
}

/// A cut string as the head that stayed and the number of bytes that were left out.
pub fn cut_info(text: &str) -> Option<(&str, usize)> {
    let at = text.rfind(CUT)?;
    let digits = &text[at + 1..];
    (!digits.is_empty() && digits.len() < 19 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| (&text[..at], digits.parse().unwrap_or(0)))
}

/// A string as the part to show and, when the reader cut it, the note that says how much was left out: `[296.0 KiB omitted]`, one wording for all strings.
pub fn split_cut(text: &str) -> (&str, Option<String>) {
    match cut_info(text) {
        Some((head, omitted)) => (head, Some(format!("[{} omitted]", size_text(omitted)))),
        None => (text, None),
    }
}

/// The record lines of `prefix` and `text`: one, and for a text the reader cut the note as a second, so that the clip of a record cannot take it.
pub fn with_note(prefix: &str, text: &str) -> Vec<String> {
    let (text, note) = split_cut(text);
    std::iter::once(format!("{prefix}{text}")).chain(note).collect()
}

/// A string to read line by line: a cut one is its head and then the note on a line of its own (a record line is clipped, so the note must not share the line of a long head).
pub fn shown(text: &str) -> Cow<'_, str> {
    match split_cut(text) {
        (head, Some(note)) => Cow::Owned(format!("{head}\n{note}")),
        (_, None) => Cow::Borrowed(text),
    }
}

/// Whether any string of `value`, a key too, was cut.
pub fn has_cut(value: &Value) -> bool {
    match value {
        Value::String(text) => cut_info(text).is_some(),
        Value::Array(items) => items.iter().any(has_cut),
        Value::Object(map) => map.iter().any(|(key, item)| cut_info(key).is_some() || has_cut(item)),
        _ => false,
    }
}

/// The size of the picture of an image block with base64 data, and whether its data was cut: the one rule for it, in the record of the host and in the record file.
pub fn image_size(block: &Value) -> Option<(usize, bool)> {
    let data = block.pointer("/source/data").and_then(Value::as_str)?;
    if block.pointer("/source/type").and_then(Value::as_str) != Some("base64") {
        return None;
    }
    let (chars, cut) = match cut_info(data) {
        Some((head, omitted)) => (head.len() + omitted, true),
        None => (data.len(), false),
    };
    Some((chars / 4 * 3, cut))
}

/// `[image 225.0 KiB]`, or `[image 225.0 KiB omitted]` when its data was cut, or `[image]` when the block says nothing about its size.
pub fn image_line(block: &Value) -> String {
    match image_size(block) {
        Some((bytes, true)) => format!("[image {} omitted]", size_text(bytes)),
        Some((bytes, false)) => format!("[image {}]", size_text(bytes)),
        None => "[image]".to_string(),
    }
}

struct Str {
    start: usize,        // where its opening quote is in what is kept
    raw: usize,          // bytes written so far, between the quotes
    escape: bool,        // the last byte was a backslash
    head: Option<usize>, // the bytes of it that stayed, once it is cut: later bytes are counted and not kept
}

/// The string that was finished and is the longest so far: the first to be cut when a later string does not help.
struct Big {
    start: usize,
    end: usize, // where its closing quote is in what is kept
}

/// How many bytes of the string `bytes` (the part after its opening quote) to keep as its head: whole characters and whole escapes, at most `max`,
/// and not ending after the first half of an escaped surrogate pair.
fn safe_head(bytes: &[u8], max: usize) -> usize {
    let (mut at, mut last, mut high) = (0, 0, false);
    while at < bytes.len() {
        let n = match bytes[at] {
            b'\\' if bytes.get(at + 1) == Some(&b'u') => 6,
            b'\\' => 2,
            b if b >= 0xF0 => 4,
            b if b >= 0xE0 => 3,
            b if b >= 0xC0 => 2,
            _ => 1,
        };
        if at + n > max.min(bytes.len()) {
            break;
        }
        high = n == 6 && std::str::from_utf8(&bytes[at + 2..at + 6]).ok().and_then(|h| u16::from_str_radix(h, 16).ok()).is_some_and(|c| (0xD800..0xDC00).contains(&c));
        last = at;
        at += n;
    }
    if high { last } else { at }
}

fn mark(omitted: usize) -> Vec<u8> {
    format!("\\u0001{omitted}").into_bytes()
}

/// Makes room in `kept` by cutting the string in flight, and then the longest finished one; true when it is within `limit`, not counting the `marks` of cut strings.
fn make_room(kept: &mut Vec<u8>, string: &mut Option<Str>, big: &mut Option<Big>, marks: &mut usize, limit: usize) -> bool {
    if let Some(s) = string.as_mut().filter(|s| s.head.is_none() && s.raw > HEAD) {
        let head = safe_head(&kept[s.start + 1..], HEAD);
        kept.truncate(s.start + 1 + head);
        s.head = Some(head);
    }
    if kept.len() > limit + *marks {
        if let Some(b) = big.take() {
            let head = safe_head(&kept[b.start + 1..b.end], HEAD);
            let (from, left_out) = (b.start + 1 + head, b.end - (b.start + 1 + head));
            let mark = mark(left_out);
            kept.splice(from..b.end, mark.iter().copied());
            *marks += mark.len();
            if let Some(s) = string.as_mut() {
                s.start = s.start + mark.len() - left_out; // the string in flight comes after it
            }
        }
    }
    kept.len() <= limit + *marks
}

/// A cut string that the line ended inside is closed, so that what was kept shows how long it was.
fn closed(mut kept: Vec<u8>, string: Option<Str>) -> Vec<u8> {
    if let Some(Str { head: Some(head), raw, .. }) = string {
        kept.extend(mark(raw - head));
        kept.push(b'"');
    }
    kept
}

/// The next line of `reader`, holding at most `limit` bytes of it (and one more string being read).
pub fn read_line(reader: &mut impl BufRead, limit: usize) -> std::io::Result<Next> {
    let mut kept: Vec<u8> = Vec::new();
    let (mut total, mut over, mut any) = (0usize, false, false);
    let mut string: Option<Str> = None;
    let mut big: Option<Big> = None;
    let mut marks = 0usize; // bytes of `kept` that are marks of cut strings: they are not held against the limit, and there are at most limit / HEAD of them
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(match (any, over) {
                (false, _) => Next::End,
                (true, true) => Next::Over { bytes: total },
                (true, false) => Next::Unended { kept: closed(kept, string) },
            });
        }
        any = true;
        let chunk_len = chunk.len();
        let (mut at, mut line_end) = (0, None);
        while at < chunk.len() {
            let rest = &chunk[at..];
            // a long run of bytes that are not kept is passed over at once
            let run = if over {
                Some(rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len()))
            } else {
                match &string {
                    Some(s) if s.head.is_some() && !s.escape => Some(rest.iter().position(|&b| b == b'"' || b == b'\\' || b == b'\n').unwrap_or(rest.len())),
                    _ => None,
                }
            };
            if let Some(n) = run.filter(|&n| n > 0) {
                total += n;
                if let Some(s) = string.as_mut().filter(|_| !over) {
                    s.raw += n;
                }
                at += n;
                continue;
            }
            let b = chunk[at];
            at += 1;
            if b == b'\n' {
                line_end = Some(at);
                break;
            }
            total += 1;
            if over {
                continue;
            }
            match string.as_mut() {
                None => {
                    kept.push(b);
                    if b == b'"' {
                        string = Some(Str { start: kept.len() - 1, raw: 0, escape: false, head: None });
                    }
                }
                Some(s) if s.escape => {
                    s.escape = false;
                    s.raw += 1;
                    if s.head.is_none() {
                        kept.push(b);
                    }
                }
                Some(s) if b == b'"' => {
                    match s.head {
                        Some(head) => {
                            let mark = mark(s.raw - head);
                            marks += mark.len();
                            kept.extend(mark);
                        }
                        None if s.raw > HEAD && big.as_ref().map_or(true, |g| g.end - g.start < s.raw + 1) => big = Some(Big { start: s.start, end: kept.len() }),
                        None => {}
                    }
                    kept.push(b);
                    string = None;
                }
                Some(s) => {
                    s.escape = b == b'\\';
                    s.raw += 1;
                    if s.head.is_none() {
                        kept.push(b);
                    }
                }
            }
            if kept.len() > limit + marks && !make_room(&mut kept, &mut string, &mut big, &mut marks, limit) {
                (over, kept, string, big) = (true, Vec::new(), None, None);
            }
        }
        reader.consume(line_end.unwrap_or(chunk_len));
        if line_end.is_some() {
            if over {
                return Ok(Next::Over { bytes: total });
            }
            return Ok(Next::Whole { kept: closed(kept, string), bytes: total });
        }
    }
}

pub fn lines<R: Read>(stream: R, limit: usize) -> impl Iterator<Item = Line> {
    let mut reader = BufReader::with_capacity(64 * 1024, stream);
    std::iter::from_fn(move || match read_line(&mut reader, limit) {
        Ok(Next::Whole { kept, bytes }) => Some(match String::from_utf8(kept) {
            Ok(text) => Line::Text { text, bytes },
            Err(_) => Line::Skipped { bytes },
        }),
        Ok(Next::Over { bytes }) => Some(Line::Skipped { bytes }),
        _ => None, // the stream ended; a last line with no line end is not a line
    })
}

/// Lines of a program's output with a length limit: a longer line is dropped whole, so a program that never ends a line cannot grow the host.
pub fn plain(stream: std::process::ChildStdout, limit: usize) -> impl Iterator<Item = String> {
    let mut reader = BufReader::new(stream);
    std::iter::from_fn(move || loop {
        let mut buf = Vec::new();
        let n = reader.by_ref().take(limit as u64).read_until(b'\n', &mut buf).ok()?;
        if n == 0 {
            return None;
        }
        if buf.last() != Some(&b'\n') {
            // too long without a newline: skip to the end of this line
            let mut skip = Vec::new();
            loop {
                skip.clear();
                match reader.by_ref().take(limit as u64).read_until(b'\n', &mut skip) {
                    Ok(0) | Err(_) => return None,
                    Ok(_) if skip.last() == Some(&b'\n') => break,
                    Ok(_) => continue,
                }
            }
            continue;
        }
        buf.pop();
        if let Ok(text) = String::from_utf8(buf) {
            return Some(text);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LIMIT: usize = 256 * 1024;

    fn read(data: &[u8], limit: usize) -> Vec<Line> {
        lines(data, limit).collect()
    }

    fn text(line: &Line) -> (&str, usize) {
        match line {
            Line::Text { text, bytes } => (text, *bytes),
            Line::Skipped { bytes } => panic!("skipped, {bytes} bytes"),
        }
    }

    /// The shape of the real program's line for an image that a tool returned (read from its output, 2.1.310): the data twice, `media_type` after the data.
    fn image_message(chars: usize) -> String {
        let data = "A".repeat(chars);
        json!({"type": "user", "message": {"role": "user", "content": [{"tool_use_id": "t1", "type": "tool_result", "content": [{"type": "image", "source": {"type": "base64", "data": data, "media_type": "image/png"}}]}]},
               "uuid": "u1", "tool_use_result": {"type": "image", "file": {"base64": data, "type": "image/png", "originalSize": chars / 4 * 3}}, "last": "kept"})
        .to_string()
    }

    #[test]
    fn a_line_at_or_under_the_limit_comes_byte_for_byte_even_with_long_strings_and_escapes() {
        let long = json!({"text": "a\"b\\c\u{e9}\n".repeat(20_000)}).to_string(); // about 240 KiB
        assert!(long.len() > 200_000 && long.len() < LIMIT);
        let exact = format!("[\"{}\"]", "z".repeat(LIMIT - 4));
        assert_eq!(exact.len(), LIMIT);
        let got = read(format!("{long}\n{exact}\n{{}}\n").as_bytes(), LIMIT);
        assert_eq!(got.len(), 3);
        assert_eq!((text(&got[0]), text(&got[1]), text(&got[2])), ((long.as_str(), long.len()), (exact.as_str(), LIMIT), ("{}", 2)));
    }

    #[test]
    fn a_string_that_takes_the_line_over_the_limit_keeps_its_first_4096_bytes_and_says_how_many_were_left_out() {
        let line = image_message(300 * 1024); // two strings of 300 KiB: 600 KiB
        let got = read(format!("{line}\n{{\"after\":1}}\n").as_bytes(), LIMIT);
        assert_eq!(got.len(), 2);
        let (kept, bytes) = text(&got[0]);
        assert_eq!(bytes, line.len());
        assert!(kept.len() < 2 * HEAD + 1024, "{} bytes kept", kept.len());
        let value: Value = serde_json::from_str(kept).unwrap();
        let block = &value["message"]["content"][0]["content"][0];
        assert_eq!((block["type"].as_str(), block["source"]["media_type"].as_str()), (Some("image"), Some("image/png")));
        let data = block["source"]["data"].as_str().unwrap();
        assert_eq!(cut_info(data), Some(("A".repeat(HEAD).as_str(), 300 * 1024 - HEAD)));
        assert_eq!(cut_info(value["tool_use_result"]["file"]["base64"].as_str().unwrap()).map(|(_, n)| n), Some(300 * 1024 - HEAD), "the second copy too");
        assert_eq!((value["uuid"].as_str(), value["last"].as_str(), value["tool_use_result"]["file"]["type"].as_str()), (Some("u1"), Some("kept"), Some("image/png")));
        assert!(has_cut(&value) && !has_cut(&json!({"a": ["b", {"c": "plain"}]})));
        assert_eq!(image_line(block), "[image 225.0 KiB omitted]", "300 KiB of base64 is a picture of 225 KiB");
        assert_eq!(text(&got[1]), ("{\"after\":1}", 11));
    }

    #[test]
    fn a_long_text_over_the_limit_reads_as_its_head_and_the_size_left_out_in_one_wording() {
        let line = json!({"text": "t".repeat(300 * 1024), "n": 5}).to_string();
        let got = read(format!("{line}\n").as_bytes(), LIMIT);
        let value: Value = serde_json::from_str(text(&got[0]).0).unwrap();
        let shown = shown(value["text"].as_str().unwrap()).into_owned();
        assert_eq!((shown.len(), value["n"].as_i64()), (HEAD + "\n[296.0 KiB omitted]".len(), Some(5)));
        assert!(shown.starts_with(&"t".repeat(HEAD)) && shown.ends_with("\n[296.0 KiB omitted]"), "{shown:?}");
        assert_eq!(split_cut(value["text"].as_str().unwrap()).1.as_deref(), Some("[296.0 KiB omitted]"));
        assert_eq!(&*super::shown("plain"), "plain");
    }

    /// The line `{"a":"aaa"   }` one byte over `limit`, with its string `n` bytes long: the blanks behind it are what goes over the limit.
    fn finished_string_then_blanks(limit: usize, n: usize) -> String {
        let head = format!("{{\"a\":\"{}\"", "a".repeat(n));
        format!("{head}{}}}", " ".repeat(limit - head.len()))
    }

    /// The line `{    "a":"aaa...` that goes over `limit` while its string is `n` bytes long, and goes on 10,000 bytes more with no end to the string.
    fn string_in_flight_when_over(limit: usize, n: usize) -> String {
        format!("{{{}\"a\":\"{}", " ".repeat(limit - n - 5), "a".repeat(n + 10_000))
    }

    #[test]
    fn a_string_of_4096_bytes_is_never_cut_and_one_of_4097_is_when_it_is_in_flight() {
        let limit = 20_000;
        for (n, cut) in [(HEAD, false), (HEAD + 1, true)] {
            let line = string_in_flight_when_over(limit, n);
            let got = read(format!("{line}\n").as_bytes(), limit);
            match (&got[0], cut) {
                (Line::Text { text, bytes }, true) => {
                    let at = text.rfind("\\u0001").unwrap();
                    assert_eq!((*bytes, text.len() - at), (line.len(), "\\u0001".len() + (n + 10_000 - HEAD).to_string().len() + 1));
                    assert_eq!(text[at + 6..text.len() - 1].parse::<usize>().unwrap(), n + 10_000 - HEAD);
                    assert_eq!(text[..at].rsplit('"').next().unwrap().len(), HEAD);
                }
                (Line::Skipped { bytes }, false) => assert_eq!(*bytes, line.len()),
                (other, _) => panic!("{n} bytes in flight: {}", match other { Line::Text { text, bytes } => format!("text {} of {bytes}", text.len()), Line::Skipped { bytes } => format!("skipped {bytes}") }),
            }
        }
    }

    #[test]
    fn a_string_of_4096_bytes_is_never_cut_and_one_of_4097_is_when_it_is_finished() {
        let limit = 20_000;
        for (n, cut) in [(HEAD, false), (HEAD + 1, true)] {
            let line = finished_string_then_blanks(limit, n);
            assert_eq!(line.len(), limit + 1);
            let got = read(format!("{line}\n").as_bytes(), limit);
            match (&got[0], cut) {
                (Line::Text { text, bytes }, true) => {
                    let value: Value = serde_json::from_str(text).unwrap();
                    assert_eq!((*bytes, cut_info(value["a"].as_str().unwrap()).map(|(h, left_out)| (h.len(), left_out))), (line.len(), Some((HEAD, 1))));
                }
                (Line::Skipped { bytes }, false) => assert_eq!(*bytes, line.len()),
                (other, _) => panic!("{n} bytes finished: {}", match other { Line::Text { text, bytes } => format!("text {} of {bytes}", text.len()), Line::Skipped { bytes } => format!("skipped {bytes}") }),
            }
        }
    }

    #[test]
    fn the_cut_never_ends_inside_an_escape_a_character_or_a_surrogate_pair() {
        for pad in 0..16 {
            for piece in ["\\u00e9", "\\\"", "\\\\", "\\ud83d\\ude00", "\u{e9}", "\u{20ac}", "\u{1f600}"] {
                let s = format!("{}{piece}{}", "a".repeat(HEAD - 14 + pad), "b".repeat(5000));
                let line = format!("{{\"s\":\"{s}\",\"n\":7}}");
                let got = read(format!("{line}\n").as_bytes(), 4500);
                let value: Value = serde_json::from_str(text(&got[0]).0).unwrap_or_else(|e| panic!("pad {pad} piece {piece}: {e}"));
                let (head, left_out) = cut_info(value["s"].as_str().unwrap()).unwrap();
                assert!(head.len() <= HEAD && head.chars().all(|c| c != '\u{fffd}'), "pad {pad} piece {piece}");
                let whole: Value = serde_json::from_str(&format!("\"{s}\"")).unwrap();
                assert!(whole.as_str().unwrap().starts_with(head), "pad {pad} piece {piece}");
                // the head is the written bytes up to the last whole token within 4096 (a token is at most 12 bytes, and a pair of surrogates is cut before its first half), the rest is counted as written
                assert!((s.len() - HEAD..=s.len() - HEAD + 14).contains(&left_out), "pad {pad} piece {piece}: {left_out} left out of {}", s.len());
                assert_eq!(value["n"].as_i64(), Some(7));
            }
        }
    }

    #[test]
    fn a_finished_string_is_cut_when_the_line_goes_over_the_limit_after_it() {
        let line = format!("{{\"a\":\"{}\",\"b\":\"short\",\"c\":[1,2,3,4,5,6]}}", "q".repeat(LIMIT - 40)); // just under, then 20 bytes more
        let line = format!("{line}{}", " ".repeat(40)); // the last bytes are no string at all
        assert!(line.len() > LIMIT);
        let got = read(format!("{line}\n").as_bytes(), LIMIT);
        let value: Value = serde_json::from_str(text(&got[0]).0).unwrap();
        assert_eq!((cut_info(value["a"].as_str().unwrap()).map(|(h, _)| h.len()), value["b"].as_str(), value["c"][5].as_i64()), (Some(HEAD), Some("short"), Some(6)));
    }

    #[test]
    fn a_cut_string_that_the_line_ends_inside_is_closed_and_a_line_that_is_still_too_long_is_skipped_with_its_size() {
        let unended = format!("{{\"s\":\"{}", "y".repeat(300_000));
        let junk = "x".repeat(3_000_000);
        let many: String = (0..10_000).map(|n| format!("\"{n}{}\",", "m".repeat(5000))).collect(); // 10,000 strings of 5 KiB: every cut still leaves too much
        let nested = "[".repeat(100_000);
        let data = format!("{unended}\n{junk}\n[{many}]\n{nested}\nlast\n");
        let got = read(data.as_bytes(), LIMIT);
        assert_eq!(got.len(), 5);
        let (kept, bytes) = text(&got[0]);
        assert!(bytes == unended.len() && kept.ends_with("\\u0001295904\""), "{}", &kept[kept.len() - 20..]);
        assert!(serde_json::from_str::<Value>(kept).is_err());
        assert!(matches!(got[1], Line::Skipped { bytes } if bytes == junk.len()));
        assert!(matches!(got[2], Line::Skipped { bytes } if bytes == many.len() + 2));
        assert_eq!(text(&got[3]).1, nested.len(), "100,000 opening brackets are a line of 100 KiB; the parser refuses its depth and does not panic");
        assert!(serde_json::from_str::<Value>(text(&got[3]).0).is_err());
        assert_eq!(text(&got[4]), ("last", 4));
    }

    #[test]
    fn a_line_that_is_not_text_an_empty_line_and_a_last_line_with_no_line_end() {
        let in_a_string = [b"{\"s\":\"".as_slice(), &[0xff, 0xfe], b"\"}\n"].concat();
        let got = read(&[b"one\n".as_slice(), &in_a_string, b"\n", b"two"].concat(), LIMIT);
        assert_eq!(got.len(), 3);
        assert_eq!(text(&got[0]).0, "one");
        assert!(matches!(got[1], Line::Skipped { bytes: 10 }));
        assert_eq!(text(&got[2]), ("", 0), "an empty line is a line");
        // a long line with no line end before the stream ends is reported, a short one is not a line
        let long = "x".repeat(LIMIT + 10);
        assert!(matches!(read(long.as_bytes(), LIMIT)[0], Line::Skipped { bytes } if bytes == LIMIT + 10));
        assert!(read(b"short", LIMIT).is_empty());
        let mut reader = std::io::Cursor::new(b"short".to_vec());
        assert!(matches!(read_line(&mut reader, LIMIT).unwrap(), Next::Unended { kept } if kept == b"short") && matches!(read_line(&mut reader, LIMIT).unwrap(), Next::End));
    }

    #[test]
    fn a_string_of_exactly_4096_bytes_is_not_marked_even_when_another_string_of_the_line_is_cut_to_make_room() {
        let (a, b) = ("a".repeat(5000), "b".repeat(HEAD));
        let limit = format!("{{\"a\":\"{a}\",\"b\":\"{b}").len() - 1; // the line goes over the limit at the last byte of b
        let got = read(format!("{{\"a\":\"{a}\",\"b\":\"{b}\"}}\n").as_bytes(), limit);
        let value: Value = serde_json::from_str(text(&got[0]).0).unwrap();
        assert_eq!(cut_info(value["a"].as_str().unwrap()).map(|(h, n)| (h.len(), n)), Some((HEAD, 5000 - HEAD)));
        assert_eq!(value["b"].as_str(), Some(b.as_str()));
    }

    #[test]
    fn the_size_of_a_picture_is_the_length_of_its_data_over_four_times_three_cut_or_not() {
        let block = |data: String| json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": data}});
        assert_eq!(image_line(&block("A".repeat(6))), "[image 3 B]");
        assert_eq!(image_line(&block("A".repeat(1366))), "[image 1023 B]", "1366 / 4 * 3, not 1366 * 3 / 4");
        assert_eq!(image_line(&block(format!("{}{CUT}2", "A".repeat(HEAD)))), "[image 3.0 KiB omitted]", "the head and what was left out: 4098 characters");
        assert_eq!(image_line(&json!({"type": "image", "source": {"type": "url", "url": "x"}})), "[image]");
        assert_eq!(image_line(&json!({"type": "image", "source": {"type": "base64"}})), "[image]");
    }

    #[test]
    fn bytes_that_are_not_utf8_in_a_long_string_go_with_the_part_that_is_cut_away_or_make_a_skipped_line() {
        let away = [b"{\"s\":\"".as_slice(), &vec![b'a'; 300_000], &[0xff, 0xfe, 0xc3], &vec![b'b'; 10], b"\",\"n\":1}\n"].concat();
        let kept = [b"{\"s\":\"".as_slice(), &[0xff], &vec![b'a'; 300_000], b"\"}\n"].concat();
        let got = read(&[away.as_slice(), kept.as_slice(), b"{}\n"].concat(), LIMIT);
        assert_eq!(got.len(), 3);
        let value: Value = serde_json::from_str(text(&got[0]).0).unwrap();
        assert_eq!((cut_info(value["s"].as_str().unwrap()).map(|(h, n)| (h.len(), n)), value["n"].as_i64()), (Some((HEAD, 300_013 - HEAD)), Some(1)));
        assert!(matches!(got[1], Line::Skipped { bytes } if bytes == kept.len() - 1), "a head that is not text is a line with its size, not nothing");
        assert_eq!(text(&got[2]).0, "{}");
    }

    struct Repeat {
        left: u64,
        tail: Vec<u8>,
    }

    impl Read for Repeat {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.left == 0 {
                let n = self.tail.len().min(buf.len());
                buf[..n].copy_from_slice(&self.tail[..n]);
                self.tail.drain(..n);
                return Ok(n);
            }
            let n = (self.left as usize).min(buf.len());
            buf[..n].fill(b'A');
            self.left -= n as u64;
            Ok(n)
        }
    }

    #[test]
    fn a_line_of_64_mib_that_is_one_string_is_read_with_what_is_held_bounded_by_the_limit() {
        let size = 64 * 1024 * 1024u64;
        let source = std::io::Cursor::new(b"{\"data\":\"".to_vec()).chain(Repeat { left: size, tail: b"\",\"n\":2}\n{\"next\":true}\n".to_vec() });
        let got: Vec<Line> = lines(source, LIMIT).collect();
        assert_eq!(got.len(), 2);
        let Line::Text { text: kept, bytes } = &got[0] else { panic!("skipped") };
        assert!(*bytes as u64 > size && kept.len() < 2 * HEAD && kept.capacity() <= 2 * LIMIT, "{bytes} read, {} kept, capacity {}", kept.len(), kept.capacity());
        let value: Value = serde_json::from_str(kept).unwrap();
        assert_eq!((cut_info(value["data"].as_str().unwrap()).map(|(_, n)| n as u64), value["n"].as_i64()), (Some(size - HEAD as u64), Some(2)));
        assert_eq!(text(&got[1]).0, "{\"next\":true}");
    }
}
