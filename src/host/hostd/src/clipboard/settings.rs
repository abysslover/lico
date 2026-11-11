//! Host settings in `settings.json` beside `handoff` in the state folder:
//! - `clipboard_write` (M02-P04, S04b; SF-06): whether an AGENT may be offered `clipboard_write`; fails CLOSED (default `false`).
//! - `notifications` (M02-P06, S02; NT-02): whether `lico-host` runs `notify-send` when a background session finishes or asks for approval; default `true`,
//!   switched off when `settings.json` is a valid owner-only file with `"notifications": false`.

use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::open::open_regular;
use crate::provider_record::O_NOFOLLOW;

/// The most bytes of the settings file; a larger file is ignored.
pub const MAX_SETTINGS_BYTES: u64 = 4096;

/// The file name, in the state folder.
pub const FILE_NAME: &str = "settings.json";

/// The settings file of a host: beside `handoff`, in the state folder.
pub fn settings_path(handoff_dir: &Path) -> PathBuf {
    handoff_dir.parent().unwrap_or(Path::new(".")).join(FILE_NAME)
}

/// Reads the JSON object of `path` when it passes the regular-file and permission check.
pub fn read_object(path: &Path) -> Option<Map<String, Value>> {
    let opened = open_regular(path, 2, MAX_SETTINGS_BYTES).ok()?;
    if opened.file.metadata().map_or(true, |m| m.mode() & 0o022 != 0) {
        return None; // the group or others can write it
    }
    let mut bytes = Vec::new();
    if opened.file.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return None;
    }
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// Whether the agent tool `clipboard_write` may be offered, judged now from the file at `path`. Any doubt is `false`.
pub fn clipboard_write_enabled(path: &Path) -> bool {
    read_object(path).is_some_and(|map| map.get("clipboard_write") == Some(&Value::Bool(true)))
}

/// Whether desktop notifications (`notify-send`) are on, judged now from `path`. Default `true`; `false` when set to boolean `false` in a valid file.
pub fn notifications_enabled(path: &Path) -> bool {
    read_object(path).map_or(true, |map| map.get("notifications") != Some(&Value::Bool(false)))
}

/// The settings object reported in `status`.
pub fn status_json(handoff_dir: &Path) -> Value {
    let path = settings_path(handoff_dir);
    json!({
        "clipboard_write": clipboard_write_enabled(&path),
        "notifications": notifications_enabled(&path),
    })
}

/// Updates `clipboard_write` and/or `notifications` in `settings.json` atomically (mode 0600, preserving other valid keys; refuses a link or folder).
pub fn set_settings(handoff_dir: &Path, clipboard_write: Option<bool>, notifications: Option<bool>) -> crate::core::Reply {
    if clipboard_write.is_none() && notifications.is_none() {
        return Err("bad_request");
    }
    let path = settings_path(handoff_dir);
    let parent = path.parent().unwrap_or(Path::new("."));
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !meta.file_type().is_file() {
            return Err("settings_failed");
        }
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent).map_err(|_| "settings_failed")?;
    let mut map = read_object(&path).unwrap_or_default();
    if let Some(enabled) = clipboard_write {
        map.insert("clipboard_write".into(), Value::Bool(enabled));
    }
    if let Some(enabled) = notifications {
        map.insert("notifications".into(), Value::Bool(enabled));
    }
    let mut text = Value::Object(map).to_string();
    text.push('\n');
    if text.len() as u64 > MAX_SETTINGS_BYTES {
        return Err("settings_failed");
    }
    let tmp = parent.join(format!(".{FILE_NAME}.{}", crate::core::random_hex(8)));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW)
        .open(&tmp)
        .and_then(|mut file| {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err("settings_failed");
    }
    Ok(crate::session::obj(json!({"settings": status_json(handoff_dir)})))
}

#[allow(dead_code)]
pub fn set_clipboard_write(handoff_dir: &Path, enabled: bool) -> crate::core::Reply {
    set_settings(handoff_dir, Some(enabled), None)
}
