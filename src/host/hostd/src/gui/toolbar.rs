//! Top menu bar and clickable primary-action toolbar (`M04-P01`).
//!
//! Draws both the top menu bar (`File`, `Session`, `View`, `Help`) and the action toolbar beneath it,
//! plus any open dropdown menu on top, keeping `ui.rs` compact and exposing exact hit-boxes in `ui_dump()["toolbar"]`.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::super::controls::{self, Action};
use super::icons::{self, Icon};
use super::super::menu::{Cmd, MenuRow, RowKind, TITLES};
use super::super::App;

#[path = "picker.rs"]
mod picker;
#[path = "wizard_cards.rs"]
mod wizard_cards;

pub struct ToolBtn {
    pub label: String,
    pub id: &'static str,
    pub cmd: Cmd,
    pub active: bool,
    pub reason: Option<&'static str>,
    /// An icon button (`M22-P04`): square, drawn by the window; `label` is its word (the state, the hover line). `None`: a text button.
    pub icon: Option<Icon>,
}

#[derive(Default)]
pub struct ToolbarGeom {
    pub rects: Vec<[f32; 4]>,
    pub host_items: Vec<(usize, [f32; 4])>,
    /// The button the pointer is over (its word is shown beside the buttons).
    pub hover: Option<usize>,
    /// The icons of the wizard's first step as the window drew them last, and the one the pointer is over (`M22-P04b`, see `picker.rs`).
    pub tiles: Vec<[f32; 4]>,
    pub tile_hover: Option<usize>,
    /// The boxes of the wizard's second and third step as the window drew them last: for each row's line, the rectangle of each of its boxes (`M22-P07`, see `wizard_cards.rs`).
    pub cards: Vec<(usize, Vec<[f32; 4]>)>,
}

/// Returns `(menu_h, total_top_h)` in physical pixels (`(0.0, 0.0)` when `--menu` is off).
pub fn bar_heights(menu_shown: bool, scale: f32, line_h: f32) -> (f32, f32) {
    if !menu_shown {
        return (0.0, 0.0);
    }
    let s = scale.max(1.0);
    let menu_h = line_h * s + 4.0 * s;
    let tool_h = line_h * s + 6.0 * s;
    (menu_h, menu_h + tool_h)
}

/// How the toolbar's row is spaced, as `(padding of a text button, gap between buttons)` in units of the scale. `widths` are the widths of the buttons before padding (an icon is its side, a text
/// button its text), `texts` how many of them are text buttons, `room` the width from the first button to the right edge. At the usual spacing when the row fits; when it does not the padding and
/// the gap are tightened first, so that a large text size does not hide the buttons at the end of the row (the page buttons of the Files panel) before it has to (`M22-P04b`: the account icon).
pub(super) fn toolbar_spacing(widths: &[f32], texts: usize, room: f32, s: f32) -> (f32, f32) {
    let need = |pad: f32, gap: f32| widths.iter().sum::<f32>() + texts as f32 * pad * s + widths.len().saturating_sub(1) as f32 * gap * s;
    if need(14.0, 5.0) <= room { (14.0, 5.0) } else { (6.0, 3.0) }
}

impl App {
    /// The toolbar (`M22-P04`, `M22-P04b`): four icon buttons, Accounts (a person), New session (a plus), New Home session (a house) and Open browser (a browser window). Everything else is in the
    /// menus. The account icon is lit while the host says there is no account at all (it is the first step); the plus and the house are never greyed, because Local LLM needs no account
    /// (the wizard greys the Claude and Codex icons itself). With the Files panel open the panel's own buttons follow the icons: they act on what the panel shows.
    pub(crate) fn toolbar_entries(&self) -> Vec<ToolBtn> {
        let icon_btn = |label: &str, id: &'static str, cmd: Cmd, icon: Icon| ToolBtn { label: label.into(), id, cmd, active: false, reason: None, icon: Some(icon) };
        let none_yet = self.ui.account_kinds.as_ref().is_some_and(Vec::is_empty);
        let mut accounts = icon_btn(if none_yet { "Accounts: none yet, add one to use Claude or Codex" } else { "Accounts" }, "accounts", Cmd::Accounts, Icon::Account);
        accounts.active = none_yet;
        let mut out = vec![
            accounts,
            icon_btn("New session", "new_session", Cmd::NewSession, Icon::Plus),
            icon_btn("New Home session", "new_home", Cmd::NewGlobalSession, Icon::House),
            icon_btn("Open browser", "new_browser", Cmd::NewBrowser, Icon::Browser),
        ];
        let in_files = self.ui.panel.as_ref().is_some_and(|p| p.step == super::super::panel::Step::Files);
        if in_files {
            let (has_prev, has_next) = self.files_page_nav_state();
            let hidden_label = if self.ui.show_hidden { "Hidden: on" } else { "Hidden: off" };
            let (host_label, host_active, host_reason) = self.files_host_toolbar_state();
            let new_here_reason = self.files_new_here_reason();
            let text = |label: String, id: &'static str, cmd: Cmd, active: bool, reason: Option<&'static str>| ToolBtn { label, id, cmd, active, reason, icon: None };
            out.extend([
                text(host_label, "files_host", Cmd::CycleFilesHost, host_active, host_reason),
                text("+ New here".into(), "new_here", Cmd::NewInViewedDir, false, new_here_reason),
                text("Refresh".into(), "refresh_files", Cmd::RefreshFiles, false, None),
                text(hidden_label.into(), "toggle_hidden", Cmd::ToggleHiddenFiles, self.ui.show_hidden, None),
                text("< Prev".into(), "prev_page", Cmd::FilesPrevPage, false, if has_prev { None } else { Some("already on first page") }),
                text("Next >".into(), "next_page", Cmd::FilesNextPage, false, if has_next { None } else { Some("already on last page") }),
            ]);
        }
        if self.can_copy() {
            out.push(ToolBtn { label: "Copy".into(), id: "copy", cmd: Cmd::Copy, active: false, reason: None, icon: None });
        }
        out
    }

