//! On-demand working-directory listing and bounded regular-file preview (`M04-P04`).
//!
//! Reads a chosen directory or regular UTF-8 file strictly once per explicit call (`folder.list`, `folder.read`),
//! never installing `inotify` watchers or background polling timers. Reuses `folders::check_folder` so host state,
//! credentials, logs, and scratch roots are refused, and opens regular files with `O_NOFOLLOW | O_NONBLOCK` plus `fstat`.

use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use serde_json::{json, Value};

use crate::core::{Reply, Shared};
use crate::provider_record::{O_NOFOLLOW, O_NONBLOCK};
use crate::session::obj;

pub const MAX_ENTRIES: usize = 500;
pub const MAX_PREVIEW_BYTES: usize = 64 * 1024;

struct EntryItem {
    name: String,
    kind: &'static str,
    size: u64,
    is_html: bool,
}

fn is_html_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

/// Lists one directory on demand: directories first, then files/symlinks in case-insensitive name order.
pub fn folder_list(shared: &Shared, cwd: &str) -> Reply {
    let canon = crate::folders::check_folder(shared, cwd)?;
    let canon_path = Path::new(&canon);
    let parent = canon_path
        .parent()
        .and_then(|p| p.to_str())
        .filter(|s| !s.is_empty() && *s != canon)
        .and_then(|s| crate::folders::check_folder(shared, s).ok());

    let iter = std::fs::read_dir(canon_path).map_err(|_| "folder_unreadable")?;
    let mut items: Vec<EntryItem> = Vec::new();
    for entry in iter.flatten() {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if name.is_empty() || name.chars().count() > 255 || name.chars().any(crate::folders::refused_char) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let (kind, size, is_html) = if ft.is_symlink() {
            ("link", 0, false)
        } else if ft.is_dir() {
            ("dir", 0, false)
        } else if ft.is_file() {
            let len = entry.metadata().map_or(0, |m| m.len());
            ("file", len, is_html_name(&name))
        } else {
            continue;
        };
        items.push(EntryItem { name, kind, size, is_html });
    }

    items.sort_by(|a, b| {
        let da = u8::from(a.kind != "dir");
        let db = u8::from(b.kind != "dir");
        da.cmp(&db)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    let truncated = items.len() > MAX_ENTRIES;
    items.truncate(MAX_ENTRIES);
    let entries: Vec<Value> = items
        .into_iter()
        .map(|e| json!({"name": e.name, "kind": e.kind, "size": e.size, "is_html": e.is_html}))
        .collect();

    Ok(obj(json!({
        "cwd": canon,
        "parent": parent,
        "entries": entries,
        "truncated": truncated,
    })))
}

fn decode_utf8_preview(raw: &[u8], truncated: bool) -> Option<&str> {
    if raw.contains(&0) {
        return None;
    }
    match std::str::from_utf8(raw) {
        Ok(s) => Some(s),
        Err(e) if truncated && e.error_len().is_none() => std::str::from_utf8(&raw[..e.valid_up_to()]).ok(),
        Err(_) => None,
    }
}

fn sanitize_preview_text(raw_str: &str) -> String {
    let lf = raw_str.replace("\r\n", "\n").replace('\r', "\n");
    lf.chars()
        .filter(|&c| c == '\n' || c == '\t' || !crate::folders::refused_char(c))
        .collect()
}

/// Reads at most `64 KiB` of a regular file inside an allowed working directory, refusing symlinks and special files.
pub fn folder_read(shared: &Shared, path: &str) -> Reply {
    if !crate::folders::valid_path_shape(path) {
        return Err("bad_request");
    }
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("folder_not_absolute");
    }
    if path == "/" || path.ends_with('/') {
        if let Err(code @ "folder_protected") = crate::folders::check_folder(shared, path) {
            return Err(code);
        }
        return Err("file_not_regular");
    }
    let Some(name) = p
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != "." && *n != ".." && n.chars().count() <= 255 && !n.chars().any(crate::folders::refused_char))
    else {
        return Err("file_not_regular");
    };
    let Some(raw_parent) = p.parent().and_then(|d| d.to_str()) else {
        return Err("file_not_regular");
    };
    let parent_arg = if raw_parent.is_empty() { "/" } else { raw_parent };
    let canon_parent = crate::folders::check_folder(shared, parent_arg)?;
    let target = Path::new(&canon_parent).join(name);
    let Some(full_str) = target.to_str().filter(|s| crate::folders::valid_path_shape(s)).map(str::to_string) else {
        return Err("bad_request");
    };

    let lmeta = std::fs::symlink_metadata(&target).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "file_missing"
        } else {
            "file_unreadable"
        }
    })?;
    if lmeta.file_type().is_symlink() || !lmeta.file_type().is_file() {
        if lmeta.file_type().is_dir() {
            if let Err(code @ "folder_protected") = crate::folders::check_folder(shared, &full_str) {
                return Err(code);
            }
        }
        return Err("file_not_regular");
    }

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(&target)
        .map_err(|_| "file_unreadable")?;
    let fd_meta = file.metadata().map_err(|_| "file_unreadable")?;
    if !fd_meta.is_file() {
        return Err("file_not_regular");
    }

    let mut raw = Vec::new();
    file.take((MAX_PREVIEW_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|_| "file_unreadable")?;
    let truncated = raw.len() > MAX_PREVIEW_BYTES;
    if truncated {
        raw.truncate(MAX_PREVIEW_BYTES);
    }

    let Some(decoded) = decode_utf8_preview(&raw, truncated) else {
        return Ok(obj(json!({
            "path": full_str,
            "cwd": canon_parent,
            "name": name,
            "size": fd_meta.len(),
            "binary": true,
            "truncated": false,
            "text": "",
        })));
    };
    let clean = sanitize_preview_text(decoded);
    Ok(obj(json!({
        "path": full_str,
        "cwd": canon_parent,
        "name": name,
        "size": fd_meta.len(),
        "binary": false,
        "truncated": truncated,
        "text": clean,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempTree(PathBuf);
    impl TempTree {
        fn new() -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("lico-fsview-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dummy_shared(state_root: &Path) -> std::sync::Arc<Shared> {
        for sub in ["reg", "logs", "accounts", "cred", "ws", "handoff"] {
            std::fs::create_dir_all(state_root.join(sub)).unwrap();
        }
        let accounts_dir = state_root.join("accounts");
        let cfg = crate::core::Config {
            login_bin: None,
            login_args: Vec::new(),
            login_timeout: 5,
            log_dir: state_root.join("logs"),
            agent: PathBuf::from("/bin/true"),
            retain: 64,
            window: 64,
            queue_bytes: 65536,
            payload: 64,
            max_sessions: 8,
            codex: PathBuf::from("/bin/true"),
            codex_args: Vec::new(),
            claude: PathBuf::from("/bin/true"),
            claude_args: Vec::new(),
            cred_bin: PathBuf::from("/bin/true"),
            cred_args: Vec::new(),
            cred_dir: state_root.join("cred"),
            job_bin: PathBuf::from("/bin/true"),
            workspace_root: state_root.join("ws"),
            handoff_dir: state_root.join("handoff"),
            accounts_dir: accounts_dir.clone(),
            registry_dir: state_root.join("reg"),
            text: true,
            realistic_text: false,
        };
        std::sync::Arc::new(Shared {
            cfg,
            sessions: std::sync::RwLock::new(Vec::new()),
            views: std::sync::Mutex::new(std::collections::HashMap::new()),
            started: std::time::Instant::now(),
            pending_creates: std::sync::atomic::AtomicUsize::new(0),
            accounts: std::sync::Mutex::new(crate::accounts::Accounts::load(&accounts_dir).unwrap()),
            logins: std::sync::Mutex::new(std::collections::HashMap::new()),
            clip: Default::default(),
        })
    }

    #[test]
    fn folder_list_sorts_dirs_first_marks_html_and_skips_hidden_names() {
        let tree = TempTree::new();
        let state = tree.0.join("state");
        let shared = dummy_shared(&state);
        let work = tree.0.join("work");
        std::fs::create_dir_all(work.join("zeta")).unwrap();
        std::fs::create_dir_all(work.join("Alpha")).unwrap();
        std::fs::write(work.join("report.HTML"), b"<h1>ok</h1>").unwrap();
        std::fs::write(work.join("notes.md"), b"hello").unwrap();
        std::fs::write(work.join("bad\u{202E}name.txt"), b"skip").unwrap();
        std::os::unix::fs::symlink(work.join("notes.md"), work.join("shortcut")).unwrap();

        let res = folder_list(&shared, work.to_str().unwrap()).expect("folder_list ok");
        let entries = res["entries"].as_array().unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["Alpha", "zeta", "notes.md", "report.HTML", "shortcut"]);
        assert_eq!(entries[0]["kind"], json!("dir"));
        assert_eq!(entries[2]["is_html"], json!(false));
        assert_eq!(entries[3]["is_html"], json!(true));
        assert_eq!(entries[4]["kind"], json!("link"));
    }

    #[test]
    fn folder_list_and_read_refuse_protected_state_dirs_and_symlinks() {
        let tree = TempTree::new();
        let state = tree.0.join("state");
        let shared = dummy_shared(&state);
        let work = tree.0.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let secret = state.join("logs").join("secret.txt");
        std::fs::write(&secret, b"private").unwrap();
        let real = work.join("real.txt");
        std::fs::write(&real, b"ok").unwrap();
        let link = work.join("link.txt");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert_eq!(folder_list(&shared, state.join("logs").to_str().unwrap()), Err("folder_protected"));
        assert_eq!(folder_read(&shared, secret.to_str().unwrap()), Err("folder_protected"));
        assert_eq!(folder_read(&shared, link.to_str().unwrap()), Err("file_not_regular"));
        assert_eq!(folder_read(&shared, work.to_str().unwrap()), Err("file_not_regular"));
        assert_eq!(folder_read(&shared, "relative/path.txt"), Err("folder_not_absolute"));
        assert_eq!(folder_read(&shared, work.join("missing.txt").to_str().unwrap()), Err("file_missing"));
    }

    #[test]
    fn folder_read_detects_binary_normalizes_crlf_and_truncates_utf8_cleanly() {
        let tree = TempTree::new();
        let state = tree.0.join("state");
        let shared = dummy_shared(&state);
        let work = tree.0.join("work");
        std::fs::create_dir_all(&work).unwrap();

        let bin = work.join("image.png");
        std::fs::write(&bin, b"\x89PNG\r\n\x1a\n\x00\x01").unwrap();
        let r_bin = folder_read(&shared, bin.to_str().unwrap()).unwrap();
        assert_eq!(r_bin["binary"], json!(true));
        assert_eq!(r_bin["text"], json!(""));

        let txt = work.join("readme.txt");
        std::fs::write(&txt, "line1\r\nline2\r한글\u{202E}\tkept\n").unwrap();
        let r_txt = folder_read(&shared, txt.to_str().unwrap()).unwrap();
        assert_eq!(r_txt["binary"], json!(false));
        assert_eq!(r_txt["truncated"], json!(false));
        assert_eq!(r_txt["text"], json!("line1\nline2\n한글\tkept\n"));

        // 65,535 ASCII bytes + 3-byte '한' straddling the 65,536-byte boundary
        let mut big = vec![b'a'; MAX_PREVIEW_BYTES - 1];
        big.extend_from_slice("한글".as_bytes());
        let big_path = work.join("big.txt");
        std::fs::write(&big_path, &big).unwrap();
        let r_big = folder_read(&shared, big_path.to_str().unwrap()).unwrap();
        assert_eq!(r_big["binary"], json!(false));
        assert_eq!(r_big["truncated"], json!(true));
        assert_eq!(r_big["text"].as_str().unwrap().len(), MAX_PREVIEW_BYTES - 1);
    }
}
