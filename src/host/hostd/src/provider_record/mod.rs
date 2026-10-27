//! The whole conversation of a Claude Code session, read from the program's own record (S05).
//!
//! The program writes one JSON object per line to `<its home>/projects/<project folder>/<its session id>.jsonl`. The host knows the home it gave the session and the id the program
//! told it, so the file is found by `projects/*/<id>.jsonl` and the project folder's name is never rebuilt. Nothing else under the home is opened (the sign-in material lives there too).
//! The home is the one the host gave the program; below it no link is followed, and more than one file of the id is refused, not chosen between.
//!
//! The record is read as a stream, one line in memory (a line over `MAX_LINE` is read past and counted), and turned into a text for a receiver: who said what and when, every tool call and
//! result whole, an image by its size. What is left out is said in the head, by name and count, so that a shortened record never looks whole. The head and the lines at the left edge
//! are the host's; every word of the conversation is cleaned like an agent's text (`leads::agent_text`) and indented, so nothing in it can pass for a line of the host.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::leads::agent_text;
use crate::transfer::size_text;

/// The program versions whose record layout this reader knows: each one's record was compared with the reader's keys (`version.cli` of `capabilities.json`, the one the adapter was measured on, is among them).
/// A message of any other version refuses the whole read.
pub const READ_VERSIONS: [&str; 2] = ["2.1.310", "2.1.311"];

/// `a`, `a and b`, `a, b and c`.
fn and_words<S: AsRef<str>>(list: &[S]) -> String {
    match list {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [init @ .., last] => format!("{} and {}", init.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", "), last.as_ref()),
    }
}

/// The versions the reader knows, for the words of a refusal.
pub fn versions_text() -> String {
    and_words(&READ_VERSIONS)
}
/// A line longer than this is not held: it is read past and counted.
pub const MAX_LINE: usize = 4 << 20;
/// A record file longer than this is not read.
pub const MAX_RECORD: u64 = 1 << 30;
/// Project folders looked at for the id; a home with more is not a home of one person's work.
pub const MAX_PROJECTS: usize = 20_000;
const MAX_NAMES: usize = 64; // distinct names counted in "Not shown", `other` among them
const INDENT: &str = "  ";

// `open` flags of Linux; the value of O_NOFOLLOW differs between the two architectures this program is built for.
#[cfg(target_arch = "x86_64")]
const O_NOFOLLOW: i32 = 0o400000;
#[cfg(target_arch = "aarch64")]
const O_NOFOLLOW: i32 = 0o100000;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("the value of O_NOFOLLOW is not known for this architecture");
const O_NONBLOCK: i32 = 0o4000; // a pipe that was put in place of the file is not waited for

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Fault {
    NotFound,
    Ambiguous,
    Format,
    TooLarge,
    Unreadable,
}

impl Fault {
    /// The error name the host answers with.
    pub fn code(self) -> &'static str {
        match self {
            Fault::NotFound => "record_not_found",
            Fault::Ambiguous => "record_ambiguous",
            Fault::Format => "record_format",
            Fault::TooLarge => "too_large",
            Fault::Unreadable => "busy",
        }
    }
}

/// The home the host gives a Claude program for a session: the private one of the account the session is bound to, else the host's own (`CLAUDE_CONFIG_DIR`, else `$HOME/.claude`),
/// as `claude.rs` starts the program. None when the account is gone or the host has no absolute home to name.
pub fn home_of(shared: &crate::core::Shared, account: Option<&str>) -> Option<PathBuf> {
    match account {
        Some(account) => absolute(shared.accounts.lock().unwrap().homes(account)?.provider_home),
        None => host_home(std::env::var_os("CLAUDE_CONFIG_DIR"), std::env::var_os("HOME")),
    }
}

