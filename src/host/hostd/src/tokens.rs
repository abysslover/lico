//! Per-session provider token telemetry (`M07-P01`).
//!
//! Collects cumulative `input` and `output` token counts reported by official provider streams:
//! - Claude Code CLI (`stream-json`): live mid-turn `"assistant"` `message.usage` (`input_tokens` +
//!   `cache_creation_input_tokens` + `cache_read_input_tokens`, `output_tokens`) keyed by `message.id`,
//!   reconciled on `"result"` `usage` so multi-call turns update live without double-counting.
//! - Codex `app-server`: `"thread/tokenUsage/updated"` (`tokenUsage.total` `inputTokens` +
//!   `cachedInputTokens`, `outputTokens`) and turn-completion `usage` payloads, folded cleanly across
//!   session reconnects.
//!
//! Unreported is never guessed: `annotate` omits `"tokens"` until a provider emits at least one usage
//! payload (or persisted counts are restored from `sessions.json`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

#[derive(Clone, Debug, Default)]
struct TokenEntry {
    seen: bool,
    completed_in: u64,
    completed_out: u64,
    open_claude_msgs: HashMap<String, (u64, u64)>,
    anon_seq: u64,
    codex_base_in: u64,
    codex_base_out: u64,
    codex_thread_in: u64,
    codex_thread_out: u64,
}

impl TokenEntry {
    fn open_claude_sum(&self) -> (u64, u64) {
        self.open_claude_msgs.values().fold((0, 0), |(ai, ao), &(i, o)| (ai.saturating_add(i), ao.saturating_add(o)))
    }

    fn totals(&self) -> (u64, u64) {
        let (ci, co) = self.open_claude_sum();
        let inp = self
            .completed_in
            .saturating_add(ci)
            .saturating_add(self.codex_base_in)
            .saturating_add(self.codex_thread_in);
        let out = self
            .completed_out
            .saturating_add(co)
            .saturating_add(self.codex_base_out)
            .saturating_add(self.codex_thread_out);
        (inp, out)
    }
}

fn store() -> &'static Mutex<HashMap<String, TokenEntry>> {
    static STORE: OnceLock<Mutex<HashMap<String, TokenEntry>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn u64_any(obj: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| obj.get(*k).and_then(Value::as_u64))
}

/// Extract `(total_input_tokens, output_tokens)` from a Claude or Codex usage object.
fn extract_usage_pair(usage: &Value) -> Option<(u64, u64)> {
    if !usage.is_object() {
        return None;
    }
    let raw_in = u64_any(usage, &["input_tokens", "inputTokens"]);
    let raw_out = u64_any(usage, &["output_tokens", "outputTokens"]);
    if raw_in.is_none() && raw_out.is_none() {
        return None;
    }
    let cache_create = u64_any(usage, &["cache_creation_input_tokens", "cacheCreationInputTokens"]).unwrap_or(0);
    let cache_read = u64_any(usage, &["cache_read_input_tokens", "cached_input_tokens", "cachedInputTokens"]).unwrap_or(0);
    // Note: in Anthropic API usage, `input_tokens` is uncached input and cache_* are separate; in OpenAI/Codex
    // `inputTokens` already includes `cachedInputTokens` when `cachedInputTokens <= inputTokens`.
    let base_in = raw_in.unwrap_or(0);
    let is_camel = usage.get("inputTokens").is_some() || usage.get("outputTokens").is_some();
    let total_in = if is_camel {
        base_in.max(cache_read)
    } else {
        base_in.saturating_add(cache_create).saturating_add(cache_read)
    };
    Some((total_in, raw_out.unwrap_or(0)))
}

/// Observe live mid-turn usage on a Claude `"assistant"` stream event.
pub fn observe_claude_assistant(session: &str, message: &Value) {
    if session.is_empty() {
        return;
    }
    let inner = &message["message"];
    let usage = if inner["usage"].is_object() { &inner["usage"] } else { &message["usage"] };
    let Some((inp, out)) = extract_usage_pair(usage) else { return };
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(session.to_string()).or_default();
    let key = match inner["id"].as_str().filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => {
            entry.anon_seq += 1;
            format!("__anon_{}", entry.anon_seq)
        }
    };
    entry.open_claude_msgs.insert(key, (inp, out));
    entry.seen = true;
}

/// Reconcile turn usage on a Claude `"result"` stream event.
pub fn observe_claude_result(session: &str, message: &Value) {
    if session.is_empty() {
        return;
    }
    let res_pair = extract_usage_pair(&message["usage"]);
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = (if res_pair.is_some() {
        Some(guard.entry(session.to_string()).or_default())
    } else {
        guard.get_mut(session)
    }) else {
        return;
    };
    let (open_in, open_out) = entry.open_claude_sum();
    entry.open_claude_msgs.clear();
    if let Some((turn_in, turn_out)) = res_pair {
        entry.completed_in = entry.completed_in.saturating_add(turn_in.max(open_in));
        entry.completed_out = entry.completed_out.saturating_add(turn_out.max(open_out));
        entry.seen = true;
        crate::registry::mark_dirty();
    } else if open_in > 0 || open_out > 0 {
        entry.completed_in = entry.completed_in.saturating_add(open_in);
        entry.completed_out = entry.completed_out.saturating_add(open_out);
        crate::registry::mark_dirty();
    }
}

