//! `turns` of a chat: the visible user and assistant messages of its transcript, counted by a bounded stream.
//! A transcript over `MAX_BYTES` or `MAX_LINES` has no count (`None`: unknown), never a wrong one. Only the items of a returned page are counted,
//! and a small cache keyed by (file, modification time, size) keeps a repeated listing from reading the files again.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::{claude_msg_text, codex_msg_text, open_regular_nofollow};

pub(super) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const MAX_LINES: usize = 100_000;
const CACHE_ENTRIES: usize = 512;

type Cache = HashMap<PathBuf, (u64, u64, Option<u32>)>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The turns of the transcript `path` of a `provider` chat, or `None` when it cannot be read or is over the bound.
pub(super) fn count(provider: &str, path: &Path) -> Option<u32> {
    let (file, len, mtime) = open_regular_nofollow(path)?;
    if let Some(&(m, l, n)) = cache().lock().unwrap().get(path) {
        if m == mtime && l == len {
            return n;
        }
    }
    let counted = if len > MAX_BYTES { None } else { count_lines(provider, BufReader::new(file.take(MAX_BYTES))) };
    let mut table = cache().lock().unwrap();
    if table.len() >= CACHE_ENTRIES {
        table.clear();
    }
    table.insert(path.to_path_buf(), (mtime, len, counted));
    counted
}

/// Count the messages of a transcript read as lines: a Claude `user` or `assistant` record with text, a Codex `response_item` message with text
/// (tool results, tool calls, side chains and the program's own context text do not count).
pub(super) fn count_lines(provider: &str, reader: impl BufRead) -> Option<u32> {
    let mut turns = 0u32;
    for (index, line) in reader.lines().enumerate() {
        if index >= MAX_LINES {
            return None;
        }
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let counts = if provider == "claude" {
            match v.get("type").and_then(Value::as_str) {
                Some(role @ ("user" | "assistant")) => claude_msg_text(&v, role == "user").is_some(),
                _ => false,
            }
        } else {
            codex_msg_text(&v).is_some()
        };
        turns += u32::from(counts);
    }
    Some(turns)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(text.as_bytes().to_vec())
    }

    #[test]
    fn claude_counts_text_messages_and_not_tool_calls_or_tool_results_or_side_chains() {
        let text = r#"{"type":"user","message":{"role":"user","content":"q"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"a"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}
{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}
{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"side"}]}}
not json at all
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"b"}]}}
"#;
        assert_eq!(count_lines("claude", lines(text)), Some(3));
    }

    #[test]
    fn codex_counts_messages_and_not_the_programs_own_context() {
        let text = r#"{"type":"session_meta","payload":{"id":"x","cwd":"/w"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n<cwd>/w</cwd>"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"q"}]}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"a"}]}}
{"type":"response_item","payload":{"type":"function_call","name":"shell"}}
"#;
        assert_eq!(count_lines("codex", lines(text)), Some(2));
    }

    #[test]
    fn a_transcript_over_the_line_bound_has_no_count() {
        let one = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"q\"}}\n";
        assert_eq!(count_lines("claude", lines(&one.repeat(MAX_LINES))), Some(MAX_LINES as u32));
        assert_eq!(count_lines("claude", lines(&one.repeat(MAX_LINES + 1))), None);
    }
}
