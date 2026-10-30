//! The setting `clipboard_write` (plan M02-P04, S04b; SF-06): whether an AGENT may be given a tool that writes the clipboard.
//!
//! One place: the file `settings.json` in the host's state folder (the folder that holds `handoff`, `accounts` and `registry`; it is `--state-dir`, or the
//! folder of the socket). The content is a JSON object; `{"clipboard_write": true}` switches the tool on. It is read fresh at each use, so switching it
//! needs no restart. It fails CLOSED: a missing file, any value but the JSON boolean `true`, bad JSON, a file over 4 KiB, a link, a directory, a pipe, a
//! file of another user, or a file that the group or others can write all mean OFF. The command `lico clipboard write` is not gated by it (a person runs
//! that); the setting is for the agent tool of S07.

use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::open::open_regular;

/// The most bytes of the settings file; a larger file is OFF.
pub const MAX_SETTINGS_BYTES: u64 = 4096;

/// The file name, in the state folder.
pub const FILE_NAME: &str = "settings.json";

/// The settings file of a host: beside `handoff`, in the state folder.
pub fn settings_path(handoff_dir: &Path) -> PathBuf {
    handoff_dir.parent().unwrap_or(Path::new(".")).join(FILE_NAME)
}

/// Whether the agent tool `clipboard_write` may be offered, judged now from the file at `path`. Any doubt is `false`.
pub fn clipboard_write_enabled(path: &Path) -> bool {
    let Ok(opened) = open_regular(path, 2, MAX_SETTINGS_BYTES) else { return false };
    if opened.file.metadata().map_or(true, |m| m.mode() & 0o022 != 0) {
        return false; // the group or others can write it
    }
    let mut bytes = Vec::new();
    if opened.file.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return false;
    }
    matches!(serde_json::from_slice::<serde_json::Value>(&bytes), Ok(serde_json::Value::Object(map)) if map.get("clipboard_write") == Some(&serde_json::Value::Bool(true)))
}
