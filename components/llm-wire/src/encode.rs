//! Request bodies for both APIs. Pure: the same request gives the same bytes.

use serde_json::{json, Map, Value};

use crate::types::{Api, EncodeError, Encoded, Message, Request, ToolCall, ToolDef};

pub const CONTENT_TYPE: &str = "application/json";
/// The largest body `encode` gives; a conversation that does not fit is the caller's to shorten.
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;

const OPENAI_PATH: &str = "/v1/chat/completions";
const OLLAMA_PATH: &str = "/api/chat";

/// The request as the bytes to `POST` and the path to post them to.
pub fn encode(api: Api, request: &Request) -> Result<Encoded, EncodeError> {
    if request.model.is_empty() {
        return Err(EncodeError::EmptyModel);
    }
    if request.messages.is_empty() {
        return Err(EncodeError::NoMessages);
    }
    for (i, t) in request.tools.iter().enumerate() {
        if request.tools[..i].iter().any(|earlier| earlier.name == t.name) {
            return Err(EncodeError::DuplicateTool(t.name.clone()));
        }
    }
    let mut body = Map::new();
    body.insert("model".into(), json!(request.model));
    body.insert("messages".into(), Value::Array(request.messages.iter().map(|m| message(api, m)).collect()));
    body.insert("stream".into(), json!(request.stream));
    if !request.tools.is_empty() {
        body.insert("tools".into(), Value::Array(request.tools.iter().map(tool_def).collect()));
    }
    match api {
        Api::OpenAi => {
            if request.stream && request.include_usage {
                body.insert("stream_options".into(), json!({"include_usage": true}));
            }
            if let Some(n) = request.max_output_tokens {
                body.insert("max_tokens".into(), json!(n));
            }
        }
        Api::Ollama => {
            let mut options = Map::new();
            if let Some(n) = request.context_tokens {
                options.insert("num_ctx".into(), json!(n));
            }
            if let Some(n) = request.max_output_tokens {
                options.insert("num_predict".into(), json!(n));
            }
            if !options.is_empty() {
                body.insert("options".into(), Value::Object(options));
            }
        }
    }
    let bytes = serde_json::to_vec(&Value::Object(body)).map_err(|_| EncodeError::TooLarge)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(EncodeError::TooLarge);
    }
    Ok(Encoded { path: if api == Api::OpenAi { OPENAI_PATH } else { OLLAMA_PATH }, body: bytes })
}

fn tool_def(t: &ToolDef) -> Value {
    json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}})
}

fn message(api: Api, m: &Message) -> Value {
    match m {
        Message::System(text) => json!({"role": "system", "content": text}),
        Message::User(text) => json!({"role": "user", "content": text}),
        Message::Assistant { text, calls } if calls.is_empty() => json!({"role": "assistant", "content": text}),
        Message::Assistant { text, calls } => match api {
            Api::OpenAi => json!({
                "role": "assistant",
                "content": if text.is_empty() { Value::Null } else { json!(text) },
                "tool_calls": calls.iter().map(openai_call).collect::<Vec<_>>(),
            }),
            Api::Ollama => json!({
                "role": "assistant",
                "content": text,
                "tool_calls": calls.iter().map(|c| json!({"function": {"name": c.name, "arguments": c.arguments}})).collect::<Vec<_>>(),
            }),
        },
        Message::Tool { call_id, name, content } => match api {
            Api::OpenAi => json!({"role": "tool", "tool_call_id": call_id, "content": content}),
            Api::Ollama => json!({"role": "tool", "tool_name": name, "content": content}),
        },
    }
}

/// OpenAI wants the arguments as a JSON text, not an object.
fn openai_call(c: &ToolCall) -> Value {
    json!({"id": c.id, "type": "function", "function": {"name": c.name, "arguments": c.arguments.to_string()}})
}
