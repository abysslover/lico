//! `lico clipboard write`: put a text or a PNG picture on the CLIPBOARD through the window (plan M02-P04, S04b; XF-04, decisions L1 to L3 of mail 122).
//!
//! `lico clipboard write (--text STRING | --text-file FILE | --image FILE) [--timeout-ms N]`. The command is a client of the host: it sends the op
//! `clipboard.write` on the host socket. The HOST holds no X connection and cannot own a selection, so it hands the request to ONE window (the newest
//! connection that attached a view of kind `gui`) and answers when the window has taken the selection and said so with `clipboard.done` (`clipwrite.rs`).
//! A text travels in the request; a picture travels as an absolute PATH that the WINDOW opens with `open::open_regular`. Only CLIPBOARD is written.
//!
//! One JSON line, exit 0 on success, 1 for a usage error and 2 for any other failure (as `lico capture` and `lico clipboard read` do). The words for a
//! host that does not run are those of every command that needs the host: a line on standard error and exit 3 (cannot reach the host, the host closed
//! the connection) or 4 (timed out).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::open::{open_regular, read_checked, Fault};
use super::job::finish;
use super::ClipError;

/// The most bytes of a text, by `--text` and by `--text-file` (the limit of an `input`).
pub const TEXT_MAX: usize = 64 * 1024;
/// The deadline of the window's answer, by default and at most.
pub const TIMEOUT_DEFAULT_MS: u64 = 3000;
pub const TIMEOUT_CEILING_MS: u64 = 600_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Text(String),
    TextFile(PathBuf),
    Image(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Params {
    pub source: Source,
    pub timeout_ms: u64,
}

fn usage(m: impl Into<String>) -> ClipError {
    ClipError::new("usage", m)
}

fn absolute(flag: &str, text: &str) -> Result<PathBuf, ClipError> {
    if text.is_empty() {
        return Err(usage(format!("{flag} needs a file name")));
    }
    let path = Path::new(text);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir().map_err(|e| usage(format!("no working folder: {e}")))?.join(path))
}

/// The words after `clipboard write`.
pub fn parse_args(args: &[String]) -> Result<Params, ClipError> {
    let (mut source, mut timeout) = (None, None);
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        if !matches!(flag, "--text" | "--text-file" | "--image" | "--timeout-ms") {
            return Err(usage(format!("unknown argument {flag}")));
        }
        let value = args.get(i + 1).ok_or_else(|| usage(format!("{flag} needs a value")))?;
        match flag {
            "--timeout-ms" => {
                let n = if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) { value.parse::<u64>().ok() } else { None };
                match n {
                    Some(n) if (1..=TIMEOUT_CEILING_MS).contains(&n) && timeout.replace(n).is_none() => {}
                    Some(n) if (1..=TIMEOUT_CEILING_MS).contains(&n) => return Err(usage("--timeout-ms is given twice")),
                    _ => return Err(usage(format!("--timeout-ms needs a whole number from 1 to {TIMEOUT_CEILING_MS}"))),
                }
            }
            _ => {
                let made = match flag {
                    "--text" => {
                        if value.is_empty() {
                            return Err(usage("--text is empty: there is nothing to put on the clipboard"));
                        }
                        Source::Text(value.clone())
                    }
                    "--text-file" => Source::TextFile(absolute(flag, value)?),
                    _ => Source::Image(absolute(flag, value)?),
                };
                if source.replace(made).is_some() {
                    return Err(usage("give exactly one of --text, --text-file and --image, once"));
                }
            }
        }
        i += 2;
    }
    let source = source.ok_or_else(|| usage("give one of --text STRING, --text-file FILE and --image FILE"))?;
    Ok(Params { source, timeout_ms: timeout.unwrap_or(TIMEOUT_DEFAULT_MS) })
}

fn text_fault(f: Fault) -> ClipError {
    ClipError::new(f.reason(), f.detail())
}

