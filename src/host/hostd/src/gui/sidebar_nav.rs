//! Persistent `[ Directories ]` navigation history (`Back <` / `Forward >`) and page-aware bookmarks (`M07-P05`).
//!
//! Tracks visited `(ssh_host, cwd, global, page)` locations in order so the user can step backward (`역순`, `<`, `Alt+Left`,
//! `MouseButton::Back`) and forward (`정순`, `>`, `Alt+Right`, `MouseButton::Forward`) across both directory changes and
//! pagination pages, and bookmark any directory/page (`☆` / `★` + `Marks (N) v` popup) with full persistence in `settings.json`.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use crate::clipboard::settings::MAX_DIR_NAV;
use super::controls::{self, Action, Rect};
use super::panel::files_ui::HOST_POPUP_MAX_VIS;
use super::panel::AsyncJob;
use super::sidebar::{norm_path, paths_match};
use super::sidebar_render::clip_cols;
use super::App;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirHistoryEntry {
    pub cwd: String,
    pub ssh_host: Option<String>,
    pub global: bool,
    pub page: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirBookmarkEntry {
    pub cwd: String,
    pub ssh_host: Option<String>,
    pub global: bool,
    pub page: usize,
    pub label: String,
}

#[derive(Clone, Default)]
pub struct BookmarkItemGeom {
    pub index: usize,
    pub rect: Rect,
    pub remove_rect: Rect,
}

#[derive(Default)]
pub struct DirNavState {
    pub history: Vec<DirHistoryEntry>,
    pub index: usize,
    pub bookmarks: Vec<DirBookmarkEntry>,
    pub popup_open: bool,
    pub popup_sel: usize,
    pub popup_top: usize,
    pub loaded: bool,
    pub dirty: bool,
    pub stepping: bool,
    pub target_page: Option<usize>,
    pub suppress_session_sync: bool,
}

impl DirHistoryEntry {
    pub fn to_json(&self) -> Value {
        let mut obj = json!({
            "cwd": self.cwd,
            "global": self.global,
            "page": self.page.min(10_000),
        });
        if let Some(ref h) = self.ssh_host {
            obj["ssh_host"] = json!(h);
        }
        obj
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        let obj = v.as_object()?;
        let cwd = obj.get("cwd")?.as_str()?.to_string();
        let global = obj.get("global")?.as_bool()?;
        let page = obj.get("page")?.as_u64()?.min(10_000) as usize;
        let ssh_host = obj.get("ssh_host").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        if !global && cwd.is_empty() {
            return None;
        }
        Some(Self {
            cwd: if cwd.is_empty() { cwd } else { norm_path(&cwd).to_string() },
            ssh_host,
            global,
            page,
        })
    }
}

impl DirBookmarkEntry {
    pub fn to_json(&self) -> Value {
        let mut obj = json!({
            "cwd": self.cwd,
            "global": self.global,
            "page": self.page.min(10_000),
            "label": self.label,
        });
        if let Some(ref h) = self.ssh_host {
            obj["ssh_host"] = json!(h);
        }
        obj
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        let obj = v.as_object()?;
        let cwd = obj.get("cwd")?.as_str()?.to_string();
        let global = obj.get("global")?.as_bool()?;
        let page = obj.get("page")?.as_u64()?.min(10_000) as usize;
        let label = obj.get("label")?.as_str()?.to_string();
        let ssh_host = obj.get("ssh_host").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        if label.trim().is_empty() || (!global && cwd.is_empty()) {
            return None;
        }
        Some(Self {
            cwd: if cwd.is_empty() { cwd } else { norm_path(&cwd).to_string() },
            ssh_host,
            global,
            page,
            label,
        })
    }

    pub fn matches_loc(&self, loc: &DirHistoryEntry) -> bool {
        self.global == loc.global
            && self.ssh_host == loc.ssh_host
            && self.page == loc.page
            && (self.global || paths_match(&self.cwd, &loc.cwd))
    }
}

fn make_bookmark_label(loc: &DirHistoryEntry) -> String {
    let base = if loc.global {
        "Global".to_string()
    } else {
        let leaf = if loc.cwd == "/" {
            "/"
        } else {
            loc.cwd
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(loc.cwd.as_str())
        };
        match &loc.ssh_host {
            Some(h) => format!("ssh:{h}:{leaf}"),
            None => leaf.to_string(),
        }
    };
    let full = if loc.page > 0 {
        format!("{base} (p.{})", loc.page + 1)
    } else {
        base
    };
    let clean: String = full
        .chars()
        .filter(|c| !crate::folders::refused_char(*c))
        .take(64)
        .collect();
    if clean.trim().is_empty() || crate::folders::shows_nothing(&clean) {
        "Bookmark".to_string()
    } else {
        clean
    }
}

impl DirNavState {
    pub fn can_back(&self) -> bool {
        !self.history.is_empty() && self.index.min(self.history.len() - 1) > 0
    }

    pub fn can_forward(&self) -> bool {
        !self.history.is_empty() && self.index + 1 < self.history.len()
    }

    pub fn sync_popup_scroll(&mut self, max_vis: usize) {
        let total = self.bookmarks.len();
        if total == 0 {
            self.popup_sel = 0;
            self.popup_top = 0;
            return;
        }
        let vis = max_vis.max(1);
        self.popup_sel = self.popup_sel.min(total - 1);
        let max_top = total.saturating_sub(vis);
        if self.popup_sel < self.popup_top {
            self.popup_top = self.popup_sel;
        } else if self.popup_sel >= self.popup_top + vis {
            self.popup_top = self.popup_sel + 1 - vis;
        }
        self.popup_top = self.popup_top.min(max_top);
    }
}

impl App {
    pub(crate) fn sync_nav_from_settings(&mut self, settings: &Value) {
        if self.ui.sidebar.nav.loaded || !settings.is_object() {
            return;
        }
        self.ui.sidebar.nav.loaded = true;
        if let Some(arr) = settings.get("dir_history").and_then(Value::as_array) {
            let parsed: Vec<DirHistoryEntry> = arr.iter().filter_map(DirHistoryEntry::from_json).take(MAX_DIR_NAV).collect();
            if !parsed.is_empty() {
                let raw_idx = settings
                    .get("dir_history_idx")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize)
                    .unwrap_or(parsed.len() - 1);
                self.ui.sidebar.nav.index = raw_idx.min(parsed.len() - 1);
                self.ui.sidebar.nav.history = parsed;
            }
        }
        if let Some(arr) = settings.get("dir_bookmarks").and_then(Value::as_array) {
            self.ui.sidebar.nav.bookmarks = arr.iter().filter_map(DirBookmarkEntry::from_json).take(MAX_DIR_NAV).collect();
        }
    }

    pub(crate) fn current_dir_nav_entry(&self) -> Option<DirHistoryEntry> {
        let sb = &self.ui.sidebar;
        if sb.unreachable || (!sb.global_scope && sb.cwd.is_empty()) {
            return None;
        }
        let cwd = if sb.cwd.is_empty() {
            String::new()
        } else {
            norm_path(&sb.cwd).to_string()
        };
        Some(DirHistoryEntry {
            cwd,
            ssh_host: sb.ssh_host.clone(),
            global: sb.global_scope,
            page: sb.page,
        })
    }

    pub(crate) fn sidebar_is_bookmarked(&self) -> bool {
        let Some(loc) = self.current_dir_nav_entry() else {
            return false;
        };
        self.ui.sidebar.nav.bookmarks.iter().any(|b| b.matches_loc(&loc))
    }

    pub(crate) fn sidebar_restore_initial_nav(&mut self) -> bool {
        if self.ui.sidebar.nav.history.is_empty() {
            return false;
        }
        let idx = self.ui.sidebar.nav.index.min(self.ui.sidebar.nav.history.len() - 1);
        self.ui.sidebar.nav.index = idx;
        let target = self.ui.sidebar.nav.history[idx].clone();
        self.ui.sidebar.nav.stepping = true;
        self.ui.sidebar.nav.target_page = Some(target.page);
        self.ui.sidebar.global_scope = target.global;
        let dir = if target.cwd.is_empty() {
            self.initial_files_cwd()
        } else {
            target.cwd
        };
        self.sidebar_start_dir(target.ssh_host, &dir, true, false, true);
        true
    }

    pub(crate) fn sidebar_record_current_location(&mut self) {
        let Some(loc) = self.current_dir_nav_entry() else {
            return;
        };
        let nav = &mut self.ui.sidebar.nav;
        if nav.history.is_empty() {
            nav.history.push(loc);
            nav.index = 0;
        } else {
            let cur = nav.index.min(nav.history.len() - 1);
            if nav.history[cur] == loc {
                nav.index = cur;
                return;
            }
            nav.history.truncate(cur + 1);
            nav.history.push(loc);
            if nav.history.len() > MAX_DIR_NAV {
                let drop_n = nav.history.len() - MAX_DIR_NAV;
                nav.history.drain(0..drop_n);
            }
            nav.index = nav.history.len() - 1;
        }
        self.save_sidebar_nav_async(true, true, false);
    }

    fn build_sidebar_nav_patch(&self, inc_hist: bool, inc_idx: bool, inc_bm: bool) -> Option<Value> {
        if self.ui.markers || self.socket.is_empty() {
            return None;
        }
        let nav = &self.ui.sidebar.nav;
        let mut req = json!({"op": "settings.set"});
        let mut any = false;
        if inc_hist {
            let arr: Vec<Value> = nav.history.iter().map(DirHistoryEntry::to_json).collect();
            req["dir_history"] = Value::Array(arr);
            any = true;
        }
        if inc_idx && !nav.history.is_empty() {
            req["dir_history_idx"] = json!(nav.index.min(nav.history.len() - 1));
            any = true;
        }
        if inc_bm {
            let arr: Vec<Value> = nav.bookmarks.iter().map(DirBookmarkEntry::to_json).collect();
            req["dir_bookmarks"] = Value::Array(arr);
            any = true;
        }
        any.then_some(req)
    }

    fn save_sidebar_nav_async(&mut self, inc_hist: bool, inc_idx: bool, inc_bm: bool) {
        let Some(req) = self.build_sidebar_nav_patch(inc_hist, inc_idx, inc_bm) else {
            return;
        };
        self.ui.sidebar.nav.dirty = true;
        self.spawn_host_call(req, 3, |_| AsyncJob::SidebarNavSave);
    }

    pub(crate) fn on_sidebar_nav_saved(&mut self, reply: Value) {
        if reply["ok"] == json!(1) {
            self.ui.sidebar.nav.dirty = false;
        }
    }

    pub(crate) fn flush_sidebar_nav_state(&mut self) {
        if !self.ui.sidebar.nav.dirty {
            return;
        }
        if let Some(req) = self.build_sidebar_nav_patch(true, true, true) {
            let reply = self.host_call_within(req, 2);
            if reply["ok"] == json!(1) {
                self.ui.sidebar.nav.dirty = false;
            }
        }
    }

    pub(crate) fn sidebar_nav_step(&mut self, delta: isize) {
        if delta == 0 || !self.sidebar_shown() || self.ui.sidebar.nav.history.is_empty() {
            return;
        }
        self.close_sidebar_host_popup();
        self.close_sidebar_bookmarks_popup();
        self.ui.sidebar.filter_focused = false;

        let len = self.ui.sidebar.nav.history.len();
        let cur = self.ui.sidebar.nav.index.min(len - 1) as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        if next == cur as usize {
            return;
        }
        self.ui.sidebar.nav.index = next;
        let target = self.ui.sidebar.nav.history[next].clone();
        self.ui.sidebar.filter.clear();

        let same_loaded = self.ui.sidebar.ssh_host == target.ssh_host
            && (target.cwd.is_empty() || paths_match(&self.ui.sidebar.cwd, &target.cwd))
            && self.ui.sidebar.loading.is_none()
            && !self.ui.sidebar.unreachable
            && (!self.ui.sidebar.entries.is_empty() || target.cwd.is_empty());

        if same_loaded {
            self.ui.sidebar.nav.stepping = false;
            self.ui.sidebar.nav.target_page = None;
            self.ui.sidebar.global_scope = target.global;
            self.ui.sidebar.page = target.page;
            self.chats_sync_if_open(false);
            self.save_sidebar_nav_async(false, true, false);
            self.dirty = true;
            return;
        }

        self.ui.sidebar.nav.stepping = true;
        self.ui.sidebar.nav.target_page = Some(target.page);
        self.ui.sidebar.global_scope = target.global;
        let dir = if target.cwd.is_empty() {
            self.initial_files_cwd()
        } else {
            target.cwd.clone()
        };
        let fallback = target.ssh_host.is_some() && dir != "~";
        self.sidebar_start_dir(target.ssh_host, &dir, true, false, fallback);
        self.save_sidebar_nav_async(false, true, false);
        self.dirty = true;
    }

    pub(crate) fn sidebar_bookmarks_popup_open(&self) -> bool {
        self.sidebar_shown() && !self.ui.sidebar.collapsed && self.ui.sidebar.nav.popup_open && self.ui.sidebar.tab == 0
    }

    pub(crate) fn close_sidebar_bookmarks_popup(&mut self) {
        if self.ui.sidebar.nav.popup_open {
            self.ui.sidebar.nav.popup_open = false;
            self.dirty = true;
        }
    }

    pub(crate) fn hover_sidebar_bookmark_item(&mut self, idx: usize) -> bool {
        if !self.sidebar_bookmarks_popup_open() {
            return false;
        }
        if idx < self.ui.sidebar.nav.bookmarks.len() && self.ui.sidebar.nav.popup_sel != idx {
            self.ui.sidebar.nav.popup_sel = idx;
            return true;
        }
        false
    }

    pub(crate) fn sidebar_toggle_bookmark(&mut self) {
        self.close_sidebar_host_popup();
        self.ui.sidebar.filter_focused = false;
        let Some(loc) = self.current_dir_nav_entry() else {
            return;
        };
        if let Some(pos) = self.ui.sidebar.nav.bookmarks.iter().position(|b| b.matches_loc(&loc)) {
            let removed = self.ui.sidebar.nav.bookmarks.remove(pos);
            self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
            self.ui.flash = Some(format!("Removed bookmark: {}", removed.label));
        } else {
            let label = make_bookmark_label(&loc);
            if self.ui.sidebar.nav.bookmarks.len() >= MAX_DIR_NAV {
                self.ui.sidebar.nav.bookmarks.remove(0);
            }
            self.ui.flash = Some(format!("Bookmarked: {label}"));
            self.ui.sidebar.nav.bookmarks.push(DirBookmarkEntry {
                cwd: loc.cwd,
                ssh_host: loc.ssh_host,
                global: loc.global,
                page: loc.page,
                label,
            });
            self.ui.sidebar.nav.popup_sel = self.ui.sidebar.nav.bookmarks.len() - 1;
            self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
        }
        self.save_sidebar_nav_async(false, false, true);
        self.dirty = true;
    }

    pub(crate) fn toggle_sidebar_bookmarks_popup(&mut self) {
        if self.ui.sidebar.nav.popup_open {
            self.close_sidebar_bookmarks_popup();
            return;
        }
        self.menu_close();
        self.close_files_host_popup();
        self.close_sidebar_host_popup();
        self.ui.sidebar.filter_focused = false;
        if let Some(loc) = self.current_dir_nav_entry() {
            if let Some(pos) = self.ui.sidebar.nav.bookmarks.iter().position(|b| b.matches_loc(&loc)) {
                self.ui.sidebar.nav.popup_sel = pos;
            }
        }
        self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
        self.ui.sidebar.nav.popup_open = true;
        self.dirty = true;
    }

    pub(crate) fn select_sidebar_bookmark(&mut self, idx: usize) {
        self.ui.sidebar.nav.popup_open = false;
        self.ui.sidebar.filter_focused = false;
        let Some(bm) = self.ui.sidebar.nav.bookmarks.get(idx).cloned() else {
            self.dirty = true;
            return;
        };
        self.ui.sidebar.filter.clear();

        let same_loaded = self.ui.sidebar.ssh_host == bm.ssh_host
            && (bm.cwd.is_empty() || paths_match(&self.ui.sidebar.cwd, &bm.cwd))
            && self.ui.sidebar.loading.is_none()
            && !self.ui.sidebar.unreachable
            && (!self.ui.sidebar.entries.is_empty() || bm.cwd.is_empty());

        if same_loaded {
            self.ui.sidebar.nav.stepping = false;
            self.ui.sidebar.nav.target_page = None;
            self.ui.sidebar.global_scope = bm.global;
            self.ui.sidebar.page = bm.page;
            self.chats_sync_if_open(false);
            self.sidebar_record_current_location();
            self.dirty = true;
            return;
        }

        self.ui.sidebar.nav.stepping = false;
        self.ui.sidebar.nav.target_page = Some(bm.page);
        self.ui.sidebar.global_scope = bm.global;
        let dir = if bm.cwd.is_empty() {
            self.initial_files_cwd()
        } else {
            bm.cwd.clone()
        };
        let fallback = bm.ssh_host.is_some() && dir != "~";
        self.sidebar_start_dir(bm.ssh_host, &dir, true, false, fallback);
        self.dirty = true;
    }

    pub(crate) fn remove_sidebar_bookmark(&mut self, idx: usize) {
        if idx < self.ui.sidebar.nav.bookmarks.len() {
            let removed = self.ui.sidebar.nav.bookmarks.remove(idx);
            self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
            if self.ui.sidebar.nav.bookmarks.is_empty() {
                self.ui.sidebar.nav.popup_open = false;
            }
            self.ui.flash = Some(format!("Removed bookmark: {}", removed.label));
            self.save_sidebar_nav_async(false, false, true);
            self.dirty = true;
        }
    }

    pub(crate) fn insert_nav_json(&self, out: &mut Value) {
        let sb = &self.ui.sidebar;
        let nav = &sb.nav;
        let cur_loc = self.current_dir_nav_entry();
        let bm_items: Vec<Value> = nav
            .bookmarks
            .iter()
            .enumerate()
            .map(|(idx, bm)| {
                let active = cur_loc.as_ref().is_some_and(|loc| bm.matches_loc(loc));
                let g = sb.geom.bookmark_items.iter().find(|it| it.index == idx);
                json!({
                    "index": idx,
                    "label": bm.label,
                    "cwd": if bm.cwd.is_empty() { Value::Null } else { json!(bm.cwd) },
                    "ssh_host": bm.ssh_host,
                    "global": bm.global,
                    "page": bm.page + 1,
                    "active": active,
                    "rect": g.map(|it| it.rect),
                    "remove_rect": g.map(|it| it.remove_rect),
                })
            })
            .collect();
        out["can_back"] = json!(nav.can_back());
        out["can_forward"] = json!(nav.can_forward());
        out["history_index"] = if nav.history.is_empty() {
            Value::Null
        } else {
            json!(nav.index.min(nav.history.len() - 1))
        };
        out["history_len"] = json!(nav.history.len());
        out["bookmarked"] = json!(self.sidebar_is_bookmarked());
        out["bookmarks_popup_open"] = json!(self.sidebar_bookmarks_popup_open());
        out["back_rect"] = json!(sb.geom.back_rect);
        out["forward_rect"] = json!(sb.geom.forward_rect);
        out["bookmark_toggle_rect"] = json!(sb.geom.bookmark_toggle_rect);
        out["bookmarks_btn_rect"] = json!(sb.geom.bookmarks_btn_rect);
        out["bookmarks"] = Value::Array(bm_items);
    }

    pub(crate) fn draw_sidebar_bookmarks_popup(&mut self, buf: &mut [u32], width: usize, height: usize) {
        if !self.sidebar_bookmarks_popup_open() {
            return;
        }
        let Some(anchor) = self.ui.sidebar.geom.bookmarks_btn_rect else {
            return;
        };
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let list_w = self.effective_list_w(s);
        let cur_loc = self.current_dir_nav_entry();
        let bms = self.ui.sidebar.nav.bookmarks.clone();
        let sel_idx = self.ui.sidebar.nav.popup_sel;
        let vis = bms.len().min(HOST_POPUP_MAX_VIS);
        let top_idx = self.ui.sidebar.nav.popup_top.min(bms.len().saturating_sub(vis));
        let char_w = self.char_w();

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let pop_w = ((list_w as f32 - 8.0 * s).max(220.0 * s)).min(width as f32 - 8.0 * s);
        let x0 = (anchor[2] - pop_w).max(4.0 * s);
        let y0 = anchor[3] + 2.0 * s;
        let rows_shown = vis.max(1);
        let pop_h = rows_shown as f32 * lh + 4.0 * s;

        controls::fill(buf, width, height, [x0 - 1.0, y0 - 1.0, x0 + pop_w + 1.0, y0 + pop_h + 1.0], 0xFF3E_5268);
        controls::fill(buf, width, height, [x0, y0, x0 + pop_w, y0 + pop_h], 0xFF16_202D);

        let ink = Color::rgb(0xEC, 0xF2, 0xF8);
        let muted = Color::rgb(0x9C, 0xAA, 0xBC);
        let gold = Color::rgb(0xFA, 0xD2, 0x6E);
        let del_col = Color::rgb(0xD8, 0x84, 0x84);

        if bms.is_empty() {
            let max_c = ((pop_w - 12.0 * s) / (char_w * s)).max(12.0) as usize;
            let msg = clip_cols("No bookmarks yet (click \u{2606} to save)", max_c);
            controls.text(fs, swash, buf, width, height, x0 + 6.0 * s, y0 + 2.0 * s, (x0 + pop_w - 4.0 * s) as i32, &msg, muted);
            self.ui.sidebar.geom.bookmark_items.clear();
            return;
        }

        let rm_w = controls.width(fs, "\u{00d7}") + 12.0 * s;
        let label_cols = (((pop_w - rm_w - 14.0 * s).max(40.0)) / (char_w * s)).max(8.0) as usize;
        let mut items = Vec::new();
        for row_i in 0..vis {
            let idx = top_idx + row_i;
            let bm = &bms[idx];
            let r = [
                x0 + 2.0 * s,
                y0 + 2.0 * s + row_i as f32 * lh,
                x0 + pop_w - 2.0 * s,
                y0 + 2.0 * s + (row_i + 1) as f32 * lh,
            ];
            if idx == sel_idx {
                controls::fill(buf, width, height, r, 0xFF28_4260);
            }
            let active = cur_loc.as_ref().is_some_and(|loc| bm.matches_loc(loc));
            let mark = if active { "\u{2605} " } else { "  " };
            let shown = clip_cols(&format!("{mark}{}", bm.label), label_cols);
            let main_r = [r[0], r[1], r[2] - rm_w, r[3]];
            let rm_r = [r[2] - rm_w, r[1], r[2], r[3]];
            controls.text(
                fs,
                swash,
                buf,
                width,
                height,
                main_r[0] + 4.0 * s,
                main_r[1],
                main_r[2] as i32,
                &shown,
                if active { gold } else if idx == sel_idx { ink } else { muted },
            );
            controls.target(main_r, Action::SidebarBookmarkItem(idx));
            controls.text(fs, swash, buf, width, height, rm_r[0] + 4.0 * s, rm_r[1], rm_r[2] as i32, "\u{00d7}", del_col);
            controls.target(rm_r, Action::SidebarBookmarkRemove(idx));
            items.push(BookmarkItemGeom { index: idx, rect: main_r, remove_rect: rm_r });
        }
        self.ui.sidebar.geom.bookmark_items = items;
    }

    pub(crate) fn sidebar_nav_popup_click(&mut self, hit: Option<Action>) -> bool {
        if !self.sidebar_bookmarks_popup_open() {
            return false;
        }
        match hit {
            Some(Action::SidebarBookmarkItem(idx)) => self.select_sidebar_bookmark(idx),
            Some(Action::SidebarBookmarkRemove(idx)) => self.remove_sidebar_bookmark(idx),
            _ => self.close_sidebar_bookmarks_popup(),
        }
        true
    }

    pub(crate) fn sidebar_nav_wheel(&mut self, step: isize) -> bool {
        if !self.ui.sidebar.nav.popup_open {
            return false;
        }
        let max_idx = self.ui.sidebar.nav.bookmarks.len().saturating_sub(1) as isize;
        self.ui.sidebar.nav.popup_sel = (self.ui.sidebar.nav.popup_sel as isize)
            .saturating_add(step.clamp(-4, 4))
            .clamp(0, max_idx) as usize;
        self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
        self.dirty = true;
        true
    }

    pub(crate) fn sidebar_nav_key(&mut self, event: &KeyEvent) -> bool {
        if self.sidebar_bookmarks_popup_open() {
            match &event.logical_key {
                Key::Named(NamedKey::Escape) if !event.repeat => self.close_sidebar_bookmarks_popup(),
                Key::Named(NamedKey::Enter | NamedKey::Space) if !event.repeat => {
                    if self.ui.sidebar.nav.bookmarks.is_empty() {
                        self.close_sidebar_bookmarks_popup();
                    } else {
                        let sel = self.ui.sidebar.nav.popup_sel;
                        self.select_sidebar_bookmark(sel);
                    }
                }
                Key::Named(NamedKey::Delete | NamedKey::Backspace) if !event.repeat => {
                    if !self.ui.sidebar.nav.bookmarks.is_empty() {
                        let sel = self.ui.sidebar.nav.popup_sel;
                        self.remove_sidebar_bookmark(sel);
                    }
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.ui.sidebar.nav.popup_sel = self.ui.sidebar.nav.popup_sel.saturating_sub(1);
                    self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
                    self.dirty = true;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    let max_i = self.ui.sidebar.nav.bookmarks.len().saturating_sub(1);
                    self.ui.sidebar.nav.popup_sel = (self.ui.sidebar.nav.popup_sel + 1).min(max_i);
                    self.ui.sidebar.nav.sync_popup_scroll(HOST_POPUP_MAX_VIS);
                    self.dirty = true;
                }
                _ => {}
            }
            return true;
        }
        if !self.ui.sidebar.collapsed
            && self.ui.sidebar.tab == 0
            && self.ui.mods.alt_key()
            && !self.ui.mods.control_key()
            && !self.ui.mods.shift_key()
        {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowLeft) => {
                    if !event.repeat {
                        self.sidebar_nav_step(-1);
                    }
                    return true;
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if !event.repeat {
                        self.sidebar_nav_step(1);
                    }
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}
