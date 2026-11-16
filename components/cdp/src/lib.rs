//! `lico-cdp` (SPEC.md): a synchronous Chrome DevTools Protocol client over one WebSocket on this machine. Numbered calls matched to their replies, events kept in a bounded
//! queue, deadlines and cancellation, and pure builders of the parameters. No thread, no async. The only dependencies are `lico-ws` and `serde_json`.

pub use lico_ws::{Canceller, Limits};

use lico_ws::Message;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// The events kept for the caller between two reads: a tab sends a frame every few tens of milliseconds while it changes, and a caller that reads each time it wakes never has
/// more than a few; 64 is room for a slow moment, and when it is full the OLDEST is dropped (a newer frame replaces an older one) and counted by `dropped()`.
pub const MAX_QUEUED_EVENTS: usize = 64;
/// How long a call waits for its reply when the caller has no better figure: the engine's own deadlines are 30 s for a navigation and 60 s for a command (its documentation).
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub method: String,
    pub params: Value,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The endpoint answered the call with a CDP error.
    Remote { code: i64, message: String },
    Timeout,
    Cancelled,
    Closed,
    TooBig,
    Protocol(&'static str),
    Ws(lico_ws::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Remote { code, message } => write!(f, "the endpoint answered with the error {code}: {message}"),
            Error::Timeout => write!(f, "timed out"),
            Error::Cancelled => write!(f, "cancelled"),
            Error::Closed => write!(f, "the connection is closed"),
            Error::TooBig => write!(f, "a message is over the cap"),
            Error::Protocol(why) => write!(f, "protocol error: {why}"),
            Error::Ws(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

fn from_ws(e: lico_ws::Error) -> Error {
    match e {
        lico_ws::Error::Timeout => Error::Timeout,
        lico_ws::Error::Cancelled => Error::Cancelled,
        lico_ws::Error::Closed => Error::Closed,
        lico_ws::Error::TooBig => Error::TooBig,
        lico_ws::Error::Protocol(why) => Error::Protocol(why),
        other => Error::Ws(other),
    }
}

pub struct Client {
    conn: lico_ws::Conn,
    next_id: u64,
    queue: VecDeque<Event>,
    dropped: u64,
    stray: u64,
    dead: bool,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client").field("next_id", &self.next_id).field("queued", &self.queue.len()).field("dropped", &self.dropped).finish()
    }
}

/// One incoming message, sorted.
enum Got {
    Reply { id: u64, outcome: Result<Value, Error> },
    Event(Event),
}

fn sort(text: &str) -> Result<Got, Error> {
    let v: Value = serde_json::from_str(text).map_err(|_| Error::Protocol("a message that is not JSON"))?;
    let o = v.as_object().ok_or(Error::Protocol("a message that is not a JSON object"))?;
    if let Some(id) = o.get("id") {
        let id = id.as_u64().ok_or(Error::Protocol("an id that is not a number"))?;
        let outcome = match o.get("error") {
            Some(e) => Err(Error::Remote { code: e.get("code").and_then(Value::as_i64).unwrap_or(0), message: e.get("message").and_then(Value::as_str).unwrap_or("").to_string() }),
            None => Ok(o.get("result").cloned().unwrap_or_else(|| json!({}))),
        };
        return Ok(Got::Reply { id, outcome });
    }
    match o.get("method").and_then(Value::as_str) {
        Some(method) => Ok(Got::Event(Event {
            method: method.to_string(),
            params: o.get("params").cloned().unwrap_or_else(|| json!({})),
            session_id: o.get("sessionId").and_then(Value::as_str).map(str::to_string),
        })),
        None => Err(Error::Protocol("a message that is neither a reply nor an event")),
    }
}

impl Client {
    /// Connects to `ADDRESS:PORT` at `path`; the token, if any, goes into one `Authorization: Bearer` header and nowhere else.
    pub fn connect(addr: SocketAddr, path: &str, token: Option<&str>, limits: &Limits) -> Result<Client, Error> {
        let bearer = token.map(|t| format!("Bearer {t}"));
        let headers: Vec<(&str, &str)> = bearer.iter().map(|b| ("Authorization", b.as_str())).collect();
        let conn = lico_ws::connect(&lico_ws::Request { addr, path, headers: &headers }, limits).map_err(Error::Ws)?;
        Ok(Client { conn, next_id: 1, queue: VecDeque::new(), dropped: 0, stray: 0, dead: false })
    }

    /// Events dropped because the queue was full.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Replies nobody was waiting for (a late reply of a call that timed out, or an id this client never sent).
    pub fn stray(&self) -> u64 {
        self.stray
    }

    pub fn canceller(&self) -> Canceller {
        self.conn.canceller()
    }

    fn push(&mut self, e: Event) {
        if self.queue.len() >= MAX_QUEUED_EVENTS {
            self.queue.pop_front();
            self.dropped += 1;
        }
        self.queue.push_back(e);
    }

    /// Reads one message within `deadline`; a reply is handed to `want`, an event is queued.
    fn pump(&mut self, deadline: Instant) -> Result<Option<(u64, Result<Value, Error>)>, Error> {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::Timeout);
        }
        self.conn.set_read_timeout(Some(left));
        let text = match self.conn.recv() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Binary(_)) => return Err(self.fail(Error::Protocol("a binary message"))),
            Ok(Message::Close { .. }) => return Err(self.fail(Error::Closed)),
            Err(lico_ws::Error::Timeout) => return Err(Error::Timeout),
            Err(e) => return Err(self.fail(from_ws(e))),
        };
        match sort(&text) {
            Ok(Got::Reply { id, outcome }) => Ok(Some((id, outcome))),
            Ok(Got::Event(e)) => {
                self.push(e);
                Ok(None)
            }
            Err(e) => Err(self.fail(e)),
        }
    }

    fn fail(&mut self, e: Error) -> Error {
        self.dead = true;
        e
    }

    /// One call: sends it and waits for the reply with its id; events that arrive meanwhile are queued. `params` is an object (or `Null`, sent as `{}`).
    pub fn call(&mut self, method: &str, params: Value, session: Option<&str>, timeout: Duration) -> Result<Value, Error> {
        if self.dead {
            return Err(Error::Closed);
        }
        let params = match params {
            Value::Null => json!({}),
            p if p.is_object() => p,
            _ => return Err(Error::Protocol("params is not an object")),
        };
        let id = self.next_id;
        self.next_id += 1;
        let mut message = Map::new();
        message.insert("id".into(), json!(id));
        message.insert("method".into(), json!(method));
        message.insert("params".into(), params);
        if let Some(s) = session {
            message.insert("sessionId".into(), json!(s));
        }
        if let Err(e) = self.conn.send_text(&Value::Object(message).to_string()) {
            return Err(self.fail(from_ws(e)));
        }
        let deadline = Instant::now() + timeout;
        loop {
            match self.pump(deadline)? {
                Some((got, outcome)) if got == id => return outcome,
                Some(_) => self.stray += 1,
                None => {}
            }
        }
    }

    /// The next event: the oldest queued one, else what arrives within `timeout` (`Ok(None)` when nothing does).
    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<Event>, Error> {
        if let Some(e) = self.queue.pop_front() {
            return Ok(Some(e));
        }
        if self.dead {
            return Err(Error::Closed);
        }
        let deadline = Instant::now() + timeout;
        loop {
            match self.pump(deadline) {
                Ok(Some(_)) => self.stray += 1,
                Ok(None) => {
                    if let Some(e) = self.queue.pop_front() {
                        return Ok(Some(e));
                    }
                }
                Err(Error::Timeout) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }
}

// ---- the commands: names and the parameters they take (KNOWN FROM DOCUMENTATION: Chrome's; the engine may lack some) ------------------------------------------------------

pub mod method {
    pub const NAVIGATE: &str = "Page.navigate";
    pub const SET_VIEWPORT: &str = "Emulation.setDeviceMetricsOverride";
    pub const START_SCREENCAST: &str = "Page.startScreencast";
    pub const SCREENCAST_ACK: &str = "Page.screencastFrameAck";
    pub const SCREENCAST_FRAME: &str = "Page.screencastFrame";
    pub const CAPTURE_SCREENSHOT: &str = "Page.captureScreenshot";
    pub const MOUSE: &str = "Input.dispatchMouseEvent";
    pub const KEY: &str = "Input.dispatchKeyEvent";
    pub const INSERT_TEXT: &str = "Input.insertText";
    pub const CREATE_TARGET: &str = "Target.createTarget";
    pub const ATTACH: &str = "Target.attachToTarget";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Pressed,
    Released,
    Moved,
    Wheel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    None,
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Down,
    Up,
    Char,
}

pub mod cmd {
    use super::{Button, ImageFormat, KeyKind, MouseKind};
    use serde_json::{json, Value};

    fn format(f: ImageFormat) -> &'static str {
        match f {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
        }
    }

    pub fn navigate(url: &str) -> Value {
        json!({"url": url})
    }

    pub fn set_viewport(width: u32, height: u32, scale: f64) -> Value {
        json!({"width": width, "height": height, "deviceScaleFactor": scale, "mobile": false})
    }

    pub fn start_screencast(f: ImageFormat, quality: u32, max_width: u32, max_height: u32, every_nth: u32) -> Value {
        json!({"format": format(f), "quality": quality, "maxWidth": max_width, "maxHeight": max_height, "everyNthFrame": every_nth})
    }

    /// `frame_id` is the `sessionId` number of the `screencastFrame` event.
    pub fn screencast_ack(frame_id: u64) -> Value {
        json!({"sessionId": frame_id})
    }

    pub fn capture_screenshot(f: ImageFormat) -> Value {
        json!({"format": format(f)})
    }

    /// `delta` is the vertical wheel movement (the horizontal one is 0).
    pub fn mouse(kind: MouseKind, x: f64, y: f64, button: Button, clicks: u32, delta: f64) -> Value {
        let kind = match kind {
            MouseKind::Pressed => "mousePressed",
            MouseKind::Released => "mouseReleased",
            MouseKind::Moved => "mouseMoved",
            MouseKind::Wheel => "mouseWheel",
        };
        let button = match button {
            Button::None => "none",
            Button::Left => "left",
            Button::Middle => "middle",
            Button::Right => "right",
        };
        json!({"type": kind, "x": x, "y": y, "button": button, "clickCount": clicks, "deltaX": 0.0, "deltaY": delta})
    }

    /// `text` is left out of the parameters when it is empty.
    pub fn key(kind: KeyKind, key: &str, code: &str, text: &str) -> Value {
        let kind = match kind {
            KeyKind::Down => "keyDown",
            KeyKind::Up => "keyUp",
            KeyKind::Char => "char",
        };
        let mut v = json!({"type": kind, "key": key, "code": code});
        if !text.is_empty() {
            v["text"] = json!(text);
        }
        v
    }

    pub fn insert_text(text: &str) -> Value {
        json!({"text": text})
    }

    pub fn create_target(url: &str) -> Value {
        json!({"url": url})
    }

    pub fn attach(target_id: &str) -> Value {
        json!({"targetId": target_id, "flatten": true})
    }
}
