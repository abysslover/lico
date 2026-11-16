//! Ollama's `/api/show`: what a model says about itself. Both facts are `None` when the server does not say; nothing is guessed.

use serde_json::{json, Value};

use crate::types::Encoded;

/// The facts the host asks for: the model's context size in tokens, and whether it can call tools.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Show {
    pub context_tokens: Option<u64>,
    pub tools: Option<bool>,
}

/// The request for a model's facts.
pub fn show_request(model: &str) -> Encoded {
    Encoded { path: "/api/show", body: serde_json::to_vec(&json!({"model": model})).unwrap_or_default() }
}

/// The reply: the `*.context_length` of `model_info`, and whether `capabilities` lists `tools`.
pub fn read_show(body: &[u8]) -> Show {
    let Ok(v) = serde_json::from_slice::<Value>(body) else { return Show::default() };
    let context_tokens = v["model_info"].as_object().and_then(|m| m.iter().find(|(k, n)| k.ends_with(".context_length") && n.is_u64()).and_then(|(_, n)| n.as_u64()));
    let tools = v["capabilities"].as_array().map(|c| c.iter().any(|x| x.as_str() == Some("tools")));
    Show { context_tokens, tools }
}
