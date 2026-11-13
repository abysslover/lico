//! Two-tab left workspace sidebar (`[ Directories ]` | `[ Chats ]`) for the two-pane workbench (`M07-P02`).
//!
//! Replaces the legacy left session list in `lico gui` (`self.ui.workbench && self.ui.menu`) since open
//! sessions live in the top chat tab bar (`gui/tabs.rs`). Supports `Ctrl+B` collapse/expand, local and
//! SSH (`Host: local v` / `Host: <alias> v`) directory navigation, `.dotfiles` toggle, explicit refresh,
//! instant in-memory filename filtering, active working-directory / `Global` highlighting, auto-sync on
//! session tab switch (non-blocking via `spawn_host_call`, zero background timers), pagination, and
//! unified file activation (wide inline UTF-8 preview with `Esc` back to chat, or system default opener).

use std::process::{Command, Stdio};

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::controls::{Action, Rect};
use super::panel::files_ui::{parse_entries, FileItem, HOST_POPUP_MAX_VIS};
use super::panel::AsyncJob;
use super::ui::LIST_W;
use super::App;

#[derive(Clone, Default)]
pub struct SidebarEntryGeom {
    pub index: usize,
    pub name: String,
    pub kind: String,
    pub external: bool,
    pub rect: Rect,
}

#[derive(Clone, Default)]
pub struct SidebarGeom {
    pub dir_tab_rect: Option<Rect>, pub chats_tab_rect: Option<Rect>, pub calls_tab_rect: Option<Rect>,
    pub back_rect: Option<Rect>, pub forward_rect: Option<Rect>,
    pub bookmark_toggle_rect: Option<Rect>, pub bookmarks_btn_rect: Option<Rect>,
    pub host_rect: Option<Rect>,
    pub dotfiles_rect: Option<Rect>,
    pub refresh_rect: Option<Rect>,
    pub new_here_rect: Option<Rect>,
    pub new_chat_rect: Option<Rect>,
    pub filter_rect: Option<Rect>,
    pub filter_clear_rect: Option<Rect>,
    pub global_rect: Option<Rect>,
    pub active_dir_rect: Option<Rect>,
    pub jump_active_rect: Option<Rect>,
    pub parent_rect: Option<Rect>,
    pub prev_rect: Option<Rect>,
    pub next_rect: Option<Rect>,
    pub entries: Vec<SidebarEntryGeom>,
    pub host_items: Vec<(usize, Rect)>,
    pub bookmark_items: Vec<super::sidebar_nav::BookmarkItemGeom>,
}

#[derive(Default)]
pub struct SidebarState {
    pub collapsed: bool,
    pub tab: u8, // 0 = Directories, 1 = Chats
    pub initialized: bool,
    pub global_scope: bool,
    pub ssh_host: Option<String>,
    pub ssh_hosts: Vec<String>,
    pub host_popup_open: bool,
    pub host_popup_sel: usize,
    pub host_popup_top: usize,
    pub cwd: String,
    pub parent: Option<String>,
    pub entries: Vec<FileItem>,
    pub truncated: bool,
    pub filter: String,
    pub filter_focused: bool,
    pub page: usize,
    pub page_size: usize,
    pub pending_req: u64,
    pub loading: Option<String>,
    pub error: Option<String>,
    pub unreachable: bool,
    pub load_count: u64,
    pub refresh_count: u64,
    pub last_opened: Option<String>,
    pub nav: super::sidebar_nav::DirNavState,
    pub chats: super::sidebar_chats::ChatsSidebarState,
    pub newchat: Option<super::newchat::NewChat>, // the "+ New chat" modal while it is open (`M10-P03`)
    pub calls: super::sidebar_calls::CallsSidebarState,
    pub geom: SidebarGeom,
}

