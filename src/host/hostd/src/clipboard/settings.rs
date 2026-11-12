//! Host settings in `settings.json` beside `handoff` in the state folder:
//! - `clipboard_write` (M02-P04, S04b; SF-06): whether an AGENT may be offered `clipboard_write`; fails CLOSED (default `false`).
//! - `notifications` (M02-P06, S02; NT-02): whether `lico-host` runs `notify-send` when a background session finishes or asks for approval; default `true`,
//!   switched off when `settings.json` is a valid owner-only file with `"notifications": false`.
//! - `group_by` (M03-P02): how `lico gui` groups sessions in the sidebar (`"account"` default, or `"workspace"` rolling worktrees up under their repo).
//! - `font_px` (M03-P02): logical text size in `lico gui` (`12..=20`, default `14`).
//! - `last_cwd`, `last_view`, `last_session` (M05-P01): last working directory and workspace view (`"start"` | `"files"` | `"session"`) restored across `lico gui` restarts.

use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::open::open_regular;
use crate::provider_record::O_NOFOLLOW;

/// The most bytes of the settings file; a larger file is ignored.
pub const MAX_SETTINGS_BYTES: u64 = 32768;

/// Maximum persisted `[ Directories ]` navigation history items and bookmarks.
pub const MAX_DIR_NAV: usize = 48;

/// Default, minimum, and maximum logical text sizes in `lico gui`.
pub const DEFAULT_FONT_PX: i64 = 14;
pub const MIN_FONT_PX: i64 = 12;
pub const MAX_FONT_PX: i64 = 20;

/// The file name, in the state folder.
pub const FILE_NAME: &str = "settings.json";

#[derive(Clone, Copy, Default)]
pub struct Patch<'a> {
    pub clipboard_write: Option<bool>,
    pub notifications: Option<bool>,
    pub group_by: Option<&'a str>,
    pub font_px: Option<i64>,
    pub last_cwd: Option<&'a str>,
    pub last_view: Option<&'a str>,
    pub last_session: Option<&'a str>,
    pub show_hidden: Option<bool>,
    pub dir_history: Option<&'a Vec<Value>>,
    pub dir_history_idx: Option<i64>,
    pub dir_bookmarks: Option<&'a Vec<Value>>,
}

impl<'a> Patch<'a> {
    pub fn of_map(map: &'a Map<String, Value>) -> Self {
        Self {
            clipboard_write: map.get("clipboard_write").and_then(Value::as_bool),
            notifications: map.get("notifications").and_then(Value::as_bool),
            group_by: map.get("group_by").and_then(Value::as_str),
            font_px: map.get("font_px").and_then(Value::as_i64),
            last_cwd: map.get("last_cwd").and_then(Value::as_str),
            last_view: map.get("last_view").and_then(Value::as_str),
            last_session: map.get("last_session").and_then(Value::as_str),
            show_hidden: map.get("show_hidden").and_then(Value::as_bool),
            dir_history: map.get("dir_history").and_then(Value::as_array),
            dir_history_idx: map.get("dir_history_idx").and_then(Value::as_i64),
            dir_bookmarks: map.get("dir_bookmarks").and_then(Value::as_array),
        }
    }
}

fn valid_dir_nav_item(value: &Value, with_label: bool) -> bool {
    let Some(obj) = value.as_object() else { return false };
    let required: &[&str] = if with_label {
        &["cwd", "global", "page", "label"]
    } else {
        &["cwd", "global", "page"]
    };
    if !required.iter().all(|k| obj.contains_key(*k)) {
        return false;
    }
    obj.iter().all(|(k, v)| match k.as_str() {
        "cwd" => matches!(v, Value::String(s) if s.is_empty() || crate::folders::valid_path_shape(s)),
        "global" => v.is_boolean(),
        "page" => v.as_i64().is_some_and(|n| (0..=10_000).contains(&n)),
        "ssh_host" => matches!(v, Value::Null) || matches!(v, Value::String(s) if crate::ssh::valid_ssh_host(s)),
        "label" if with_label => matches!(
            v,
            Value::String(s)
                if !s.is_empty()
                    && s.chars().count() <= 80
                    && !s.chars().any(crate::folders::refused_char)
                    && !crate::folders::shows_nothing(s)
        ),
        _ => false,
    })
}

