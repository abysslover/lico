//! The safe open of a file whose bytes are handed to the clipboard (plan M02-P04, S04b; D5 asks the same of the pasted file of S05).
//!
//! `open_regular` never follows a symbolic link at the last name (`O_NOFOLLOW`), never waits for a pipe (`O_NONBLOCK`), and judges the file on the
//! OPENED descriptor with `fstat`, not on the name: a regular file, owned by the user who runs the program, of a size from `min` to `max` bytes.
//! `read_checked` then reads at most `max + 1` bytes and refuses a file whose length changed between the `fstat` and the end of the read, so a file that
//! grows while it is read is an error, not a cut. A link in an earlier part of the path is followed (the kernel does that); only the last name is judged,
//! which is what a caller can ask of a path it did not make.

use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use crate::provider_record::{O_NOFOLLOW, O_NONBLOCK};

/// The most bytes of a picture file the clipboard takes: 3 MiB (the same limit as the reader's default).
pub const MAX_IMAGE_FILE: u64 = 3 * 1024 * 1024;

extern "C" {
    fn geteuid() -> u32;
}

/// Why a file was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Fault {
    /// It could not be opened (missing, no right, a link at the last name, a loop).
    Open(String),
    /// Opened, but not a regular file (a directory, a pipe, a device, a socket).
    NotRegular,
    /// A regular file of another user.
    NotYours,
    /// Smaller than `min` bytes (the size is given).
    TooSmall(u64),
    /// Larger than `max` bytes (the size is given: the one `fstat` said, or `max + 1` when the read found more).
    TooBig(u64),
    /// It could not be read to its end, or its length changed while it was read.
    Read(String),
}

/// An opened file and the size `fstat` said.
#[derive(Debug)]
pub struct Opened {
    pub file: File,
    pub size: u64,
}

/// Opens `path` (not followed if it is a link) and checks the descriptor: a regular file of the user, `min <= size <= max`.
pub fn open_regular(path: &Path, min: u64, max: u64) -> Result<Opened, Fault> {
    let file = std::fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_NONBLOCK).open(path).map_err(|e| {
        let why = if e.raw_os_error() == Some(40) { "a symbolic link is not followed".to_string() } else { e.to_string() };
        Fault::Open(why)
    })?;
    let meta = file.metadata().map_err(|e| Fault::Open(e.to_string()))?;
    if !meta.file_type().is_file() {
        return Err(Fault::NotRegular);
    }
    // SAFETY: geteuid has no arguments and cannot fail.
    if meta.uid() != unsafe { geteuid() } {
        return Err(Fault::NotYours);
    }
    let size = meta.len();
    if size < min {
        return Err(Fault::TooSmall(size));
    }
    if size > max {
        return Err(Fault::TooBig(size));
    }
    Ok(Opened { file, size })
}

/// Writes `bytes` into the EMPTY file at `path` that the host made for a pasted picture (`picture.slot`, plan M02-P04 S05b): opened for writing only,
/// WITHOUT creating it, without truncating it and without following a link at the last name (`O_NOFOLLOW`), never waiting for a pipe (`O_NONBLOCK`), and
/// judged on the opened descriptor: a regular file of this user with no bytes in it yet (a size over 0 is `TooBig`). Every byte is written, then the file is
/// closed. Faults: those of `open_regular`, and `Open` with the reason for a write that failed.
pub fn write_slot(path: &Path, bytes: &[u8]) -> Result<(), Fault> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).custom_flags(O_NOFOLLOW | O_NONBLOCK).open(path).map_err(|e| {
        let why = if e.raw_os_error() == Some(40) { "a symbolic link is not followed".to_string() } else { e.to_string() };
        Fault::Open(why)
    })?;
    let meta = file.metadata().map_err(|e| Fault::Open(e.to_string()))?;
    if !meta.file_type().is_file() {
        return Err(Fault::NotRegular);
    }
    // SAFETY: geteuid has no arguments and cannot fail.
    if meta.uid() != unsafe { geteuid() } {
        return Err(Fault::NotYours);
    }
    if meta.len() > 0 {
        return Err(Fault::TooBig(meta.len()));
    }
    file.write_all(bytes).and_then(|_| file.flush()).map_err(|e| Fault::Open(format!("the write failed: {e}")))
}

/// The whole file, at most `max` bytes. A file that holds more than `max` bytes when the read gets there is `TooBig`; one whose length is not the length
/// `fstat` said is `Read` (it changed while it was read).
pub fn read_checked(opened: Opened, max: u64) -> Result<Vec<u8>, Fault> {
    read_from(opened.file, opened.size, max)
}

/// `read_checked` over any reader (the tests feed it a reader that grows).
pub(crate) fn read_from(reader: impl Read, size: u64, max: u64) -> Result<Vec<u8>, Fault> {
    let mut bytes = Vec::with_capacity(size.min(max) as usize);
    reader.take(max + 1).read_to_end(&mut bytes).map_err(|e| Fault::Read(e.to_string()))?;
    if bytes.len() as u64 > max {
        return Err(Fault::TooBig(max + 1));
    }
    if bytes.len() as u64 != size {
        return Err(Fault::Read(format!("the file changed while it was read ({size} bytes, then {})", bytes.len())));
    }
    Ok(bytes)
}

impl Fault {
    /// The reason the command and the window report: `too_large` for a size, `bad_image` for an empty file, `write_failed` for everything that stopped the file being used.
    pub fn reason(&self) -> &'static str {
        match self {
            Fault::TooBig(_) => "too_large",
            Fault::TooSmall(_) => "bad_image",
            _ => "write_failed",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Fault::Open(why) => format!("the file cannot be opened: {why}"),
            Fault::NotRegular => "the file is not a regular file".to_string(),
            Fault::NotYours => "the file belongs to another user".to_string(),
            Fault::TooSmall(n) => format!("the file has {n} bytes, too few"),
            Fault::TooBig(n) => format!("the file has {n} bytes, too many"),
            Fault::Read(why) => format!("the file cannot be read: {why}"),
        }
    }
}
