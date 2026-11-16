//! A scripted fake of a Local LLM server, for tests (see `SPEC.md`). It listens on `127.0.0.1` with an ephemeral port only, answers what its `Script` says,
//! and records what it was sent. It is written from the public documentation of the two APIs and shares no code with `lico-llm-wire` on purpose: the two
//! implementations check each other.

mod script;
mod server;
mod shapes;

pub use script::{Behaviour, Body, Ending, Reply, Script};
pub use server::{Fake, Seen};
pub use shapes::{ollama_error, ollama_ndjson, openai_error, openai_sse, Piece};
