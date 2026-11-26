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
    /// `M22-P03`: the call id of the room this session is in (the mark `⇄` on its tab), and the rect of the mark, whose click opens the room.
    pub room: Option<String>,
    pub mark_rect: Option<[f32; 4]>,
    pub rect: [f32; 4],
    pub close_rect: [f32; 4],
}

#[derive(Default)]
pub struct TabsGeom {
    pub items: Vec<TabItemGeom>,
    pub prev_rect: Option<[f32; 4]>,
    pub next_rect: Option<[f32; 4]>,
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
        let prefix = if row.provider == "local" {
            "L" // it has no folder, and is not a Home session
        } else if row.is_global() {
            "H"
        } else if row.provider == "claude" {
            "C"
        } else if row.provider == "agy" {
            "A"
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
                    "room": t.room,
                    "mark_rect": t.mark_rect,
                    "rect": t.rect,
                    "close_rect": t.close_rect,
                })
            })
            .collect();
        let call_items: Vec<Value> = self
            .ui
            .calls
            .tab_items_geom
            .iter()
            .map(|c| {
                json!({
                    "call_id": c.call_id,
                    "label": c.label,
                    "active": c.active,
                    "selected": c.selected,
                    "status": c.status,
                    "round": c.round,
                    "rect": c.rect,
                    "close_rect": c.close_rect,
                })
            })
            .collect();
        json!({
            "shown": self.tabs_shown(),
            "scroll": self.ui.tab_scroll,
            "total": self.ui.numbered.len(),
            "items": items,
            "call_items": call_items,
            "prev_rect": self.ui.tabs_geom.prev_rect,
            "next_rect": self.ui.tabs_geom.next_rect,
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
        self.ui.calls.tab_items_geom.clear();
        self.ui.terminal.tab_geom.clear();
        if !self.tabs_shown() || list_w >= width || y1 <= y0 {
            return;
        }
        let s = self.ui.scale.max(1.0);
        let call_open = self.active_call_open();
        let active_call_id = self.ui.calls.active_call.clone();
        let total = self.ui.numbered.len();
        if total == 0 {
            self.ui.tab_scroll = 0;
        } else if self.ui.tab_scroll >= total {
            self.ui.tab_scroll = total - 1;
        }
        let entries: Vec<(String, String, bool, bool, String, bool, bool, Option<String>)> = self
            .ui
            .numbered
            .iter()
            .filter_map(|id| {
                let r = self.ui.rows.iter().find(|r| &r.session == id)?;
                Some((
                    r.session.clone(),
                    self.tab_caption(r),
                    r.is_global(),
                    !call_open && r.session == self.session,
                    r.state.clone(),
                    self.row_unread(r),
                    !r.approvals.is_empty(),
                    self.ui.sidebar.calls.room_of(&r.session).map(|c| c.call_id.clone()),
                ))
            })
            .collect();
        let call_entries: Vec<(String, String, bool, bool, String, u32)> = self
            .ui
            .calls
            .open_tabs
            .iter()
            .map(|cid| {
                let (raw_title, active, paused, status, round) = self
                    .ui
                    .sidebar
                    .calls
                    .summaries
                    .iter()
                    .find(|c| &c.call_id == cid)
                    .map(|c| (c.title.clone(), c.active, c.paused, c.status.clone(), c.round))
                    .or_else(|| {
                        self.ui.calls.details.get(cid).map(|d| {
                            (d.title.clone(), d.active, d.paused, d.status.clone(), d.round)
                        })
                    })
                    .unwrap_or_else(|| (cid.clone(), false, false, "idle".into(), 0));
                let compact = raw_title.replace(" \u{00b7} ", "\u{00b7}");
                let badge = if paused {
                    " [paused]"
                } else if active {
                    " \u{25cf}"
                } else {
                    ""
                };
                let cap = format!("\u{21c4} {}{badge}", truncate_cols(&compact, 20));
                let sel = self.ui.panel.is_none() && active_call_id.as_deref() == Some(cid.as_str());
                (cid.clone(), cap, active, sel, status, round)
            })
            .collect();
        let browser_entries: Vec<(String, String, bool, bool)> = self.ui.browser.tabs.iter().map(|t| (t.id.clone(), super::super::browser_view::caption(t), self.browser_active() && self.ui.browser.active.as_deref() == Some(t.id.as_str()), t.local)).collect();
        // `M23-P04a`: a terminal tab in which an agent runs shows the agent's picture where the text `>_` was
        let strip = self.terminal_strip();
        let terminal_entries: Vec<(String, String, bool)> = strip.iter().map(|(id, cap, shown, _)| (id.clone(), cap.clone(), *shown)).collect();
        let terminal_pictures: Vec<Option<super::icons::Icon>> = strip.iter().map(|e| e.3).collect();
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

        // `M22-P04`: the row holds tabs only; a new session, a Home session and a browser tab are the buttons of the toolbar and the entries of the Session menu.
        let nav_w = controls.width(fs, "<") + 10.0 * s;
        let mut tabs_limit_x = x_right - 6.0 * s;

        let close_w = controls.width(fs, "\u{00d7}") + 8.0 * s;
        // the mark of a session that is in a room sits after its caption: `<caption> ⇄`
        let mark_glyph = "\u{21c4}";
        let widths: Vec<f32> = entries
            .iter()
            .map(|(_, cap, .., room)| controls.width(fs, &if room.is_some() { format!("{cap} {mark_glyph}") } else { cap.clone() }) + 14.0 * s + close_w)
            .collect();
        let call_widths: Vec<f32> = call_entries
            .iter()
            .map(|(_, cap, ..)| controls.width(fs, cap) + 14.0 * s + close_w)
            .collect();
        let browser_widths: Vec<f32> = browser_entries.iter().map(|(_, cap, ..)| controls.width(fs, cap) + 14.0 * s + close_w).collect();
        let picture_side = (by1 - by0 - 2.0 * s).max(8.0); // the picture of an agent in a terminal tab: a square the height of the tab, less a margin
        let terminal_widths: Vec<f32> =
            terminal_entries.iter().zip(&terminal_pictures).map(|((_, cap, _), picture)| controls.width(fs, cap) + 14.0 * s + close_w + if picture.is_some() { picture_side + 4.0 * s } else { 0.0 }).collect();
        let total_w: f32 = widths.iter().chain(&call_widths).chain(&browser_widths).chain(&terminal_widths).map(|w| *w + 3.0 * s).sum();
        let all_fit = self.ui.tab_scroll == 0 && total_w <= (tabs_limit_x - (x_left + 6.0 * s)).max(0.0);
        let show_nav = (!entries.is_empty() || !call_entries.is_empty() || !browser_entries.is_empty() || !terminal_entries.is_empty()) && !all_fit;
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

        if entries.is_empty() && call_entries.is_empty() && browser_entries.is_empty() && terminal_entries.is_empty() {
            controls.text(
                fs,
                swash,
                buf,
                width,
                height,
                x_left + 8.0 * s,
                by0 + s,
                tabs_limit_x as i32,
                "No open tabs",
                dim,
            );
        } else {
            let mut tx = x_left + 6.0 * s;
            let call_reserve: f32 = if call_entries.iter().any(|c| c.3) {
                call_widths.iter().map(|w| *w + 3.0 * s).sum::<f32>().min((tabs_limit_x - tx) * 0.48)
            } else {
                0.0
            };
            let sess_limit_x = (tabs_limit_x - call_reserve).max(tx);
            for idx in self.ui.tab_scroll..entries.len() {
                let (ref sid, ref cap, is_global, selected, ref state, unread, has_app, ref room) = entries[idx];
                let tw = widths[idx];
                if tx + tw > sess_limit_x && (idx > self.ui.tab_scroll || call_reserve > 0.0) {
                    break;
                }
                let rx1 = (tx + tw).min(sess_limit_x);
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
                let label = if room.is_some() { format!("{cap} {mark_glyph}") } else { cap.clone() };
                controls.text(fs, swash, buf, width, height, tx + 7.0 * s, by0 + s, (crect[0] - 2.0 * s) as i32, &label, text_col);
                controls.target(trect, Action::Tab(sid.clone()));
                // the mark: the last glyph of the label; it is registered after the tab, so its click wins and opens the room
                let mark_rect = room.as_ref().and_then(|cid| {
                    let x1 = (tx + 7.0 * s + controls.width(fs, &label)).min(crect[0] - 2.0 * s);
                    let x0 = x1 - controls.width(fs, mark_glyph) - 6.0 * s;
                    (x1 > x0 && x0 > trect[0]).then(|| {
                        let r = [x0, by0, x1, y1 - 1.0];
                        controls.target(r, Action::CallTab(cid.clone()));
                        r
                    })
                });
                controls.text(fs, swash, buf, width, height, crect[0] + 4.0 * s, by0 + s, crect[2] as i32, "\u{00d7}", muted);
                controls.target(crect, Action::CloseTab(sid.clone()));
                self.ui.tabs_geom.items.push(TabItemGeom {
                    session: sid.clone(),
                    label,
                    room: room.clone(),
                    mark_rect,
                    global: is_global,
                    selected,
                    state: state.clone(),
                    unread,
                    rect: trect,
                    close_rect: crect,
                });
                tx = rx1 + 3.0 * s;
            }
            for (cidx, (cid, cap, active, selected, status, round)) in call_entries.iter().enumerate() {
                let cw = call_widths[cidx];
                if tx + close_w + 16.0 * s > tabs_limit_x {
                    break;
                }
                let rx1 = (tx + cw).min(tabs_limit_x);
                let trect = [tx, by0, rx1, y1 - 1.0];
                let teal_col = Color::rgb(0x64, 0xE2, 0xCB);
                let (fill, text_col) = if *selected {
                    (0xFF19_3338, ink)
                } else if *active {
                    (0xFF16_292E, teal_col)
                } else {
                    (0xFF16_232A, teal_col)
                };
                controls::fill(buf, width, height, trect, fill);
                if *selected {
                    controls::fill(buf, width, height, [trect[0], trect[1], trect[2], trect[1] + 2.0 * s], 0xFF38_B2AC);
                }
                let crect = [rx1 - close_w - 2.0 * s, by0 + 2.0 * s, rx1 - 2.0 * s, by1];
                controls.text(fs, swash, buf, width, height, tx + 7.0 * s, by0 + s, (crect[0] - 2.0 * s) as i32, cap, text_col);
                controls.target(trect, Action::CallTab(cid.clone()));
                controls.text(fs, swash, buf, width, height, crect[0] + 4.0 * s, by0 + s, crect[2] as i32, "\u{00d7}", muted);
                controls.target(crect, Action::CloseCallTab(cid.clone()));
                self.ui.calls.tab_items_geom.push(super::super::call_view::CallTabGeom {
                    call_id: cid.clone(),
                    label: cap.clone(),
                    active: *active,
                    selected: *selected,
                    status: status.clone(),
                    round: *round,
                    rect: trect,
                    close_rect: crect,
                });
                tx = rx1 + 3.0 * s;
            }
            for (bid, cap, selected, local) in &browser_entries {
                let bw = controls.width(fs, cap) + 14.0 * s + close_w;
                if tx + close_w + 16.0 * s > tabs_limit_x {
                    break;
                }
                let rx1 = (tx + bw).min(tabs_limit_x);
                let trect = [tx, by0, rx1, y1 - 1.0];
                let blue = Color::rgb(0x8F, 0xB4, 0xFF);
                controls::fill(buf, width, height, trect, if *selected { 0xFF1E_2A4A } else { 0xFF17_1F36 });
                if *selected {
                    controls::fill(buf, width, height, [trect[0], trect[1], trect[2], trect[1] + 2.0 * s], if *local { 0xFFE0_9A4A } else { 0xFF6A_8CFF });
                }
                let crect = [rx1 - close_w - 2.0 * s, by0 + 2.0 * s, rx1 - 2.0 * s, by1];
                controls.text(fs, swash, buf, width, height, tx + 7.0 * s, by0 + s, (crect[0] - 2.0 * s) as i32, cap, if *selected { ink } else { blue });
                controls.target(trect, Action::BrowserTab(bid.clone()));
                controls.text(fs, swash, buf, width, height, crect[0] + 4.0 * s, by0 + s, crect[2] as i32, "\u{00d7}", muted);
                controls.target(crect, Action::CloseBrowserTab(bid.clone()));
                tx = rx1 + 3.0 * s;
            }
            for (((tid, cap, selected), tw), picture) in terminal_entries.iter().zip(&terminal_widths).zip(&terminal_pictures) {
                if tx + close_w + 16.0 * s > tabs_limit_x {
                    break;
                }
                let rx1 = (tx + *tw).min(tabs_limit_x);
                let trect = [tx, by0, rx1, y1 - 1.0];
                let green = Color::rgb(0x9B, 0xD9, 0x8E);
                controls::fill(buf, width, height, trect, if *selected { 0xFF1D_3324 } else { 0xFF16_2219 });
                if *selected {
                    controls::fill(buf, width, height, [trect[0], trect[1], trect[2], trect[1] + 2.0 * s], 0xFF5C_C46B);
                }
                let crect = [rx1 - close_w - 2.0 * s, by0 + 2.0 * s, rx1 - 2.0 * s, by1];
                let (fill_colour, ink_colour) = (if *selected { 0xFF1D_3324 } else { 0xFF16_2219 }, if *selected { 0xFFEC_F1F8 } else { 0xFF9B_D98E });
                let prect = picture.map(|icon| {
                    let r = [tx + 5.0 * s, by0 + s, tx + 5.0 * s + picture_side, by0 + s + picture_side];
                    super::icons::draw(buf, width, height, icon, r, ink_colour, fill_colour);
                    r
                });
                let text_x = tx + 7.0 * s + prect.map_or(0.0, |r| r[2] - r[0] + 4.0 * s);
                controls.text(fs, swash, buf, width, height, text_x, by0 + s, (crect[0] - 2.0 * s) as i32, cap, if *selected { ink } else { green });
                controls.target(trect, Action::TerminalTab(tid.clone()));
                controls.text(fs, swash, buf, width, height, crect[0] + 4.0 * s, by0 + s, crect[2] as i32, "\u{00d7}", muted);
                controls.target(crect, Action::CloseTerminalTab(tid.clone()));
                self.ui.terminal.tab_geom.push((tid.clone(), trect, crect, prect));
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
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::scroll_oracle::app;
    use super::super::Row;

    #[test]
    fn a_tab_names_its_kind_by_one_letter_and_a_local_session_is_l_not_x() {
        let a = app(1.0, false);
        let row = |provider: &str| Row { session: "s".into(), provider: provider.into(), name: "n".into(), cwd: "/w".into(), state: "running".into(), ..Default::default() };
        let captions: Vec<String> = ["claude", "codex", "local"].iter().map(|k| a.tab_caption(&row(k))).collect();
        assert!(captions[0].starts_with("C\u{b7}") && captions[1].starts_with("X\u{b7}") && captions[2].starts_with("L\u{b7}"), "{captions:?}");
        let no_folder = Row { cwd: String::new(), ..row("local") }; // a local session has no folder, and is still not a Global general session
        assert!(a.tab_caption(&no_folder).starts_with("L\u{b7}"), "{}", a.tab_caption(&no_folder));
    }
}
