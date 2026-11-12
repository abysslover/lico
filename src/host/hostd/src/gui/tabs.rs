//! Multi-session horizontal tab bar at the top of the right main pane (`M07-P01`).
//!
//! Displays open sessions in `self.ui.numbered` order (`Alt+1..9`, `Ctrl+Tab`), marks `Global`
//! general sessions (`G · ...`), active/working turns (`●`), pending approvals (`!`), unread
//! background turns (`•`), and closed sessions (`○`), and provides per-tab close (`×`), overflow
//! scroll (`<` / `>`), `+` (`Ctrl+N`), and `+G` (`Ctrl+Shift+N` new Global session) buttons.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::super::controls::{self, Action};
use super::super::editor;
use super::super::App;
use super::Row;

#[derive(Clone, Default)]
pub struct TabItemGeom {
    pub session: String,
    pub label: String,
    pub global: bool,
    pub selected: bool,
    pub state: String,
    pub unread: bool,
    pub rect: [f32; 4],
    pub close_rect: [f32; 4],
}

#[derive(Default)]
pub struct TabsGeom {
    pub items: Vec<TabItemGeom>,
    pub prev_rect: Option<[f32; 4]>,
    pub next_rect: Option<[f32; 4]>,
    pub new_rect: Option<[f32; 4]>,
    pub new_global_rect: Option<[f32; 4]>,
}

pub fn height_px(shown: bool, scale: f32, line_h: f32) -> f32 {
    if shown {
        (line_h + 6.0) * scale.max(1.0)
    } else {
        0.0
    }
}

fn truncate_cols(text: &str, max_cols: usize) -> String {
    let width = |c: char| if editor::wide(c) { 2 } else { 1 };
    let total: usize = text.chars().map(width).sum();
    if total <= max_cols {
        return text.to_string();
    }
    let mut out = String::new();
    let mut cols = 0;
    let limit = max_cols.saturating_sub(1);
    for c in text.chars() {
        let w = width(c);
        if cols + w > limit {
            break;
        }
        out.push(c);
        cols += w;
    }
    out.push('…');
    out
}

impl App {
    pub(crate) fn tabs_shown(&self) -> bool {
        self.ui.workbench && self.ui.menu
    }

    pub(crate) fn step_tab_scroll(&mut self, delta: isize) {
        let max_scroll = self.ui.numbered.len().saturating_sub(1) as isize;
        let next = (self.ui.tab_scroll as isize + delta).clamp(0, max_scroll) as usize;
        if next != self.ui.tab_scroll {
            self.ui.tab_scroll = next;
            self.dirty = true;
        }
    }

    pub(crate) fn sync_active_tab_scroll(&mut self) {
        if let Some(idx) = self.ui.numbered.iter().position(|id| *id == self.session) {
            if idx < self.ui.tab_scroll {
                self.ui.tab_scroll = idx;
            }
            self.ui.tab_ensure_visible = true;
        }
    }

    fn tab_caption(&self, row: &Row) -> String {
        let prefix = if row.is_global() {
            "G"
        } else if row.provider == "claude" {
            "C"
        } else {
            "X"
        };
        let title = truncate_cols(&row.name(), 18);
        let badge = if !row.approvals.is_empty() {
            " !"
        } else if row.holds_a_turn() {
            " \u{25cf}"
        } else if self.row_unread(row) {
            " \u{2022}"
        } else if row.state != "running" {
            " \u{25cb}"
        } else {
            ""
        };
        format!("{prefix}\u{00b7}{title}{badge}")
    }

