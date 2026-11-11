//! The agent tools `capture`, `clipboard_read` and `clipboard_write` on the tool connection, with the person's approval (plan M02-P04, S07a; SF-06, XF-02, XF-04).
//!
//! APPROVAL PATH (D5): the tools use the host's path of `send` and `history`. `pending::ask` puts a card on the AGENT'S OWN row, the person answers it in the window
//! (Y or N) or with the socket op `approval.answer`, `pending::answer_at` calls `calls::perform` after every lock is dropped, and the agent is told by a notice of the
//! host. Nothing waits inside the HTTP call (the connection has a 5 s deadline).
//!
//! - `clipboard_write` has the form of `send`: the yes PERFORMS (the relay `clipwrite`, text only). It is listed and callable only while the setting is on, which is
//!   read at `tools/list`, at the call and again when the person says yes. The relay waits for the window's confirmation; the window that was answered is the one
//!   that must confirm, and it stands still while it answers, so the relay runs on a thread of its own and a second notice says how it went.
//! - `capture` and `clipboard_read` have TWO calls. The person's data goes TO the agent as the answer of a tool call (an image block), which a notice cannot
//!   carry. Call 1 asks (arguments are checked first); the yes stores a single-use GRANT (`grants`) on the asking session; call 2 with the same canonical target
//!   takes the grant BEFORE the work starts and does the work in the helper process (`work`). A different target needs its own yes.

pub mod grants;
pub(crate) mod work;
#[cfg(test)]
mod tests;

use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::calls::{card_line, counted, window, Window};
use crate::capture::{self, Kind};
use crate::core::Shared;
use crate::leads;
use crate::mcp::Answer;
use crate::pending::{self, Asked, Request};

pub use work::prepare;

/// The most bytes of text one `clipboard_write` carries.
pub const TEXT_MAX: usize = 64 * 1024;

/// Whether `tool` is one whose yes allows a later call instead of doing the thing (the words of the notice differ).
pub fn is_tool(tool: &str) -> bool {
    matches!(tool, "capture" | "clipboard_read" | "clipboard_write" | "computer")
}

/// What a call asks for, after its arguments were checked.
struct Spec {
    tool: &'static str,
    /// The canonical form of the arguments: what a grant is matched on.
    target: String,
    /// How the person's request is named in the words the agent is told with.
    who: String,
    /// The line on the card.
    card: String,
    /// What call 2 does.
    job: Job,
}

enum Job {
    Capture { kind: &'static str, args: Vec<String> },
    Read { kind: String },
}

const CAPTURE_USAGE: &str = "bad_request: capture takes one optional `target`: `screen` (when left out), `window:<id>` (a decimal or 0x number) or `region:x,y,w,h` (four whole numbers)";
const READ_USAGE: &str = "bad_request: clipboard_read takes one optional `kind`: `text`, `image` or `auto` (when left out)";

/// A string argument that may be left out; a value that is not a string, or an empty one, is a bad request.
fn optional<'a>(arguments: &'a Map<String, Value>, name: &str, usage: &str) -> Result<Option<&'a str>, String> {
    match arguments.get(name) {
        None => Ok(None),
        Some(Value::String(s)) if !s.is_empty() && s.len() <= 64 => Ok(Some(s)),
        Some(_) => Err(usage.to_string()),
    }
}

fn capture_spec(arguments: &Map<String, Value>) -> Result<Spec, String> {
    let target = optional(arguments, "target", CAPTURE_USAGE)?.unwrap_or("screen");
    let mut words: Vec<String> = match target.split_once(':') {
        None if target == "screen" => vec!["--screen".into()],
        Some(("window", v)) => vec!["--window".into(), v.into()],
        Some(("region", v)) => vec!["--region".into(), v.into()],
        _ => return Err(CAPTURE_USAGE.into()),
    };
    words.extend(["--out".into(), "/unused".into()]); // the parser of the command wants one; the host chooses its own at call 2
    let request = capture::parse_args(&words).map_err(|_| CAPTURE_USAGE.to_string())?;
    let (canonical, args, kind, who, sees) = match request.kind {
        Kind::Screen => ("screen".to_string(), vec!["--screen".to_string()], "screen", "capture of the whole screen".to_string(), "what is on it"),
        Kind::Window(id) => (format!("window:{id}"), vec!["--window".into(), id.to_string()], "window", format!("capture of window {id}"), "what is in it"),
        Kind::Region { x, y, w, h } => {
            if w < 1 || h < 1 || x < 0 || y < 0 {
                return Err(CAPTURE_USAGE.into()); // never a region that cannot be on a screen: nothing is asked for it
            }
            (format!("region:{x},{y},{w},{h}"), vec!["--region".into(), format!("{x},{y},{w},{h}")], "region", format!("capture of the region {x},{y},{w},{h} of the screen"), "what is in it")
        }
    };
    let card = format!("{who}: the agent will see {sees}");
    Ok(Spec { tool: "capture", target: canonical, who, card, job: Job::Capture { kind, args } })
}

