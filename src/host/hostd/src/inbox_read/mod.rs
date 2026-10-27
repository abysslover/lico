//! The tool `read_handoff` (S07): a file the host stored in the caller's own inbox (`<handoff dir>/<caller id>`, see `handoff`), read in parts so that a long text does not
//! send the agent to its shell. It needs no approval: the folder is the caller's own, the tool only reads, and a part is at most `PART` bytes.
//!
//! The path is taken apart by components and nothing is looked at before that: it must lie below the caller's inbox (an absolute path as a notice gives it, or a path
//! relative to the inbox), without `..`. Then the walk goes down from the inbox one name at a time and refuses a link at every hop, the file is opened without following
//! a link and without waiting for a pipe, and the handle that was opened (not the name) must be a regular file, the one the walk saw. A path outside the inbox is refused the
//! same way whether the file there exists or not, so the tool cannot be used to look for files. This keeps the other sessions' inboxes apart; it is not a sandbox (SF-08):
//! a program with a shell reads whatever the user can.

use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use crate::core::Shared;
use crate::handoff::{KEEP_BYTES, KEEP_DAYS};
use crate::provider_record::{O_NOFOLLOW, O_NONBLOCK};

/// The most bytes one call returns.
pub const PART: usize = 32 * 1024;
const LEAST: usize = 4; // a part holds one character at the least, whatever the limit
const PATH_MAX: usize = 4096;
const ELOOP: i32 = 40;

/// Why a read was refused, before it has the words the model reads.
#[derive(Debug, PartialEq)]
enum Fault {
    Needs,
    Outside,
    Link,
    Special,
    Folder,
    Gone,
    Past(u64),
    Inside,
    Busy,
}

impl Fault {
    fn words(&self) -> String {
        match self {
            Fault::Needs => "bad_request: read_handoff needs `path` (a file in your handoff folder, as a message from the host gave it) and may have `offset` and `limit`, whole numbers of bytes".into(),
            Fault::Outside => "not_allowed: read_handoff reads files in your own handoff folder only".into(),
            Fault::Link => "not_allowed: a link on that path is not followed".into(),
            Fault::Special => "not_allowed: that is not a plain file".into(),
            Fault::Folder => "bad_request: that is a folder, not a file".into(),
            Fault::Gone => format!("not_found: there is no such file in your handoff folder; it may have been removed by the retention rules: files are kept for at most {KEEP_DAYS} days and {} MiB in all", KEEP_BYTES >> 20),
            Fault::Past(size) => format!("bad_request: offset is past the end of the file, which is {size} bytes"),
            Fault::Inside => "bad_request: offset is inside a character; start at `next` of the last answer".into(),
            Fault::Busy => "busy: the file could not be read just now; try again in a moment".into(),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Fault::Needs | Fault::Folder | Fault::Past(_) | Fault::Inside => "bad_request",
            Fault::Outside | Fault::Link | Fault::Special => "not_allowed",
            Fault::Gone => "not_found",
            Fault::Busy => "busy",
        }
    }
}

/// The tool `read_handoff`, for the session that holds the token.
pub fn read_handoff(shared: &Shared, caller: &str, arguments: &Map<String, Value>) -> Result<String, String> {
    let done = read_in(&shared.cfg.handoff_dir, caller, arguments);
    eprintln!("{}", audit_line(caller, done.as_ref().map_or_else(|f| f.name(), |_| "read")));
    done.map_err(|fault| fault.words())
}

fn read_in(root: &Path, caller: &str, arguments: &Map<String, Value>) -> Result<String, Fault> {
    let path = arguments.get("path").and_then(Value::as_str).filter(|p| !p.is_empty() && p.len() <= PATH_MAX && !p.contains('\0')).ok_or(Fault::Needs)?;
    let offset = number(arguments, "offset").map_err(|()| Fault::Needs)?.unwrap_or(0);
    let limit = number(arguments, "limit").map_err(|()| Fault::Needs)?;
    if limit == Some(0) || !crate::registry::valid_session_id(caller) {
        return Err(Fault::Needs);
    }
    let names = below(&root.join(caller), path)?;
    let (at, seen) = walk(root, caller, &names)?;
    let (file, size) = open_seen(&at, &seen)?;
    let (text, end) = part(file, size, offset, limit.map_or(PART, |l| (l.min(PART as u64) as usize).max(LEAST)))?;
    Ok(json!({"offset": offset, "end": end, "size": size, "next": (end < size).then_some(end), "text": text}).to_string())
}

