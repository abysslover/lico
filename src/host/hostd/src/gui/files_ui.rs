//! In-window Working Directory File Viewer (`M04-P04`, paged navigation `M05-P02`, unified open `M05-P03`,
//! hidden-file `.dotfiles` toggle `M06-P01`; `Host` combo-box & async SSH live in `files_host.rs`).

use std::process::{Command, Stdio};

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::super::App;
use super::files_render::{calc_page_size, refusal_text, wrap_preview};
use super::Step;

pub const HOST_POPUP_MAX_VIS: usize = 12;

#[derive(Clone, Debug)]
pub struct FileItem {
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub is_html: bool,
    pub external: bool,
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

#[derive(Clone, Debug)]
pub struct PrevHostSnap {
    pub ssh_host: Option<String>,
    pub cwd: String,
    pub parent: Option<String>,
    pub entries: Vec<FileItem>,
    pub truncated: bool,
    pub cursor: usize,
}

pub struct Explorer {
    pub ssh_host: Option<String>,
    pub ssh_hosts: Vec<String>,
    pub host_popup_open: bool,
    pub host_popup_sel: usize,
    pub host_popup_top: usize,
    pub pending_req: u64,
    pub loading: Option<String>,
    pub unreachable: bool,
    pub prev_host: Option<PrevHostSnap>,
    pub cwd: String,
    pub parent: Option<String>,
    pub entries: Vec<FileItem>,
    pub truncated: bool,
    pub cursor: usize,
    pub preview: Option<FilePreview>,
    pub load_count: u64,
    pub refresh_count: u64,
    pub last_opened: Option<String>,
    pub preview_from_sidebar: bool,
}

impl Explorer {
    pub(super) fn new(ssh_host: Option<String>, ssh_hosts: Vec<String>, cwd: String) -> Self {
        Self {
            ssh_host,
            ssh_hosts,
            host_popup_open: false,
            host_popup_sel: 0,
            host_popup_top: 0,
            pending_req: 0,
            loading: None,
            unreachable: false,
            prev_host: None,
            cwd,
            parent: None,
            entries: Vec::new(),
            truncated: false,
            cursor: 0,
            preview: None,
            load_count: 0,
            refresh_count: 0,
            last_opened: None,
            preview_from_sidebar: false,
        }
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

pub(crate) fn parse_entries(reply: &Value) -> Vec<FileItem> {
    reply["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            let is_html = e["is_html"] == json!(true);
            FileItem {
                name: e["name"].as_str().unwrap_or("").to_string(),
                kind: e["kind"].as_str().unwrap_or("file").to_string(),
                size: e["size"].as_u64().unwrap_or(0),
                is_html,
                external: e["external"] == json!(true) || is_html,
            }
        })
        .collect()
}

impl App {
    pub(crate) fn files_page_nav_state(&self) -> (bool, bool) {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return (false, false);
        };
        if ex.unreachable || (ex.loading.is_some() && ex.entries.is_empty()) {
            return (false, false);
        }
        if let Some(pv) = &ex.preview {
            let ps = calc_page_size(self.ui.panel_rows, true);
            return (pv.top > 0, pv.top + ps < pv.lines.len());
        }
        let ps = calc_page_size(self.ui.panel_rows, false);
        let (_, _, page_idx, total_pages) = ex.page_window(ps);
        (page_idx > 0, page_idx + 1 < total_pages)
    }

    pub(crate) fn files_step_page(&mut self, dir: isize) {
        if self.files_host_popup_open() {
            self.move_host_popup_sel(dir.saturating_mul(HOST_POPUP_MAX_VIS as isize));
            return;
        }
        let panel_rows = self.ui.panel_rows;
        let Some(ex) = self
            .ui
            .panel
            .as_mut()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_mut())
        else {
            return;
        };
        if let Some(pv) = ex.preview.as_mut() {
            let ps = calc_page_size(panel_rows, true);
            let max_top = pv.lines.len().saturating_sub(ps) as isize;
            pv.top = (pv.top as isize).saturating_add(dir.saturating_mul(ps as isize)).clamp(0, max_top) as usize;
            self.dirty = true;
            return;
        }
        let ps = calc_page_size(panel_rows, false);
        let (_, _, page_idx, total_pages) = ex.page_window(ps);
        let max_idx = ex.nav_len().saturating_sub(1);
        if dir > 0 {
            if page_idx + 1 < total_pages {
                ex.cursor = ((page_idx + 1) * ps).min(max_idx);
            } else {
                ex.cursor = max_idx;
            }
        } else if page_idx > 0 {
            ex.cursor = (page_idx - 1) * ps;
        } else {
            ex.cursor = 0;
        }
        self.dirty = true;
    }

