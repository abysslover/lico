//! `lico clipboard read` and the hidden job `clipboard_read` (the same function): the clipboard as one JSON line (plan M02-P04, S04a; XF-04).
//!
//! `lico clipboard read [--selection clipboard|primary] [--kind text|image|auto] [--out FILE] [--max-bytes N] [--timeout-ms N]`. One JSON line, exit
//! 0 on success, 1 for a usage error and 2 for any other failure (as `lico capture` does). A text is in the line (JSON-escaped); a picture goes to
//! `--out`, which is written with the safe writer of `capture::out` (a temporary file in the same folder, create-new, mode 0600, rename; nothing is
//! left after a failure), and the line carries its absolute path and the size from the PNG header. The core, `run`, takes its arguments and returns
//! the answer or the error as values; only `command` prints.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{read_selection, Clip, ClipError, ReadLimits, Selection, Want};
use crate::capture::out::Pending;

/// The most bytes of a text, by default.
pub const TEXT_DEFAULT: usize = 64 * 1024;
/// The most bytes of a picture, by default.
pub const IMAGE_DEFAULT: usize = 3 * 1024 * 1024;
/// The most a caller may ask for with `--max-bytes`: the answer is held in memory.
pub const MAX_BYTES_CEILING: usize = 64 * 1024 * 1024;
/// The deadline of the whole read, by default and at most.
pub const TIMEOUT_DEFAULT_MS: u64 = 2000;
pub const TIMEOUT_CEILING_MS: u64 = 600_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Params {
    pub selection: Selection,
    pub want: Want,
    pub out: Option<PathBuf>,
    pub max_bytes: Option<usize>,
    pub timeout_ms: u64,
}

fn usage(m: impl Into<String>) -> ClipError {
    ClipError::new("usage", m)
}

fn number(flag: &str, text: &str, ceiling: u64) -> Result<u64, ClipError> {
    let n = if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) { text.parse::<u64>().ok() } else { None };
    match n {
        Some(n) if (1..=ceiling).contains(&n) => Ok(n),
        _ => Err(usage(format!("{flag} needs a whole number from 1 to {ceiling}"))),
    }
}

/// The words after `clipboard read`.
pub fn parse_args(args: &[String]) -> Result<Params, ClipError> {
    let (mut selection, mut kind, mut out, mut max_bytes, mut timeout) = (None, None, None, None, None);
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        if !matches!(flag, "--selection" | "--kind" | "--out" | "--max-bytes" | "--timeout-ms") {
            return Err(usage(format!("unknown argument {flag}")));
        }
        let value = args.get(i + 1).ok_or_else(|| usage(format!("{flag} needs a value")))?;
        let doubled = || usage(format!("{flag} is given twice"));
        match flag {
            "--selection" => {
                let s = match value.as_str() {
                    "clipboard" => Selection::Clipboard,
                    "primary" => Selection::Primary,
                    _ => return Err(usage("--selection is clipboard or primary")),
                };
                if selection.replace(s).is_some() {
                    return Err(doubled());
                }
            }
            "--kind" => {
                let k = match value.as_str() {
                    "text" => Want::Text,
                    "image" => Want::Image,
                    "auto" => Want::Auto,
                    _ => return Err(usage("--kind is text, image or auto")),
                };
                if kind.replace(k).is_some() {
                    return Err(doubled());
                }
            }
            "--out" => {
                if value.is_empty() {
                    return Err(usage("--out needs a file name"));
                }
                if out.replace(value.clone()).is_some() {
                    return Err(doubled());
                }
            }
            "--max-bytes" => {
                if max_bytes.replace(number(flag, value, MAX_BYTES_CEILING as u64)? as usize).is_some() {
                    return Err(doubled());
                }
            }
            _ => {
                if timeout.replace(number(flag, value, TIMEOUT_CEILING_MS)?).is_some() {
                    return Err(doubled());
                }
            }
        }
        i += 2;
    }
    let want = kind.unwrap_or(Want::Text);
    let out = match (want, out) {
        (Want::Text, Some(_)) => return Err(usage("--out is for --kind image and --kind auto; a text is in the answer")),
        (Want::Text, None) => None,
        (_, None) => return Err(usage("--out FILE is required for --kind image and --kind auto")),
        (_, Some(o)) => {
            let o = Path::new(&o);
            Some(if o.is_absolute() { o.to_path_buf() } else { std::env::current_dir().map_err(|e| usage(format!("no working folder: {e}")))?.join(o) })
        }
    };
    Ok(Params { selection: selection.unwrap_or(Selection::Clipboard), want, out, max_bytes, timeout_ms: timeout.unwrap_or(TIMEOUT_DEFAULT_MS) })
}