/// Fold prior Codex thread counters into base before a new `codex app-server` process starts.
pub fn on_codex_spawn(session: &str) {
    if session.is_empty() {
        return;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = guard.get_mut(session) {
        if entry.codex_thread_in > 0 || entry.codex_thread_out > 0 {
            entry.codex_base_in = entry.codex_base_in.saturating_add(entry.codex_thread_in);
            entry.codex_base_out = entry.codex_base_out.saturating_add(entry.codex_thread_out);
            entry.codex_thread_in = 0;
            entry.codex_thread_out = 0;
        }
    }
}

/// Observe Codex `"thread/tokenUsage/updated"` or `"turn/completed"` notifications.
pub fn observe_codex(session: &str, method: &str, params: &Value) {
    if session.is_empty() {
        return;
    }
    if method == "thread/tokenUsage/updated" {
        let total_obj = params
            .pointer("/tokenUsage/total")
            .or_else(|| params.pointer("/token_usage/total"))
            .unwrap_or(&params["tokenUsage"]);
        if let Some((inp, out)) = extract_usage_pair(total_obj) {
            let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
            let entry = guard.entry(session.to_string()).or_default();
            entry.codex_thread_in = entry.codex_thread_in.max(inp);
            entry.codex_thread_out = entry.codex_thread_out.max(out);
            entry.seen = true;
            crate::registry::mark_dirty();
        }
        return;
    }
    if method == "turn/completed" {
        let turn_usage = params.pointer("/turn/usage").or_else(|| params.get("usage"));
        if let Some(u) = turn_usage {
            if let Some((inp, out)) = extract_usage_pair(u) {
                let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
                let entry = guard.entry(session.to_string()).or_default();
                if entry.codex_thread_in == 0 && entry.codex_thread_out == 0 {
                    entry.completed_in = entry.completed_in.saturating_add(inp);
                    entry.completed_out = entry.completed_out.saturating_add(out);
                }
                entry.seen = true;
                crate::registry::mark_dirty();
            }
        }
    }
}

/// Restore persisted token totals from `sessions.json` during host recovery.
pub fn restore(session: &str, input: u64, output: u64) {
    if session.is_empty() {
        return;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(session.to_string()).or_default();
    entry.completed_in = input;
    entry.completed_out = output;
    entry.seen = true;
}

/// Snapshot `(input, output)` for `sessions.json` persistence when observed.
pub fn persisted_pair(session: &str) -> Option<(u64, u64)> {
    let guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.get(session)?;
    entry.seen.then(|| entry.totals())
}

/// Attach `"tokens": {"input": N, "output": M}` to a session's status object when observed.
pub fn annotate(entry: &mut Value, session: &str) {
    if let Some((inp, out)) = persisted_pair(session) {
        entry["tokens"] = json!({"input": inp, "output": out});
    }
}

/// Remove stored token telemetry when a session is purged.
pub fn forget(session: &str) {
    if !session.is_empty() {
        store().lock().unwrap_or_else(|e| e.into_inner()).remove(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_live_assistant_and_result_reconcile_without_double_counting() {
        let sid = "s_tok_claude_test";
        forget(sid);
        assert_eq!(persisted_pair(sid), None);

        observe_claude_assistant(
            sid,
            &json!({"type": "assistant", "message": {"id": "msg_1", "usage": {"input_tokens": 120, "cache_read_input_tokens": 1000, "output_tokens": 45}}}),
        );
        assert_eq!(persisted_pair(sid), Some((1120, 45)));

        // Streaming update for same msg_1 replaces msg_1 rather than adding twice
        observe_claude_assistant(
            sid,
            &json!({"type": "assistant", "message": {"id": "msg_1", "usage": {"input_tokens": 120, "cache_read_input_tokens": 1000, "output_tokens": 80}}}),
        );
        assert_eq!(persisted_pair(sid), Some((1120, 80)));

        // Second API call in same turn
        observe_claude_assistant(
            sid,
            &json!({"type": "assistant", "message": {"id": "msg_2", "usage": {"input_tokens": 50, "cache_creation_input_tokens": 200, "output_tokens": 30}}}),
        );
        assert_eq!(persisted_pair(sid), Some((1370, 110)));

        // Turn result locks in the turn total cleanly
        observe_claude_result(
            sid,
            &json!({"type": "result", "subtype": "success", "usage": {"input_tokens": 170, "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 200, "output_tokens": 110}}),
        );
        assert_eq!(persisted_pair(sid), Some((1370, 110)));
        forget(sid);
    }

    #[test]
    fn codex_thread_token_usage_accumulates_across_reconnects() {
        let sid = "s_tok_codex_test";
        forget(sid);
        observe_codex(
            sid,
            "thread/tokenUsage/updated",
            &json!({"tokenUsage": {"total": {"inputTokens": 4500, "cachedInputTokens": 3000, "outputTokens": 620}}}),
        );
        assert_eq!(persisted_pair(sid), Some((4500, 620)));

        on_codex_spawn(sid);
        observe_codex(
            sid,
            "thread/tokenUsage/updated",
            &json!({"tokenUsage": {"total": {"inputTokens": 1500, "outputTokens": 180}}}),
        );
        assert_eq!(persisted_pair(sid), Some((6000, 800)));
        forget(sid);
    }
}
