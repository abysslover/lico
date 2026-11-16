//! Reading a failure: what an HTTP error status and body (or an error object inside a stream) say, as a kind the host can act on and a text it can show.
//! The kinds come from the status first and the words second; the words are heuristics, and a text that matches none is `Unknown`, never a guess that acts.

use serde_json::Value;

use crate::types::{Api, Failure, FailureKind};

/// The most characters of the server's text that are kept.
pub const MAX_MESSAGE_CHARS: usize = 300;

/// `status` is the HTTP status, or 0 for an error object that arrived inside a stream.
pub fn classify(api: Api, status: u16, body: &[u8]) -> Failure {
    let (message, code) = read(api, body);
    let lower = format!("{} {}", message.to_lowercase(), code.to_lowercase());
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    let kind = if has(&["does not support tools", "doesn't support tools", "do not support tools", "tools are not supported", "tool calling is not supported", "does not support function", "does not support tool"]) {
        FailureKind::ToolsUnsupported
    } else if has(&["context length", "context_length", "context window", "prompt is too long", "too many tokens", "exceeds the context", "maximum context"]) {
        FailureKind::ContextTooLong
    } else if status == 401 || status == 403 || has(&["api_key", "authentication"]) {
        FailureKind::KeyNeeded
    } else if status == 404 || (has(&["model"]) && has(&["not found", "does not exist", "not_found"])) {
        FailureKind::ModelMissing
    } else if status == 429 || status == 503 {
        FailureKind::Busy
    } else if status >= 500 {
        FailureKind::ServerError
    } else if (400..500).contains(&status) {
        FailureKind::BadRequest
    } else {
        FailureKind::Unknown
    };
    let _ = api;
    Failure { kind, message: clean(&message) }
}

/// The server's text and its code or type, from an error body of either API, or the body itself when it is not JSON.
fn read(_api: Api, body: &[u8]) -> (String, String) {
    let text = String::from_utf8_lossy(body);
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return (text.into_owned(), String::new()) };
    let as_text = |x: &Value| x.as_str().map(str::to_string).or_else(|| x.as_i64().map(|n| n.to_string())).unwrap_or_default();
    match v.get("error") {
        Some(Value::String(s)) => (s.clone(), String::new()),
        Some(e @ Value::Object(_)) => (as_text(&e["message"]), format!("{} {}", as_text(&e["code"]), as_text(&e["type"]))),
        _ => (v.get("message").map(as_text).unwrap_or_else(|| text.into_owned()), String::new()),
    }
}

/// Newlines and tabs become spaces, other control characters go, and the text is cut to `MAX_MESSAGE_CHARS` characters.
fn clean(s: &str) -> String {
    let spaced: String = s.chars().filter_map(|c| if matches!(c, '\n' | '\t' | '\r') { Some(' ') } else if c.is_control() { None } else { Some(c) }).collect();
    spaced.trim().chars().take(MAX_MESSAGE_CHARS).collect()
}