impl Params {
    pub fn limits(&self) -> ReadLimits {
        ReadLimits {
            text_max: self.max_bytes.unwrap_or(TEXT_DEFAULT),
            image_max: self.max_bytes.unwrap_or(IMAGE_DEFAULT),
            deadline: Duration::from_millis(self.timeout_ms),
        }
    }
}

fn write_failed(e: std::io::Error) -> ClipError {
    ClipError::new("write_failed", e.to_string())
}

/// Writes the picture the safe way; the folder of `dest` must exist and nothing is left behind after an error.
pub(crate) fn save(dest: &Path, bytes: &[u8]) -> Result<(), ClipError> {
    let (pending, mut file) = Pending::create(dest).map_err(write_failed)?;
    file.write_all(bytes).and_then(|_| file.flush()).map_err(write_failed)?;
    drop(file);
    pending.commit().map_err(write_failed)
}

/// The core: the answer or the error as values. `display` is the display name (`None`: `$DISPLAY`).
pub fn run(args: &[String], display: Option<&str>) -> Result<Value, ClipError> {
    let params = parse_args(args)?;
    let read = read_selection(display, params.selection, params.want, &params.limits())?;
    let selection = params.selection.name();
    match read.clip {
        Clip::Text(text) => Ok(json!({"ok": 1, "kind": "text", "selection": selection, "text": text, "bytes": text.len()})),
        Clip::Png { bytes, width, height } => {
            let dest = params.out.expect("an image read has --out (checked by parse_args)");
            save(&dest, &bytes)?;
            Ok(json!({"ok": 1, "kind": "image", "selection": selection, "path": dest.to_string_lossy(), "bytes": bytes.len(), "width": width, "height": height}))
        }
    }
}

/// `lico clipboard ...`: the words after `clipboard`, and the host socket (`write` is a client of the host; `read` needs no host).
pub fn cli(args: &[String], socket: &str) -> ! {
    match args.first().map(String::as_str) {
        Some("read") => read_command(&args[1..]),
        Some("write") => super::write::command(&args[1..], socket),
        Some(other) => finish(Err(usage(format!("unknown clipboard command {other} (read or write)")))),
        None => finish(Err(usage("clipboard needs a command (read or write)"))),
    }
}

/// The words after `clipboard read` (also the hidden job `clipboard_read`): prints one JSON line and exits.
pub fn read_command(args: &[String]) -> ! {
    finish(run(args, None))
}

/// The answer as ONE line for any reader of lines: JSON leaves U+0085, U+2028 and U+2029 as they are, and a line splitter of a language cuts at them, so
/// they are written as escapes (they can only be inside a string, where the escape means the same character).
pub(crate) fn one_line(answer: &Value) -> String {
    answer.to_string().replace('\u{85}', "\\u0085").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
}

pub(super) fn finish(result: Result<Value, ClipError>) -> ! {
    let (line, code) = match &result {
        Ok(answer) => (answer.clone(), 0),
        Err(e) => (json!({"ok": 0, "error": e.reason, "detail": e.detail}), if e.reason == "usage" { 1 } else { 2 }),
    };
    println!("{}", one_line(&line));
    std::process::exit(code);
}
