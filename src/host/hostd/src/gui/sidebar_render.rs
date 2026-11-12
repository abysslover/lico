//! Pixel rendering, host combo-box popup, and UI-dump serialization for the two-tab left sidebar (`M07-P02`).

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::controls::{self, Action};
use super::editor;
use super::panel::files_ui::HOST_POPUP_MAX_VIS;
use super::sidebar::{SidebarEntryGeom, SidebarGeom};
use super::ui::memrow;
use super::{App, BACKGROUND};

pub(super) fn clip_cols(text: &str, max_cols: usize) -> String {
    let width = |c: char| if editor::wide(c) { 2 } else { 1 };
    let total: usize = text.chars().map(width).sum();
    if total <= max_cols {
        return text.to_string();
    }
    let budget = max_cols.saturating_sub(1);
    let (mut out, mut used) = (String::new(), 0);
    for c in text.chars() {
        let w = width(c);
        if used + w > budget {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

pub(super) fn tail_path(path: &str, max_cols: usize) -> String {
    if path.is_empty() {
        return "(none)".into();
    }
    let width = |c: char| if editor::wide(c) { 2 } else { 1 };
    let total: usize = path.chars().map(width).sum();
    if total <= max_cols {
        return path.to_string();
    }
    let budget = max_cols.saturating_sub(1);
    let mut rev = Vec::new();
    let mut used = 0;
    for c in path.chars().rev() {
        let w = width(c);
        if used + w > budget {
            break;
        }
        rev.push(c);
        used += w;
    }
    rev.reverse();
    format!("…{}", rev.into_iter().collect::<String>())
}

fn short_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0}K", (bytes as f64 / 1024.0).max(1.0))
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}

impl App {
    pub(crate) fn sidebar_json(&self) -> Value {
        let sb = &self.ui.sidebar;
        let filtered = sb.filtered_indices();
        let (_, _, page_idx, total_pages) = sb.page_window(filtered.len(), sb.page_size.max(1));
        let entries: Vec<Value> = sb
            .geom
            .entries
            .iter()
            .map(|e| {
                json!({
                    "index": e.index,
                    "name": e.name,
                    "kind": e.kind,
                    "external": e.external,
                    "rect": e.rect,
                })
            })
            .collect();
        let choices = sb.host_choices();
        let host_items: Vec<Value> = choices
            .iter()
            .enumerate()
            .map(|(idx, ch)| {
                let active = *ch == sb.ssh_host;
                let label = match ch {
                    None => format!("{}local (this machine)", if active { "* " } else { "  " }),
                    Some(h) => format!("{}ssh: {h}", if active { "* " } else { "  " }),
                };
                let rect = sb.geom.host_items.iter().find(|(i, _)| *i == idx).map(|(_, r)| *r);
                json!({"index": idx, "host": ch, "label": label, "active": active, "rect": rect})
            })
            .collect();
        let mut out = json!({
            "shown": self.sidebar_shown(),
            "collapsed": sb.collapsed,
            "tab": if sb.tab == 0 { "directories" } else { "past" },
            "tab_index": sb.tab,
            "global_scope": sb.global_scope,
            "active_cwd_highlighted": self.sidebar_is_active_cwd(),
            "global_highlighted": self.sidebar_is_global_highlighted(),
            "can_jump_active": self.sidebar_can_jump_active(),
            "ssh_host": sb.ssh_host,
            "cwd": if sb.cwd.is_empty() { Value::Null } else { json!(sb.cwd) },
            "parent": sb.parent,
            "filter": sb.filter,
            "filter_focused": sb.filter_focused,
            "page": page_idx + 1,
            "page_size": sb.page_size,
            "total_pages": total_pages,
            "total_entries": sb.entries.len(),
            "filtered_count": filtered.len(),
            "load_count": sb.load_count,
            "refresh_count": sb.refresh_count,
            "loading": sb.loading,
            "unreachable": sb.unreachable,
            "error": sb.error,
            "last_opened": sb.last_opened,
            "host_popup_open": sb.host_popup_open,
            "dir_tab_rect": sb.geom.dir_tab_rect,
            "past_tab_rect": sb.geom.past_tab_rect,
            "host_rect": sb.geom.host_rect,
            "dotfiles_rect": sb.geom.dotfiles_rect,
            "refresh_rect": sb.geom.refresh_rect,
            "new_here_rect": sb.geom.new_here_rect,
            "filter_rect": sb.geom.filter_rect,
            "filter_clear_rect": sb.geom.filter_clear_rect,
            "global_rect": sb.geom.global_rect,
            "active_dir_rect": sb.geom.active_dir_rect,
            "jump_active_rect": sb.geom.jump_active_rect,
            "parent_rect": sb.geom.parent_rect,
            "prev_rect": sb.geom.prev_rect,
            "next_rect": sb.geom.next_rect,
            "entries": entries,
            "host_items": host_items,
        });
        self.insert_past_json(&mut out);
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_sidebar(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        list_w: usize,
        top: f32,
        chat_top: f32,
        hint_y: f32,
        total_mem: &str,
    ) {
        self.ui.sidebar.geom = SidebarGeom::default();
        if !self.sidebar_shown() || self.ui.sidebar.collapsed || list_w < 40 || hint_y <= top {
            return;
        }

        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let x_right = (list_w.saturating_sub(1)) as f32;
        let max_cols = ((x_right - 12.0 * s) / (self.char_w() * s)).max(12.0) as usize;

        // Paint full Column 1 background cleanly so `Cause::Shift` never double-blends anti-aliased sidebar glyphs.
        controls::fill(buf, width, height, [0.0, top, x_right, hint_y], BACKGROUND);

        let tab_idx = self.ui.sidebar.tab;
        let is_active_cwd = self.sidebar_is_active_cwd();
        let is_glob_hi = self.sidebar_is_global_highlighted();
        let can_jump = self.sidebar_can_jump_active();
        let show_hidden = self.ui.show_hidden;
        let ssh_host = self.ui.sidebar.ssh_host.clone();
        let cwd = self.ui.sidebar.cwd.clone();
        let parent = self.ui.sidebar.parent.clone();
        let filter = self.ui.sidebar.filter.clone();
        let filter_focused = self.ui.sidebar.filter_focused;
        let loading = self.ui.sidebar.loading.clone();
        let unreachable = self.ui.sidebar.unreachable;
        let error = self.ui.sidebar.error.clone();
        let filtered = self.ui.sidebar.filtered_indices();
        let char_w = self.char_w();

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let ink = Color::rgb(0xEA, 0xF0, 0xF7);
        let muted = Color::rgb(0x98, 0xA6, 0xB8);
        let dim = Color::rgb(0x5E, 0x6C, 0x7E);
        let dir_ink = Color::rgb(0x88, 0xC8, 0xFF);
        let gold_ink = Color::rgb(0xFA, 0xD2, 0x6E);
        let teal_ink = Color::rgb(0x64, 0xE2, 0xCB);

        // 1. Top 2-Tab Header (`[ Directories ]` | `[ Past ]`) aligned with chat tab bar (`top..chat_top`).
        let tab_y1 = if chat_top > top + 8.0 * s { chat_top } else { top + lh + 6.0 * s };
        controls::fill(buf, width, height, [0.0, top, x_right, tab_y1], 0xFF13_1A23);
        controls::fill(buf, width, height, [0.0, tab_y1 - 1.0, x_right, tab_y1], 0xFF2B_3646);
        let mid_x = (x_right * 0.54).floor();
        let d_rect = [2.0 * s, top + 2.0 * s, mid_x - 1.0 * s, tab_y1 - 1.0];
        let p_rect = [mid_x + 1.0 * s, top + 2.0 * s, x_right - 2.0 * s, tab_y1 - 1.0];
        let (d_fill, d_col) = if tab_idx == 0 { (0xFF21_3147, ink) } else { (0xFF17_202B, muted) };
        let (p_fill, p_col) = if tab_idx == 1 { (0xFF21_3147, ink) } else { (0xFF17_202B, muted) };
        controls::fill(buf, width, height, d_rect, d_fill);
        controls::fill(buf, width, height, p_rect, p_fill);
        let active_bar = if tab_idx == 0 { d_rect } else { p_rect };
        controls::fill(buf, width, height, [active_bar[0], active_bar[1], active_bar[2], active_bar[1] + 2.0 * s], 0xFF56_B4FF);
        controls.text(fs, swash, buf, width, height, d_rect[0] + 8.0 * s, d_rect[1] + s, d_rect[2] as i32, "Directories", d_col);
        controls.target(d_rect, Action::SidebarTab(0));
        controls.text(fs, swash, buf, width, height, p_rect[0] + 8.0 * s, p_rect[1] + s, p_rect[2] as i32, "Past", p_col);
        controls.target(p_rect, Action::SidebarTab(1));

        let mut geom = SidebarGeom {
            dir_tab_rect: Some(d_rect),
            past_tab_rect: Some(p_rect),
            ..Default::default()
        };

        let mem_y = hint_y - lh;
        let mut y = tab_y1 + 4.0 * s;
        self.ui.sidebar.past.page_size = super::sidebar_past::calc_past_ps(tab_y1, hint_y, lh, s);

        if tab_idx == 1 {
            self.draw_past_sidebar(buf, width, height, x_right, tab_y1, hint_y, total_mem, geom);
            return;
        }

        // 2. Controls row: `[Host: local v]`  `[.dot]`  `[R]`  `[+]`
        let row_h = lh + 2.0 * s;
        let plus_w = controls.width(fs, "+") + 12.0 * s;
        let ref_w = controls.width(fs, "R") + 12.0 * s;
        let dot_w = controls.width(fs, ".dot") + 10.0 * s;
        let new_rect = [x_right - 4.0 * s - plus_w, y, x_right - 4.0 * s, y + row_h];
        let ref_rect = [new_rect[0] - 3.0 * s - ref_w, y, new_rect[0] - 3.0 * s, y + row_h];
        let dot_rect = [ref_rect[0] - 3.0 * s - dot_w, y, ref_rect[0] - 3.0 * s, y + row_h];
        let host_rect = [4.0 * s, y, (dot_rect[0] - 4.0 * s).max(40.0 * s), y + row_h];

        let host_lbl = match &ssh_host {
            Some(h) => clip_cols(&format!("ssh:{h} v"), 18),
            None => "Host: local v".to_string(),
        };
        let host_fill = if self.ui.sidebar.host_popup_open || ssh_host.is_some() { 0xFF22_364E } else { 0xFF1A_2432 };
        controls.button(fs, swash, buf, width, height, host_rect, &host_lbl, host_fill, ink, Action::SidebarHost);
        controls.button(
            fs,
            swash,
            buf,
            width,
            height,
            dot_rect,
            ".dot",
            if show_hidden { 0xFF25_3D58 } else { 0xFF1A_2432 },
            if show_hidden { dir_ink } else { muted },
            Action::SidebarDotfiles,
        );
        controls.button(fs, swash, buf, width, height, ref_rect, "R", 0xFF1A_2432, ink, Action::SidebarRefresh);
        controls.button(fs, swash, buf, width, height, new_rect, "+", 0xFF21_364B, ink, Action::SidebarNewHere);
        geom.host_rect = Some(host_rect);
        geom.dotfiles_rect = Some(dot_rect);
        geom.refresh_rect = Some(ref_rect);
        geom.new_here_rect = Some(new_rect);
        y += row_h + 3.0 * s;

        // 3. Quick filename filter box: `Filter: <query>` + `×` clear
        let f_rect = [4.0 * s, y, x_right - 4.0 * s, y + row_h];
        let f_fill = if filter_focused { 0xFF1E_2D42 } else { 0xFF16_1E29 };
        controls::fill(buf, width, height, f_rect, f_fill);
        if filter_focused {
            controls::fill(buf, width, height, [f_rect[0], f_rect[3] - 1.0, f_rect[2], f_rect[3]], 0xFF56_B4FF);
        }
        let has_clear = filter_focused || !filter.is_empty();
        let clr_w = if has_clear { controls.width(fs, "\u{00d7}") + 10.0 * s } else { 0.0 };
        let f_text = if filter.is_empty() && !filter_focused {
            "Filter files...".to_string()
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

        // 4. Global general scope pill/row
        let g_rect = [4.0 * s, y, x_right - 4.0 * s, y + lh + 2.0 * s];
        let (g_fill, g_col, g_lbl) = if is_glob_hi {
            (0xFF17_3436, teal_ink, "\u{2605} Global (general)  [active]")
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

        // 5. Highlighted Active / Viewed Working Directory row (`★ <cwd> [active]` or `<cwd>` + `[-> active]`)
        let dir_rect = [4.0 * s, y, x_right - 4.0 * s, y + lh + 3.0 * s];
        let jump_w = if can_jump { controls.width(fs, "-> active") + 10.0 * s } else { 0.0 };
        let dir_fill = if is_active_cwd { 0xFF1E_3552 } else { 0xFF16_202D };
        controls::fill(buf, width, height, dir_rect, dir_fill);
        if is_active_cwd {
            controls::fill(buf, width, height, [dir_rect[0], dir_rect[1], dir_rect[0] + 3.0 * s, dir_rect[3]], 0xFFF5_C451);
        }
        let raw_dir = match &ssh_host {
            Some(h) => format!("ssh:{h}:{}", if cwd.is_empty() { "~" } else { cwd.as_str() }),
            None => if cwd.is_empty() { "(loading...)".into() } else { cwd.clone() },
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

        // 6. Optional `[..] (parent)` row when not filtering
        if parent.is_some() && filter.trim().is_empty() && !unreachable {
            let up_rect = [4.0 * s, y, x_right - 4.0 * s, y + lh];
            controls.text(fs, swash, buf, width, height, up_rect[0] + 4.0 * s, y, up_rect[2] as i32, "[..]  parent directory", muted);
            controls.target(up_rect, Action::SidebarParent);
            geom.parent_rect = Some(up_rect);
            y += lh + 2.0 * s;
        }

        // Reserve space at bottom for Pagination row (`lh + 4*s`) + Memory total row (`lh`)
        let nav_y = mem_y - lh - 4.0 * s;
        let list_avail_h = (nav_y - y).max(0.0);
        let ps = ((list_avail_h / lh).floor() as usize).clamp(1, 40);
        self.ui.sidebar.page_size = ps;

        // 7. Directory entries or status message
        if unreachable {
            controls.text(
                fs,
                swash,
                buf,
                width,
                height,
                6.0 * s,
                y,
                x_right as i32,
                &clip_cols("SSH host unreachable.", max_cols),
                Color::rgb(0xF0, 0x76, 0x76),
            );
            controls.text(
                fs,
                swash,
                buf,
                width,
                height,
                6.0 * s,
                y + lh,
                x_right as i32,
                &clip_cols("Click Host above to switch.", max_cols),
                muted,
            );
        } else if let Some(ref msg) = loading.filter(|_| self.ui.sidebar.entries.is_empty()) {
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, &clip_cols(msg, max_cols), muted);
        } else if let Some(ref err) = error.filter(|_| self.ui.sidebar.entries.is_empty()) {
            controls.text(
                fs,
                swash,
                buf,
                width,
                height,
                6.0 * s,
                y,
                x_right as i32,
                &clip_cols(&format!("Cannot read ({err})"), max_cols),
                Color::rgb(0xF0, 0x76, 0x76),
            );
        } else if filtered.is_empty() {
            let empty_msg = if filter.trim().is_empty() { "(empty directory)" } else { "(no matching files)" };
            controls.text(fs, swash, buf, width, height, 6.0 * s, y, x_right as i32, empty_msg, dim);
        } else {
            let (start, end, page_idx, _) = self.ui.sidebar.page_window(filtered.len(), ps);
            self.ui.sidebar.page = page_idx;
            for &entry_idx in &filtered[start..end] {
                if y + lh > nav_y + 1.0 {
                    break;
                }
                let item = &self.ui.sidebar.entries[entry_idx];
                let r = [2.0 * s, y, x_right - 2.0 * s, y + lh];
                let (label, row_col) = match item.kind.as_str() {
                    "dir" => (clip_cols(&format!("\u{25b8} {}/", item.name), max_cols), dir_ink),
                    "link" => (clip_cols(&format!("@ {}", item.name), max_cols), muted),
                    _ if item.external => (
                        clip_cols(&format!("  {} [ext]", item.name), max_cols),
                        Color::rgb(0x9C, 0xD8, 0xC8),
                    ),
                    _ => {
                        let sz = short_bytes(item.size);
                        let name_budget = max_cols.saturating_sub(sz.len() + 4);
                        (format!("  {}  {sz}", clip_cols(&item.name, name_budget)), ink)
                    }
                };
                controls.text(fs, swash, buf, width, height, r[0] + 4.0 * s, y, r[2] as i32, &label, row_col);
                controls.target(r, Action::SidebarEntry(entry_idx));
                geom.entries.push(SidebarEntryGeom {
                    index: entry_idx,
                    name: item.name.clone(),
                    kind: item.kind.clone(),
                    external: item.external,
                    rect: r,
                });
                y += lh;
            }
        }

        // 8. Pagination Footer (`<  Page X/Y (N)  >`) + Memory Total Line
        if nav_y > tab_y1 + lh * 3.0 {
            let (_, _, page_idx, total_pages) = self.ui.sidebar.page_window(filtered.len(), ps);
            let has_prev = page_idx > 0;
            let has_next = page_idx + 1 < total_pages;
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
            let pg_lbl = format!("Page {}/{} ({})", page_idx + 1, total_pages, filtered.len());
            let pg_w = controls.width(fs, &pg_lbl);
            let pg_x = ((x_right - pg_w) * 0.5).max(pv_rect[2] + 4.0 * s);
            controls.text(fs, swash, buf, width, height, pg_x, nav_y, (nx_rect[0] - 2.0 * s) as i32, &pg_lbl, muted);
            geom.prev_rect = Some(pv_rect);
            geom.next_rect = Some(nx_rect);
        }

        memrow::draw_total(controls, fs, swash, buf, width, height, [4.0 * s, mem_y, x_right - 2.0, nav_y], total_mem, muted);
        self.ui.sidebar.geom = geom;
    }

    pub(crate) fn draw_sidebar_host_popup(&mut self, buf: &mut [u32], width: usize, height: usize) {
        if !self.sidebar_host_popup_open() {
            return;
        }
        let Some(anchor) = self.ui.sidebar.geom.host_rect else {
            return;
        };
        let choices = self.ui.sidebar.host_choices();
        if choices.is_empty() {
            return;
        }
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let vis = choices.len().min(HOST_POPUP_MAX_VIS);
        let top_idx = self.ui.sidebar.host_popup_top.min(choices.len().saturating_sub(vis));
        let sel_idx = self.ui.sidebar.host_popup_sel;
        let cur_host = self.ui.sidebar.ssh_host.clone();

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let pop_w = (260.0 * s).min(width as f32 - anchor[0] - 4.0 * s);
        let x0 = anchor[0];
        let y0 = anchor[3] + 2.0 * s;
        let pop_h = vis as f32 * lh + 4.0 * s;
        controls::fill(buf, width, height, [x0 - 1.0, y0 - 1.0, x0 + pop_w + 1.0, y0 + pop_h + 1.0], 0xFF3A_4A5E);
        controls::fill(buf, width, height, [x0, y0, x0 + pop_w, y0 + pop_h], 0xFF16_202D);

        let ink = Color::rgb(0xEC, 0xF2, 0xF8);
        let muted = Color::rgb(0x9C, 0xAA, 0xBC);
        let mut items = Vec::new();
        for row_i in 0..vis {
            let idx = top_idx + row_i;
            let ch = &choices[idx];
            let r = [x0 + 2.0 * s, y0 + 2.0 * s + row_i as f32 * lh, x0 + pop_w - 2.0 * s, y0 + 2.0 * s + (row_i + 1) as f32 * lh];
            if idx == sel_idx {
                controls::fill(buf, width, height, r, 0xFF28_4260);
            }
            let active = *ch == cur_host;
            let label = match ch {
                None => format!("{}local (this machine)", if active { "* " } else { "  " }),
                Some(h) => format!("{}ssh: {h}", if active { "* " } else { "  " }),
            };
            controls.text(fs, swash, buf, width, height, r[0] + 6.0 * s, r[1], r[2] as i32, &label, if active || idx == sel_idx { ink } else { muted });
            controls.target(r, Action::SidebarHostItem(idx));
            items.push((idx, r));
        }
        self.ui.sidebar.geom.host_items = items;
    }
}
