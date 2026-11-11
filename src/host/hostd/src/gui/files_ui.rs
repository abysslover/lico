//! In-window Working Directory File Viewer (`M04-P04`).
//!
//! Opens a working directory strictly on demand (`Files` toolbar button, `View > Working directory files`, `Ctrl+E`),
//! refreshes only on explicit `F5` or toolbar `Refresh`, starts a new session directly in the viewed directory
//! (`+ New here` / `N`), previews regular UTF-8 files (`<= 64 KiB`), and launches `.html`/`.htm` files in the
//! external browser (`xdg-open`, respecting `LICO_OPEN_CMD` and `LICO_NO_AUTO_OPEN`).

use std::process::{Command, Stdio};

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::super::editor::{hidden, wide};
use super::super::App;
use super::Step;

const SHOWN: usize = 14;
const WIDE: usize = 96;

#[derive(Clone, Debug)]
pub struct FileItem {
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub is_html: bool,
}

#[derive(Clone, Debug)]
pub struct FilePreview {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub binary: bool,
    pub truncated: bool,
    pub lines: Vec<String>,
    pub top: usize,
}

pub struct Explorer {
    pub cwd: String,
    pub parent: Option<String>,
    pub entries: Vec<FileItem>,
    pub truncated: bool,
    pub cursor: usize,
    pub preview: Option<FilePreview>,
    pub load_count: u64,
    pub refresh_count: u64,
    pub last_opened: Option<String>,
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn wrap_preview(text: &str) -> Vec<String> {
    let mut parts: Vec<&str> = text.split('\n').collect();
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    let mut out = Vec::new();
    for part in parts {
        let (mut line, mut cols) = (String::new(), 0);
        for c in part.chars().flat_map(|c| if c == '\t' { vec![' '; 4] } else { vec![c] }) {
            if c.is_control() || hidden(c) {
                continue;
            }
            let w = if wide(c) { 2 } else { 1 };
            if cols + w > WIDE {
                out.push(std::mem::take(&mut line));
                cols = 0;
            }
            line.push(c);
            cols += w;
        }
        out.push(line);
    }
    out
}

fn refusal_text(code: &str) -> String {
    match code {
        "folder_missing" => "That folder no longer exists.".into(),
        "folder_not_directory" => "That path is not a directory.".into(),
        "folder_unreadable" => "That directory cannot be read.".into(),
        "folder_protected" => "Lico keeps its own private state there.".into(),
        "file_missing" => "That file no longer exists (press F5 to refresh).".into(),
        "file_not_regular" => "Only regular files are previewed (symlinks and special files are skipped).".into(),
        "file_unreadable" => "That file could not be read.".into(),
        other => format!("Could not read ({other})."),
    }
}

fn spawn_external(program: &str, target: &str) -> bool {
    match Command::new(program)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

impl Explorer {
    fn nav_len(&self) -> usize {
        1 + usize::from(self.parent.is_some()) + self.entries.len()
    }

    fn window(&self) -> (usize, usize) {
        let total = self.nav_len();
        let max_start = total.saturating_sub(SHOWN);
        let start = self.cursor.saturating_sub(SHOWN / 2).min(max_start);
        (start, (start + SHOWN).min(total))
    }

    fn row_label(&self, nav_idx: usize) -> String {
        let mark = if nav_idx == self.cursor { '>' } else { ' ' };
        if nav_idx == 0 {
            return format!("{mark} [+ New session in this directory]   (N or click)");
        }
        let mut rest = nav_idx - 1;
        if self.parent.is_some() {
            if rest == 0 {
                return format!("{mark} [..]  (parent directory)");
            }
            rest -= 1;
        }
        let Some(item) = self.entries.get(rest) else { return String::new() };
        match item.kind.as_str() {
            "dir" => format!("{mark} [dir]   {}/", item.name),
            "link" => format!("{mark} [link]  {}  (symlink)", item.name),
            _ if item.is_html => format!("{mark} [html]  {}  ({})  [open in browser]", item.name, format_bytes(item.size)),
            _ => format!("{mark} [file]  {}  ({})", item.name, format_bytes(item.size)),
        }
    }

    fn selected_entry(&self) -> Option<&FileItem> {
        let base = 1 + usize::from(self.parent.is_some());
        self.cursor.checked_sub(base).and_then(|i| self.entries.get(i))
    }
}

impl App {
    fn initial_files_cwd(&self) -> String {
        if let Some(row) = self.selected_row() {
            if !row.worktree_repo.is_empty() {
                return row.worktree_repo.clone();
            }
            if !row.cwd.is_empty() {
                return row.cwd.clone();
            }
        }
        if !self.ui.saved_cwd.is_empty() {
            return self.ui.saved_cwd.clone();
        }
        let opts = self.host_call_within(json!({"op": "session.options"}), 3);
        if let Some(cwd) = opts["last"]["cwd"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| opts["recent"].as_array().and_then(|a| a.first()).and_then(Value::as_str))
        {
            return cwd.to_string();
        }
        std::env::current_dir()
            .ok()
            .and_then(|d| d.to_str().map(str::to_string))
            .unwrap_or_else(|| "/".to_string())
    }

    pub(crate) fn files_open(&mut self) {
        let start = self.initial_files_cwd();
        self.panel_open(Step::Files);
        if let Some(p) = self.ui.panel.as_mut() {
            p.files = Some(Explorer {
                cwd: start.clone(),
                parent: None,
                entries: Vec::new(),
                truncated: false,
                cursor: 0,
                preview: None,
                load_count: 0,
                refresh_count: 0,
                last_opened: None,
            });
        }
        if !self.files_load_dir(&start, false) && start != "/" {
            let _ = self.files_load_dir("/", false);
        }
    }

    pub(crate) fn files_open_at(&mut self, dir: &str) {
        self.panel_open(Step::Files);
        if let Some(p) = self.ui.panel.as_mut() {
            p.files = Some(Explorer {
                cwd: dir.to_string(),
                parent: None,
                entries: Vec::new(),
                truncated: false,
                cursor: 0,
                preview: None,
                load_count: 0,
                refresh_count: 0,
                last_opened: None,
            });
        }
        if !self.files_load_dir(dir, false) {
            self.panel_close();
            self.persist_workspace(Some(""), Some("start"), None);
        }
    }

    fn files_load_dir(&mut self, dir: &str, keep_cursor: bool) -> bool {
        let reply = self.host_call_within(json!({"op": "folder.list", "cwd": dir}), 4);
        let Some(p) = self.ui.panel.as_mut() else { return false };
        if reply["ok"] != json!(1) {
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            p.last_error = err.clone();
            p.notice = refusal_text(&err);
            self.dirty = true;
            return false;
        }
        let entries: Vec<FileItem> = reply["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| FileItem {
                name: e["name"].as_str().unwrap_or("").to_string(),
                kind: e["kind"].as_str().unwrap_or("file").to_string(),
                size: e["size"].as_u64().unwrap_or(0),
                is_html: e["is_html"] == json!(true),
            })
            .collect();
        let canon = reply["cwd"].as_str().unwrap_or(dir).to_string();
        if let Some(ex) = p.files.as_mut() {
            ex.cwd = canon.clone();
            ex.parent = reply["parent"].as_str().map(str::to_string);
            ex.entries = entries;
            ex.truncated = reply["truncated"] == json!(true);
            ex.preview = None;
            ex.load_count += 1;
            let max_idx = ex.nav_len().saturating_sub(1);
            ex.cursor = if keep_cursor { ex.cursor.min(max_idx) } else { 0 };
        }
        p.last_error.clear();
        p.notice.clear();
        self.persist_workspace(Some(&canon), Some("files"), None);
        self.dirty = true;
        true
    }

    fn files_load_preview(&mut self, path: &str) -> bool {
        let reply = self.host_call_within(json!({"op": "folder.read", "path": path}), 4);
        let Some(p) = self.ui.panel.as_mut() else { return false };
        if reply["ok"] != json!(1) {
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            p.last_error = err.clone();
            p.notice = refusal_text(&err);
            self.dirty = true;
            return false;
        }
        let binary = reply["binary"] == json!(true);
        let lines = if binary {
            Vec::new()
        } else {
            wrap_preview(reply["text"].as_str().unwrap_or(""))
        };
        if let Some(ex) = p.files.as_mut() {
            let prev_top = ex.preview.as_ref().filter(|pv| pv.path == path).map_or(0, |pv| pv.top);
            let max_top = lines.len().saturating_sub(SHOWN);
            ex.preview = Some(FilePreview {
                path: reply["path"].as_str().unwrap_or(path).to_string(),
                name: reply["name"].as_str().unwrap_or("").to_string(),
                size: reply["size"].as_u64().unwrap_or(0),
                binary,
                truncated: reply["truncated"] == json!(true),
                lines,
                top: prev_top.min(max_top),
            });
            ex.load_count += 1;
        }
        p.last_error.clear();
        p.notice.clear();
        self.dirty = true;
        true
    }

    /// Explicit refresh (`F5` or toolbar `Refresh`): the only way an already-opened directory or file preview re-reads disk.
    pub(crate) fn files_refresh(&mut self) {
        let Some((cwd, preview_path)) = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .map(|ex| (ex.cwd.clone(), ex.preview.as_ref().map(|pv| pv.path.clone())))
        else {
            return;
        };
        let ok = match preview_path {
            Some(path) => self.files_load_preview(&path),
            None => self.files_load_dir(&cwd, true),
        };
        if let Some(p) = self.ui.panel.as_mut() {
            if let Some(ex) = p.files.as_mut() {
                ex.refresh_count += 1;
            }
            if ok {
                p.notice = "Refreshed.".into();
            }
        }
        self.dirty = true;
    }

    pub(crate) fn files_new_session_here(&mut self) {
        let target = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .map(|ex| ex.cwd.clone())
            .or_else(|| self.selected_row().filter(|r| !r.cwd.is_empty()).map(|r| r.cwd.clone()));
        match target {
            Some(cwd) => self.form_open_at(cwd),
            None => self.form_open(),
        }
    }

    fn open_external_target(&mut self, target: &str, display_name: &str) {
        let custom = std::env::var("LICO_OPEN_CMD").ok().filter(|s| !s.trim().is_empty());
        let notice = if let Some(cmd) = custom {
            if spawn_external(&cmd, target) {
                format!("Opened in browser: {display_name}")
            } else {
                format!("Could not run {cmd} for {display_name}.")
            }
        } else if std::env::var_os("LICO_NO_AUTO_OPEN").is_some() {
            format!("Browser open suppressed (LICO_NO_AUTO_OPEN): {display_name}")
        } else if spawn_external("xdg-open", target) {
            format!("Opened in browser: {display_name}")
        } else {
            format!("xdg-open is not available: {target}")
        };
        if let Some(p) = self.ui.panel.as_mut() {
            if let Some(ex) = p.files.as_mut() {
                ex.last_opened = Some(target.to_string());
            }
            p.notice = notice;
        }
        self.dirty = true;
    }

    fn files_activate(&mut self, force_preview: bool) {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return;
        };
        if ex.cursor == 0 {
            let cwd = ex.cwd.clone();
            self.form_open_at(cwd);
            return;
        }
        if let (1, Some(parent)) = (ex.cursor, ex.parent.clone()) {
            let _ = self.files_load_dir(&parent, false);
            return;
        }
        let Some(item) = ex.selected_entry().cloned() else { return };
        let full = if ex.cwd == "/" { format!("/{}", item.name) } else { format!("{}/{}", ex.cwd, item.name) };
        match item.kind.as_str() {
            "dir" => {
                let _ = self.files_load_dir(&full, false);
            }
            "link" => {
                if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = "Symbolic links are not followed by the file viewer.".into();
                }
                self.dirty = true;
            }
            _ if item.is_html && !force_preview => {
                self.open_external_target(&full, &item.name);
            }
            _ => {
                let _ = self.files_load_preview(&full);
            }
        }
    }

    pub(crate) fn files_view(&self, p: &super::Panel) -> (String, Vec<String>) {
        let Some(ex) = p.files.as_ref() else {
            return ("Files".into(), Vec::new());
        };
        if let Some(pv) = &ex.preview {
            let mut body = vec!["[<- Back to directory]   (Backspace or Esc)".into(), String::new()];
            if pv.binary {
                body.push("(binary file — text preview is shown only for UTF-8 files)".into());
            } else if pv.lines.is_empty() || (pv.lines.len() == 1 && pv.lines[0].is_empty()) {
                body.push("(empty file)".into());
            } else {
                let end = (pv.top + SHOWN).min(pv.lines.len());
                body.extend(pv.lines[pv.top..end].iter().cloned());
                body.push(String::new());
                body.push(format!(
                    "lines {}-{} of {}{}   |   Up/Down/PgUp/PgDn scroll   F5 refresh   N new session here",
                    pv.top + 1,
                    end,
                    pv.lines.len(),
                    if pv.truncated { " (first 64 KiB)" } else { "" }
                ));
            }
            return (format!("File: {}  ({})", pv.path, format_bytes(pv.size)), body);
        }

        let (start, end) = ex.window();
        let mut body: Vec<String> = (start..end).map(|i| ex.row_label(i)).collect();
        body.push(String::new());
        body.push(format!(
            "{} entries{}   |   F5 refresh   N new session here   Enter open   V view HTML source",
            ex.entries.len(),
            if ex.truncated { " (first 500 shown)" } else { "" }
        ));
        (format!("Files: {}", ex.cwd), body)
    }

    pub(crate) fn files_json(&self) -> Value {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return Value::Null;
        };
        let entries: Vec<Value> = ex
            .entries
            .iter()
            .map(|e| json!({"name": e.name, "kind": e.kind, "size": e.size, "is_html": e.is_html}))
            .collect();
        let preview = ex.preview.as_ref().map(|pv| {
            json!({
                "path": pv.path,
                "name": pv.name,
                "size": pv.size,
                "binary": pv.binary,
                "truncated": pv.truncated,
                "lines_total": pv.lines.len(),
                "top": pv.top,
            })
        });
        json!({
            "cwd": ex.cwd,
            "parent": ex.parent,
            "mode": if ex.preview.is_some() { "preview" } else { "dir" },
            "cursor": ex.cursor,
            "entry_count": ex.entries.len(),
            "entries": entries,
            "truncated": ex.truncated,
            "load_count": ex.load_count,
            "refresh_count": ex.refresh_count,
            "last_opened": ex.last_opened,
            "preview": preview,
        })
    }

    pub(crate) fn files_click(&mut self, line: usize) -> bool {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return false;
        };
        if ex.preview.is_some() {
            if line == 0 {
                if let Some(p) = self.ui.panel.as_mut() {
                    if let Some(ex) = p.files.as_mut() {
                        ex.preview = None;
                    }
                    p.notice.clear();
                }
                self.dirty = true;
                return true;
            }
            return false;
        }
        let (start, end) = ex.window();
        if line >= end.saturating_sub(start) {
            return false;
        }
        let target = start + line;
        if let Some(p) = self.ui.panel.as_mut() {
            if let Some(ex) = p.files.as_mut() {
                ex.cursor = target;
            }
        }
        self.files_activate(false);
        true
    }

    pub(crate) fn files_key(&mut self, event: &KeyEvent) -> bool {
        let is_files = self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Files);
        if !is_files {
            return false;
        }
        if self.ui.mods.control_key() || self.ui.mods.alt_key() {
            return false;
        }
        if matches!(&event.logical_key, Key::Named(NamedKey::F5)) {
            if !event.repeat {
                self.files_refresh();
            }
            return true;
        }
        let in_preview = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).is_some_and(|ex| ex.preview.is_some());
        if in_preview {
            match &event.logical_key {
                Key::Named(NamedKey::Escape | NamedKey::Backspace) if !event.repeat => {
                    if let Some(p) = self.ui.panel.as_mut() {
                        if let Some(ex) = p.files.as_mut() {
                            ex.preview = None;
                        }
                        p.notice.clear();
                    }
                    self.dirty = true;
                }
                Key::Named(NamedKey::ArrowUp) => self.files_scroll_preview(-1),
                Key::Named(NamedKey::ArrowDown) => self.files_scroll_preview(1),
                Key::Named(NamedKey::PageUp) => self.files_scroll_preview(-(SHOWN as isize)),
                Key::Named(NamedKey::PageDown) => self.files_scroll_preview(SHOWN as isize),
                Key::Named(NamedKey::Home) => self.files_scroll_preview(isize::MIN / 2),
                Key::Named(NamedKey::End) => self.files_scroll_preview(isize::MAX / 2),
                Key::Character(c) if !event.repeat && c.eq_ignore_ascii_case("n") => self.files_new_session_here(),
                _ => {}
            }
            return true;
        }

        match &event.logical_key {
            Key::Named(NamedKey::Escape) if !event.repeat => {
                self.panel_close();
                self.sync_view_after_panel_close();
            }
            Key::Named(NamedKey::Backspace) if !event.repeat => {
                if let Some(parent) = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).and_then(|ex| ex.parent.clone()) {
                    let _ = self.files_load_dir(&parent, false);
                }
            }
            Key::Named(NamedKey::Enter) if !event.repeat => self.files_activate(false),
            Key::Named(NamedKey::ArrowUp) => self.files_move_cursor(-1),
            Key::Named(NamedKey::ArrowDown) => self.files_move_cursor(1),
            Key::Named(NamedKey::PageUp) => self.files_move_cursor(-(SHOWN as isize)),
            Key::Named(NamedKey::PageDown) => self.files_move_cursor(SHOWN as isize),
            Key::Named(NamedKey::Home) => self.files_move_cursor(isize::MIN / 2),
            Key::Named(NamedKey::End) => self.files_move_cursor(isize::MAX / 2),
            Key::Character(c) if !event.repeat => match c.to_ascii_lowercase().as_str() {
                "n" => self.files_new_session_here(),
                "v" => self.files_activate(true),
                "o" => {
                    if let Some((cwd, item)) = self
                        .ui
                        .panel
                        .as_ref()
                        .and_then(|p| p.files.as_ref())
                        .and_then(|ex| Some((ex.cwd.clone(), ex.selected_entry()?.clone())))
                    {
                        if item.is_html {
                            let full = if cwd == "/" { format!("/{}", item.name) } else { format!("{cwd}/{}", item.name) };
                            self.open_external_target(&full, &item.name);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        true
    }

    fn files_move_cursor(&mut self, delta: isize) {
        if let Some(ex) = self.ui.panel.as_mut().and_then(|p| p.files.as_mut()) {
            let max_idx = ex.nav_len().saturating_sub(1) as isize;
            ex.cursor = (ex.cursor as isize).saturating_add(delta).clamp(0, max_idx) as usize;
            self.dirty = true;
        }
    }

    fn files_scroll_preview(&mut self, delta: isize) {
        if let Some(pv) = self.ui.panel.as_mut().and_then(|p| p.files.as_mut()).and_then(|ex| ex.preview.as_mut()) {
            let max_top = pv.lines.len().saturating_sub(SHOWN) as isize;
            pv.top = (pv.top as isize).saturating_add(delta).clamp(0, max_top) as usize;
            self.dirty = true;
        }
    }
}