/// A whole number given as a number or as digits (a program passes both); none when it is left out or null.
pub(crate) fn number(arguments: &Map<String, Value>, name: &str) -> Result<Option<u64>, ()> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_u64().map(Some).ok_or(()),
        Some(Value::String(s)) if !s.is_empty() && s.len() <= 19 && s.bytes().all(|b| b.is_ascii_digit()) => s.parse().map(Some).map_err(|_| ()),
        Some(_) => Err(()),
    }
}

/// The names of `path` below `inbox`: a path that begins with the inbox, or one relative to it; no `..`, nothing that is not a plain name.
fn below(inbox: &Path, path: &str) -> Result<Vec<OsString>, Fault> {
    let given = Path::new(path);
    let relative = if given.is_absolute() { given.strip_prefix(inbox).map_err(|_| Fault::Outside)? } else { given };
    let mut names = Vec::new();
    for part in relative.components() {
        match part {
            Component::Normal(name) => names.push(name.to_os_string()),
            Component::CurDir => {} // `components` gives one only at the start of a path
            _ => return Err(Fault::Outside),
        }
    }
    if names.is_empty() {
        return Err(Fault::Folder); // the inbox itself
    }
    Ok(names)
}

/// Walk down from `root` through the inbox of `caller` and `names`, a link refused at every hop: the path of the file and what the walk saw of it.
fn walk(root: &Path, caller: &str, names: &[OsString]) -> Result<(PathBuf, std::fs::Metadata), Fault> {
    let mut at: PathBuf = root.join(caller);
    let mut last = None;
    for (n, name) in std::iter::once(&OsString::from(caller)).chain(names).enumerate() {
        if n > 0 {
            at.push(name);
        }
        let seen = std::fs::symlink_metadata(&at).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { Fault::Gone } else { Fault::Busy })?;
        let kind = seen.file_type();
        if kind.is_symlink() {
            return Err(Fault::Link);
        }
        if n < names.len() && !kind.is_dir() {
            return Err(Fault::Gone); // a file where a folder should be: nothing is below it
        }
        if n == names.len() {
            if kind.is_dir() {
                return Err(Fault::Folder);
            }
            if !kind.is_file() {
                return Err(Fault::Special);
            }
            last = Some(seen);
        }
    }
    last.map(|seen| (at, seen)).ok_or(Fault::Gone)
}

/// Open the file the walk saw, without following a link and without waiting for a pipe, and check the handle that was opened: a regular file, and the one the walk saw.
fn open_seen(path: &Path, seen: &std::fs::Metadata) -> Result<(std::fs::File, u64), Fault> {
    let file = std::fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_NONBLOCK).open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Fault::Gone,
        _ if e.raw_os_error() == Some(ELOOP) => Fault::Link, // a link took the place of the file between the walk and the open
        _ => Fault::Busy,
    })?;
    let opened = file.metadata().map_err(|_| Fault::Busy)?;
    if !opened.file_type().is_file() {
        return Err(Fault::Special);
    }
    if (opened.dev(), opened.ino()) != (seen.dev(), seen.ino()) {
        return Err(Fault::Busy); // not the file the walk saw: it was replaced under the read
    }
    Ok((file, opened.len()))
}

/// The part of the file that starts at `offset` and holds at most `limit` bytes, cut at a character: the text and where it ends.
fn part(mut file: std::fs::File, size: u64, offset: u64, limit: usize) -> Result<(String, u64), Fault> {
    if offset > size {
        return Err(Fault::Past(size));
    }
    file.seek(SeekFrom::Start(offset)).map_err(|_| Fault::Busy)?;
    let mut bytes = Vec::with_capacity(limit + 3);
    file.take(limit as u64 + 3).read_to_end(&mut bytes).map_err(|_| Fault::Busy)?; // three more, to see whether a character goes across the end of the part
    if bytes.first().is_some_and(|b| b & 0xc0 == 0x80) {
        return Err(Fault::Inside);
    }
    let mut cut = bytes.len().min(limit);
    while cut < bytes.len() && cut > 0 && bytes[cut] & 0xc0 == 0x80 {
        cut -= 1;
    }
    bytes.truncate(cut);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), offset + cut as u64))
}

/// The one line the host logs for a call of `read_handoff`: the session and how it came out; never the path or a byte of the file.
fn audit_line(session: &str, result: &str) -> String {
    format!("lico-host: call session={session} tool=read_handoff result={result}")
}

#[cfg(test)]
mod tests;