fn read_spec(arguments: &Map<String, Value>) -> Result<Spec, String> {
    let kind = optional(arguments, "kind", READ_USAGE)?.unwrap_or("auto");
    let what = match kind {
        "text" => "text",
        "image" => "picture",
        "auto" => "text or picture",
        _ => return Err(READ_USAGE.into()),
    };
    Ok(Spec {
        tool: "clipboard_read",
        target: kind.to_string(),
        who: format!("read of the person's clipboard ({what})"),
        card: format!("read of your clipboard ({what}): the agent will see it"),
        job: Job::Read { kind: kind.to_string() },
    })
}

/// The tool `capture`.
pub fn capture(shared: &Arc<Shared>, caller: &str, arguments: &Map<String, Value>) -> Result<Answer, String> {
    two_calls(shared, caller, arguments, capture_spec, window(), Instant::now(), pending::held_text(), &shared.cfg.job_bin)
}

/// The tool `clipboard_read`.
pub fn clipboard_read(shared: &Arc<Shared>, caller: &str, arguments: &Map<String, Value>) -> Result<Answer, String> {
    two_calls(shared, caller, arguments, read_spec, window(), Instant::now(), pending::held_text(), &shared.cfg.job_bin)
}

/// A call of `capture` or `clipboard_read`: with a live grant for this target the work is done (the grant is used up first); without one the person is asked.
#[allow(clippy::too_many_arguments)]
fn two_calls(
    shared: &Arc<Shared>, caller: &str, arguments: &Map<String, Value>, spec_of: fn(&Map<String, Value>) -> Result<Spec, String>, window: &Mutex<Window>, at: Instant,
    held: &Arc<AtomicUsize>, bin: &Path,
) -> Result<Answer, String> {
    pending::sweep(shared, caller);
    counted(window, caller, at)?;
    let spec = spec_of(arguments)?; // a bad argument is refused here: nothing is asked
    let now = pending::now();
    if has_grant(shared, caller, spec.tool, &spec.target, now) {
        // one such work at a time: a second at once is busy and keeps the grant for the next call
        let Some(_one) = work::try_lock() else { return Err(work::busy()) };
        if take_grant(shared, caller, spec.tool, &spec.target, now) {
            let dir = work::captures_dir(&shared.cfg.handoff_dir);
            return match &spec.job {
                Job::Capture { kind, args } => work::capture(bin, dir, kind, args),
                Job::Read { kind } => work::read(bin, dir, kind),
            };
        }
    }
    let asked = pending::ask(shared, caller, &Request { tool: spec.tool, target: &spec.target, who: &spec.who, card: &spec.card, text: "" }, now, held)?;
    let (id, again) = match asked {
        Asked::New(id) => (id, false),
        Asked::Again(id) => (id, true),
    };
    let mut reply = json!({
        "status": "pending", "approval_id": id,
        "message": format!(
            "Nothing was {} yet. The person has been asked to allow this {}. End your turn now; Lico will tell you here when they decide, and then call {} again with the same arguments. A restart of Lico drops the request.",
            if spec.tool == "capture" { "captured" } else { "read" }, spec.who, spec.tool
        ),
    });
    if again {
        reply["already_pending"] = json!(true);
    }
    Ok(Answer::Text(reply.to_string()))
}

fn has_grant(shared: &Shared, caller: &str, tool: &str, target: &str, now: u64) -> bool {
    crate::core::find(shared, caller).is_some_and(|found| found.lock().unwrap().pending.grants.has(tool, target, now))
}

fn take_grant(shared: &Shared, caller: &str, tool: &str, target: &str, now: u64) -> bool {
    crate::core::find(shared, caller).is_some_and(|found| found.lock().unwrap().pending.grants.take(tool, target, now))
}

fn give_grant(shared: &Shared, caller: &str, tool: &'static str, target: &str, now: u64) -> bool {
    crate::core::find(shared, caller).map(|found| found.lock().unwrap().pending.grants.give(tool, target, now)).is_some()
}

/// The tool `clipboard_write`: the same form as `send`. Only text; it is cleaned as a `send` is, once, and the card, the pages and the write all get this string.
pub fn clipboard_write(shared: &Arc<Shared>, caller: &str, arguments: &Map<String, Value>) -> Result<Answer, String> {
    write_call(shared, caller, arguments, window(), Instant::now(), pending::held_text())
}

