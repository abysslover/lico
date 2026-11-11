//! Top menu bar and clickable primary-action toolbar (`M04-P01`).
//!
//! Draws both the top menu bar (`File`, `Session`, `View`, `Help`) and the action toolbar beneath it,
//! plus any open dropdown menu on top, keeping `ui.rs` compact and exposing exact hit-boxes in `ui_dump()["toolbar"]`.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::super::controls::{self, Action};
use super::super::menu::{Cmd, Entry, TITLES};
use super::super::panel::confirm::Act;
use super::super::App;

pub struct ToolBtn {
    pub label: String,
    pub id: &'static str,
    pub cmd: Cmd,
    pub active: bool,
    pub reason: Option<&'static str>,
}

#[derive(Default)]
pub struct ToolbarGeom {
    pub rects: Vec<[f32; 4]>,
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

impl App {
    pub(crate) fn toolbar_entries(&self) -> Vec<ToolBtn> {
        let none_open = if self.session.is_empty() { Some("no session is open") } else { None };
        let sel = self.selected_row();
        let (state_label, state_id, state_cmd, state_reason) = if sel.is_some_and(|r| r.state == "disconnected") {
            ("Reconnect".to_string(), "resume", Cmd::Resume, self.resume_reason())
        } else {
            ("Close".to_string(), "close", Cmd::Close, self.ask_reason(Act::Close))
        };
        let mode_label = match sel.map(|r| r.mode.as_str()) {
            Some("default") => "Mode: ask",
            _ => "Mode: auto",
        };
        let group_label = if self.ui.group_by_workspace { "Group: dir" } else { "Group: acct" };
        let in_files = self.ui.panel.as_ref().is_some_and(|p| p.step == super::super::panel::Step::Files);
        if in_files {
            return vec![
                ToolBtn { label: "+ New".into(), id: "new_session", cmd: Cmd::NewSession, active: false, reason: None },
                ToolBtn { label: "Accounts".into(), id: "accounts", cmd: Cmd::Accounts, active: false, reason: None },
                ToolBtn { label: "Files".into(), id: "files", cmd: Cmd::Files, active: true, reason: None },
                ToolBtn { label: "+ New here".into(), id: "new_here", cmd: Cmd::NewInViewedDir, active: false, reason: None },
                ToolBtn { label: "Refresh".into(), id: "refresh_files", cmd: Cmd::RefreshFiles, active: false, reason: None },
                ToolBtn { label: group_label.into(), id: "toggle_group", cmd: Cmd::ToggleGroupBy, active: self.ui.group_by_workspace, reason: None },
            ];
        }
        vec![
            ToolBtn { label: "+ New".into(), id: "new_session", cmd: Cmd::NewSession, active: false, reason: None },
            ToolBtn { label: "Accounts".into(), id: "accounts", cmd: Cmd::Accounts, active: false, reason: None },
            ToolBtn { label: "Files".into(), id: "files", cmd: Cmd::Files, active: false, reason: None },
            ToolBtn { label: state_label, id: state_id, cmd: state_cmd, active: false, reason: state_reason },
            ToolBtn { label: "Rename".into(), id: "rename", cmd: Cmd::Rename, active: false, reason: self.ask_reason(Act::Rename) },
            ToolBtn { label: mode_label.into(), id: "toggle_mode", cmd: Cmd::ToggleMode, active: sel.is_some_and(|r| r.mode == "default"), reason: self.mode_reason() },
            ToolBtn { label: group_label.into(), id: "toggle_group", cmd: Cmd::ToggleGroupBy, active: self.ui.group_by_workspace, reason: None },
            ToolBtn { label: "Find".into(), id: "find", cmd: Cmd::FindConversation, active: self.ui.search.open && !self.ui.search.scope_all, reason: none_open },
        ]
    }