    pub(crate) fn tabs_json(&self) -> Value {
        let items: Vec<Value> = self
            .ui
            .tabs_geom
            .items
            .iter()
            .map(|t| {
                json!({
                    "session": t.session,
                    "label": t.label,
                    "global": t.global,
                    "selected": t.selected,
                    "state": t.state,
                    "unread": t.unread,
                    "rect": t.rect,
                    "close_rect": t.close_rect,
                })
            })
            .collect();
        json!({
            "shown": self.tabs_shown(),
            "scroll": self.ui.tab_scroll,
            "total": self.ui.numbered.len(),
            "items": items,
            "prev_rect": self.ui.tabs_geom.prev_rect,
            "next_rect": self.ui.tabs_geom.next_rect,
            "new_rect": self.ui.tabs_geom.new_rect,
            "new_global_rect": self.ui.tabs_geom.new_global_rect,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_tab_bar(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        list_w: usize,
        y0: f32,
        y1: f32,
    ) {
        self.ui.tabs_geom = TabsGeom::default();
        if !self.tabs_shown() || list_w >= width || y1 <= y0 {
            return;
        }
        let s = self.ui.scale.max(1.0);
        let total = self.ui.numbered.len();
        if total == 0 {
            self.ui.tab_scroll = 0;
        } else if self.ui.tab_scroll >= total {
            self.ui.tab_scroll = total - 1;
        }
        let entries: Vec<(String, String, bool, bool, String, bool, bool)> = self
            .ui
            .numbered
            .iter()
            .filter_map(|id| {
                let r = self.ui.rows.iter().find(|r| &r.session == id)?;
                Some((
                    r.session.clone(),
                    self.tab_caption(r),
                    r.is_global(),
                    r.session == self.session,
                    r.state.clone(),
                    self.row_unread(r),
                    !r.approvals.is_empty(),
                ))
            })
            .collect();
        let ensure = std::mem::take(&mut self.ui.tab_ensure_visible);
        let active_idx = entries.iter().position(|e| e.3);

        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let x_left = list_w as f32;
        let x_right = width as f32;
        controls::fill(buf, width, height, [x_left, y0, x_right, y1], 0xFF13_1A23);
        controls::fill(buf, width, height, [x_left, y1 - 1.0, x_right, y1], 0xFF2B_3646);

        let (by0, by1) = (y0 + 2.0 * s, y1 - 2.0 * s);
        let ink = Color::rgb(0xEC, 0xF1, 0xF8);
        let muted = Color::rgb(0x9E, 0xAB, 0xBC);
        let dim = Color::rgb(0x5E, 0x6B, 0x7C);

        // Right-anchored `+` and `+G` buttons so creating a workspace or Global session is always 1 click away.
        let g_w = controls.width(fs, "+G") + 12.0 * s;
        let n_w = controls.width(fs, "+") + 12.0 * s;
        let nav_w = controls.width(fs, "<") + 10.0 * s;
        let g_rect = [x_right - 4.0 * s - g_w, by0, x_right - 4.0 * s, by1];
        let n_rect = [g_rect[0] - 4.0 * s - n_w, by0, g_rect[0] - 4.0 * s, by1];
        let mut tabs_limit_x = n_rect[0] - 6.0 * s;

        let close_w = controls.width(fs, "\u{00d7}") + 8.0 * s;
        let widths: Vec<f32> = entries
            .iter()
            .map(|(_, cap, ..)| controls.width(fs, cap) + 14.0 * s + close_w)
            .collect();
        let all_fit = self.ui.tab_scroll == 0
            && widths.iter().map(|w| *w + 3.0 * s).sum::<f32>() <= (tabs_limit_x - (x_left + 6.0 * s)).max(0.0);
        let show_nav = !entries.is_empty() && !all_fit;
        let (mut prev_rect, mut next_rect) = (None, None);
        if show_nav {
            let nx_rect = [tabs_limit_x - nav_w, by0, tabs_limit_x, by1];
            let pv_rect = [nx_rect[0] - 3.0 * s - nav_w, by0, nx_rect[0] - 3.0 * s, by1];
            tabs_limit_x = pv_rect[0] - 6.0 * s;
            prev_rect = Some(pv_rect);
            next_rect = Some(nx_rect);
        }

        if ensure {
            if let Some(aidx) = active_idx {
                while self.ui.tab_scroll < aidx {
                    let span: f32 = widths[self.ui.tab_scroll..=aidx].iter().map(|w| *w + 3.0 * s).sum();
                    if x_left + 6.0 * s + span <= tabs_limit_x {
                        break;
                    }
                    self.ui.tab_scroll += 1;
                }
            }
        }

        if entries.is_empty() {
            controls.text(
                fs,
                swash,
                buf,
                width,
                height,
                x_left + 8.0 * s,
                by0 + s,
                tabs_limit_x as i32,
                "No open session tabs  \u{00b7}  click + or +G (Global)",
                dim,
            );
        } else {
            let mut tx = x_left + 6.0 * s;
            for idx in self.ui.tab_scroll..entries.len() {
                let (ref sid, ref cap, is_global, selected, ref state, unread, has_app) = entries[idx];
                let tw = widths[idx];
                if tx + tw > tabs_limit_x && idx > self.ui.tab_scroll {
                    break;
                }
                let rx1 = (tx + tw).min(tabs_limit_x);
                if rx1 <= tx + close_w + 8.0 * s {
                    break;
                }
                let trect = [tx, by0, rx1, y1 - 1.0];
                let (fill, text_col) = if selected {
                    (0xFF21_3147, ink)
                } else if has_app {
                    (0xFF28_2218, Color::rgb(0xF6, 0xBD, 0x58))
                } else if unread {
                    (0xFF1A_2838, Color::rgb(0x80, 0xCB, 0xFF))
                } else {
                    (0xFF18_212D, muted)
                };
                controls::fill(buf, width, height, trect, fill);
                if selected {
                    let accent = if is_global { 0xFF4E_D4B8 } else { 0xFF56_B4FF };
                    controls::fill(buf, width, height, [trect[0], trect[1], trect[2], trect[1] + 2.0 * s], accent);
                }
                let crect = [rx1 - close_w - 2.0 * s, by0 + 2.0 * s, rx1 - 2.0 * s, by1];
                controls.text(fs, swash, buf, width, height, tx + 7.0 * s, by0 + s, (crect[0] - 2.0 * s) as i32, cap, text_col);
                controls.target(trect, Action::Tab(sid.clone()));
                controls.text(fs, swash, buf, width, height, crect[0] + 4.0 * s, by0 + s, crect[2] as i32, "\u{00d7}", muted);
                controls.target(crect, Action::CloseTab(sid.clone()));
                self.ui.tabs_geom.items.push(TabItemGeom {
                    session: sid.clone(),
                    label: cap.clone(),
                    global: is_global,
                    selected,
                    state: state.clone(),
                    unread,
                    rect: trect,
                    close_rect: crect,
                });
                tx = rx1 + 3.0 * s;
            }
        }

        if let (Some(pr), Some(nr)) = (prev_rect, next_rect) {
            let has_prev = self.ui.tab_scroll > 0;
            let shown_end = self.ui.tab_scroll + self.ui.tabs_geom.items.len();
            let has_next = shown_end < entries.len();
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                pr,
                "<",
                if has_prev { 0xFF22_2E3E } else { 0xFF18_202B },
                if has_prev { ink } else { dim },
                Action::TabScroll(-1),
            );
            controls.button(
                fs,
                swash,
                buf,
                width,
                height,
                nr,
                ">",
                if has_next { 0xFF22_2E3E } else { 0xFF18_202B },
                if has_next { ink } else { dim },
                Action::TabScroll(1),
            );
            self.ui.tabs_geom.prev_rect = Some(pr);
            self.ui.tabs_geom.next_rect = Some(nr);
        }

        controls.button(fs, swash, buf, width, height, n_rect, "+", 0xFF23_3144, ink, Action::NewTab);
        controls.button(
            fs,
            swash,
            buf,
            width,
            height,
            g_rect,
            "+G",
            0xFF1D_3536,
            Color::rgb(0x64, 0xE2, 0xCB),
            Action::NewGlobal,
        );
        self.ui.tabs_geom.new_rect = Some(n_rect);
        self.ui.tabs_geom.new_global_rect = Some(g_rect);
    }
}
