//! Output files that are never half-written and never follow a link, and the size guard of the agent copy.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// What the provider takes for ONE image, in bytes of the file we write.
///
/// Source: the vision guide of the provider (the Claude platform documentation, page "Vision", section on the size of an image, read on
/// 2026-10-29): "10 MB (base64-encoded) on the Claude API and Claude Models on Google Cloud (Google managed)", "5 MB (base64-encoded) on Amazon
/// Bedrock", "10 MB on claude.ai". Base64 is 4 bytes for 3, and MB is taken as 10^6 (the stricter reading), so the first-party API takes
/// 10_000_000 / 4 * 3 = 7_500_000 bytes of PNG and Bedrock 3_750_000. The constant is the first-party API's figure, because that is what the Claude
/// Code program uses; the guard is a function of the limit, so a stricter provider is one number.
pub const AGENT_LIMIT: u64 = 7_500_000;

/// Whether an agent copy of `bytes` bytes may be delivered under `limit`.
pub fn fits(bytes: u64, limit: u64) -> bool {
    bytes <= limit
}

/// A file made next to its final name with create-new semantics (so a symbolic link or an existing file at the temporary name is an error, never
/// followed) and mode 0600; `commit` renames it onto the final name, which replaces a link there instead of writing through it.
pub struct Pending {
    pub tmp: PathBuf,
    pub dest: PathBuf,
    done: bool,
}

static COUNT: AtomicUsize = AtomicUsize::new(0);

impl Pending {
    /// The folder of `dest` must exist. Returns the pending file and its open handle.
    pub fn create(dest: &Path) -> io::Result<(Pending, File)> {
        let name = dest.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
        let folder = match dest.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let tmp = folder.join(format!(".{}.{}-{}.tmp", name.to_string_lossy(), std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        Ok((Pending { tmp, dest: dest.to_path_buf(), done: false }, file))
    }

    pub fn commit(mut self) -> io::Result<()> {
        fs::rename(&self.tmp, &self.dest)?;
        self.done = true;
        Ok(())
    }

    /// The bytes written so far (read from the file, after it was flushed).
    pub fn len(&self) -> io::Result<u64> {
        Ok(fs::metadata(&self.tmp)?.len())
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.done {
            let _ = fs::remove_file(&self.tmp);
        }
    }
}
