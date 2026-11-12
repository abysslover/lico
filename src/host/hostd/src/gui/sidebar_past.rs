//! Full-pagination `[ Past ]` sidebar tab (`session.past`, zero top-N truncation) and 1-click
//! session resume (`session.resume` for disconnected tabs, `session.create` with `resume_id` for
//! external/closed `claude` and `codex` `.jsonl` transcripts) (`M07-P03`).

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::controls::{self, Action, Rect};
use super::panel::AsyncJob;
use super::sidebar::{norm_path, SidebarGeom};
use super::sidebar_render::{clip_cols, tail_path};
use super::ui::memrow;
use super::App;

#[derive(Clone, Debug)]
pub struct PastEntry {
    pub provider: String,
    pub provider_session: String,
    pub account: Option<String>,
    pub account_label: Option<String>,
    pub ssh_host: Option<String>,
    pub cwd: Option<String>,
    pub global: bool,
    pub title: String,
    pub updated_at: u64,
    pub open_session: Option<String>,
    pub open_state: Option<String>,
}

#[derive(Clone, Default)]
pub struct PastEntryGeom {
    pub index: usize,
    pub provider: String,
    pub provider_session: String,
    pub title: String,
    pub open_session: Option<String>,
    pub open_state: Option<String>,
    pub rect: Rect,
}

#[derive(Default)]
pub struct PastSidebarState {
    pub scope_key: String,
    pub items: Vec<PastEntry>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub pending_req: u64,
    pub loading: bool,
    pub error: Option<String>,
    pub load_count: u64,
    pub resume_pending_req: u64,
    pub entries_geom: Vec<PastEntryGeom>,
}

pub(super) fn calc_past_ps(tab_y1: f32, hint_y: f32, lh: f32, s: f32) -> usize {
    let list_top = tab_y1 + 4.0 * lh + 26.0 * s;
    let nav_y = hint_y - 2.0 * lh - 4.0 * s;
    let avail = (nav_y - list_top).max(0.0);
    let card_h = (2.0 * lh + 4.0 * s).max(16.0);
    ((avail / card_h).floor() as usize).clamp(3, 25)
}

fn rel_age(now: u64, updated_at: u64) -> String {
    if updated_at == 0 || now <= updated_at {
        return "now".into();
    }
    let d = now - updated_at;
    if d < 60 {
        format!("{d}s")
    } else if d < 3600 {
        format!("{}m", d / 60)
    } else if d < 86_400 {
        format!("{}h", d / 3600)
    } else {
        format!("{}d", d / 86_400)
    }
}

fn parse_past_items(reply: &Value) -> Vec<PastEntry> {
    reply["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| {
            let provider = it["provider"].as_str()?.to_string();
            let provider_session = it["provider_session"].as_str()?.to_string();
            let title = it["title"].as_str().unwrap_or(&provider_session).to_string();
            Some(PastEntry {
                provider,
                provider_session,
                account: it["account"].as_str().map(str::to_string),
                account_label: it["account_label"].as_str().map(str::to_string),
                ssh_host: it["ssh_host"].as_str().map(str::to_string),
                cwd: it["cwd"].as_str().map(str::to_string),
                global: it["global"] == json!(true),
                title,
                updated_at: it["updated_at"].as_u64().unwrap_or(0),
                open_session: it["open_session"].as_str().map(str::to_string),
                open_state: it["open_state"].as_str().map(str::to_string),
            })
        })
        .collect()
}

impl PastSidebarState {
    pub fn total_pages(&self) -> usize {
        let step = self.page_size.max(1);
        self.total.div_ceil(step).max(1)
    }
}

impl App {
    fn past_target_scope(&self) -> Option<(bool, Option<String>, Option<String>, String)> {
        let sb = &self.ui.sidebar;
        let host = sb.ssh_host.clone();
        let host_s = host.as_deref().unwrap_or("");
        let q = sb.filter.trim();
        if sb.global_scope {
            let key = format!("global|{host_s}|{q}");
            return Some((true, host, None, key));
        }
        let dir = if !sb.cwd.is_empty() {
            Some(sb.cwd.clone())
        } else {
            let (_, act_h, act_d) = self.active_session_scope();
            if act_h == host { act_d } else { None }
        }?;
        let key = format!("dir|{host_s}|{}|{q}", norm_path(&dir));
        Some((false, host, Some(dir), key))
    }

