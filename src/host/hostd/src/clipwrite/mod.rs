//! The host's part of `lico clipboard write` (plan M02-P04, S04b; XF-04, decision L1 of mail 122).
//!
//! The host holds no X connection and cannot own a selection (S03), so a write is a relay. The op `clipboard.write` (from the command) is forwarded as
//! the pushed line `{"ev":"clip.write","req":ID,"kind":"text"|"image","text"|"path":...}` to ONE window, the newest connection that attached a view of
//! kind `gui`, and the host answers the command when that window confirms with the op `clipboard.done` (carrying the request id the host gave). The
//! window takes the selection first and confirms only when `get_selection_owner(CLIPBOARD)` says its helper window owns it.
//!
//! - no window attached: `no_window` at once, no waiting;
//! - no confirmation within `timeout_ms` (default 3000, 1 to 600000): `timeout`, and the request is forgotten, so a late confirmation finds nothing;
//! - a confirmation with an unknown id, or from a connection other than the one that was asked, is refused (`unknown_request`) and changes nothing;
//! - the window's connection ends while the host waits: `no_window`;
//! - the host keeps no copy of the text or of the path after the answer (the pushed line is consumed by the window; the table holds only the
//!   connection and a channel).
//!
//! The wait happens in the thread of the connection of the command, which has nothing else to do; the window's confirmation arrives on another
//! connection's thread and ends the wait through a channel.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::{random_hex, Reply, Shared};
use crate::session::{encode, obj, Sub};

#[cfg(test)]
mod tests;

/// The deadline of the window's confirmation, by default and at most, in milliseconds.
pub const TIMEOUT_DEFAULT_MS: i64 = 3000;
pub const TIMEOUT_CEILING_MS: i64 = 600_000;

/// What the window said.
enum Answer {
    Done(Map<String, Value>),
    Failed(&'static str),
}

struct Waiting {
    conn: Arc<Sub>,
    tx: mpsc::Sender<Answer>,
}

/// The requests that wait for a window's confirmation, and the order in which views of kind `gui` were attached.
#[derive(Default)]
pub struct Relay {
    waiting: Mutex<HashMap<String, Waiting>>,
    order: AtomicU64,
}

impl Relay {
    /// How many requests are waiting (the tests read it: a finished request leaves nothing).
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        self.waiting.lock().unwrap().len()
    }
}

/// Records that the view `view` is of kind `gui`, and the newest. Called right after `view.attach` with that kind.
pub fn mark_gui(shared: &Shared, view: &str) {
    let seq = shared.clip.order.fetch_add(1, Ordering::Relaxed) + 1;
    if let Some(entry) = shared.views.lock().unwrap().get_mut(view) {
        entry.gui_seq = Some(seq);
    }
}

/// The connection of the newest window: of the views of kind `gui` that still have a live connection, the one attached last.
fn newest_window(shared: &Shared) -> Option<Arc<Sub>> {
    let views = shared.views.lock().unwrap();
    views
        .values()
        .filter_map(|v| match (&v.conn, v.gui_seq) {
            (Some(conn), Some(seq)) if !conn.closed.load(Ordering::Relaxed) => Some((seq, conn.clone())),
            _ => None,
        })
        .max_by_key(|(seq, _)| *seq)
        .map(|(_, conn)| conn)
}

/// The op `clipboard.write`: forward to the newest window and wait for its confirmation. The request was checked by `ipc::valid_request`.
pub fn write(shared: &Shared, request: &Map<String, Value>) -> Reply {
    let Some(window) = newest_window(shared) else { return Err("no_window") };
    let timeout = Duration::from_millis(request.get("timeout_ms").and_then(Value::as_i64).unwrap_or(TIMEOUT_DEFAULT_MS).clamp(1, TIMEOUT_CEILING_MS) as u64);
    let id = format!("c{}", random_hex(8));
    let (tx, rx) = mpsc::channel();
    shared.clip.waiting.lock().unwrap().insert(id.clone(), Waiting { conn: window.clone(), tx });
    let mut line = obj(json!({"ev": "clip.write", "req": id, "kind": request.get("kind").cloned().unwrap_or(Value::Null)}));
    for key in ["text", "path"] {
        if let Some(value) = request.get(key) {
            line.insert(key.to_string(), value.clone());
        }
    }
    window.push(encode(line));
    let result = wait(&rx, &window, timeout);
    // the request is forgotten whatever happened: a late confirmation finds nothing
    shared.clip.waiting.lock().unwrap().remove(&id);
    result
}

fn wait(rx: &mpsc::Receiver<Answer>, window: &Arc<Sub>, timeout: Duration) -> Reply {
    let end = Instant::now() + timeout;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("timeout");
        }
        // short steps, so that a window whose connection ended is noticed
        match rx.recv_timeout(left.min(Duration::from_millis(100))) {
            Ok(Answer::Done(body)) => return Ok(body),
            Ok(Answer::Failed(code)) => return Err(code),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if window.closed.load(Ordering::Relaxed) {
                    return Err("no_window");
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("no_window"),
        }
    }
}

/// The reasons a window may give; anything else is taken as `x_error`.
fn reason_of(text: &str) -> &'static str {
    match text {
        "too_large" => "too_large",
        "bad_image" => "bad_image",
        "write_failed" => "write_failed",
        _ => "x_error",
    }
}

/// The op `clipboard.done`: the window's confirmation (or failure) of the request `req`. Only the connection that was asked is heard.
pub fn done(shared: &Shared, from: &Arc<Sub>, request: &Map<String, Value>) -> Reply {
    let id = request.get("req").and_then(Value::as_str).unwrap_or_default();
    let mut waiting = shared.clip.waiting.lock().unwrap();
    match waiting.get(id) {
        Some(w) if Arc::ptr_eq(&w.conn, from) => {}
        _ => return Err("unknown_request"),
    }
    let w = waiting.remove(id).expect("checked above");
    drop(waiting);
    let answer = if request.get("ok").and_then(Value::as_bool) == Some(true) {
        let mut body = Map::new();
        for key in ["kind", "bytes", "width", "height"] {
            if let Some(value) = request.get(key) {
                body.insert(key.to_string(), value.clone());
            }
        }
        Answer::Done(body)
    } else {
        Answer::Failed(reason_of(request.get("error").and_then(Value::as_str).unwrap_or_default()))
    };
    let _ = w.tx.send(answer); // the waiter may have timed out a moment ago: nothing to do
    Ok(obj(json!({"req": id})))
}
