//! The host side of a pasted picture (plan M02-P04, S05a; requirements XF-05, SF-04).
//!
//! The window cannot know the host's state folder, so the host hands out a place: the op `picture.slot` makes an EMPTY file `<32 hex>.png` (mode 0600, created
//! with `O_CREAT | O_EXCL | O_NOFOLLOW`) in the folder `pasted` (mode 0700, beside `handoff` in the state folder, never in the repository or a workspace) and
//! answers its name and its absolute path. The window writes the picture there, then sends `input` with the member `attachment` (the NAME). The host builds the
//! path itself (`pasted_dir(handoff_dir)/<name>`), never takes one from a request, opens the file without following a link (`clipboard::open`), checks the
//! picture by its header, and gives Claude an image block. The file is removed after every `input` that names one, whatever the outcome (`with_cleanup`).
//!
//! A file that nobody used does not stay: every file in the folder goes at host start, and a slot call removes the files older than 10 minutes; at most 8
//! files can be there at once (`busy`). The reply of a slot is for the person's own window; the agent never learns where the file is (`transfer.rs`).

mod base64;

use serde_json::json;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::clipboard::open::{open_regular, read_checked, Fault, MAX_IMAGE_FILE};
use crate::core::{Reply, Shared};
use crate::provider_record::O_NOFOLLOW;
use crate::session::obj;

#[cfg(test)]
mod tests;

/// The most files in the folder at once; the ninth slot is `busy`.
pub const MAX_SLOTS: usize = 8;
/// A slot older than this many seconds is removed by the next slot call.
pub const MAX_AGE_SECS: u64 = 600;
/// A name is 32 lowercase hex digits and `.png`.
pub const NAME_LEN: usize = 36;
/// The smallest file that can be a picture: the PNG signature.
const MIN_FILE: u64 = 8;

/// One slot call at a time, so that two calls cannot both see 7 files and make the eighth and ninth.
static SLOT_LOCK: Mutex<()> = Mutex::new(());

/// The folder of pasted pictures: beside the handoff folder (the manner of `clipboard::settings::settings_path`), so that no field of `Config` is needed.
pub fn pasted_dir(handoff_dir: &Path) -> PathBuf {
    handoff_dir.parent().unwrap_or(Path::new(".")).join("pasted")
}

/// A name the host would hand out: exactly 32 lowercase hex digits and `.png`.
pub fn valid_name(name: &str) -> bool {
    name.len() == NAME_LEN && name.ends_with(".png") && name[..32].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The folder, made if it is not there (0700). A link or a file in its place is not followed: it is replaced by a folder.
fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    if let Ok(meta) = std::fs::symlink_metadata(dir) {
        if !meta.is_dir() {
            std::fs::remove_file(dir)?;
        }
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Removes one name: a file, a link (the link itself, never its target), a pipe, or an empty folder.
fn remove_entry(path: &Path) {
    if std::fs::remove_file(path).is_err() {
        let _ = std::fs::remove_dir(path);
    }
}

/// Removes every entry of the folder (at host start).
fn clear(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        remove_entry(&entry.path());
    }
}

/// Removes the entries last changed more than `max_age` seconds before `now` (seconds since the epoch); a link is judged by itself, not by its target.
fn sweep(dir: &Path, now: u64, max_age: u64) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let old = std::fs::symlink_metadata(entry.path()).map_or(true, |m| now.saturating_sub(m.mtime().max(0) as u64) > max_age);
        if old {
            remove_entry(&entry.path());
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// At host start: the folder exists, is 0700 and holds nothing.
pub fn prepare(handoff_dir: &Path) -> std::io::Result<()> {
    let dir = pasted_dir(handoff_dir);
    ensure_dir(&dir)?;
    clear(&dir);
    Ok(())
}

/// The op `picture.slot`: sweep the old files, refuse with `busy` when 8 are there, otherwise make an empty file and hand out its name and path.
pub fn slot(shared: &Shared) -> Reply {
    let _one_at_a_time = SLOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = pasted_dir(&shared.cfg.handoff_dir);
    ensure_dir(&dir).map_err(|_| "slot_failed")?;
    sweep(&dir, now_secs(), MAX_AGE_SECS);
    if std::fs::read_dir(&dir).map_or(MAX_SLOTS, |it| it.count()) >= MAX_SLOTS {
        return Err("busy");
    }
    let dir = std::fs::canonicalize(&dir).map_err(|_| "slot_failed")?; // the window gets an absolute path
    for _ in 0..4 {
        let name = format!("{}.png", crate::core::random_hex(16));
        let path = dir.join(&name);
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(O_NOFOLLOW).open(&path) {
            Ok(file) => {
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600)); // whatever the umask took away
                return Ok(obj(json!({"name": name, "path": path.to_string_lossy()})));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => break,
        }
    }
    Err("slot_failed")
}

/// Runs `run` with the attachment name of an `input` request, then removes that file whatever `run` answered (sent, refused, a repeat, an unknown session).
pub fn with_cleanup(shared: &Shared, attachment: Option<&str>, run: impl FnOnce(Option<&str>) -> Reply) -> Reply {
    let result = run(attachment);
    if let Some(name) = attachment.filter(|n| valid_name(n)) {
        remove_entry(&pasted_dir(&shared.cfg.handoff_dir).join(name));
    }
    result
}

/// A picture that is ready to send: its size on disk and its base64 text.
pub struct Picture {
    pub size: u64,
    pub base64: String,
}

/// The picture named by `attachment` (None when the input has none). The kind of the session is judged FIRST (`supported`: only a Claude session takes one;
/// no guess at the shape of a Codex item), then the file is opened without following a link and judged on the descriptor (a regular file of the user, 8
/// bytes to 3 MiB), read, and its PNG header checked (sides 1 to 8000). Errors: `attachment_unsupported`, `attachment_gone`, `attachment_too_large`, `bad_image`.
pub fn take(shared: &Shared, attachment: Option<&str>, supported: bool) -> Result<Option<Picture>, &'static str> {
    let Some(name) = attachment else { return Ok(None) };
    if !supported {
        return Err("attachment_unsupported");
    }
    let path = pasted_dir(&shared.cfg.handoff_dir).join(name);
    let opened = open_regular(&path, MIN_FILE, MAX_IMAGE_FILE).map_err(fault_word)?;
    let bytes = read_checked(opened, MAX_IMAGE_FILE).map_err(fault_word)?;
    crate::clipboard::png_size(&bytes).map_err(|_| "bad_image")?;
    Ok(Some(Picture { size: bytes.len() as u64, base64: base64::encode(&bytes) }))
}

fn fault_word(fault: Fault) -> &'static str {
    match fault {
        Fault::TooBig(_) => "attachment_too_large",
        Fault::TooSmall(_) => "bad_image",
        Fault::Open(_) | Fault::NotRegular | Fault::NotYours | Fault::Read(_) => "attachment_gone",
    }
}

/// What the transcript shows for an input: `[picture 245 KiB] ` in front of the text when there is a picture (rounded to the nearest KiB, at least 1). The provider
/// gets the text unchanged.
pub fn echo(text: &str, picture: Option<&Picture>) -> String {
    match picture {
        Some(p) => format!("[picture {} KiB] {text}", ((p.size + 512) / 1024).max(1)),
        None => text.to_string(),
    }
}