    pub(crate) fn past_sync_if_open(&mut self, force: bool) {
        if !self.sidebar_shown() || self.socket.is_empty() || self.ui.sidebar.tab != 1 {
            return;
        }
        let Some((_, _, _, key)) = self.past_target_scope() else {
            return;
        };
        if force || self.ui.sidebar.past.load_count == 0 || self.ui.sidebar.past.scope_key != key {
            self.past_start_fetch(0, false);
        }
    }

    pub(crate) fn past_start_fetch(&mut self, page: usize, refreshed: bool) {
        if self.socket.is_empty() {
            return;
        }
        let Some((is_glob, ssh_host, cwd_opt, scope_key)) = self.past_target_scope() else {
            return;
        };
        let limit = self.ui.sidebar.past.page_size.max(12).clamp(1, 100);
        let offset = page.saturating_mul(limit);
        let mut req = json!({
            "op": "session.past",
            "offset": offset as i64,
            "limit": limit as i64,
        });
        if is_glob {
            req["global"] = json!(true);
        } else if let Some(ref c) = cwd_opt {
            req["cwd"] = json!(c);
        }
        if let Some(ref h) = ssh_host {
            req["ssh_host"] = json!(h);
        }
        let q = self.ui.sidebar.filter.trim();
        if !q.is_empty() {
            req["query"] = json!(q);
        }
        if refreshed {
            self.ui.sidebar.refresh_count += 1;
        }
        self.ui.sidebar.past.scope_key = scope_key.clone();
        self.ui.sidebar.past.page = page;
        self.ui.sidebar.past.loading = true;
        self.ui.sidebar.past.error = None;
        self.dirty = true;

        let has_proxy = self.ui.proxy.is_some();
        let req_id = self.spawn_host_call(req, 7, move |req_id| AsyncJob::SidebarPast {
            req_id,
            scope_key,
            page,
        });
        if has_proxy {
            self.ui.sidebar.past.pending_req = req_id;
        }
    }

    pub(crate) fn past_step_page(&mut self, delta: isize) {
        if delta == 0 {
            return;
        }
        let total_pages = self.ui.sidebar.past.total_pages();
        let cur = self.ui.sidebar.past.page.min(total_pages.saturating_sub(1));
        let next = (cur as isize + delta).clamp(0, total_pages.saturating_sub(1) as isize) as usize;
        if next != cur {
            self.past_start_fetch(next, false);
        }
    }

    pub(crate) fn past_activate_entry(&mut self, idx: usize) {
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.filter_focused = false;
        let Some(entry) = self.ui.sidebar.past.items.get(idx).cloned() else {
            return;
        };
        if let Some(ref sid) = entry.open_session {
            if let Some(row_state) = self.ui.rows.iter_mut().find(|r| r.session == *sid).map(|r| {
                r.tab_closed = false;
                r.state.clone()
            }) {
                self.refresh_list();
                if row_state == "running" {
                    self.panel_close();
                    self.switch_to(sid);
                    self.ui.flash = Some("Switched to open session.".into());
                    self.dirty = true;
                    return;
                }
                if row_state == "disconnected" {
                    self.panel_close();
                    self.switch_to(sid);
                    self.resume_session();
                    return;
                }
            }
        }

        let mut req = json!({
            "op": "session.create",
            "kind": entry.provider,
            "mode": "auto",
            "resume_id": entry.provider_session,
        });
        if let Some(ref acc) = entry.account {
            req["account"] = json!(acc);
        }
        let created_cwd = if entry.global { None } else { entry.cwd.clone() };
        if let Some(ref c) = created_cwd {
            req["cwd"] = json!(c);
        }
        self.ui.flash = Some(format!("Resuming {} session...", entry.provider));
        self.dirty = true;

        let local_created_cwd = if entry.ssh_host.is_some() { None } else { created_cwd };
        let has_proxy = self.ui.proxy.is_some();
        let req_id = self.spawn_host_call(req, 8, move |req_id| AsyncJob::PastResume {
            req_id,
            created_cwd: local_created_cwd,
        });
        if has_proxy {
            self.ui.sidebar.past.resume_pending_req = req_id;
        }
    }