/// The request line for the host (without the line end) and the kind. A text file is read here, by the safe open; a picture is NOT read here: its
/// path goes to the window, which opens it.
pub fn build_request(params: &Params) -> Result<(Value, &'static str), ClipError> {
    let timeout = params.timeout_ms;
    let (request, kind) = match &params.source {
        Source::Text(text) => (json!({"op": "clipboard.write", "kind": "text", "text": text, "timeout_ms": timeout}), "text"),
        Source::TextFile(path) => {
            let opened = open_regular(path, 0, TEXT_MAX as u64).map_err(text_fault)?;
            let bytes = read_checked(opened, TEXT_MAX as u64).map_err(text_fault)?;
            if bytes.is_empty() {
                return Err(usage("the text file is empty: there is nothing to put on the clipboard"));
            }
            let text = String::from_utf8(bytes).map_err(|_| ClipError::new("write_failed", "the text file is not UTF-8 text"))?;
            (json!({"op": "clipboard.write", "kind": "text", "text": text, "timeout_ms": timeout}), "text")
        }
        Source::Image(path) => {
            let Some(path) = path.to_str() else { return Err(usage("the file name is not UTF-8")) };
            (json!({"op": "clipboard.write", "kind": "image", "path": path, "timeout_ms": timeout}), "image")
        }
    };
    let text_bytes = request["text"].as_str().map_or(0, str::len);
    if text_bytes > TEXT_MAX {
        return Err(ClipError::new("too_large", format!("the text has {text_bytes} bytes, over {TEXT_MAX}")));
    }
    // the JSON line must fit what the host takes; control characters cost 6 bytes each, so a text of 64 KiB of them does not fit
    let line = request.to_string().len() + 1;
    if line > crate::core::MAX_REQUEST {
        return Err(ClipError::new("too_large", format!("the request is {line} bytes after the JSON escape, over {}", crate::core::MAX_REQUEST)));
    }
    Ok((request, kind))
}

fn die(code: i32, message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(code);
}

/// Sends the request and waits for its reply (events that come first are skipped). A host that cannot be reached, closes the connection or does not
/// answer in time ends the command as the other commands do.
fn exchange(socket: &str, request: &Value, timeout_ms: u64) -> Value {
    let stream = UnixStream::connect(socket).unwrap_or_else(|e| die(3, &format!("cannot reach the host: {e}")));
    // the host's own deadline is `timeout_ms`; this one is later, so the host's answer (`timeout`) is the one that is read
    stream.set_read_timeout(Some(Duration::from_millis(timeout_ms.saturating_add(5000)))).ok();
    let mut writer = stream.try_clone().unwrap_or_else(|e| die(3, &format!("cannot reach the host: {e}")));
    let mut reader = BufReader::new(stream);
    let mut text = request.to_string();
    text.push('\n');
    writer.write_all(text.as_bytes()).unwrap_or_else(|e| die(3, &format!("write failed: {e}")));
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => die(3, "the host closed the connection"),
            Ok(_) => match serde_json::from_str::<Value>(&line) {
                Ok(message) if message.get("ok").is_some() => return message,
                _ => continue,
            },
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => die(4, "timed out"),
            Err(_) => die(3, "the host closed the connection"),
        }
    }
}

/// What a reply of the host means: the answer line, or the error.
pub fn answer_of(reply: &Value, kind: &str) -> Result<Value, ClipError> {
    if reply["ok"] == json!(1) {
        let mut out = json!({"ok": 1, "kind": kind, "bytes": reply["bytes"]});
        if kind == "image" {
            out["width"] = reply["width"].clone();
            out["height"] = reply["height"].clone();
        }
        return Ok(out);
    }
    let detail = match reply["error"].as_str().unwrap_or("") {
        "no_window" => ("no_window", "no window is attached to the host: the clipboard is written by the window"),
        "timeout" => ("timeout", "the window did not confirm in time"),
        "too_large" => ("too_large", "the text or the picture is too large"),
        "bad_image" => ("bad_image", "the file is not a PNG picture the clipboard takes"),
        "write_failed" => ("write_failed", "the file cannot be opened or read"),
        "x_error" => ("x_error", "the window could not take the selection"),
        // the host's own words for a request it does not know or refuses (a host of an older build has no `clipboard.write`)
        "unknown_op" => ("unknown_op", "the host does not know this request (it runs an older build)"),
        other => return Err(ClipError::new("bad_request", format!("the host refused the request ({other})"))),
    };
    Err(ClipError::new(detail.0, detail.1))
}

/// The words after `clipboard write`: prints one JSON line and exits.
pub fn command(args: &[String], socket: &str) -> ! {
    let result = parse_args(args).and_then(|params| {
        let (request, kind) = build_request(&params)?;
        if socket.is_empty() {
            return Err(usage("no host socket: give --socket PATH or set LICO_SOCKET"));
        }
        answer_of(&exchange(socket, &request, params.timeout_ms), kind)
    });
    finish(result)
}