pub fn valid_dir_history_array(value: &Value) -> bool {
    matches!(value, Value::Array(arr) if arr.len() <= MAX_DIR_NAV && arr.iter().all(|v| valid_dir_nav_item(v, false)))
}

pub fn valid_dir_bookmarks_array(value: &Value) -> bool {
    matches!(value, Value::Array(arr) if arr.len() <= MAX_DIR_NAV && arr.iter().all(|v| valid_dir_nav_item(v, true)))
}

fn is_state_dir_path(settings_file: &Path, raw: &str) -> bool {
    if raw.is_empty() {
        return false;
    }
    let Some(state_dir) = settings_file.parent() else { return false };
    let p = Path::new(raw);
    if p == state_dir || p.starts_with(state_dir) {
        return true;
    }
    if let Ok(state_canon) = std::fs::canonicalize(state_dir) {
        if p == state_canon || p.starts_with(&state_canon) {
            return true;
        }
        if let Ok(p_canon) = std::fs::canonicalize(p) {
            if p_canon == state_canon || p_canon.starts_with(&state_canon) {
                return true;
            }
        }
    }
    false
}

fn clean_dir_nav_entries(path: &Path, arr: &[Value], with_label: bool) -> Vec<Value> {
    arr.iter()
        .filter(|v| valid_dir_nav_item(v, with_label))
        .filter(|v| {
            let obj = v.as_object().unwrap();
            let global = obj.get("global").and_then(Value::as_bool).unwrap_or(false);
            let cwd = obj.get("cwd").and_then(Value::as_str).unwrap_or("");
            let ssh = obj.get("ssh_host").and_then(Value::as_str).filter(|s| !s.is_empty());
            if !global && cwd.is_empty() {
                return false;
            }
            if ssh.is_none() && !cwd.is_empty() && (!Path::new(cwd).is_absolute() || is_state_dir_path(path, cwd)) {
                return false;
            }
            true
        })
        .take(MAX_DIR_NAV)
        .cloned()
        .collect()
}

pub fn valid_session_ref(text: &str) -> bool {
    !text.is_empty() && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

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

/// Session-list grouping mode in `lico gui`: `"workspace"` when explicitly set in a valid owner-only file, else `"account"`.
pub fn group_by_value(path: &Path) -> &'static str {
    match read_object(path).and_then(|map| map.get("group_by").and_then(Value::as_str).map(str::to_string)).as_deref() {
        Some("workspace") => "workspace",
        _ => "account",
    }
}

/// Logical text size in `lico gui` (`12..=20`, default `14`).
pub fn font_px_value(path: &Path) -> i64 {
    read_object(path)
        .and_then(|map| map.get("font_px").and_then(Value::as_i64))
        .filter(|n| (MIN_FONT_PX..=MAX_FONT_PX).contains(n))
        .unwrap_or(DEFAULT_FONT_PX)
}

/// Saved workspace view in `lico gui`: `"files"` or `"session"` when valid, else `"start"`.
pub fn last_view_value(path: &Path) -> &'static str {
    match read_object(path).and_then(|map| map.get("last_view").and_then(Value::as_str).map(str::to_string)).as_deref() {
        Some("files") => "files",
        Some("session") => "session",
        _ => "start",
    }
}

/// Saved session id in `lico gui`, when present and valid.
pub fn last_session_value(path: &Path) -> Option<String> {
    read_object(path)
        .and_then(|map| map.get("last_session").and_then(Value::as_str).map(str::to_string))
        .filter(|s| valid_session_ref(s))
}