fn write_call(shared: &Arc<Shared>, caller: &str, arguments: &Map<String, Value>, window: &Mutex<Window>, at: Instant, held: &Arc<AtomicUsize>) -> Result<Answer, String> {
    pending::sweep(shared, caller);
    counted(window, caller, at)?;
    let usage = || "bad_request: clipboard_write needs `text` (not empty, and not only characters that draw nothing)".to_string();
    let text = arguments.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()).ok_or_else(usage)?;
    let cleaned = leads::agent_text(text);
    let (text, removed) = (cleaned.text.as_str(), cleaned.removed);
    if text.is_empty() || crate::folders::shows_nothing(text) {
        return Err(usage());
    }
    if text.len() > TEXT_MAX {
        return Err(format!("too_large: the text has {} bytes and the clipboard takes {TEXT_MAX} at most; it is refused, not cut", text.len()));
    }
    let (card, who) = (card_line(&format!("write {} bytes to your clipboard", text.len()), text, removed), format!("write of {} bytes to the person's clipboard", text.len()));
    let asked = pending::ask(shared, caller, &Request { tool: "clipboard_write", target: "clipboard", who: &who, card: &card, text }, pending::now(), held)?;
    let (id, again) = match asked {
        Asked::New(id) => (id, false),
        Asked::Again(id) => (id, true),
    };
    let mut reply = json!({
        "status": "pending", "approval_id": id,
        "message": format!("Nothing was written yet. The person has been asked to approve this {who}. Lico will tell you here when they decide; you can end your turn. A restart of Lico drops the request."),
    });
    if again {
        reply["already_pending"] = json!(true);
    }
    Ok(Answer::Text(reply.to_string()))
}

/// What the person's yes does for one of the three tools. Err: the refusal the host gives now, in the words the model reads.
pub(crate) fn perform(shared: &Arc<Shared>, caller: &str, id: i64, tool: &str, target: &str, who: &str, text: &str) -> Result<String, String> {
    match tool {
        "capture" | "clipboard_read" => {
            let tool: &'static str = if tool == "capture" { "capture" } else { "clipboard_read" };
            if !give_grant(shared, caller, tool, target, pending::now()) {
                return Err("session_exited: this session has ended".into());
            }
            let (nothing, minutes) = (if tool == "capture" { "captured" } else { "read" }, grants::VALID_SECS / 60);
            Ok(format!("the {who} was allowed once. Nothing was {nothing} yet: call {tool} again with the same arguments within {minutes} minutes; Lico takes it at that call."))
        }
        "clipboard_write" => {
            // the third check of three: the person may have switched writing off after the call
            if !crate::clipboard::settings::clipboard_write_enabled(&crate::clipboard::settings::settings_path(&shared.cfg.handoff_dir)) {
                return Err("not_allowed: clipboard writing was switched off; nothing was written".into());
            }
            if !crate::clipwrite::has_window(shared) {
                return Err(format!("no_window: {}", write_words("no_window")));
            }
            let (shared, caller, text, who_moved) = (shared.clone(), caller.to_string(), text.to_string(), who.to_string());
            let spawned = std::thread::Builder::new().name("clipboard-write".into()).spawn(move || {
                let words = match relay(&shared, &text) {
                    Ok(()) => format!("done: the {who_moved} succeeded; the clipboard holds it now"),
                    Err(code) => format!("not done: {code}: {}", write_words(code)),
                };
                pending::notify(&shared, &caller, id, &words);
            });
            match spawned {
                Ok(_) => Ok(format!("the {who} was handed to the person's window; Lico tells you here in a next message whether it worked")),
                Err(_) => Err("busy: the host could not start the write just now; try again in a moment".into()),
            }
        }
        _ => Err("refused: the host refused it".into()),
    }
}

/// The write through the window that owns the selection: the request the command `lico clipboard write` makes, from inside the host.
fn relay(shared: &Shared, text: &str) -> Result<(), &'static str> {
    let mut request = Map::new();
    request.insert("kind".into(), json!("text"));
    request.insert("text".into(), json!(text));
    request.insert("timeout_ms".into(), json!(crate::clipwrite::TIMEOUT_DEFAULT_MS));
    crate::clipwrite::write(shared, &request).map(|_| ())
}

/// What to do about a reason of the relay, in the words the model reads.
fn write_words(code: &str) -> &'static str {
    match code {
        "no_window" => "no window of Lico is open to take the text; ask the person to open one",
        "timeout" => "the window did not confirm in time; the text may or may not be on the clipboard",
        "too_large" => "the text is too large for the clipboard",
        "write_failed" => "the window could not use the text",
        "bad_image" => "the window could not use the text",
        _ => "the window could not take the selection",
    }
}
