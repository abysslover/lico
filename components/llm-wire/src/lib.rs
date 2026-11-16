//! The wire formats of two chat APIs, the OpenAI-compatible `/v1/chat/completions` and Ollama's `/api/chat`, as pure functions (see `SPEC.md`): build a request body,
//! read a reply (a whole body, or a stream in any split into pieces) into neutral events, read a failure, read an Ollama model's facts. No IO, no clock, no randomness,
//! no files, no network, no global state and no text cleaning: the host adapter owns all of those.

mod decode;
mod encode;
mod failure;
mod types;

pub use decode::{Decoder, MAX_CALLS, MAX_CALL_ARGUMENT_BYTES, MAX_LINE_BYTES, MAX_WHOLE_BYTES};
pub use encode::{encode, CONTENT_TYPE, MAX_REQUEST_BYTES};
pub use failure::classify;
pub use types::{Api, Done, EncodeError, Encoded, Event, Failure, FailureKind, Framing, Message, Request, StopReason, StreamError, ToolCall, ToolDef, Usage};
