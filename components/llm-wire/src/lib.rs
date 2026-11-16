//! The wire formats of two chat APIs, the OpenAI-compatible `/v1/chat/completions` and Ollama's `/api/chat`, as pure functions (see `SPEC.md`): build a request body,
//! read a reply (a whole body, or a stream in any split into pieces) into neutral events, read a failure, read an Ollama model's facts. No IO, no clock, no randomness,
//! no files, no network, no global state and no text cleaning: the host adapter owns all of those.

mod encode;
mod types;

pub use encode::{encode, CONTENT_TYPE, MAX_REQUEST_BYTES};
pub use types::{Api, EncodeError, Encoded, Message, Request, ToolCall, ToolDef};
