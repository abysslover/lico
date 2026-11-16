//! The conversation record of one local session (see `SPEC.md`): an owner-only file of JSON lines, one per message, appended as the conversation goes, with a torn last
//! line repaired on open; and `fit`, which chooses how much of the conversation fits a context window, by whole turns. The record holds what the person typed, what the
//! model answered (its text and its tool calls) and the results of those calls, and nothing else: no key, no server response, no reasoning text, no system prompt.

use lico_llm_wire::{Message, ToolCall};
use serde_json::{json, Map, Value};
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// The longest line (one message) that is written or read.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// The largest record file; the whole conversation is held in memory, so this is the memory bound of one session.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Io(std::io::ErrorKind),
    /// A line other than a torn last one is not a message of the record (1-based).
    Corrupt { line: usize },
    /// The file can be read by group or others.
    NotOwnerOnly,
    /// A symlink, a directory or another kind of file.
    NotARegularFile,
    /// The system prompt is built for each request and is never stored.
    SystemNotStored,
    /// A message whose line would be over `MAX_LINE_BYTES`.
    TooLarge,
    /// The file is at `MAX_FILE_BYTES`.
    Full,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e.kind())
    }
}

/// What opening found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    /// Messages read.
    pub loaded: usize,
    /// Bytes of a torn last line that were cut off.
    pub repaired_bytes: u64,
    pub created: bool,
}

pub struct Convo {
    file: File,
    messages: Vec<Message>,
    bytes: u64,
}

fn line_of(m: &Message) -> Result<String, Error> {
    let v = match m {
        Message::System(_) => return Err(Error::SystemNotStored),
        Message::User(text) => json!({"v": 1, "t": "user", "text": text}),
        Message::Assistant { text, calls } if calls.is_empty() => json!({"v": 1, "t": "assistant", "text": text}),
        Message::Assistant { text, calls } => json!({
            "v": 1, "t": "assistant", "text": text,
            "calls": calls.iter().map(|c| json!({"id": c.id, "name": c.name, "arguments": c.arguments})).collect::<Vec<_>>(),
        }),
        Message::Tool { call_id, name, content } => json!({"v": 1, "t": "tool", "call_id": call_id, "name": name, "content": content}),
    };
    Ok(v.to_string())
}

/// One line of the file as a message; `None` for anything that is not exactly one of the three shapes.
fn message_of(line: &str) -> Option<Message> {
    let v: Value = serde_json::from_str(line).ok()?;
    let o = v.as_object()?;
    if o.get("v")? != &json!(1) {
        return None;
    }
    let only = |allowed: &[&str]| o.keys().all(|k| allowed.contains(&k.as_str()));
    let text = |k: &str| o.get(k)?.as_str().map(str::to_string);
    match o.get("t")?.as_str()? {
        "user" if only(&["v", "t", "text"]) => Some(Message::User(text("text")?)),
        "assistant" if only(&["v", "t", "text", "calls"]) => {
            let calls = match o.get("calls") {
                None => Vec::new(),
                Some(Value::Array(a)) => a.iter().map(call_of).collect::<Option<Vec<_>>>()?,
                Some(_) => return None,
            };
            Some(Message::Assistant { text: text("text")?, calls })
        }
        "tool" if only(&["v", "t", "call_id", "name", "content"]) => Some(Message::Tool { call_id: text("call_id")?, name: text("name")?, content: text("content")? }),
        _ => None,
    }
}

fn call_of(v: &Value) -> Option<ToolCall> {
    let o: &Map<String, Value> = v.as_object()?;
    if o.len() != 3 {
        return None;
    }
    Some(ToolCall { id: o.get("id")?.as_str()?.to_string(), name: o.get("name")?.as_str()?.to_string(), arguments: o.get("arguments")?.clone() })
}