    pub(crate) fn toolbar_click(&mut self, index: usize) {
        let entries = self.toolbar_entries();
        let Some(btn) = entries.get(index) else { return };
        let (cmd, label, reason) = (btn.cmd, btn.label.clone(), btn.reason);
        self.menu_close();
        self.close_files_host_popup();
        if let Some(why) = reason {
            self.ui.flash = Some(format!("{label}: {why}"));
            self.dirty = true;
            return;
        }
        if self.ui.panel.is_some()
            && !matches!(
                cmd,
                Cmd::NewSession
                    | Cmd::NewGlobalSession
                    | Cmd::Accounts
                    | Cmd::Files
                    | Cmd::RefreshFiles
                    | Cmd::NewInViewedDir
                    | Cmd::FilesPrevPage
                    | Cmd::FilesNextPage
                    | Cmd::ToggleHiddenFiles
                    | Cmd::CycleFilesHost
                    | Cmd::Copy
            )
        {
            self.panel_close();
        }
        self.run_cmd(cmd);
        self.dirty = true;
    }

    /// Moves the hover to the button under the pointer; true when it changed (the window then redraws).
    pub(crate) fn toolbar_hover_moved(&mut self) -> bool {
        let (x, y) = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
        let over = self.ui.toolbar_geom.rects.iter().position(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]);
        let tile = self.ui.toolbar_geom.tiles.iter().position(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]);
        let changed = over != self.ui.toolbar_geom.hover || tile != self.ui.toolbar_geom.tile_hover;
        (self.ui.toolbar_geom.hover, self.ui.toolbar_geom.tile_hover) = (over, tile);
        changed
    }

    pub(crate) fn host_popup_click(&mut self, hit: Option<Action>) -> bool {
        if !self.files_host_popup_open() {
            return false;
        }
        match hit {
            Some(Action::HostItem(idx)) => self.select_files_host_index(idx),
            Some(Action::Menu(m)) => self.menu_open_at(m),
            _ => self.close_files_host_popup(),
        }
        true
    }

    pub(crate) fn toolbar_json(&self) -> Value {
        let buttons: Vec<Value> = self
            .toolbar_entries()
            .into_iter()
            .enumerate()
            .map(|(i, b)| {
                json!({
                    "id": b.id,
                    "label": b.label,
                    "icon": b.icon.map(Icon::name),
                    "enabled": b.reason.is_none(),
                    "active": b.active,
                    "reason": b.reason,
                    "rect": self.ui.toolbar_geom.rects.get(i),
                })
            })
            .collect();
        let hover = self.ui.toolbar_geom.hover.and_then(|i| self.toolbar_entries().into_iter().nth(i)).map(|b| b.label);
        json!({"shown": self.ui.menu, "buttons": buttons, "hover": hover, "host_popup": self.files_host_popup_json()})
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_top_bars(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        left: f32,
        menu_h: f32,
        bar_h: f32,
        menu_open: Option<usize>,
        menu_sel: usize,
        menu_entries: &[MenuRow],
        tool_entries: &[ToolBtn],
    ) {
        self.ui.menu_geom.titles.clear();
        self.ui.menu_geom.items.clear();
        self.ui.toolbar_geom.rects.clear();
        self.ui.toolbar_geom.host_items.clear();
        if !self.ui.menu {
            return;
        }
        let host_popup = if menu_open.is_none() { self.files_host_popup_draw_state() } else { None };
        let (maximized, over_icon) = (self.window_is_maximized(), self.ui.win_btn.hover);
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let Some(fs) = self.font_system.as_mut() else { return };
        let ink = Color::rgb(0xE0, 0xE0, 0xE0);
        let dim = super::super::panel::sendto::DIM;
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let x0 = left; // the left column (the sidebar) is not the menu's: everything here is to its right

        // 1. Menu bar (y = 0 .. menu_h)
        controls::fill(buf, width, height, [x0, 0.0, width as f32, menu_h], 0xFF1C_2430);
        let mut x = x0 + 4.0 * s;
        for (i, title) in TITLES.iter().enumerate() {
            let w = controls.width(fs, title) + 16.0 * s;
            let r = [x, 0.0, x + w, menu_h];
            if menu_open == Some(i) {
                controls::fill(buf, width, height, r, 0xFF2C_4A78);
            }
            controls.text(fs, swash, buf, width, height, x + 8.0 * s, 2.0 * s, r[2] as i32, title, ink);
            controls.target(r, Action::Menu(i));
            self.ui.menu_geom.titles.push(r);
            x += w;
        }

        let icon = crate::gui::windowstate::button_rect(width, menu_h, s);
        crate::gui::windowstate::draw_icon(controls, buf, width, height, icon, maximized, over_icon, s);
        self.ui.win_btn.rect = Some(icon);

        // 2. Action toolbar (y = menu_h .. bar_h)
        controls::fill(buf, width, height, [x0, menu_h, width as f32, bar_h], 0xFF16_1D27);
        controls::fill(buf, width, height, [x0, bar_h - 1.0, width as f32, bar_h], 0xFF2B_3440);
        let mut tx = x0 + 6.0 * s;
        let (by0, by1) = (menu_h + 2.0 * s, bar_h - 3.0 * s);
        let side = (by1 - by0).max(1.0);
        let hover = self.ui.toolbar_geom.hover;
        let mut tip: Option<String> = None;
        let widths: Vec<f32> = tool_entries.iter().map(|b| if b.icon.is_some() { side } else { controls.width(fs, &b.label) }).collect();
        let (pad, gap) = toolbar_spacing(&widths, tool_entries.iter().filter(|b| b.icon.is_none()).count(), width as f32 - 4.0 * s - tx, s);
        for (i, btn) in tool_entries.iter().enumerate() {
            let bw = if btn.icon.is_some() { side } else { widths[i] + pad * s };
            if tx + bw > width as f32 - 4.0 * s {
                break;
            }
            let r = [tx, by0, tx + bw, by1];
            let (fill, col) = if btn.reason.is_some() {
                (0xFF1B_222D, dim)
            } else if btn.active || (btn.icon.is_some() && hover == Some(i)) {
                (0xFF2C_4A78, ink)
            } else {
                (0xFF23_2E3E, ink)
            };
            match btn.icon {
                Some(icon) => {
                    controls::fill(buf, width, height, r, fill);
                    icons::draw(buf, width, height, icon, r, if btn.reason.is_some() { 0xFF70_7880 } else { 0xFFE0_E0E0 }, fill);
                    controls.target(r, Action::Toolbar(i));
                    if hover == Some(i) {
                        tip = Some(btn.label.clone());
                    }
                }
                None => controls.button(fs, swash, buf, width, height, r, &btn.label, fill, col, Action::Toolbar(i)),
            }
            self.ui.toolbar_geom.rects.push(r);
            tx += bw + gap * s;
        }
        // the word of the icon the pointer is over, after the last button
        if let Some(label) = tip {
            let lx = tx + 6.0 * s;
            if lx + controls.width(fs, &label) < width as f32 - 4.0 * s {
                controls.text(fs, swash, buf, width, height, lx, by0 + (side - lh) * 0.5, width as i32, &label, dim);
            }
        }

        // 3. Open dropdown menu OR Files Host combo-box popup (on top of toolbar and main area)
        if let Some(m) = menu_open {
            let (ih, pad) = (lh + 2.0 * s, 8.0 * s);
            let mut inner = 0.0f32;
            let mut lefts = Vec::new();
            for e in menu_entries {
                let mark = match e.kind {
                    RowKind::Group(_, true) => "v ",
                    RowKind::Group(_, false) => "> ",
                    RowKind::Entry(_) => "",
                };
                let left = match e.reason {
                    Some(reason) => format!("{mark}{}   ({reason})", e.label),
                    None => format!("{mark}{}", e.label),
                };
                let key_w = if e.key.is_empty() { 0.0 } else { controls.width(fs, e.key) };
                inner = inner.max(controls.width(fs, &left) + 24.0 * s + key_w + 14.0 * s * f32::from(e.depth));
                lefts.push(left);
            }
            let w = inner + 2.0 * pad;
            let x = self.ui.menu_geom.titles[m][0].min(width as f32 - 4.0 * s - w).max(x0 + 4.0 * s);
            let panel = [x, menu_h, x + w, menu_h + ih * menu_entries.len() as f32 + 4.0 * s];
            controls::fill(buf, width, height, [panel[0] - 1.0, panel[1], panel[2] + 1.0, panel[3] + 1.0], 0xFF3A_4A60);
            controls::fill(buf, width, height, panel, 0xFF1C_2430);
            for (i, e) in menu_entries.iter().enumerate() {
                let r = [x, menu_h + 2.0 * s + ih * i as f32, x + w, menu_h + 2.0 * s + ih * (i + 1) as f32];
                if i == menu_sel && e.reason.is_none() {
                    controls::fill(buf, width, height, r, 0xFF2C_4A78);
                }
                let colour = if e.reason.is_none() { ink } else { dim };
                controls.text(fs, swash, buf, width, height, x + pad + 14.0 * s * f32::from(e.depth), r[1] + s, r[2] as i32, &lefts[i], colour);
                if !e.key.is_empty() {
                    let key_w = controls.width(fs, e.key);
                    controls.text(fs, swash, buf, width, height, r[2] - pad - key_w, r[1] + s, r[2] as i32, e.key, colour);
                }
                controls.target(r, Action::Item(m, i));
                self.ui.menu_geom.items.push(r);
            }
        } else if let Some((choices, cur_host, sel, top)) = host_popup {
            let (ih, pad) = (lh + 2.0 * s, 8.0 * s);
            let host_btn_idx = tool_entries.iter().position(|b| b.id == "files_host").unwrap_or(3);
            let (anchor_x, btn_w) = self
                .ui
                .toolbar_geom
                .rects
                .get(host_btn_idx)
                .map_or((80.0 * s, 120.0 * s), |r| (r[0], r[2] - r[0]));
            let max_vis = super::super::panel::files_ui::HOST_POPUP_MAX_VIS;
            let start = top.min(choices.len().saturating_sub(1));
            let end = (start + max_vis).min(choices.len());
            let mut inner = (btn_w - 2.0 * pad).max(0.0);
            let mut rows_txt: Vec<(usize, String)> = Vec::with_capacity(end.saturating_sub(start));
            for (idx, ch) in choices.iter().enumerate().take(end).skip(start) {
                let active = *ch == cur_host;
                let mark = if active { "* " } else { "  " };
                let label = match ch {
                    None => format!("{mark}local (this machine)"),
                    Some(h) => format!("{mark}ssh: {h}"),
                };
                inner = inner.max(controls.width(fs, &label) + 12.0 * s);
                rows_txt.push((idx, label));
            }
            let footer = (choices.len() > max_vis).then(|| {
                let ft = format!("  Up/Down or wheel  ({}/{})", sel + 1, choices.len());
                inner = inner.max(controls.width(fs, &ft) + 12.0 * s);
                ft
            });
            let w = inner + 2.0 * pad;
            let x = anchor_x.min(width as f32 - 4.0 * s - w).max(x0 + 4.0 * s);
            let total_rows = rows_txt.len() + usize::from(footer.is_some());
            let box_r = [x, bar_h, x + w, bar_h + ih * total_rows as f32 + 4.0 * s];
            controls::fill(buf, width, height, [box_r[0] - 1.0, box_r[1], box_r[2] + 1.0, box_r[3] + 1.0], 0xFF3A_4A60);
            controls::fill(buf, width, height, box_r, 0xFF1C_2430);
            for (vis_i, (idx, label)) in rows_txt.iter().enumerate() {
                let r = [x, bar_h + 2.0 * s + ih * vis_i as f32, x + w, bar_h + 2.0 * s + ih * (vis_i + 1) as f32];
                if *idx == sel {
                    controls::fill(buf, width, height, r, 0xFF2C_4A78);
                }
                controls.text(fs, swash, buf, width, height, x + pad, r[1] + s, r[2] as i32, label, ink);
                controls.target(r, Action::HostItem(*idx));
                self.ui.toolbar_geom.host_items.push((*idx, r));
            }
            if let Some(ft) = footer {
                let fy = bar_h + 2.0 * s + ih * rows_txt.len() as f32;
                controls.text(fs, swash, buf, width, height, x + pad, fy + s, (x + w) as i32, &ft, dim);
            }
        }
    }
}