/// The host's own home for the program, given the two variables it is made from.
fn host_home(config_dir: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> Option<PathBuf> {
    absolute(config_dir.map(PathBuf::from).or_else(|| home.map(|home| PathBuf::from(home).join(".claude")))?)
}

fn absolute(home: PathBuf) -> Option<PathBuf> {
    home.is_absolute().then_some(home) // a relative home would be read from wherever the host happens to stand
}

/// The id the program gave its session: letters, digits, dash and underscore, 64 at most, no leading dash (the same rule as the host applies when it keeps it).
pub fn valid_provider_session(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && !id.starts_with('-') && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_dir())
}

/// The record file of the session `id` under `home`: exactly one match, a regular file, no link at any hop below the home.
pub fn locate(home: &Path, id: &str) -> Result<std::fs::File, Fault> {
    if !valid_provider_session(id) {
        return Err(Fault::NotFound);
    }
    let projects = home.join("projects");
    if !real_dir(&projects) {
        return Err(Fault::NotFound);
    }
    let name = format!("{id}.jsonl");
    let mut matches: Vec<PathBuf> = Vec::new();
    for (looked, entry) in std::fs::read_dir(&projects).map_err(|_| Fault::NotFound)?.enumerate() {
        if looked >= MAX_PROJECTS {
            return Err(Fault::NotFound);
        }
        let Ok(entry) = entry else { continue };
        if !real_dir(&entry.path()) {
            continue; // a link or a file among the project folders is not one
        }
        let candidate = entry.path().join(&name);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            matches.push(candidate);
        }
    }
    let [path] = matches.as_slice() else { return Err(if matches.is_empty() { Fault::NotFound } else { Fault::Ambiguous }) };
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file()) {
        return Err(Fault::NotFound);
    }
    let file = std::fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_NONBLOCK).open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Fault::NotFound,
        _ if e.raw_os_error() == Some(40) => Fault::NotFound, // ELOOP: a link took the place of the file between the two looks
        _ => Fault::Unreadable,
    })?;
    match file.metadata() {
        Ok(meta) if meta.file_type().is_file() => Ok(file), // the handle that was opened, not the name it was opened by
        Ok(_) => Err(Fault::NotFound),
        Err(_) => Err(Fault::Unreadable),
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Line {
    Whole,
    Unended,
    Long,
    End,
}

/// The next line into `buf` (without its line end); a line over `MAX_LINE` is read past and not kept (`buf` is empty then). `Unended` is a last line with no line end.
pub fn next_line(reader: &mut impl BufRead, buf: &mut Vec<u8>) -> std::io::Result<Line> {
    buf.clear();
    let (mut any, mut long) = (false, false);
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return Ok(match (any, long) {
                (false, _) => Line::End,
                (true, true) => Line::Long,
                (true, false) => Line::Unended,
            });
        }
        any = true;
        let end = chunk.iter().position(|&b| b == b'\n');
        let part = &chunk[..end.unwrap_or(chunk.len())];
        if !long {
            if buf.len() + part.len() > MAX_LINE {
                long = true;
                *buf = Vec::new(); // what was held is let go, not kept
            } else {
                buf.extend_from_slice(part);
            }
        }
        let used = end.map_or(chunk.len(), |at| at + 1);
        reader.consume(used);
        if end.is_some() {
            return Ok(if long { Line::Long } else { Line::Whole });
        }
    }
}

// ------------------------------------------------------------------------------ what was seen
#[derive(Default)]
struct Tally {
    user: u64,
    assistant: u64,
    results: u64,
    summaries: u64,
    compactions: u64,
    first: Option<String>,
    last: Option<String>,
    hidden: usize,
    unparsed: u64,
    long: u64,
    cut: bool,
    versions: Vec<String>, // those of the messages read, in the order they first appear
    left_out: BTreeMap<String, u64>,
}

impl Tally {
    fn saw(&mut self, version: &str) {
        if !self.versions.iter().any(|seen| seen == version) {
            self.versions.push(version.to_string());
        }
    }

