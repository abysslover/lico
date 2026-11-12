//! Combo-box `Host` dropdown selector and non-blocking SSH remote loader for `Files` (`M06-P03`).

use serde_json::{json, Value};

use super::super::App;
use super::files_render::{calc_page_size, refusal_text, wrap_preview};
use super::files_ui::{parse_entries, FilePreview, PrevHostSnap, HOST_POPUP_MAX_VIS};
use super::{AsyncJob, Step};

impl App {
    pub(crate) fn files_host_popup_open(&self) -> bool {
        self.ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .is_some_and(|ex| ex.host_popup_open)
    }

    pub(crate) fn close_files_host_popup(&mut self) {
        if let Some(ex) = self
            .ui
            .panel
            .as_mut()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_mut())
        {
            if ex.host_popup_open {
                ex.host_popup_open = false;
                self.dirty = true;
            }
        }
    }

    pub(crate) fn hover_files_host_item(&mut self, idx: usize) -> bool {
        let Some(ex) = self
            .ui
            .panel
            .as_mut()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_mut())
            .filter(|ex| ex.host_popup_open)
        else {
            return false;
        };
        let total = ex.host_choices().len();
        if idx < total && ex.host_popup_sel != idx {
            ex.host_popup_sel = idx;
            return true;
        }
        false
    }

    pub(crate) fn files_new_here_reason(&self) -> Option<&'static str> {
        let ex = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref())?;
        if ex.unreachable {
            Some("SSH host is unreachable")
        } else if ex.loading.is_some() && ex.entries.is_empty() {
            Some("connecting to SSH host")
        } else {
            None
        }
    }

    pub(super) fn fetch_ssh_host_aliases(&self) -> Vec<String> {
        if self.socket.is_empty() {
            return Vec::new();
        }
        let reply = self.host_call_within(json!({"op": "ssh.hosts"}), 2);
        reply["hosts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| h["host"].as_str().map(super::super::clean))
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub(crate) fn files_host_toolbar_state(&self) -> (String, bool, Option<&'static str>) {
        let Some(ex) = self.ui.panel.as_ref().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_ref()) else {
            return ("Host: local v".into(), false, None);
        };
        let reason = if ex.ssh_hosts.is_empty() && ex.ssh_host.is_none() {
            Some("no SSH hosts in ~/.ssh/config")
        } else {
            None
        };
        let active = ex.host_popup_open || ex.ssh_host.is_some();
        match &ex.ssh_host {
            Some(h) => (format!("Host: {h} v"), active, reason),
            None => ("Host: local v".into(), active, reason),
        }
    }

    pub(super) fn move_host_popup_sel(&mut self, delta: isize) {
        if let Some(ex) = self
            .ui
            .panel
            .as_mut()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_mut())
            .filter(|ex| ex.host_popup_open)
        {
            let max_idx = ex.host_choices().len().saturating_sub(1) as isize;
            ex.host_popup_sel = (ex.host_popup_sel as isize).saturating_add(delta).clamp(0, max_idx) as usize;
            ex.sync_popup_scroll(HOST_POPUP_MAX_VIS);
            self.dirty = true;
        }
    }

    /// Opens or closes the `Host` combo-box dropdown without triggering any SSH connection.
    pub(crate) fn toggle_files_host_popup(&mut self) {
        if self.files_host_popup_open() {
            self.close_files_host_popup();
            return;
        }
        let fresh = self.fetch_ssh_host_aliases();
        self.menu_close();
        let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) else { return };
        let Some(ex) = p.files.as_mut() else { return };
        ex.ssh_hosts = fresh;
        if ex.ssh_hosts.is_empty() && ex.ssh_host.is_none() {
            p.notice = "No SSH host aliases found in ~/.ssh/config.".into();
            self.dirty = true;
            return;
        }
        let choices = ex.host_choices();
        ex.host_popup_sel = choices.iter().position(|c| *c == ex.ssh_host).unwrap_or(0);
        ex.sync_popup_scroll(HOST_POPUP_MAX_VIS);
        ex.host_popup_open = true;
        self.dirty = true;
    }

    pub(crate) fn select_files_host_index(&mut self, idx: usize) {
        let choice = self
            .ui
            .panel
            .as_mut()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_mut())
            .and_then(|ex| {
                ex.host_popup_open = false;
                ex.host_choices().get(idx).cloned()
            });
        if let Some(target_host) = choice {
            self.switch_files_host(target_host);
        }
        self.dirty = true;
    }

    pub(super) fn switch_files_host(&mut self, next_host: Option<String>) {
        let already_ok = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .is_some_and(|ex| ex.ssh_host == next_host && !ex.unreachable && ex.loading.is_none());
        if already_ok {
            self.dirty = true;
            return;
        }
        self.ui.async_seq = self.ui.async_seq.wrapping_add(1).max(1);
        if let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) {
            p.notice.clear();
            p.last_error.clear();
            if let Some(ex) = p.files.as_mut() {
                if !ex.unreachable && ex.loading.is_none() && (!ex.cwd.is_empty() || !ex.entries.is_empty()) {
                    ex.prev_host = Some(PrevHostSnap {
                        ssh_host: ex.ssh_host.clone(),
                        cwd: ex.cwd.clone(),
                        parent: ex.parent.clone(),
                        entries: ex.entries.clone(),
                        truncated: ex.truncated,
                        cursor: ex.cursor,
                    });
                }
                ex.ssh_host = next_host.clone();
                ex.pending_req = 0;
                ex.loading = None;
                ex.unreachable = false;
                ex.entries.clear();
                ex.parent = None;
                ex.preview = None;
                ex.cursor = 0;
            }
        }
        match next_host {
            Some(h) => self.files_start_remote_dir(&h, "~", false, false, false),
            None => {
                let start = self.initial_files_cwd();
                if !self.files_load_dir(&start, false) && start != "/" {
                    let _ = self.files_load_dir("/", false);
                }
            }
        }
        self.dirty = true;
    }

    pub(super) fn restore_prev_files_host(&mut self) -> bool {
        self.ui.async_seq = self.ui.async_seq.wrapping_add(1).max(1);
        let snap = self
            .ui
            .panel
            .as_mut()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_mut())
            .and_then(|ex| ex.prev_host.take());
        if let Some(prev) = snap {
            if let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) {
                p.notice.clear();
                p.last_error.clear();
                if let Some(ex) = p.files.as_mut() {
                    ex.ssh_host = prev.ssh_host;
                    ex.cwd = prev.cwd;
                    ex.parent = prev.parent;
                    ex.entries = prev.entries;
                    ex.truncated = prev.truncated;
                    ex.cursor = prev.cursor;
                    ex.preview = None;
                    ex.pending_req = 0;
                    ex.loading = None;
                    ex.unreachable = false;
                }
            }
            self.dirty = true;
            return true;
        }
        self.switch_files_host(None);
        true
    }

    pub(super) fn cancel_files_loading(&mut self) {
        self.ui.async_seq = self.ui.async_seq.wrapping_add(1).max(1);
        let empty = self
            .ui
            .panel
            .as_ref()
            .filter(|p| p.step == Step::Files)
            .and_then(|p| p.files.as_ref())
            .is_some_and(|ex| ex.entries.is_empty() && ex.preview.is_none());
        if empty {
            let _ = self.restore_prev_files_host();
            if let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) {
                p.notice = "Cancelled SSH connection.".into();
            }
        } else if let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) {
            if let Some(ex) = p.files.as_mut() {
                ex.pending_req = 0;
                ex.loading = None;
            }
            p.notice = "Cancelled.".into();
        }
        self.dirty = true;
    }

    pub(super) fn files_start_remote_dir(&mut self, host: &str, dir: &str, keep_cursor: bool, fallback_tilde: bool, refreshed: bool) {
        let mut req = json!({"op": "folder.list", "cwd": dir, "ssh_host": host});
        if self.ui.show_hidden {
            req["show_hidden"] = json!(true);
        }
        let msg = if dir == "~" {
            format!("Connecting to ssh:{host}...")
        } else {
            format!("Loading ssh:{host}:{dir}...")
        };
        if let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) {
            if let Some(ex) = p.files.as_mut() {
                ex.unreachable = false;
                if !ex.entries.is_empty() || ex.preview.is_some() {
                    p.notice.clone_from(&msg);
                } else {
                    p.notice.clear();
                }
                ex.loading = Some(msg);
            }
        }
        self.dirty = true;
        let has_proxy = self.ui.proxy.is_some();
        let host_owned = host.to_string();
        let dir_owned = dir.to_string();
        let req_id = self.spawn_host_call(req, 7, move |req_id| AsyncJob::FilesDir {
            req_id,
            ssh_host: host_owned,
            dir: dir_owned,
            keep_cursor,
            fallback_tilde,
            refreshed,
        });
        if has_proxy {
            if let Some(ex) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_mut()) {
                ex.pending_req = req_id;
            }
        }
    }

    pub(super) fn files_start_remote_preview(&mut self, host: &str, path: &str, refreshed: bool) {
        let req = json!({"op": "folder.read", "path": path, "ssh_host": host});
        let msg = format!("Reading ssh:{host}:{path}...");
        if let Some(p) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files) {
            p.notice.clone_from(&msg);
            if let Some(ex) = p.files.as_mut() {
                ex.loading = Some(msg);
            }
        }
        self.dirty = true;
        let has_proxy = self.ui.proxy.is_some();
        let host_owned = host.to_string();
        let path_owned = path.to_string();
        let req_id = self.spawn_host_call(req, 7, move |req_id| AsyncJob::FilesPreview {
            req_id,
            ssh_host: host_owned,
            path: path_owned,
            refreshed,
        });
        if has_proxy {
            if let Some(ex) = self.ui.panel.as_mut().filter(|p| p.step == Step::Files).and_then(|p| p.files.as_mut()) {
                ex.pending_req = req_id;
            }
        }
    }

    pub(super) fn on_files_async(&mut self, job: AsyncJob, reply: Value) {
        let has_proxy = self.ui.proxy.is_some();
        match job {
            AsyncJob::FilesDir { req_id, ssh_host, dir, keep_cursor, fallback_tilde, refreshed } => {
                let matched = self
                    .ui
                    .panel
                    .as_ref()
                    .filter(|p| p.step == Step::Files)
                    .and_then(|p| p.files.as_ref())
                    .is_some_and(|ex| (ex.pending_req == req_id || !has_proxy) && ex.ssh_host.as_deref() == Some(ssh_host.as_str()));
                if !matched {
                    return;
                }
                if let Some(ex) = self.ui.panel.as_mut().and_then(|p| p.files.as_mut()) {
                    ex.pending_req = 0;
                    ex.loading = None;
                }
                if reply["ok"] != json!(1) {
                    let err = reply["error"].as_str().unwrap_or("unknown").to_string();
                    let transport_dead = matches!(err.as_str(), "ssh_unreachable" | "host_timeout" | "host_unreachable");
                    if fallback_tilde && !transport_dead && dir != "~" {
                        self.files_start_remote_dir(&ssh_host, "~", false, false, refreshed);
                        return;
                    }
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.last_error = err.clone();
                        let is_empty = p.files.as_ref().is_some_and(|ex| ex.entries.is_empty() && ex.preview.is_none());
                        if is_empty && (transport_dead || err == "folder_unreadable") {
                            if let Some(ex) = p.files.as_mut() {
                                ex.unreachable = true;
                                ex.cursor = 0;
                            }
                            p.notice.clear();
                        } else {
                            p.notice = refusal_text(&err);
                        }
                    }
                    self.dirty = true;
                    return;
                }
                let entries = parse_entries(&reply);
                let canon = reply["cwd"].as_str().unwrap_or(&dir).to_string();
                if let Some(p) = self.ui.panel.as_mut() {
                    if let Some(ex) = p.files.as_mut() {
                        ex.cwd = canon;
                        ex.parent = reply["parent"].as_str().map(str::to_string);
                        ex.entries = entries;
                        ex.truncated = reply["truncated"] == json!(true);
                        ex.preview = None;
                        ex.unreachable = false;
                        ex.load_count += 1;
                        let max_idx = ex.nav_len().saturating_sub(1);
                        ex.cursor = if keep_cursor { ex.cursor.min(max_idx) } else { 0 };
                    }
                    p.last_error.clear();
                    p.notice = if refreshed { "Refreshed.".into() } else { String::new() };
                }
                self.dirty = true;
            }
            AsyncJob::FilesPreview { req_id, ssh_host, path, refreshed } => {
                let max_cols = self.files_row_cols();
                let ps = calc_page_size(self.ui.panel_rows, true);
                let matched = self
                    .ui
                    .panel
                    .as_ref()
                    .filter(|p| p.step == Step::Files)
                    .and_then(|p| p.files.as_ref())
                    .is_some_and(|ex| (ex.pending_req == req_id || !has_proxy) && ex.ssh_host.as_deref() == Some(ssh_host.as_str()));
                if !matched {
                    return;
                }
                if let Some(ex) = self.ui.panel.as_mut().and_then(|p| p.files.as_mut()) {
                    ex.pending_req = 0;
                    ex.loading = None;
                }
                let Some(p) = self.ui.panel.as_mut() else { return };
                if reply["ok"] != json!(1) {
                    let err = reply["error"].as_str().unwrap_or("unknown").to_string();
                    p.last_error = err.clone();
                    p.notice = refusal_text(&err);
                    self.dirty = true;
                    return;
                }
                let binary = reply["binary"] == json!(true);
                let lines = if binary { Vec::new() } else { wrap_preview(reply["text"].as_str().unwrap_or(""), max_cols) };
                if let Some(ex) = p.files.as_mut() {
                    let prev_top = ex.preview.as_ref().filter(|pv| pv.path == path).map_or(0, |pv| pv.top);
                    let max_top = lines.len().saturating_sub(ps);
                    ex.preview = Some(FilePreview {
                        path: reply["path"].as_str().unwrap_or(&path).to_string(),
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
                p.notice = if refreshed { "Refreshed.".into() } else { String::new() };
                self.dirty = true;
            }
            _ => {}
        }
    }
}