    pub(crate) fn files_wheel(&mut self, lines: isize) -> bool {
        let is_files = self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Files);
        if !is_files {
            return false;
        }
        if lines == 0 {
            return true;
        }
        let step = -lines;
        if self.files_host_popup_open() {
            self.move_host_popup_sel(step.clamp(-4, 4));
            return true;
        }
        let in_preview = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).is_some_and(|ex| ex.preview.is_some());
        if in_preview {
            self.files_scroll_preview(step);
        } else {
            self.files_move_cursor(step);
        }
        true
    }

    pub(crate) fn initial_files_cwd(&self) -> String {
        if let Some(row) = self.selected_row().filter(|r| r.ssh_host.is_empty()) {
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
        let remote_sel = self
            .selected_row()
            .filter(|r| !r.ssh_host.is_empty())
            .map(|r| (r.ssh_host.clone(), if r.cwd.is_empty() { "~".to_string() } else { r.cwd.clone() }));
        let start = remote_sel.as_ref().map_or_else(|| self.initial_files_cwd(), |(_, d)| d.clone());
        let init_host = remote_sel.map(|(h, _)| h);
        let ssh_hosts = self.fetch_ssh_host_aliases();
        self.panel_open(Step::Files);
        if let Some(p) = self.ui.panel.as_mut() {
            p.files = Some(Explorer::new(init_host.clone(), ssh_hosts, start.clone()));
        }
        if let Some(host) = init_host {
            self.files_start_remote_dir(&host, &start, false, start != "~", false);
        } else if !self.files_load_dir(&start, false) && start != "/" {
            let _ = self.files_load_dir("/", false);
        }
    }

    pub(crate) fn files_open_sidebar_preview(&mut self, ssh_host: Option<String>, cwd: &str, full_path: &str) {
        let ssh_hosts = self.fetch_ssh_host_aliases();
        self.panel_open(Step::Files);
        if let Some(p) = self.ui.panel.as_mut() {
            let mut ex = Explorer::new(ssh_host.clone(), ssh_hosts, cwd.to_string());
            ex.preview_from_sidebar = true;
            p.files = Some(ex);
        }
        if let Some(host) = ssh_host {
            self.files_start_remote_preview(&host, full_path, false);
        } else if self.files_load_preview(full_path, true) {
            let delegated_external = self
                .ui
                .panel
                .as_ref()
                .and_then(|p| p.files.as_ref())
                .is_some_and(|ex| ex.preview.is_none());
            if delegated_external {
                self.panel_close();
                self.sync_view_after_panel_close();
            }
        }
    }

    pub(crate) fn files_open_at(&mut self, dir: &str) {
        let ssh_hosts = self.fetch_ssh_host_aliases();
        self.panel_open(Step::Files);
        if let Some(p) = self.ui.panel.as_mut() {
            p.files = Some(Explorer::new(None, ssh_hosts, dir.to_string()));
        }
        if !self.files_load_dir(dir, false) {
            self.panel_close();
            self.persist_workspace(Some(""), Some("start"), None);
        }
    }

    pub(crate) fn files_load_dir(&mut self, dir: &str, keep_cursor: bool) -> bool {
        let ssh_host = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).and_then(|ex| ex.ssh_host.clone());
        if let Some(host) = ssh_host {
            self.files_start_remote_dir(&host, dir, keep_cursor, false, false);
            return true;
        }
        let mut req = json!({"op": "folder.list", "cwd": dir});
        if self.ui.show_hidden {
            req["show_hidden"] = json!(true);
        }
        let reply = self.host_call_within(req, 4);
        let Some(p) = self.ui.panel.as_mut() else { return false };
        if reply["ok"] != json!(1) {
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            p.last_error = err.clone();
            p.notice = refusal_text(&err);
            self.dirty = true;
            return false;
        }
        let entries = parse_entries(&reply);
        let canon = reply["cwd"].as_str().unwrap_or(dir).to_string();
        if let Some(ex) = p.files.as_mut() {
            ex.cwd = canon.clone();
            ex.parent = reply["parent"].as_str().map(str::to_string);
            ex.entries = entries;
            ex.truncated = reply["truncated"] == json!(true);
            ex.preview = None;
            ex.unreachable = false;
            ex.loading = None;
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

    fn files_load_preview(&mut self, path: &str, delegate_binary: bool) -> bool {
        let ssh_host = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).and_then(|ex| ex.ssh_host.clone());
        if let Some(host) = ssh_host {
            self.files_start_remote_preview(&host, path, false);
            return true;
        }
        let req = json!({"op": "folder.read", "path": path});
        let reply = self.host_call_within(req, 4);
        let max_cols = self.files_row_cols();
        let ps = calc_page_size(self.ui.panel_rows, true);
        if reply["ok"] == json!(1) && reply["binary"] == json!(true) && delegate_binary {
            let full = reply["path"].as_str().unwrap_or(path).to_string();
            let name = reply["name"].as_str().unwrap_or(path).to_string();
            self.open_external_target(&full, &name);
            return true;
        }
        let Some(p) = self.ui.panel.as_mut() else { return false };
        if reply["ok"] != json!(1) {
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            p.last_error = err.clone();
            p.notice = refusal_text(&err);
            self.dirty = true;
            return false;
        }
        let binary = reply["binary"] == json!(true);
        let lines = if binary { Vec::new() } else { wrap_preview(reply["text"].as_str().unwrap_or(""), max_cols) };
        if let Some(ex) = p.files.as_mut() {
            let prev_top = ex.preview.as_ref().filter(|pv| pv.path == path).map_or(0, |pv| pv.top);
            let max_top = lines.len().saturating_sub(ps);
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

    /// Toggle persistent dotfile visibility (`show_hidden`, default `false`) and reload any open directory listing.
    pub(crate) fn toggle_hidden_files(&mut self) {
        let next = !self.ui.show_hidden;
        let reply = self.host_call(json!({"op": "settings.set", "show_hidden": next}));
        if reply["ok"] == json!(1) {
            self.sync_settings(&reply["settings"]);
            self.send("{\"op\":\"status\"}");
            let state_word = if self.ui.show_hidden { "shown" } else { "hidden" };
            let open_dir = self
                .ui
                .panel
                .as_ref()
                .filter(|p| p.step == Step::Files)
                .and_then(|p| p.files.as_ref())
                .filter(|ex| ex.preview.is_none() && !ex.unreachable && ex.loading.is_none())
                .map(|ex| ex.cwd.clone());
            if let Some(dir) = open_dir {
                let _ = self.files_load_dir(&dir, true);
                if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = format!("Hidden files: {state_word}.");
                }
            } else {
                self.ui.flash = Some(format!("Hidden files: {state_word}."));
            }
            self.sidebar_sync_hidden_toggle();
        } else {
            self.ui.flash = Some("Could not change hidden-file setting.".into());
        }
        self.dirty = true;
    }

    /// Explicit refresh (`F5` or toolbar `Refresh`): the only way an already-opened directory or file preview re-reads disk.
    pub(crate) fn files_refresh(&mut self) {
        let Some((ssh_host, unreachable, cwd, preview_path)) = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .map(|ex| (ex.ssh_host.clone(), ex.unreachable, ex.cwd.clone(), ex.preview.as_ref().map(|pv| pv.path.clone())))
        else {
            return;
        };
        if let Some(p) = self.ui.panel.as_mut() {
            if let Some(ex) = p.files.as_mut() {
                ex.refresh_count += 1;
            }
        }
        if let Some(host) = ssh_host {
            let target_dir = if unreachable || cwd.is_empty() { "~" } else { cwd.as_str() };
            match preview_path {
                Some(path) if !unreachable => self.files_start_remote_preview(&host, &path, true),
                _ => self.files_start_remote_dir(&host, target_dir, !unreachable, false, true),
            }
            return;
        }
        let ok = match preview_path {
            Some(path) => self.files_load_preview(&path, false),
            None => self.files_load_dir(&cwd, true),
        };
        if ok {
            if let Some(p) = self.ui.panel.as_mut() {
                p.notice = "Refreshed.".into();
            }
        }
        self.dirty = true;
    }

    pub(crate) fn files_new_session_here(&mut self) {
        if let Some(why) = self.files_new_here_reason() {
            self.ui.flash = Some(format!("+ New here: {why}"));
            self.dirty = true;
            return;
        }
        let target = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .map(|ex| (ex.cwd.clone(), ex.ssh_host.clone()))
            .or_else(|| {
                self.selected_row()
                    .filter(|r| !r.cwd.is_empty())
                    .map(|r| (r.cwd.clone(), (!r.ssh_host.is_empty()).then(|| r.ssh_host.clone())))
            });
        match target {
            Some((cwd, ssh_host)) => self.form_open_at_host(cwd, ssh_host),
            None => self.form_open(),
        }
    }

    fn open_external_target(&mut self, target: &str, display_name: &str) {
        let custom = std::env::var("LICO_OPEN_CMD").ok().filter(|s| !s.trim().is_empty());
        let notice = if let Some(cmd) = custom {
            if spawn_external(&cmd, target) {
                format!("Opened with system default: {display_name}")
            } else {
                format!("Could not run {cmd} for {display_name}.")
            }
        } else if std::env::var_os("LICO_NO_AUTO_OPEN").is_some() {
            format!("System open suppressed (LICO_NO_AUTO_OPEN): {display_name}")
        } else if spawn_external("xdg-open", target) {
            format!("Opened with system default: {display_name}")
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

    fn files_activate(&mut self) {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return;
        };
        if ex.loading.is_some() && ex.entries.is_empty() && ex.preview.is_none() {
            self.toggle_files_host_popup();
            return;
        }
        if ex.unreachable {
            if ex.cursor == 0 {
                self.toggle_files_host_popup();
            } else {
                let _ = self.restore_prev_files_host();
            }
            return;
        }
        if ex.cursor == 0 {
            let (cwd, ssh_host) = (ex.cwd.clone(), ex.ssh_host.clone());
            self.form_open_at_host(cwd, ssh_host);
            return;
        }
        if let (1, Some(parent)) = (ex.cursor, ex.parent.clone()) {
            let _ = self.files_load_dir(&parent, false);
            return;
        }
        let Some(item) = ex.selected_entry().cloned() else { return };
        let ssh_host = ex.ssh_host.clone();
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
            _ if item.external => {
                if let Some(h) = ssh_host {
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.notice = format!("Remote non-text file on ssh:{h} ({}) — local system opener applies only to local files.", item.name);
                    }
                    self.dirty = true;
                } else {
                    self.open_external_target(&full, &item.name);
                }
            }
            _ => {
                let _ = self.files_load_preview(&full, true);
            }
        }
    }

    pub(crate) fn files_click(&mut self, line: usize) -> bool {
        let ps = calc_page_size(self.ui.panel_rows, false);
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return false;
        };
        if ex.preview.is_some() {
            if line == 0 {
                let from_sb = ex.preview_from_sidebar;
                if from_sb {
                    self.panel_close();
                    self.sync_view_after_panel_close();
                } else if let Some(p) = self.ui.panel.as_mut() {
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
        let (start, end, _, _) = ex.page_window(ps);
        if line >= end.saturating_sub(start) {
            return false;
        }
        let target = start + line;
        if let Some(p) = self.ui.panel.as_mut() {
            if let Some(ex) = p.files.as_mut() {
                ex.cursor = target;
            }
        }
        self.files_activate();
        true
    }

    pub(crate) fn files_key(&mut self, event: &KeyEvent) -> bool {
        let is_files = self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Files);
        if !is_files {
            return false;
        }
        if self.files_host_popup_open() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) if !event.repeat => self.close_files_host_popup(),
                Key::Named(NamedKey::Enter | NamedKey::Space) if !event.repeat => {
                    let sel = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).map_or(0, |ex| ex.host_popup_sel);
                    self.select_files_host_index(sel);
                }
                Key::Named(NamedKey::ArrowUp) => self.move_host_popup_sel(-1),
                Key::Named(NamedKey::ArrowDown) => self.move_host_popup_sel(1),
                Key::Named(NamedKey::PageUp) => self.move_host_popup_sel(-(HOST_POPUP_MAX_VIS as isize)),
                Key::Named(NamedKey::PageDown) => self.move_host_popup_sel(HOST_POPUP_MAX_VIS as isize),
                Key::Named(NamedKey::Home) => self.move_host_popup_sel(isize::MIN / 2),
                Key::Named(NamedKey::End) => self.move_host_popup_sel(isize::MAX / 2),
                Key::Character(c) if !event.repeat && c.eq_ignore_ascii_case("h") && !self.ui.mods.control_key() => {
                    self.close_files_host_popup();
                }
                _ => {}
            }
            return true;
        }
        if self.ui.mods.control_key() && !self.ui.mods.alt_key() && !self.ui.mods.shift_key() {
            if let Key::Character(c) = &event.logical_key {
                if c.eq_ignore_ascii_case("h") {
                    if !event.repeat {
                        self.toggle_hidden_files();
                    }
                    return true;
                }
            }
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
        let is_loading = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).is_some_and(|ex| ex.loading.is_some());
        if is_loading && matches!(&event.logical_key, Key::Named(NamedKey::Escape)) && !event.repeat {
            self.cancel_files_loading();
            return true;
        }
        let in_preview = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).is_some_and(|ex| ex.preview.is_some());
        if in_preview {
            match &event.logical_key {
                Key::Named(NamedKey::Escape | NamedKey::Backspace) if !event.repeat => {
                    let from_sb = self
                        .ui
                        .panel
                        .as_ref()
                        .and_then(|p| p.files.as_ref())
                        .is_some_and(|ex| ex.preview_from_sidebar);
                    if from_sb {
                        self.panel_close();
                        self.sync_view_after_panel_close();
                    } else if let Some(p) = self.ui.panel.as_mut() {
                        if let Some(ex) = p.files.as_mut() {
                            ex.preview = None;
                        }
                        p.notice.clear();
                    }
                    self.dirty = true;
                }
                Key::Named(NamedKey::ArrowUp) => self.files_scroll_preview(-1),
                Key::Named(NamedKey::ArrowDown) => self.files_scroll_preview(1),
                Key::Named(NamedKey::PageUp) => self.files_step_page(-1),
                Key::Named(NamedKey::PageDown) => self.files_step_page(1),
                Key::Named(NamedKey::Home) => self.files_scroll_preview(isize::MIN / 2),
                Key::Named(NamedKey::End) => self.files_scroll_preview(isize::MAX / 2),
                Key::Character(c) if !event.repeat => match c.as_str() {
                    "[" => self.files_step_page(-1),
                    "]" => self.files_step_page(1),
                    _ if c.eq_ignore_ascii_case("h") => self.toggle_files_host_popup(),
                    _ if c.eq_ignore_ascii_case("n") => self.files_new_session_here(),
                    _ => {}
                },
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
                let is_unreach = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).is_some_and(|ex| ex.unreachable);
                if is_unreach {
                    let _ = self.restore_prev_files_host();
                } else if let Some(parent) = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()).and_then(|ex| ex.parent.clone()) {
                    let _ = self.files_load_dir(&parent, false);
                }
            }
            Key::Named(NamedKey::Enter) if !event.repeat => self.files_activate(),
            Key::Named(NamedKey::ArrowUp) => self.files_move_cursor(-1),
            Key::Named(NamedKey::ArrowDown) => self.files_move_cursor(1),
            Key::Named(NamedKey::PageUp) => self.files_step_page(-1),
            Key::Named(NamedKey::PageDown) => self.files_step_page(1),
            Key::Named(NamedKey::Home) => self.files_move_cursor(isize::MIN / 2),
            Key::Named(NamedKey::End) => self.files_move_cursor(isize::MAX / 2),
            Key::Character(c) if !event.repeat => match c.to_ascii_lowercase().as_str() {
                "[" => self.files_step_page(-1),
                "]" => self.files_step_page(1),
                "." => self.toggle_hidden_files(),
                "h" => self.toggle_files_host_popup(),
                "n" => self.files_new_session_here(),
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
        let ps = calc_page_size(self.ui.panel_rows, true);
        if let Some(pv) = self.ui.panel.as_mut().and_then(|p| p.files.as_mut()).and_then(|ex| ex.preview.as_mut()) {
            let max_top = pv.lines.len().saturating_sub(ps) as isize;
            pv.top = (pv.top as isize).saturating_add(delta).clamp(0, max_top) as usize;
            self.dirty = true;
        }
    }
}