    /// Count something that is not shown, under a name that is plain (`other` when it is not, or when there are too many).
    fn skip(&mut self, name: &str, prefix: &str) {
        let plain = !name.is_empty() && name.len() <= 40 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        let key = if plain { format!("{prefix}{name}") } else { "other".to_string() };
        let key = if self.left_out.contains_key(&key) || self.left_out.len() + 1 < MAX_NAMES { key } else { "other".to_string() }; // one place is kept for `other`
        *self.left_out.entry(key).or_insert(0) += 1;
    }
}

struct Out {
    text: String,
    limit: usize,
}

impl Out {
    fn push(&mut self, piece: &str) -> Result<(), Fault> {
        if self.text.len() + piece.len() > self.limit {
            return Err(Fault::TooLarge);
        }
        self.text.push_str(piece);
        Ok(())
    }
}

/// A text of the conversation, cleaned and indented; the count of what the cleaning took goes to `tally`. Line ends at its end are not kept.
fn lines_of(raw: &str, depth: usize, tally: &mut Tally) -> String {
    let cleaned = agent_text(raw);
    tally.hidden += cleaned.removed;
    let pad = INDENT.repeat(depth);
    let text = cleaned.text.trim_end_matches('\n');
    if text.is_empty() {
        return String::new();
    }
    text.split('\n').map(|line| if line.is_empty() { "\n".to_string() } else { format!("{pad}{line}\n") }).collect()
}

/// `2026-10-27T04:12:03.123Z` as `2026-10-27 04:12:03 UTC`; anything else is no time.
fn stamp(record: &Value) -> Option<String> {
    let text = record.get("timestamp")?.as_str()?;
    let b = text.as_bytes();
    let digits = |from: usize, to: usize| b.get(from..to).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let shaped = b.len() >= 19 && digits(0, 4) && b[4] == b'-' && digits(5, 7) && b[7] == b'-' && digits(8, 10) && b[10] == b'T' && digits(11, 13) && b[13] == b':' && digits(14, 16) && b[16] == b':' && digits(17, 19);
    shaped.then(|| format!("{} {} UTC", &text[..10], &text[11..19]))
}

fn image_line(block: &Value) -> String {
    let data = block.pointer("/source/data").and_then(Value::as_str);
    match (block.pointer("/source/type").and_then(Value::as_str), data) {
        (Some("base64"), Some(data)) => format!("[image {}]", size_text(data.len() / 4 * 3)),
        _ => "[image]".to_string(),
    }
}

/// The name of a tool: plain characters only, so that it cannot end its bracket or start another line.
fn tool_name(raw: Option<&str>) -> String {
    let cleaned = raw.map(|n| agent_text(n).text.chars().take(64).map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') { c } else { '_' }).collect::<String>());
    cleaned.filter(|n| !n.is_empty()).unwrap_or_else(|| "?".to_string())
}

/// The fields of a tool call: one per line, a value that is a text as it is, any other as compact JSON; the lines of a value after its first are indented one step more.
fn tool_input(input: &Value, tally: &mut Tally) -> String {
    let Some(fields) = input.as_object() else { return lines_of(&input.to_string(), 2, tally) };
    let mut out = String::new();
    for (key, value) in fields {
        let key = agent_text(key); // the program's word for a field is cleaned like the rest
        let value = agent_text(&value.as_str().map_or_else(|| value.to_string(), str::to_string));
        tally.hidden += key.removed + value.removed;
        let mut pieces = value.text.trim_end_matches('\n').split('\n');
        out += &format!("{INDENT}{INDENT}{}: {}\n", key.text.replace('\n', " "), pieces.next().unwrap_or(""));
        for piece in pieces {
            out += &if piece.is_empty() { "\n".to_string() } else { format!("{INDENT}{INDENT}{INDENT}{piece}\n") };
        }
    }
    out
}

fn result_text(content: &Value, tally: &mut Tally) -> String {
    match content {
        Value::String(text) => lines_of(text, 2, tally),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b.get("type").and_then(Value::as_str) {
                Some("text") => lines_of(b.get("text").and_then(Value::as_str).unwrap_or(""), 2, tally),
                Some("image") => format!("{INDENT}{INDENT}{}\n", image_line(b)),
                other => {
                    tally.skip(other.unwrap_or(""), "block ");
                    String::new()
                }
            })
            .collect(),
        _ => String::new(),
    }
}

