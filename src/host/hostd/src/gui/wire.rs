//! The three small readers and writers of the window's line protocol to the host: a string escaped for a JSON line, and a number or a string
//! read from a reply line by its key. (Moved out of `mod.rs` unchanged to make room for `picture.rs`, M02-P04 S05b.)

pub(super) fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub(super) fn jint(s: &str, key: &str) -> Option<i64> {
    let pat = format!("\"{key}\":");
    let rest = &s[s.find(&pat)? + pat.len()..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

pub(super) fn jstr<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":\"");
    let rest = &s[s.find(&pat)? + pat.len()..];
    Some(&rest[..rest.find('"')?])
}