/// Saved working directory in `lico gui`, resolved and checked to be a readable directory outside the state folder.
pub fn last_cwd_value(path: &Path) -> Option<String> {
    let raw = read_object(path).and_then(|map| map.get("last_cwd").and_then(Value::as_str).map(str::to_string))?;
    if !crate::folders::valid_path_shape(&raw) || !Path::new(&raw).is_absolute() {
        return None;
    }
    let canon = std::fs::canonicalize(&raw).ok()?;
    if !canon.is_dir() || std::fs::read_dir(&canon).is_err() {
        return None;
    }
    if let Some(state_dir) = path.parent() {
        let state_canon = std::fs::canonicalize(state_dir).unwrap_or_else(|_| state_dir.to_path_buf());
        if canon == state_canon {
            return None;
        }
        for sub in ["handoff", "logs", "log", "accounts", "cred", "workspaces", "registry"] {
            let own = state_dir.join(sub);
            let own_canon = std::fs::canonicalize(&own).unwrap_or(own);
            if canon.starts_with(&own_canon) {
                return None;
            }
        }
    }
    canon.to_str().filter(|s| crate::folders::valid_path_shape(s)).map(str::to_string)
}

/// Whether dot-prefixed directory entries (`.git`, `.env`, etc.) are shown in `lico gui` (`Files`). Default `false`.
pub fn show_hidden_enabled(path: &Path) -> bool {
    read_object(path).is_some_and(|map| map.get("show_hidden") == Some(&Value::Bool(true)))
}

/// The settings object reported in `status` and `settings.set`.
pub fn status_json(handoff_dir: &Path) -> Value {
    let path = settings_path(handoff_dir);
    let mut out = json!({
        "clipboard_write": clipboard_write_enabled(&path),
        "notifications": notifications_enabled(&path),
        "group_by": group_by_value(&path),
        "font_px": font_px_value(&path),
    });
    if show_hidden_enabled(&path) {
        out["show_hidden"] = Value::Bool(true);
    }
    let view = last_view_value(&path);
    if view != "start" {
        out["last_view"] = Value::String(view.into());
    }
    if let Some(cwd) = last_cwd_value(&path) {
        out["last_cwd"] = Value::String(cwd);
    }
    if let Some(sid) = last_session_value(&path) {
        out["last_session"] = Value::String(sid);
    }
    if let Some(map) = read_object(&path) {
        if let Some(arr) = map.get("dir_history").and_then(Value::as_array) {
            let cleaned = clean_dir_nav_entries(&path, arr, false);
            if !cleaned.is_empty() {
                let raw_idx = map
                    .get("dir_history_idx")
                    .and_then(Value::as_i64)
                    .unwrap_or((cleaned.len() - 1) as i64);
                let idx = raw_idx.clamp(0, (cleaned.len() - 1) as i64);
                out["dir_history"] = Value::Array(cleaned);
                out["dir_history_idx"] = json!(idx);
            }
        }
        if let Some(arr) = map.get("dir_bookmarks").and_then(Value::as_array) {
            let cleaned = clean_dir_nav_entries(&path, arr, true);
            if !cleaned.is_empty() {
                out["dir_bookmarks"] = Value::Array(cleaned);
            }
        }
    }
    out
}

/// Validates workspace paths against the host's folder rules (`folders::remember_cwd`) before writing `settings.json`.
pub fn apply_settings(shared: &crate::core::Shared, patch: Patch<'_>) -> crate::core::Reply {
    if let Some(raw) = patch.last_cwd {
        if !raw.is_empty() {
            let canon = crate::folders::remember_cwd(shared, raw)?;
            return update_settings(&shared.cfg.handoff_dir, Patch { last_cwd: Some(&canon), ..patch });
        }
    }
    update_settings(&shared.cfg.handoff_dir, patch)
}