/// One message of the record: its header and the blocks of its content. Messages of another agent's thread and ones the program made for itself are counted, not shown.
fn message(record: &Value, kind: &str, out: &mut Out, tally: &mut Tally) -> Result<(), Fault> {
    if record.get("isSidechain") == Some(&Value::Bool(true)) {
        tally.skip("sidechain", "");
        return Ok(());
    }
    if record.get("isMeta") == Some(&Value::Bool(true)) {
        tally.skip("meta", "");
        return Ok(());
    }
    let content = record.pointer("/message/content");
    let mut text = String::new();
    let (mut results, mut others, mut counted) = (0, 0, 0);
    match content {
        Some(Value::String(plain)) => {
            others += 1;
            text += &lines_of(plain, 1, tally);
        }
        Some(Value::Array(blocks)) => {
            for block in blocks {
                let found = block.get("type").and_then(Value::as_str);
                if found == Some("tool_result") {
                    results += 1;
                } else {
                    others += 1;
                }
                match found {
                    Some("text") => text += &lines_of(block.get("text").and_then(Value::as_str).unwrap_or(""), 1, tally),
                    Some("image") => text += &format!("{INDENT}{}\n", image_line(block)),
                    Some("tool_use") => {
                        text += &format!("{INDENT}[tool_use {}]\n", tool_name(block.get("name").and_then(Value::as_str)));
                        if let Some(input) = block.get("input") {
                            text += &tool_input(input, tally);
                        }
                    }
                    Some("tool_result") => {
                        text += &format!("{INDENT}[tool_result{}]\n", if block.get("is_error") == Some(&Value::Bool(true)) { " error" } else { "" });
                        text += &result_text(block.get("content").unwrap_or(&Value::Null), tally);
                    }
                    other => {
                        tally.skip(other.unwrap_or(""), "block ");
                        counted += 1;
                    }
                }
            }
        }
        _ => {}
    }
    if text.trim().is_empty() {
        // nothing of it is shown: no heading, and it is not one of the messages shown; what it held is counted in "Not shown" (its blocks, or `message empty` when it held none)
        if counted == 0 {
            tally.skip("empty", "message ");
        }
        return Ok(());
    }
    let role = if kind == "assistant" {
        tally.assistant += 1;
        "assistant"
    } else if record.get("isCompactSummary") == Some(&Value::Bool(true)) {
        tally.summaries += 1;
        "compaction summary"
    } else if results > 0 && others == 0 {
        tally.results += 1;
        "tool results"
    } else {
        tally.user += 1;
        "user"
    };
    let at = stamp(record);
    if let Some(at) = &at {
        tally.first.get_or_insert_with(|| at.clone());
        tally.last = Some(at.clone());
    }
    out.push(&format!("## {role} | {}\n{text}\n", at.as_deref().unwrap_or("no time")))
}

fn record(value: &Value, out: &mut Out, tally: &mut Tally) -> Result<(), Fault> {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(kind, "user" | "assistant") {
        match value.get("version").and_then(Value::as_str) {
            Some(version) if READ_VERSIONS.contains(&version) => tally.saw(version),
            _ => return Err(Fault::Format),
        }
    }
    match kind {
        "user" | "assistant" => message(value, kind, out, tally)?,
        "system" if value.get("subtype").and_then(Value::as_str) == Some("compact_boundary") => {
            tally.compactions += 1;
            out.push("[the program compacted the conversation here; what came before is replaced by the summary that follows]\n")?;
        }
        other => tally.skip(other, ""),
    }
    Ok(())
}

fn times(n: u64) -> String {
    if n == 1 {
        "1 time".to_string()
    } else {
        format!("{n} times")
    }
}

