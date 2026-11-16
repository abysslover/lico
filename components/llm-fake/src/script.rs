//! What the fake answers: rules by method and path, each a behaviour.

use std::time::Duration;

/// How the body goes out.
#[derive(Clone, Debug)]
pub enum Body {
    /// One piece with a `Content-Length`.
    Whole(Vec<u8>),
    /// `Transfer-Encoding: chunked`, each piece its own chunk.
    Chunked(Vec<Vec<u8>>),
    /// No length and no chunks: the body ends when the connection closes (HTTP/1.0 style).
    UntilClose(Vec<Vec<u8>>),
    /// These bytes and nothing else, not even a status line, then the connection closes.
    Raw(Vec<u8>),
}

/// How the reply ends. `n` counts pieces (bytes for a `Whole` body).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    Complete,
    /// Send `n` pieces, then close the connection without the end of the body.
    Cut(usize),
    /// Send `n` pieces, then say nothing more and keep the connection open.
    Silent(usize),
}

#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    /// Waited before the first byte.
    pub first_byte_delay: Duration,
    /// Waited between pieces.
    pub gap: Duration,
    pub ending: Ending,
}

impl Reply {
    /// A whole JSON body.
    pub fn json(status: u16, text: &str) -> Reply {
        Reply { status, headers: vec![("Content-Type".into(), "application/json".into())], body: Body::Whole(text.as_bytes().to_vec()), first_byte_delay: Duration::ZERO, gap: Duration::ZERO, ending: Ending::Complete }
    }
    /// A chunked body of `content_type`.
    pub fn chunked(status: u16, content_type: &str, pieces: Vec<Vec<u8>>) -> Reply {
        Reply { status, headers: vec![("Content-Type".into(), content_type.into())], body: Body::Chunked(pieces), first_byte_delay: Duration::ZERO, gap: Duration::ZERO, ending: Ending::Complete }
    }
    /// Server-sent events from the pieces of `shapes::openai_sse`.
    pub fn sse(pieces: Vec<Vec<u8>>) -> Reply {
        Reply::chunked(200, "text/event-stream", pieces)
    }
    /// One JSON object per line from the pieces of `shapes::ollama_ndjson`.
    pub fn ndjson(pieces: Vec<Vec<u8>>) -> Reply {
        Reply::chunked(200, "application/x-ndjson", pieces)
    }
    pub fn header(mut self, name: &str, value: &str) -> Reply {
        self.headers.push((name.into(), value.into()));
        self
    }
    pub fn first_byte_after(mut self, d: Duration) -> Reply {
        self.first_byte_delay = d;
        self
    }
    pub fn gap(mut self, d: Duration) -> Reply {
        self.gap = d;
        self
    }
    pub fn ending(mut self, e: Ending) -> Reply {
        self.ending = e;
        self
    }
}

#[derive(Clone, Debug)]
pub enum Behaviour {
    Answer(Reply),
    /// Read the request, then close the connection without a word: a server that is gone.
    Close,
    /// Read the request and never answer; the connection stays open.
    Hang,
}

#[derive(Clone, Debug)]
pub(crate) struct Rule {
    pub method: String,
    pub path: String,
    pub behaviour: Behaviour,
    pub remaining: Option<usize>,
}

/// The rules, in order: the first that matches the method and path and is not used up answers.
#[derive(Clone, Debug, Default)]
pub struct Script {
    pub(crate) rules: Vec<Rule>,
}

impl Script {
    pub fn new() -> Script {
        Script::default()
    }
    /// Every request for `method` and `path` is answered like this.
    pub fn on(mut self, method: &str, path: &str, behaviour: Behaviour) -> Script {
        self.rules.push(Rule { method: method.into(), path: path.into(), behaviour, remaining: None });
        self
    }
    /// The next `times` requests for `method` and `path` are answered like this, then the next matching rule is used.
    pub fn times(mut self, times: usize, method: &str, path: &str, behaviour: Behaviour) -> Script {
        self.rules.push(Rule { method: method.into(), path: path.into(), behaviour, remaining: Some(times) });
        self
    }
}