/// Updates any subset of host settings in `settings.json` atomically (mode 0600, preserving other valid keys; refuses a link or folder).
pub fn update_settings(handoff_dir: &Path, patch: Patch<'_>) -> crate::core::Reply {
    if patch.clipboard_write.is_none()
        && patch.notifications.is_none()
        && patch.group_by.is_none()
        && patch.font_px.is_none()
        && patch.last_cwd.is_none()
        && patch.last_view.is_none()
        && patch.last_session.is_none()
        && patch.show_hidden.is_none()
        && patch.dir_history.is_none()
        && patch.dir_history_idx.is_none()
        && patch.dir_bookmarks.is_none()
    {
        return Err("bad_request");
    }
    if let Some(group) = patch.group_by {
        if !matches!(group, "account" | "workspace") {
            return Err("bad_request");
        }
    }
    if let Some(px) = patch.font_px {
        if !(MIN_FONT_PX..=MAX_FONT_PX).contains(&px) {
            return Err("bad_request");
        }
    }
    if let Some(view) = patch.last_view {
        if !matches!(view, "start" | "files" | "session") {
            return Err("bad_request");
        }
    }
    if let Some(sid) = patch.last_session {
        if !sid.is_empty() && !valid_session_ref(sid) {
            return Err("bad_request");
        }
    }
    if let Some(cwd) = patch.last_cwd {
        if !cwd.is_empty() && (!crate::folders::valid_path_shape(cwd) || !Path::new(cwd).is_absolute()) {
            return Err("bad_request");
        }
    }
    if let Some(arr) = patch.dir_history {
        if arr.len() > MAX_DIR_NAV || !arr.iter().all(|v| valid_dir_nav_item(v, false)) {
            return Err("bad_request");
        }
    }
    if let Some(idx) = patch.dir_history_idx {
        if !(0..MAX_DIR_NAV as i64).contains(&idx) {
            return Err("bad_request");
        }
    }
    if let Some(arr) = patch.dir_bookmarks {
        if arr.len() > MAX_DIR_NAV || !arr.iter().all(|v| valid_dir_nav_item(v, true)) {
            return Err("bad_request");
        }
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
    if let Some(enabled) = patch.clipboard_write {
        map.insert("clipboard_write".into(), Value::Bool(enabled));
    }
    if let Some(enabled) = patch.notifications {
        map.insert("notifications".into(), Value::Bool(enabled));
    }
    if let Some(group) = patch.group_by {
        map.insert("group_by".into(), Value::String(group.into()));
    }
    if let Some(px) = patch.font_px {
        map.insert("font_px".into(), json!(px));
    }
    if let Some(cwd) = patch.last_cwd {
        if cwd.is_empty() {
            map.remove("last_cwd");
        } else {
            map.insert("last_cwd".into(), Value::String(cwd.into()));
        }
    }
    if let Some(view) = patch.last_view {
        map.insert("last_view".into(), Value::String(view.into()));
    }
    if let Some(sid) = patch.last_session {
        if sid.is_empty() {
            map.remove("last_session");
        } else {
            map.insert("last_session".into(), Value::String(sid.into()));
        }
    }
    if let Some(sh) = patch.show_hidden {
        if sh {
            map.insert("show_hidden".into(), Value::Bool(true));
        } else {
            map.remove("show_hidden");
        }
    }
    if let Some(arr) = patch.dir_history {
        let cleaned = clean_dir_nav_entries(&path, arr, false);
        if cleaned.is_empty() {
            map.remove("dir_history");
            map.remove("dir_history_idx");
        } else {
            map.insert("dir_history".into(), Value::Array(cleaned));
        }
    }
    if let Some(idx) = patch.dir_history_idx {
        if map.get("dir_history").and_then(Value::as_array).is_some_and(|a| !a.is_empty()) {
            map.insert("dir_history_idx".into(), json!(idx));
        } else {
            map.remove("dir_history_idx");
        }
    }
    if let Some(arr) = patch.dir_bookmarks {
        let cleaned = clean_dir_nav_entries(&path, arr, true);
        if cleaned.is_empty() {
            map.remove("dir_bookmarks");
        } else {
            map.insert("dir_bookmarks".into(), Value::Array(cleaned));
        }
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

/// Updates `clipboard_write` and/or `notifications` in `settings.json` atomically.
pub fn set_settings(handoff_dir: &Path, clipboard_write: Option<bool>, notifications: Option<bool>) -> crate::core::Reply {
    update_settings(handoff_dir, Patch { clipboard_write, notifications, ..Default::default() })
}

#[allow(dead_code)]
pub fn set_clipboard_write(handoff_dir: &Path, enabled: bool) -> crate::core::Reply {
    set_settings(handoff_dir, Some(enabled), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_fields_round_trip_clear_cleanly_and_drop_missing_or_state_dirs_on_readback() {
        let root = std::env::temp_dir().join(format!("lico-settings-unit-{}", crate::core::random_hex(6)));
        let state = root.join("state");
        let handoff = state.join("handoff");
        let work = root.join("project");
        std::fs::create_dir_all(&handoff).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let work_canon = std::fs::canonicalize(&work).unwrap().to_string_lossy().into_owned();

        let rep = update_settings(
            &handoff,
            Patch { last_cwd: Some(&work_canon), last_view: Some("files"), last_session: Some("s01234567"), ..Default::default() },
        )
        .unwrap();
        assert_eq!(rep["settings"]["last_cwd"], json!(work_canon));
        assert_eq!(rep["settings"]["last_view"], json!("files"));
        assert_eq!(rep["settings"]["last_session"], json!("s01234567"));

        // Clearing session and deleting the directory omits both cleanly on readback.
        let _ = update_settings(&handoff, Patch { last_session: Some(""), ..Default::default() }).unwrap();
        std::fs::remove_dir_all(&work).unwrap();
        let st = status_json(&handoff);
        assert!(st.get("last_cwd").is_none() && st.get("last_session").is_none());
        assert_eq!(st["last_view"], json!("files"));

        // A directory inside the state folder is never surfaced as last_cwd even if written directly.
        let inside = std::fs::canonicalize(&handoff).unwrap().to_string_lossy().into_owned();
        let _ = update_settings(&handoff, Patch { last_cwd: Some(&inside), ..Default::default() }).unwrap();
        assert!(status_json(&handoff).get("last_cwd").is_none());

        assert_eq!(update_settings(&handoff, Patch { last_view: Some("bogus"), ..Default::default() }), Err("bad_request"));
        assert_eq!(update_settings(&handoff, Patch { last_session: Some("bad/id"), ..Default::default() }), Err("bad_request"));

        // Directory navigation history and bookmarks round-trip and drop state-folder entries.
        let hist = vec![
            json!({"cwd": "", "global": true, "page": 0}),
            json!({"cwd": inside, "global": false, "page": 0}),
            json!({"cwd": "/var/tmp", "global": false, "page": 2}),
        ];
        let bms = vec![json!({"cwd": "/var/tmp", "global": false, "page": 2, "label": "tmp (p.3)"})];
        let nav_rep = update_settings(
            &handoff,
            Patch { dir_history: Some(&hist), dir_history_idx: Some(2), dir_bookmarks: Some(&bms), ..Default::default() },
        )
        .unwrap();
        assert_eq!(nav_rep["settings"]["dir_history"].as_array().unwrap().len(), 2);
        assert_eq!(nav_rep["settings"]["dir_history_idx"], json!(1));
        assert_eq!(nav_rep["settings"]["dir_bookmarks"][0]["label"], json!("tmp (p.3)"));

        let bad_bm = vec![json!({"cwd": "/var/tmp", "global": false, "page": 0, "label": "   "})];
        assert_eq!(update_settings(&handoff, Patch { dir_bookmarks: Some(&bad_bm), ..Default::default() }), Err("bad_request"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