fn lines_word(n: u64, rest: &str) -> String {
    format!("{n} line{} {rest}", if n == 1 { "" } else { "s" })
}

fn head(tally: &Tally) -> String {
    let shown = tally.user + tally.assistant + tally.results + tally.summaries;
    let program = if tally.versions.is_empty() { "Claude Code; the record holds no message that names its version".to_string() } else { format!("Claude Code {}", and_words(&tally.versions)) };
    let mut out = format!("Conversation record, read by Lico from the program's own record of the session ({program}).\n");
    out += &if shown == 0 {
        "Messages shown: 0.\n".to_string()
    } else {
        let summaries = if tally.summaries > 0 { format!(", {} compaction summar{}", tally.summaries, if tally.summaries == 1 { "y" } else { "ies" }) } else { String::new() };
        let range = match (&tally.first, &tally.last) {
            (Some(first), Some(last)) => format!(", from {first} to {last}"),
            _ => String::new(),
        };
        format!("Messages shown: {shown} ({} user, {} assistant, {} tool results{summaries}){range}.\n", tally.user, tally.assistant, tally.results)
    };
    out += &if tally.compactions == 0 {
        "Compaction: none found in the record.\n".to_string()
    } else {
        format!("Compaction: the program compacted this conversation {}; what stood before each point was replaced by its summary, so this is not everything that was said.\n", times(tally.compactions))
    };
    if !tally.left_out.is_empty() {
        out += &format!("Not shown: {}.\n", tally.left_out.iter().map(|(name, n)| format!("{name} {n}")).collect::<Vec<_>>().join(", "));
    }
    let mut skipped = Vec::new();
    if tally.unparsed > 0 {
        skipped.push(lines_word(tally.unparsed, "that did not parse"));
    }
    if tally.long > 0 {
        skipped.push(lines_word(tally.long, &format!("over {}", size_text(MAX_LINE))));
    }
    if tally.cut {
        skipped.push("the last line was cut off".to_string());
    }
    if !skipped.is_empty() {
        out += &format!("Skipped: {}.\n", skipped.join("; "));
    }
    if tally.hidden > 0 {
        out += &format!("Hidden characters removed from the text: {}.\n", tally.hidden);
    }
    out + "Lines at the left edge were written by Lico; the text of the conversation is indented by two spaces.\n\n"
}

/// The text of a record read from `reader`; `limit` is the most it may come to (the text is refused whole when it would come to more).
pub fn render(reader: impl Read, limit: usize) -> Result<String, Fault> {
    let mut reader = BufReader::with_capacity(64 << 10, reader);
    let (mut line, mut tally, mut out) = (Vec::new(), Tally::default(), Out { text: String::new(), limit });
    loop {
        let kind = next_line(&mut reader, &mut line).map_err(|_| Fault::Unreadable)?;
        match kind {
            Line::End => break,
            Line::Long => {
                tally.long += 1;
                continue;
            }
            Line::Whole | Line::Unended => {}
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue; // an empty line is no record and no fault
        }
        match serde_json::from_slice::<Value>(&line) {
            Ok(value) => record(&value, &mut out, &mut tally)?,
            Err(_) if kind == Line::Unended => tally.cut = true,
            Err(_) => tally.unparsed += 1,
        }
    }
    let (head, mut text) = (head(&tally), out.text);
    if tally.cut {
        text += "[the last line of the record was cut off and is not shown]\n";
    }
    if head.len() + text.len() > limit {
        return Err(Fault::TooLarge);
    }
    Ok(head + &text)
}

/// Find the record of session `id` under `home` and turn it into the text a receiver reads.
pub fn read(home: &Path, id: &str, limit: usize) -> Result<String, Fault> {
    let file = locate(home, id)?;
    let len = file.metadata().map_err(|_| Fault::Unreadable)?.len();
    if len > MAX_RECORD {
        return Err(Fault::TooLarge);
    }
    render(file.take(len), limit) // what was there when it was opened: a line the program is writing now is the cut-off one
}

#[cfg(test)]
mod tests;