pub(crate) fn norm_path(p: &str) -> &str {
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() && p.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

pub(crate) fn paths_match(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && norm_path(a) == norm_path(b)
}

fn spawn_external_cmd(program: &str, target: &str) -> bool {
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

impl SidebarState {
    pub fn host_choices(&self) -> Vec<Option<String>> {
        let mut choices: Vec<Option<String>> = vec![None];
        for h in &self.ssh_hosts {
            if !choices.iter().any(|c| c.as_deref() == Some(h.as_str())) {
                choices.push(Some(h.clone()));
            }
        }
        if let Some(cur) = &self.ssh_host {
            if !choices.iter().any(|c| c.as_deref() == Some(cur.as_str())) {
                choices.push(Some(cur.clone()));
            }
        }
        choices
    }

    pub fn sync_popup_scroll(&mut self, max_vis: usize) {
        let total = self.host_choices().len();
        if total == 0 {
            self.host_popup_sel = 0;
            self.host_popup_top = 0;
            return;
        }
        let vis = max_vis.max(1);
        self.host_popup_sel = self.host_popup_sel.min(total - 1);
        let max_top = total.saturating_sub(vis);
        if self.host_popup_sel < self.host_popup_top {
            self.host_popup_top = self.host_popup_sel;
        } else if self.host_popup_sel >= self.host_popup_top + vis {
            self.host_popup_top = self.host_popup_sel + 1 - vis;
        }
        self.host_popup_top = self.host_popup_top.min(max_top);
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.trim().is_empty() {
            return (0..self.entries.len()).collect();
        }
        let needle = self.filter.trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn page_window(&self, filtered_len: usize, ps: usize) -> (usize, usize, usize, usize) {
        let step = ps.max(1);
        let total_pages = filtered_len.div_ceil(step).max(1);
        let p = self.page.min(total_pages - 1);
        let start = p * step;
        let end = (start + step).min(filtered_len);
        (start, end, p, total_pages)
    }
}

impl App {
    pub(crate) fn sidebar_shown(&self) -> bool {
        self.ui.workbench && self.ui.menu
    }

    pub(crate) fn effective_list_w(&self, scale: f32) -> usize {
        if self.sidebar_shown() && self.ui.sidebar.collapsed {
            0
        } else {
            (LIST_W as f32 * scale.max(1.0)) as usize
        }
    }

    pub(crate) fn sidebar_host_popup_open(&self) -> bool {
        self.sidebar_shown() && !self.ui.sidebar.collapsed && self.ui.sidebar.host_popup_open
    }

    pub(crate) fn close_sidebar_host_popup(&mut self) {
        if self.ui.sidebar.host_popup_open {
            self.ui.sidebar.host_popup_open = false;
            self.dirty = true;
        }
    }

    pub(crate) fn hover_sidebar_host_item(&mut self, idx: usize) -> bool {
        if !self.sidebar_host_popup_open() {
            return false;
        }
        let total = self.ui.sidebar.host_choices().len();
        if idx < total && self.ui.sidebar.host_popup_sel != idx {
            self.ui.sidebar.host_popup_sel = idx;
            return true;
        }
        false
    }

    pub(crate) fn toggle_sidebar_collapsed(&mut self) {
        if !self.sidebar_shown() {
            return;
        }
        self.ui.sidebar.collapsed = !self.ui.sidebar.collapsed;
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.nav.popup_open = false;
        self.ui.sidebar.filter_focused = false;
        self.ui.composed = None;
        self.ui.flash = Some(if self.ui.sidebar.collapsed {
            "Left sidebar hidden (Ctrl+B to show).".into()
        } else {
            "Left sidebar shown.".into()
        });
        self.dirty = true;
    }

    /// Returns `(is_global, ssh_host, active_cwd)` for the currently selected session, if any.
    pub(crate) fn active_session_scope(&self) -> (bool, Option<String>, Option<String>) {
        let Some(row) = self.selected_row() else {
            return (false, None, None);
        };
        if row.is_global() {
            return (true, None, None);
        }
        let dir = if !row.worktree_repo.is_empty() {
            row.worktree_repo.clone()
        } else {
            row.cwd.clone()
        };
        let host = (!row.ssh_host.is_empty()).then(|| row.ssh_host.clone());
        (false, host, (!dir.is_empty()).then_some(dir))
    }

    pub(crate) fn sidebar_is_active_cwd(&self) -> bool {
        if self.ui.sidebar.global_scope {
            return false;
        }
        let (is_glob, act_host, Some(act_dir)) = self.active_session_scope() else {
            return false;
        };
        !is_glob && self.ui.sidebar.ssh_host == act_host && paths_match(&self.ui.sidebar.cwd, &act_dir)
    }

    pub(crate) fn sidebar_is_global_highlighted(&self) -> bool {
        let (is_glob, _, _) = self.active_session_scope();
        self.ui.sidebar.global_scope || (is_glob && self.ui.sidebar.cwd.is_empty())
    }

    pub(crate) fn sidebar_can_jump_active(&self) -> bool {
        let (is_glob, _, act_dir) = self.active_session_scope();
        if is_glob {
            !self.ui.sidebar.global_scope
        } else if act_dir.is_some() {
            !self.sidebar_is_active_cwd()
        } else {
            false
        }
    }

    pub(crate) fn maybe_init_sidebar(&mut self) {
        if !self.sidebar_shown() || self.socket.is_empty() || self.ui.sidebar.initialized {
            return;
        }
        self.ui.sidebar.initialized = true;
        self.ui.sidebar.ssh_hosts = self.fetch_ssh_host_aliases();
        if self.sidebar_restore_initial_nav() {
            return;
        }
        if !self.session.is_empty() {
            self.sidebar_sync_to_active_session();
            if !self.ui.sidebar.cwd.is_empty() || self.ui.sidebar.loading.is_some() {
                return;
            }
        }
        let start = self.initial_files_cwd();
        self.sidebar_start_dir(None, &start, false, false, true);
    }

    pub(crate) fn sidebar_sync_to_active_session(&mut self) {
        if !self.sidebar_shown() || self.socket.is_empty() || self.ui.sidebar.nav.suppress_session_sync {
            return;
        }
        self.ui.sidebar.initialized = true;
        let (is_glob, act_host, act_dir) = self.active_session_scope();
        if is_glob {
            self.ui.sidebar.global_scope = true;
            if self.ui.sidebar.cwd.is_empty() && self.ui.sidebar.loading.is_none() {
                let start = self.initial_files_cwd();
                self.sidebar_start_dir(None, &start, false, false, true);
            } else {
                self.sidebar_record_current_location();
            }
            self.chats_sync_if_open(false);
            self.dirty = true;
            return;
        }
        if let Some(dir) = act_dir {
            self.ui.sidebar.global_scope = false;
            let same = self.ui.sidebar.ssh_host == act_host
                && paths_match(&self.ui.sidebar.cwd, &dir)
                && (!self.ui.sidebar.entries.is_empty() || self.ui.sidebar.loading.is_some())
                && !self.ui.sidebar.unreachable;
            if !same {
                let fallback = act_host.is_some() && dir != "~";
                self.sidebar_start_dir(act_host, &dir, false, false, fallback);
            } else {
                self.sidebar_record_current_location();
            }
            self.chats_sync_if_open(false);
            self.dirty = true;
        }
    }

    pub(crate) fn sidebar_jump_to_active(&mut self) {
        self.ui.sidebar.filter_focused = false;
        self.sidebar_sync_to_active_session();
    }

    pub(crate) fn sidebar_select_tab(&mut self, tab: u8) {
        let clamped = tab.min(2);
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.nav.popup_open = false;
        self.ui.sidebar.filter_focused = false;
        if self.ui.sidebar.tab != clamped {
            self.ui.sidebar.tab = clamped;
            if clamped == 1 {
                self.chats_sync_if_open(true); // the tab shows what is true now: a chat opened since is shown as open
            }
            self.dirty = true;
        }
    }

    pub(crate) fn sidebar_select_global(&mut self) {
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.nav.popup_open = false;
        self.ui.sidebar.filter_focused = false;
        self.ui.sidebar.global_scope = true;
        self.sidebar_record_current_location();
        self.chats_sync_if_open(false);
        self.dirty = true;
    }

    pub(crate) fn sidebar_start_dir(
        &mut self,
        ssh_host: Option<String>,
        dir: &str,
        keep_page: bool,
        refreshed: bool,
        fallback_root: bool,
    ) {
        if self.socket.is_empty() {
            return;
        }
        let mut req = json!({"op": "folder.list", "cwd": dir});
        if let Some(ref h) = ssh_host {
            req["ssh_host"] = json!(h);
        }
        if self.ui.show_hidden {
            req["show_hidden"] = json!(true);
        }
        let msg = match &ssh_host {
            Some(h) if dir == "~" => format!("Connecting ssh:{h}..."),
            Some(h) => format!("Loading ssh:{h}..."),
            None => format!("Loading {dir}..."),
        };
        self.ui.sidebar.ssh_host = ssh_host.clone();
        self.ui.sidebar.unreachable = false;
        self.ui.sidebar.error = None;
        self.ui.sidebar.loading = Some(msg);
        if refreshed {
            self.ui.sidebar.refresh_count += 1;
        }
        self.dirty = true;

        let has_proxy = self.ui.proxy.is_some();
        let dir_owned = dir.to_string();
        let req_id = self.spawn_host_call(req, 7, move |req_id| AsyncJob::SidebarDir {
            req_id,
            ssh_host,
            dir: dir_owned,
            keep_page,
            fallback_root,
        });
        if has_proxy {
            self.ui.sidebar.pending_req = req_id;
        }
    }

    pub(crate) fn on_sidebar_async(&mut self, job: AsyncJob, reply: Value) {
        let AsyncJob::SidebarDir {
            req_id,
            ssh_host,
            dir,
            keep_page,
            fallback_root,
        } = job
        else {
            return;
        };
        let has_proxy = self.ui.proxy.is_some();
        if (has_proxy && self.ui.sidebar.pending_req != req_id) || self.ui.sidebar.ssh_host != ssh_host {
            return;
        }
        self.ui.sidebar.pending_req = 0;
        self.ui.sidebar.loading = None;

        if reply["ok"] != json!(1) {
            self.ui.sidebar.nav.stepping = false;
            self.ui.sidebar.nav.target_page = None;
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            let transport_dead = matches!(err.as_str(), "ssh_unreachable" | "host_timeout" | "host_unreachable");
            if fallback_root && !transport_dead {
                if let Some(ref h) = ssh_host {
                    if dir != "~" {
                        self.sidebar_start_dir(Some(h.clone()), "~", false, false, false);
                        return;
                    }
                } else if dir != "/" {
                    self.sidebar_start_dir(None, "/", false, false, false);
                    return;
                }
            }
            self.ui.sidebar.unreachable = ssh_host.is_some() && (transport_dead || err == "folder_unreadable");
            self.ui.sidebar.error = Some(err.clone());
            if self.ui.sidebar.tab == 1 {
                self.ui.sidebar.chats.loading = false;
                self.ui.sidebar.chats.error = Some(err);
            }
            self.dirty = true;
            return;
        }

        self.ui.sidebar.cwd = reply["cwd"].as_str().unwrap_or(&dir).to_string();
        self.ui.sidebar.parent = reply["parent"].as_str().map(str::to_string);
        self.ui.sidebar.entries = parse_entries(&reply);
        self.ui.sidebar.truncated = reply["truncated"] == json!(true);
        self.ui.sidebar.unreachable = false;
        self.ui.sidebar.error = None;
        self.ui.sidebar.load_count += 1;
        if self.ui.sidebar.nav.stepping {
            self.ui.sidebar.nav.stepping = false;
            self.ui.sidebar.page = self.ui.sidebar.nav.target_page.take().unwrap_or(0);
        } else {
            if let Some(tp) = self.ui.sidebar.nav.target_page.take() {
                self.ui.sidebar.page = tp;
            } else if !keep_page {
                self.ui.sidebar.page = 0;
                self.ui.sidebar.filter.clear();
            }
            self.sidebar_record_current_location();
        }
        self.chats_sync_if_open(false);
        self.dirty = true;
    }

    pub(crate) fn sidebar_refresh(&mut self) {
        self.ui.sidebar.host_popup_open = false;
        if self.ui.sidebar.tab != 0 {
            self.chats_start_fetch(self.ui.sidebar.chats.page, true);
            return;
        }
        let host = self.ui.sidebar.ssh_host.clone();
        let target = if self.ui.sidebar.cwd.is_empty() || self.ui.sidebar.unreachable {
            if host.is_some() { "~".to_string() } else { self.initial_files_cwd() }
        } else {
            self.ui.sidebar.cwd.clone()
        };
        self.sidebar_start_dir(host, &target, true, true, false);
    }

    pub(crate) fn sidebar_new_session_here(&mut self) {
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.filter_focused = false;
        if self.ui.sidebar.global_scope {
            self.form_open_global();
            return;
        }
        if self.ui.sidebar.unreachable {
            self.ui.flash = Some("+ New here: SSH host is unreachable.".into());
            self.dirty = true;
            return;
        }
        if self.ui.sidebar.cwd.is_empty() {
            self.form_open();
            return;
        }
        let cwd = self.ui.sidebar.cwd.clone();
        let host = self.ui.sidebar.ssh_host.clone();
        self.form_open_at_host(cwd, host);
    }

    pub(crate) fn sidebar_go_parent(&mut self) {
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.filter_focused = false;
        if let Some(parent) = self.ui.sidebar.parent.clone() {
            self.ui.sidebar.global_scope = false;
            let host = self.ui.sidebar.ssh_host.clone();
            self.sidebar_start_dir(host, &parent, false, false, false);
        }
    }

    pub(crate) fn sidebar_activate_entry(&mut self, entry_idx: usize) {
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.filter_focused = false;
        let Some(item) = self.ui.sidebar.entries.get(entry_idx).cloned() else {
            return;
        };
        let host = self.ui.sidebar.ssh_host.clone();
        let base = self.ui.sidebar.cwd.as_str();
        let full = if base == "/" {
            format!("/{}", item.name)
        } else {
            format!("{}/{}", base.trim_end_matches('/'), item.name)
        };
        match item.kind.as_str() {
            "dir" => {
                self.ui.sidebar.global_scope = false;
                self.sidebar_start_dir(host, &full, false, false, false);
            }
            "link" => {
                self.ui.flash = Some("Broken or special symbolic link cannot be opened.".into());
                self.dirty = true;
            }
            _ if item.external => {
                if let Some(h) = host {
                    self.ui.flash = Some(format!(
                        "Remote non-text file on ssh:{h} ({}) — local opener applies to local files.",
                        item.name
                    ));
                } else {
                    self.sidebar_open_external(&full, &item.name);
                }
                self.dirty = true;
            }
            _ => {
                let cwd = self.ui.sidebar.cwd.clone();
                self.files_open_sidebar_preview(host, &cwd, &full);
            }
        }
    }

    fn sidebar_open_external(&mut self, target: &str, display_name: &str) {
        let custom = std::env::var("LICO_OPEN_CMD").ok().filter(|s| !s.trim().is_empty());
        let notice = if let Some(cmd) = custom {
            if spawn_external_cmd(&cmd, target) {
                format!("Opened with system default: {display_name}")
            } else {
                format!("Could not run {cmd} for {display_name}.")
            }
        } else if std::env::var_os("LICO_NO_AUTO_OPEN").is_some() {
            format!("System open suppressed (LICO_NO_AUTO_OPEN): {display_name}")
        } else if spawn_external_cmd("xdg-open", target) {
            format!("Opened with system default: {display_name}")
        } else {
            format!("xdg-open is not available: {target}")
        };
        self.ui.sidebar.last_opened = Some(target.to_string());
        self.ui.flash = Some(notice);
    }

    pub(crate) fn sidebar_step_page(&mut self, delta: isize) {
        if delta == 0 { return; }
        let filtered_len = self.ui.sidebar.filtered_indices().len();
        let ps = self.ui.sidebar.page_size.max(1);
        let (_, _, cur_p, total_pages) = self.ui.sidebar.page_window(filtered_len, ps);
        let next = (cur_p as isize + delta).clamp(0, total_pages.saturating_sub(1) as isize) as usize;
        if next != self.ui.sidebar.page {
            self.ui.sidebar.page = next;
            self.sidebar_record_current_location();
            self.dirty = true;
        }
    }

    pub(crate) fn toggle_sidebar_host_popup(&mut self) {
        if self.ui.sidebar.host_popup_open {
            self.close_sidebar_host_popup();
            return;
        }
        self.menu_close();
        self.close_files_host_popup();
        self.close_sidebar_bookmarks_popup();
        self.ui.sidebar.filter_focused = false;
        self.ui.sidebar.ssh_hosts = self.fetch_ssh_host_aliases();
        if self.ui.sidebar.ssh_hosts.is_empty() && self.ui.sidebar.ssh_host.is_none() {
            self.ui.flash = Some("No SSH host aliases found in ~/.ssh/config.".into());
            self.dirty = true;
            return;
        }
        let choices = self.ui.sidebar.host_choices();
        self.ui.sidebar.host_popup_sel = choices.iter().position(|c| *c == self.ui.sidebar.ssh_host).unwrap_or(0);
        self.ui.sidebar.sync_popup_scroll(HOST_POPUP_MAX_VIS);
        self.ui.sidebar.host_popup_open = true;
        self.dirty = true;
    }

    pub(crate) fn select_sidebar_host_index(&mut self, idx: usize) {
        self.ui.sidebar.host_popup_open = false;
        let Some(choice) = self.ui.sidebar.host_choices().get(idx).cloned() else {
            self.dirty = true;
            return;
        };
        self.ui.sidebar.global_scope = false;
        match choice {
            Some(h) => self.sidebar_start_dir(Some(h), "~", false, false, false),
            None => {
                let start = self.initial_files_cwd();
                self.sidebar_start_dir(None, &start, false, false, true);
            }
        }
        self.dirty = true;
    }

    pub(crate) fn sidebar_popup_click(&mut self, hit: Option<Action>) -> bool {
        if self.sidebar_nav_popup_click(hit.clone()) { return true; }
        if !self.sidebar_host_popup_open() { return false; }
        match hit {
            Some(Action::SidebarHostItem(idx)) => self.select_sidebar_host_index(idx),
            _ => self.close_sidebar_host_popup(),
        }
        true
    }

    pub(crate) fn sidebar_wheel(&mut self, lines: isize) -> bool {
        if !self.sidebar_shown() || self.ui.sidebar.collapsed { return false; }
        let list_w = self.effective_list_w(self.ui.scale);
        if (self.ui.mouse.0.max(0.0) as usize) >= list_w { return false; }
        if lines == 0 { return true; }
        let step = -lines;
        if self.sidebar_nav_wheel(step) { return true; }
        if self.ui.sidebar.host_popup_open {
            let max_idx = self.ui.sidebar.host_choices().len().saturating_sub(1) as isize;
            self.ui.sidebar.host_popup_sel = (self.ui.sidebar.host_popup_sel as isize)
                .saturating_add(step.clamp(-4, 4))
                .clamp(0, max_idx) as usize;
            self.ui.sidebar.sync_popup_scroll(HOST_POPUP_MAX_VIS);
            self.dirty = true;
            return true;
        }
        match self.ui.sidebar.tab {
            0 => self.sidebar_step_page(if step > 0 { 1 } else { -1 }),
            1 => self.chats_step_page(if step > 0 { 1 } else { -1 }),
            _ => self.calls_step_page(if step > 0 { 1 } else { -1 }),
        }
        true
    }

    pub(crate) fn sidebar_key(&mut self, event: &KeyEvent) -> bool {
        if !self.sidebar_shown() { return false; }
        let ctrl = self.ui.mods.control_key() && !self.ui.mods.alt_key();
        if ctrl && !self.ui.mods.shift_key() && super::key_letter(event) == Some('b') {
            if !event.repeat { self.toggle_sidebar_collapsed(); }
            return true;
        }
        if self.sidebar_nav_key(event) { return true; }
        if self.sidebar_host_popup_open() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) if !event.repeat => self.close_sidebar_host_popup(),
                Key::Named(NamedKey::Enter | NamedKey::Space) if !event.repeat => {
                    let sel = self.ui.sidebar.host_popup_sel;
                    self.select_sidebar_host_index(sel);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.ui.sidebar.host_popup_sel = self.ui.sidebar.host_popup_sel.saturating_sub(1);
                    self.ui.sidebar.sync_popup_scroll(HOST_POPUP_MAX_VIS);
                    self.dirty = true;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    let max_i = self.ui.sidebar.host_choices().len().saturating_sub(1);
                    self.ui.sidebar.host_popup_sel = (self.ui.sidebar.host_popup_sel + 1).min(max_i);
                    self.ui.sidebar.sync_popup_scroll(HOST_POPUP_MAX_VIS);
                    self.dirty = true;
                }
                _ => {}
            }
            return true;
        }
        if !self.ui.sidebar.filter_focused { return false; }
        match &event.logical_key {
            Key::Named(NamedKey::Escape) if !event.repeat => {
                if !self.ui.sidebar.filter.is_empty() {
                    self.ui.sidebar.filter.clear();
                    self.ui.sidebar.page = 0;
                    self.chats_sync_if_open(false);
                } else {
                    self.ui.sidebar.filter_focused = false;
                }
                self.dirty = true;
            }
            Key::Named(NamedKey::Enter) if !event.repeat => {
                self.ui.sidebar.filter_focused = false;
                self.dirty = true;
            }
            Key::Named(NamedKey::Backspace) => {
                self.ui.sidebar.filter.pop();
                self.ui.sidebar.page = 0;
                self.chats_sync_if_open(false);
                self.dirty = true;
            }
            Key::Character(s) if !self.ui.mods.control_key() && !self.ui.mods.alt_key() => {
                for ch in s.chars().filter(|c| !c.is_control() && !super::editor::hidden(*c)) {
                    if self.ui.sidebar.filter.chars().count() < 64 { self.ui.sidebar.filter.push(ch); }
                }
                self.ui.sidebar.page = 0;
                self.chats_sync_if_open(false);
                self.dirty = true;
            }
            _ => if self.ui.mods.control_key() || self.ui.mods.alt_key() { return false; },
        }
        true
    }

    pub(crate) fn sidebar_sync_hidden_toggle(&mut self) {
        if self.sidebar_shown() && !self.socket.is_empty() && !self.ui.sidebar.cwd.is_empty() && !self.ui.sidebar.unreachable {
            let host = self.ui.sidebar.ssh_host.clone();
            let cwd = self.ui.sidebar.cwd.clone();
            self.sidebar_start_dir(host, &cwd, true, false, false);
        }
    }

    pub(crate) fn sidebar_click(&mut self, action: &Action) -> bool {
        match *action {
            Action::SidebarTab(t) => self.sidebar_select_tab(t),
            Action::SidebarNavBack => self.sidebar_nav_step(-1),
            Action::SidebarNavForward => self.sidebar_nav_step(1),
            Action::SidebarBookmarkToggle => self.sidebar_toggle_bookmark(),
            Action::SidebarBookmarksBtn => self.toggle_sidebar_bookmarks_popup(),
            Action::SidebarHost => self.toggle_sidebar_host_popup(),
            Action::SidebarDotfiles => self.toggle_hidden_files(),
            Action::SidebarRefresh => self.sidebar_refresh(),
            Action::SidebarNewHere => self.sidebar_new_session_here(),
            Action::SidebarFilter => {
                self.ui.sidebar.host_popup_open = false;
                self.ui.sidebar.nav.popup_open = false;
                self.ui.sidebar.filter_focused = true;
                self.dirty = true;
            }
            Action::SidebarFilterClear => {
                self.ui.sidebar.filter.clear();
                self.ui.sidebar.filter_focused = false;
                self.ui.sidebar.page = 0;
                self.chats_sync_if_open(false);
                self.dirty = true;
            }
            Action::SidebarGlobal => self.sidebar_select_global(),
            Action::SidebarActiveDir => {
                self.ui.sidebar.global_scope = false;
                self.ui.sidebar.filter_focused = false;
                self.sidebar_record_current_location();
                self.chats_sync_if_open(false);
                self.dirty = true;
            }
            Action::SidebarJumpActive => self.sidebar_jump_to_active(),
            Action::SidebarParent => self.sidebar_go_parent(),
            Action::SidebarEntry(idx) => self.sidebar_activate_entry(idx),
            Action::SidebarChatEntry(idx) => self.chats_activate_entry(idx),
            Action::SidebarPage(d) => match self.ui.sidebar.tab {
                1 => self.chats_step_page(d),
                2 => self.calls_step_page(d),
                _ => self.sidebar_step_page(d),
            },
            _ => return self.sidebar_calls_click(action),
        }
        true
    }
}
