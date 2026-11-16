//! The neutral types: what the host adapter and the tool loop speak, whatever API is behind.

use serde_json::Value;

/// The two wire formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    /// `POST /v1/chat/completions`, replies as server-sent events.
    OpenAi,
    /// `POST /api/chat`, replies as one JSON object per line.
    Ollama,
}

/// One tool call of the model; `arguments` is always a JSON object when the decoder makes it.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// One message of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    System(String),
    User(String),
    /// What the model said, and the calls it made in that answer (empty for a plain answer).
    Assistant { text: String, calls: Vec<ToolCall> },
    /// The result of a call: `call_id` is the id of the call it answers, `name` the tool.
    Tool { call_id: String, name: String, content: String },
}

/// A tool the model may call: `parameters` is a JSON schema object.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// One request to a chat API.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    pub tools: &'a [ToolDef],
    pub stream: bool,
    /// Ollama's `options.num_ctx`; the OpenAI format has no field for it and ignores it.
    pub context_tokens: Option<u32>,
    /// `max_tokens` (OpenAI) or `options.num_predict` (Ollama).
    pub max_output_tokens: Option<u32>,
    /// OpenAI only: `stream_options: {include_usage: true}` on a streamed request.
    pub include_usage: bool,
}

/// An encoded request: the path to `POST` it to and the JSON body.
#[derive(Clone, Debug, PartialEq)]
pub struct Encoded {
    pub path: &'static str,
    pub body: Vec<u8>,
}

/// Why a request could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
    EmptyModel,
    NoMessages,
    DuplicateTool(String),
    TooLarge,
}