impl Convo {
    /// Opens the record, creating the directory (`0700`) and the file (`0600`) when they are missing, and repairs a torn last line.
    pub fn open(path: &Path) -> Result<(Convo, Opened), Error> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        }
        let created = match std::fs::symlink_metadata(path) {
            Ok(meta) => {
                if !meta.file_type().is_file() {
                    return Err(Error::NotARegularFile);
                }
                if meta.permissions().mode() & 0o077 != 0 {
                    return Err(Error::NotOwnerOnly);
                }
                false
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => return Err(e.into()),
        };
        let mut file = OpenOptions::new().read(true).append(true).create(true).mode(0o600).open(path)?;
        // the file that was opened is the file that was checked (not a link put there in between)
        let (opened, checked) = (file.metadata()?, std::fs::symlink_metadata(path)?);
        if !opened.file_type().is_file() || !checked.file_type().is_file() || (opened.dev(), opened.ino()) != (checked.dev(), checked.ino()) {
            return Err(Error::NotARegularFile);
        }
        if opened.len() > MAX_FILE_BYTES {
            return Err(Error::Full);
        }
        let mut raw = Vec::with_capacity(opened.len() as usize);
        file.read_to_end(&mut raw)?;
        let (mut messages, mut good_len, mut repaired) = (Vec::new(), 0usize, 0u64);
        let mut rest: &[u8] = &raw;
        let mut number = 0usize;
        while !rest.is_empty() {
            number += 1;
            let (line, complete, next) = match rest.iter().position(|b| *b == b'\n') {
                Some(i) => (&rest[..i], true, &rest[i + 1..]),
                None => (rest, false, &rest[rest.len()..]),
            };
            let parsed = std::str::from_utf8(line).ok().filter(|_| line.len() <= MAX_LINE_BYTES).and_then(message_of);
            match (parsed, complete) {
                (Some(m), true) => {
                    messages.push(m);
                    good_len += line.len() + 1;
                }
                (Some(m), false) => {
                    // a whole message that only lacks its newline: kept, and the newline is added
                    messages.push(m);
                    file.write_all(b"\n")?;
                    good_len += line.len() + 1;
                }
                (None, false) => {
                    repaired = line.len() as u64;
                    file.set_len(good_len as u64)?;
                }
                (None, true) => return Err(Error::Corrupt { line: number }),
            }
            rest = next;
        }
        let loaded = messages.len();
        Ok((Convo { file, messages, bytes: good_len as u64 }, Opened { loaded, repaired_bytes: repaired, created }))
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Writes the message as one line (one `write_all`, then synced) and keeps it. Nothing is written for a refused message.
    pub fn append(&mut self, m: &Message) -> Result<(), Error> {
        let mut line = line_of(m)?;
        if line.len() + 1 > MAX_LINE_BYTES {
            return Err(Error::TooLarge);
        }
        line.push('\n');
        if self.bytes + line.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Full);
        }
        let written = self.file.write_all(line.as_bytes()).and_then(|()| self.file.sync_data());
        if let Err(e) = written {
            let _ = self.file.set_len(self.bytes);
            return Err(e.into());
        }
        self.bytes += line.len() as u64;
        self.messages.push(m.clone());
        Ok(())
    }
}

// ---- fit ------------------------------------------------------------------------------------------------------------------------------

/// What `fit` chose: send `messages[start..]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fitted {
    pub start: usize,
    /// Whole turns left out, oldest first.
    pub dropped_turns: usize,
    /// The estimate of what is sent.
    pub tokens: u32,
    /// The newest turn alone is over the budget (it is kept; the caller refuses and says so).
    pub too_big: bool,
}

/// A token estimate: the bytes of the message's text (and of a call's id, name and arguments, or a result's id, name and content) over 3.5, rounded up, plus 4.
pub fn estimate_tokens(m: &Message) -> u32 {
    let bytes = match m {
        Message::System(t) | Message::User(t) => t.len(),
        Message::Assistant { text, calls } => text.len() + calls.iter().map(|c| c.id.len() + c.name.len() + c.arguments.to_string().len()).sum::<usize>(),
        Message::Tool { call_id, name, content } => call_id.len() + name.len() + content.len(),
    };
    (bytes * 2).div_ceil(7) as u32 + 4
}

/// The newest turns that fit `budget_tokens`, by whole turns (a message of the person and everything up to the next one; what comes before the first message of
/// the person is the first turn). The oldest are dropped first and the newest is always kept.
pub fn fit(messages: &[Message], budget_tokens: u32) -> Fitted {
    if messages.is_empty() {
        return Fitted { start: 0, dropped_turns: 0, tokens: 0, too_big: false };
    }
    let starts: Vec<usize> = (0..messages.len()).filter(|&i| i == 0 || matches!(messages[i], Message::User(_))).collect();
    let cost = |from: usize, to: usize| messages[from..to].iter().map(|m| estimate_tokens(m) as u64).sum::<u64>();
    let mut chosen = starts.len() - 1;
    let mut tokens = cost(starts[chosen], messages.len());
    let too_big = tokens > budget_tokens as u64;
    if !too_big {
        while chosen > 0 {
            let older = cost(starts[chosen - 1], starts[chosen]);
            if tokens + older > budget_tokens as u64 {
                break;
            }
            tokens += older;
            chosen -= 1;
        }
    }
    Fitted { start: starts[chosen], dropped_turns: chosen, tokens: tokens.min(u32::MAX as u64) as u32, too_big }
}