    pub(crate) fn toolbar_click(&mut self, index: usize) {
        let entries = self.toolbar_entries();
        let Some(btn) = entries.get(index) else { return };
        let (cmd, label, reason) = (btn.cmd, btn.label.clone(), btn.reason);
        self.menu_close();
        if let Some(why) = reason {
            self.ui.flash = Some(format!("{label}: {why}"));
            self.dirty = true;
            return;
        }
        if self.ui.panel.is_some() && !matches!(cmd, Cmd::NewSession | Cmd::Accounts | Cmd::Files | Cmd::RefreshFiles | Cmd::NewInViewedDir) {
            self.panel_close();
        }
        self.run_cmd(cmd);
        self.dirty = true;
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
                    "enabled": b.reason.is_none(),
                    "active": b.active,
                    "reason": b.reason,
                    "rect": self.ui.toolbar_geom.rects.get(i),
                })
            })
            .collect();
        json!({"shown": self.ui.menu, "buttons": buttons})
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_top_bars(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        menu_h: f32,
        bar_h: f32,
        menu_open: Option<usize>,
        menu_sel: usize,
        menu_entries: &[Entry],
        tool_entries: &[ToolBtn],
    ) {
        self.ui.menu_geom.titles.clear();
        self.ui.menu_geom.items.clear();
        self.ui.toolbar_geom.rects.clear();
        if !self.ui.menu {
            return;
        }
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let Some(fs) = self.font_system.as_mut() else { return };
        let ink = Color::rgb(0xE0, 0xE0, 0xE0);
        let dim = super::super::panel::sendto::DIM;
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let x0 = if self.ui.markers { 36.0 } else { 0.0 };

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

        // 2. Action toolbar (y = menu_h .. bar_h)
        controls::fill(buf, width, height, [0.0, menu_h, width as f32, bar_h], 0xFF16_1D27);
        controls::fill(buf, width, height, [0.0, bar_h - 1.0, width as f32, bar_h], 0xFF2B_3440);
        let mut tx = 6.0 * s;
        let (by0, by1) = (menu_h + 2.0 * s, bar_h - 3.0 * s);
        for (i, btn) in tool_entries.iter().enumerate() {
            let bw = controls.width(fs, &btn.label) + 14.0 * s;
            if tx + bw > width as f32 - 4.0 * s {
                break;
            }
            let r = [tx, by0, tx + bw, by1];
            let (fill, col) = if btn.reason.is_some() {
                (0xFF1B_222D, dim)
            } else if btn.active {
                (0xFF2C_4A78, ink)
            } else {
                (0xFF23_2E3E, ink)
            };
            controls.button(fs, swash, buf, width, height, r, &btn.label, fill, col, Action::Toolbar(i));
            self.ui.toolbar_geom.rects.push(r);
            tx += bw + 5.0 * s;
        }

        // 3. Open dropdown menu (on top of toolbar and main area)
        if let Some(m) = menu_open {
            let (ih, pad) = (lh + 2.0 * s, 8.0 * s);
            let mut inner = 0.0f32;
            let mut lefts = Vec::new();
            for e in menu_entries {
                let left = match e.reason {
                    Some(reason) => format!("{}   ({reason})", e.label),
                    None => e.label.clone(),
                };
                let key_w = if e.key.is_empty() { 0.0 } else { controls.width(fs, e.key) };
                inner = inner.max(controls.width(fs, &left) + 24.0 * s + key_w);
                lefts.push(left);
            }
            let w = inner + 2.0 * pad;
            let x = self.ui.menu_geom.titles[m][0].min(width as f32 - 4.0 * s - w).max(4.0 * s);
            let panel = [x, menu_h, x + w, menu_h + ih * menu_entries.len() as f32 + 4.0 * s];
            controls::fill(buf, width, height, [panel[0] - 1.0, panel[1], panel[2] + 1.0, panel[3] + 1.0], 0xFF3A_4A60);
            controls::fill(buf, width, height, panel, 0xFF1C_2430);
            for (i, e) in menu_entries.iter().enumerate() {
                let r = [x, menu_h + 2.0 * s + ih * i as f32, x + w, menu_h + 2.0 * s + ih * (i + 1) as f32];
                if i == menu_sel && e.reason.is_none() {
                    controls::fill(buf, width, height, r, 0xFF2C_4A78);
                }
                let colour = if e.reason.is_none() { ink } else { dim };
                controls.text(fs, swash, buf, width, height, x + pad, r[1] + s, r[2] as i32, &lefts[i], colour);
                if !e.key.is_empty() {
                    let key_w = controls.width(fs, e.key);
                    controls.text(fs, swash, buf, width, height, r[2] - pad - key_w, r[1] + s, r[2] as i32, e.key, colour);
                }
                controls.target(r, Action::Item(m, i));
                self.ui.menu_geom.items.push(r);
            }
        }
    }
}