    pub(crate) fn on_past_async(&mut self, job: AsyncJob, reply: Value) {
        let has_proxy = self.ui.proxy.is_some();
        match job {
            AsyncJob::SidebarPast { req_id, scope_key, page } => {
                if (has_proxy && self.ui.sidebar.past.pending_req != req_id)
                    || self.ui.sidebar.past.scope_key != scope_key
                {
                    return;
                }
                self.ui.sidebar.past.pending_req = 0;
                self.ui.sidebar.past.loading = false;
                if reply["ok"] != json!(1) {
                    let err = reply["error"].as_str().unwrap_or("unknown").to_string();
                    self.ui.sidebar.past.error = Some(err);
                    self.dirty = true;
                    return;
                }
                let total = reply["total"].as_u64().unwrap_or(0) as usize;
                let items = parse_past_items(&reply);
                self.ui.sidebar.past.total = total;
                self.ui.sidebar.past.items = items;
                self.ui.sidebar.past.page = page;
                self.ui.sidebar.past.error = None;
                self.ui.sidebar.past.load_count += 1;
                let tp = self.ui.sidebar.past.total_pages();
                if page >= tp && total > 0 {
                    self.past_start_fetch(tp - 1, false);
                    return;
                }
                self.dirty = true;
            }
            AsyncJob::PastResume { req_id, created_cwd } => {
                if has_proxy && self.ui.sidebar.past.resume_pending_req != req_id {
                    return;
                }
                self.ui.sidebar.past.resume_pending_req = 0;
                if reply["ok"] == json!(1) {
                    let id = reply["session"].as_str().unwrap_or("").to_string();
                    self.panel_close();
                    self.send("{\"op\":\"status\"}");
                    self.ui.scroll = 0;
                    self.show(&id, None);
                    self.ui.tab_ensure_visible = true;
                    self.persist_workspace(created_cwd.as_deref(), Some("session"), Some(&id));
                    self.ui.flash = Some("Resumed past session.".into());
                    if self.ui.sidebar.tab == 1 {
                        self.past_start_fetch(self.ui.sidebar.past.page, false);
                    }
                } else {
                    let err = reply["error"].as_str().unwrap_or("unknown");
                    self.ui.flash = Some(format!("Could not resume past session: {err}"));
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    pub(super) fn insert_past_json(&self, out: &mut Value) {
        let p = &self.ui.sidebar.past;
        let entries: Vec<Value> = p
            .entries_geom
            .iter()
            .map(|e| {
                json!({
                    "index": e.index,
                    "provider": e.provider,
                    "provider_session": e.provider_session,
                    "title": e.title,
                    "open_session": e.open_session,
                    "open_state": e.open_state,
                    "rect": e.rect,
                })
            })
            .collect();
        out["past_total"] = json!(p.total);
        out["past_page"] = json!(p.page + 1);
        out["past_page_size"] = json!(p.page_size);
        out["past_total_pages"] = json!(p.total_pages());
        out["past_loading"] = json!(p.loading);
        out["past_load_count"] = json!(p.load_count);
        out["past_error"] = json!(p.error);
        out["past_entries"] = json!(entries);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_past_sidebar(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        x_right: f32,
        tab_y1: f32,
        hint_y: f32,
        total_mem: &str,
        mut geom: SidebarGeom,
    ) {
        self.ui.sidebar.past.entries_geom.clear();
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let char_w = self.char_w();
        let max_cols = ((x_right - 12.0 * s) / (char_w * s)).max(12.0) as usize;
        let mem_y = hint_y - lh;
        let mut y = tab_y1 + 4.0 * s;

        let is_active_cwd = self.sidebar_is_active_cwd();
        let is_glob_hi = self.sidebar_is_global_highlighted();
        let can_jump = self.sidebar_can_jump_active();
        let ssh_host = self.ui.sidebar.ssh_host.clone();
        let cwd = self.ui.sidebar.cwd.clone();
        let filter = self.ui.sidebar.filter.clone();
        let filter_focused = self.ui.sidebar.filter_focused;
        let unreachable = self.ui.sidebar.unreachable;
        let now = self.clock_now();
        let cur_session = self.session.clone();

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let ink = Color::rgb(0xEA, 0xF0, 0xF7);
        let muted = Color::rgb(0x98, 0xA6, 0xB8);
        let dim = Color::rgb(0x5E, 0x6C, 0x7E);
        let dir_ink = Color::rgb(0x88, 0xC8, 0xFF);
        let gold_ink = Color::rgb(0xFA, 0xD2, 0x6E);
        let teal_ink = Color::rgb(0x64, 0xE2, 0xCB);
        let claude_ink = Color::rgb(0xF2, 0xAA, 0x6B);
        let codex_ink = Color::rgb(0x7A, 0xE0, 0xB8);

        // 1. Controls row: `[Host: local v]`  `[R]`  `[+]`
        let row_h = lh + 2.0 * s;
        let plus_w = controls.width(fs, "+") + 12.0 * s;
        let ref_w = controls.width(fs, "R") + 12.0 * s;
        let new_rect = [x_right - 4.0 * s - plus_w, y, x_right - 4.0 * s, y + row_h];
        let ref_rect = [new_rect[0] - 3.0 * s - ref_w, y, new_rect[0] - 3.0 * s, y + row_h];
        let host_rect = [4.0 * s, y, (ref_rect[0] - 4.0 * s).max(40.0 * s), y + row_h];

        let host_lbl = match &ssh_host {
            Some(h) => clip_cols(&format!("ssh:{h} v"), 20),
            None => "Host: local v".to_string(),
        };
        let host_fill = if self.ui.sidebar.host_popup_open || ssh_host.is_some() { 0xFF22_364E } else { 0xFF1A_2432 };
        controls.button(fs, swash, buf, width, height, host_rect, &host_lbl, host_fill, ink, Action::SidebarHost);
        controls.button(fs, swash, buf, width, height, ref_rect, "R", 0xFF1A_2432, ink, Action::SidebarRefresh);
        controls.button(fs, swash, buf, width, height, new_rect, "+", 0xFF21_364B, ink, Action::SidebarNewHere);
        geom.host_rect = Some(host_rect);
        geom.refresh_rect = Some(ref_rect);
        geom.new_here_rect = Some(new_rect);
        y += row_h + 3.0 * s;

        // 2. Past-session search/filter box
        let f_rect = [4.0 * s, y, x_right - 4.0 * s, y + row_h];
        controls::fill(buf, width, height, f_rect, if filter_focused { 0xFF1E_2D42 } else { 0xFF16_1E29 });
        if filter_focused {
            controls::fill(buf, width, height, [f_rect[0], f_rect[3] - 1.0, f_rect[2], f_rect[3]], 0xFF56_B4FF);
        }
        let has_clear = filter_focused || !filter.is_empty();
        let clr_w = if has_clear { controls.width(fs, "\u{00d7}") + 10.0 * s } else { 0.0 };
        let f_text = if filter.is_empty() && !filter_focused {
            "Filter past sessions...".to_string()
        } else if filter_focused {
            format!("Filter: {filter}|")
        } else {
            format!("Filter: {filter}")
        };
        let f_col = if filter.is_empty() && !filter_focused { dim } else { ink };
        controls.text(fs, swash, buf, width, height, f_rect[0] + 6.0 * s, y + s, (f_rect[2] - clr_w - 2.0 * s) as i32, &f_text, f_col);
        let main_f_rect = [f_rect[0], f_rect[1], f_rect[2] - clr_w, f_rect[3]];
        controls.target(main_f_rect, Action::SidebarFilter);
        geom.filter_rect = Some(main_f_rect);
        if has_clear {
            let c_rect = [f_rect[2] - clr_w, f_rect[1], f_rect[2], f_rect[3]];
            controls.text(fs, swash, buf, width, height, c_rect[0] + 4.0 * s, y + s, c_rect[2] as i32, "\u{00d7}", muted);
            controls.target(c_rect, Action::SidebarFilterClear);
            geom.filter_clear_rect = Some(c_rect);
        }
        y += row_h + 4.0 * s;

        // 3. Global general scope pill
        let g_rect = [4.0 * s, y, x_right - 4.0 * s, y + lh + 2.0 * s];
        let (g_fill, g_col, g_lbl) = if is_glob_hi {
            (0xFF17_3436, teal_ink, "\u{2605} Global (general)  [scope]")
        } else {
            (0xFF15_1D28, muted, "\u{25cb} Global (general)")
        };
        controls::fill(buf, width, height, g_rect, g_fill);
        if is_glob_hi {
            controls::fill(buf, width, height, [g_rect[0], g_rect[1], g_rect[0] + 3.0 * s, g_rect[3]], 0xFF4E_D4B8);
        }
        controls.text(fs, swash, buf, width, height, g_rect[0] + 7.0 * s, y + s, g_rect[2] as i32, g_lbl, g_col);
        controls.target(g_rect, Action::SidebarGlobal);
        geom.global_rect = Some(g_rect);
        y += lh + 5.0 * s;

        // 4. Active / Viewed Working Directory scope row
        let dir_rect = [4.0 * s, y, x_right - 4.0 * s, y + lh + 3.0 * s];
        let jump_w = if can_jump { controls.width(fs, "-> active") + 10.0 * s } else { 0.0 };
        let dir_selected = !self.ui.sidebar.global_scope;
        let dir_fill = if is_active_cwd {
            0xFF1E_3552
        } else if dir_selected {
            0xFF1A_293C
        } else {
            0xFF16_202D
        };
        controls::fill(buf, width, height, dir_rect, dir_fill);
        if is_active_cwd {
            controls::fill(buf, width, height, [dir_rect[0], dir_rect[1], dir_rect[0] + 3.0 * s, dir_rect[3]], 0xFFF5_C451);
        } else if dir_selected {
            controls::fill(buf, width, height, [dir_rect[0], dir_rect[1], dir_rect[0] + 3.0 * s, dir_rect[3]], 0xFF56_B4FF);
        }
        let raw_dir = match &ssh_host {
            Some(h) => format!("ssh:{h}:{}", if cwd.is_empty() { "~" } else { cwd.as_str() }),
            None => if cwd.is_empty() { "(select directory)".into() } else { cwd.clone() },
        };
        let avail_dir_cols = (((dir_rect[2] - jump_w - (dir_rect[0] + 8.0 * s)).max(32.0)) / (char_w * s)) as usize;
        let dir_label = if is_active_cwd {
            format!("\u{2605} {}", tail_path(&raw_dir, avail_dir_cols.saturating_sub(3)))
        } else {
            tail_path(&raw_dir, avail_dir_cols)
        };
        controls.text(
            fs,
            swash,
            buf,
            width,
            height,
            dir_rect[0] + 7.0 * s,
            y + 1.5 * s,
            (dir_rect[2] - jump_w - 2.0 * s) as i32,
            &dir_label,
            if is_active_cwd { gold_ink } else { ink },
        );
        let main_dir_rect = [dir_rect[0], dir_rect[1], dir_rect[2] - jump_w, dir_rect[3]];
        controls.target(main_dir_rect, Action::SidebarActiveDir);
        geom.active_dir_rect = Some(main_dir_rect);
        if can_jump {
            let j_rect = [dir_rect[2] - jump_w, dir_rect[1] + s, dir_rect[2] - s, dir_rect[3] - s];
            controls.button(fs, swash, buf, width, height, j_rect, "-> active", 0xFF28_3E5A, dir_ink, Action::SidebarJumpActive);
            geom.jump_active_rect = Some(j_rect);
        }
        y += lh + 6.0 * s;

        // 5. Past session list cards
        let nav_y = mem_y - lh - 4.0 * s;
        let card_h = 2.0 * lh + 4.0 * s;
        let past_loading = self.ui.sidebar.past.loading;
        let past_err = self.ui.sidebar.past.error.clone();
        let items = self.ui.sidebar.past.items.clone();
        let total = self.ui.sidebar.past.total;
        let page_idx = self.ui.sidebar.past.page;
        let total_pages = self.ui.sidebar.past.total_pages();

        if unreachable {
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, &clip_cols("SSH host unreachable.", max_cols), Color::rgb(0xF0, 0x76, 0x76));
        } else if past_loading && items.is_empty() {
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, "Loading past sessions...", muted);
        } else if let Some(ref err) = past_err.filter(|_| items.is_empty()) {
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, &clip_cols(&format!("Cannot read ({err})"), max_cols), Color::rgb(0xF0, 0x76, 0x76));
        } else if items.is_empty() {
            let msg = if filter.trim().is_empty() {
                "(no past sessions in this scope)"
            } else {
                "(no matching past sessions)"
            };
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, msg, dim);
        } else {
            let mut out_geom = Vec::new();
            for (idx, it) in items.iter().enumerate() {
                if y + card_h > nav_y + 2.0 * s {
                    break;
                }
                let r = [3.0 * s, y, x_right - 3.0 * s, y + card_h - 2.0 * s];
                let is_cur = it.open_session.as_deref().is_some_and(|sid| sid == cur_session);
                let fill = if is_cur {
                    0xFF1F_334B
                } else if idx % 2 == 0 {
                    0xFF15_1E29
                } else {
                    0xFF13_1B25
                };
                controls::fill(buf, width, height, r, fill);
                if is_cur {
                    controls::fill(buf, width, height, [r[0], r[1], r[0] + 3.0 * s, r[3]], 0xFF56_B4FF);
                }

                let (badge, b_col) = if it.provider == "claude" {
                    ("C\u{00b7}", claude_ink)
                } else {
                    ("X\u{00b7}", codex_ink)
                };
                let (pill_txt, pill_col) = match it.open_state.as_deref() {
                    Some("running") => (Some("[open]"), Color::rgb(0x68, 0xE0, 0xA8)),
                    Some("disconnected") => (Some("[closed]"), gold_ink),
                    Some("exited") => (Some("[ended]"), muted),
                    _ => (None, dim),
                };
                let pill_w = pill_txt.map_or(0.0, |p| controls.width(fs, p) + 8.0 * s);
                let b_w = controls.text(fs, swash, buf, width, height, r[0] + 6.0 * s, y + s, r[2] as i32, badge, b_col);
                let title_x = r[0] + 6.0 * s + b_w + 4.0 * s;
                let title_clip = (r[2] - pill_w - 4.0 * s) as i32;
                let title_cols = (((title_clip as f32 - title_x).max(24.0)) / (char_w * s)) as usize;
                controls.text(fs, swash, buf, width, height, title_x, y + s, title_clip, &clip_cols(&it.title, title_cols), ink);
                if let Some(pt) = pill_txt {
                    controls.text(fs, swash, buf, width, height, r[2] - pill_w + 2.0 * s, y + s, r[2] as i32, pt, pill_col);
                }

                let who = it
                    .account_label
                    .as_deref()
                    .or(it.ssh_host.as_deref())
                    .unwrap_or("local");
                let short_id = &it.provider_session[..it.provider_session.len().min(8)];
                let sub = format!("{} \u{00b7} {} \u{00b7} #{short_id} \u{00b7} {}", it.provider, who, rel_age(now, it.updated_at));
                controls.text(fs, swash, buf, width, height, r[0] + 8.0 * s, y + lh + s, (r[2] - 4.0 * s) as i32, &clip_cols(&sub, max_cols.saturating_sub(2)), muted);

                controls.target(r, Action::SidebarPastEntry(idx));
                out_geom.push(PastEntryGeom {
                    index: idx,
                    provider: it.provider.clone(),
                    provider_session: it.provider_session.clone(),
                    title: it.title.clone(),
                    open_session: it.open_session.clone(),
                    open_state: it.open_state.clone(),
                    rect: r,
                });
                y += card_h;
            }
            self.ui.sidebar.past.entries_geom = out_geom;
        }

        // 6. Full Pagination Footer (`<  Page X/Y (Total)  >`) + Memory Total Line
        if nav_y > tab_y1 + lh * 3.0 {
            let shown_p = page_idx.min(total_pages.saturating_sub(1));
            let has_prev = shown_p > 0;
            let has_next = shown_p + 1 < total_pages;
            let btn_w = controls.width(fs, "<") + 12.0 * s;
            let pv_rect = [4.0 * s, nav_y, 4.0 * s + btn_w, nav_y + lh];
            let nx_rect = [x_right - 4.0 * s - btn_w, nav_y, x_right - 4.0 * s, nav_y + lh];
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                pv_rect,
                "<",
                if has_prev { 0xFF20_2E40 } else { 0xFF16_1E28 },
                if has_prev { ink } else { dim },
                Action::SidebarPage(-1),
            );
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                nx_rect,
                ">",
                if has_next { 0xFF20_2E40 } else { 0xFF16_1E28 },
                if has_next { ink } else { dim },
                Action::SidebarPage(1),
            );
            let pg_lbl = format!("Page {}/{} ({total})", shown_p + 1, total_pages);
            let pg_w = controls.width(fs, &pg_lbl);
            let pg_x = ((x_right - pg_w) * 0.5).max(pv_rect[2] + 4.0 * s);
            controls.text(fs, swash, buf, width, height, pg_x, nav_y, (nx_rect[0] - 2.0 * s) as i32, &pg_lbl, muted);
            geom.prev_rect = Some(pv_rect);
            geom.next_rect = Some(nx_rect);
        }

        memrow::draw_total(controls, fs, swash, buf, width, height, [4.0 * s, mem_y, x_right - 2.0, nav_y], total_mem, muted);
        self.ui.sidebar.geom = geom;
    }
}
