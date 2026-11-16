//! The bytes of a reply in the shapes of the public documentation of the two APIs. Written here by hand, not with the code of `lico-llm-wire`.

use serde_json::{json, Value};

/// One step of an answer.
#[derive(Clone, Debug)]
pub enum Piece {
    Text(String),
    /// Reasoning text (`reasoning_content` for the OpenAI shape, `thinking` for Ollama).
    Reasoning(String),
    /// A tool call: `args` is a JSON text; for the OpenAI shape it goes out in slices of `split` characters (0: whole), for Ollama as an object.
    Call { id: Option<String>, name: String, args: String, split: usize },
    /// The finish reason (`stop`, `tool_calls`, `length`).
    Finish(String),
    Usage { prompt: u64, completion: u64 },
    /// The end: `data: [DONE]` for the OpenAI shape, the `done: true` line for Ollama.
    End,
}

fn chunk(delta: Value, finish: Option<&str>) -> Vec<u8> {
    let v = json!({"id": "chatcmpl-fake", "object": "chat.completion.chunk", "created": 1, "model": "fake", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
    format!("data: {v}\n\n").into_bytes()
}

/// The server-sent events of an OpenAI-compatible `chat/completions` stream, one piece of output per step.
pub fn openai_sse(pieces: &[Piece]) -> Vec<Vec<u8>> {
    let mut out = vec![chunk(json!({"role": "assistant", "content": ""}), None)];
    let mut calls = 0usize;
    for p in pieces {
        match p {
            Piece::Text(t) => out.push(chunk(json!({"content": t}), None)),
            Piece::Reasoning(t) => out.push(chunk(json!({"reasoning_content": t}), None)),
            Piece::Call { id, name, args, split } => {
                let first = match id {
                    Some(id) => json!({"index": calls, "id": id, "type": "function", "function": {"name": name, "arguments": ""}}),
                    None => json!({"index": calls, "type": "function", "function": {"name": name, "arguments": ""}}),
                };
                out.push(chunk(json!({"tool_calls": [first]}), None));
                let chars: Vec<char> = args.chars().collect();
                let size = if *split == 0 { chars.len().max(1) } else { *split };
                for part in chars.chunks(size) {
                    let s: String = part.iter().collect();
                    out.push(chunk(json!({"tool_calls": [{"index": calls, "function": {"arguments": s}}]}), None));
                }
                calls += 1;
            }
            Piece::Finish(r) => out.push(chunk(json!({}), Some(r))),
            Piece::Usage { prompt, completion } => {
                let v = json!({"id": "chatcmpl-fake", "object": "chat.completion.chunk", "created": 1, "model": "fake", "choices": [], "usage": {"prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion}});
                out.push(format!("data: {v}\n\n").into_bytes());
            }
            Piece::End => out.push(b"data: [DONE]\n\n".to_vec()),
        }
    }
    out
}

fn line(message: Value, done: Option<(&str, Option<(u64, u64)>)>) -> Vec<u8> {
    let mut v = json!({"model": "fake", "created_at": "2026-01-01T00:00:00Z", "message": message, "done": done.is_some()});
    if let Some((reason, usage)) = done {
        v["done_reason"] = json!(reason);
        if let Some((p, c)) = usage {
            v["prompt_eval_count"] = json!(p);
            v["eval_count"] = json!(c);
        }
    }
    format!("{v}\n").into_bytes()
}

/// The lines of an Ollama `/api/chat` stream. `Finish` and `Usage` only set what the last line says; `End` writes that line.
pub fn ollama_ndjson(pieces: &[Piece]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let (mut reason, mut usage) = ("stop".to_string(), None);
    for p in pieces {
        match p {
            Piece::Text(t) => out.push(line(json!({"role": "assistant", "content": t}), None)),
            Piece::Reasoning(t) => out.push(line(json!({"role": "assistant", "content": "", "thinking": t}), None)),
            Piece::Call { name, args, .. } => {
                let arguments = serde_json::from_str::<Value>(args).unwrap_or_else(|_| json!(args));
                out.push(line(json!({"role": "assistant", "content": "", "tool_calls": [{"function": {"name": name, "arguments": arguments}}]}), None));
            }
            Piece::Finish(r) => reason = if r == "tool_calls" { "stop".to_string() } else { r.clone() },
            Piece::Usage { prompt, completion } => usage = Some((*prompt, *completion)),
            Piece::End => out.push(line(json!({"role": "assistant", "content": ""}), Some((&reason, usage)))),
        }
    }
    out
}

/// An OpenAI-style error body.
pub fn openai_error(message: &str, code: &str) -> String {
    json!({"error": {"message": message, "type": "invalid_request_error", "param": null, "code": code}}).to_string()
}

/// An Ollama-style error body.
pub fn ollama_error(message: &str) -> String {
    json!({"error": message}).to_string()
}
