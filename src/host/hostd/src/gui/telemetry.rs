//! Color-coded bottom session telemetry strip and multi-account status summary (`M07-P01`).
//!
//! Renders a single compact telemetry row directly beneath the session prompt (`> `):
//!   `Model (Effort)  |  Tok: in:... out:...  |  5h:XX% (reset) · Wk:YY% (reset)  |  Working Directory / Global`
//! without allocating any extra rendering surface or polling providers.

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};

use super::super::controls::{self, Action};
use super::super::editor;
use super::super::menu::TITLES;
use super::super::App;
use super::Row;

#[derive(Default)]
pub struct TelemetryGeom {
    pub shown: bool,
    pub rect: Option<[f32; 4]>,
    pub cwd_rect: Option<[f32; 4]>,
    pub model_text: String,
    pub tokens_text: String,
    pub quota_text: String,
    pub quota_level: &'static str,
    pub cwd_text: String,
    pub global: bool,
}

pub fn fmt_tokens(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 100_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n < 1_000_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    }
}

pub fn fmt_reset(now: u64, resets_at: Option<u64>) -> String {
    let Some(ts) = resets_at.filter(|&t| t > now) else { return String::new() };
    let rem = ts - now;
    if rem >= 86_400 {
        format!(" ({}d{}h)", rem / 86_400, (rem % 86_400) / 3_600)
    } else if rem >= 3_600 {
        format!(" ({}h{:02}m)", rem / 3_600, (rem % 3_600) / 60)
    } else {
        format!(" ({}m)", (rem / 60).max(1))
    }
}

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn middle_elide(text: &str, max_cols: usize) -> String {
    let w = |c: char| if editor::wide(c) { 2 } else { 1 };
    let total: usize = text.chars().map(w).sum();
    if total <= max_cols || max_cols < 8 {
        return text.to_string();
    }
    let keep = max_cols.saturating_sub(1);
    let head_budget = keep / 3;
    let tail_budget = keep.saturating_sub(head_budget);
    let mut head = String::new();
    let mut hc = 0;
    for c in text.chars() {
        let cw = w(c);
        if hc + cw > head_budget {
            break;
        }
        head.push(c);
        hc += cw;
    }
    let mut tail_chars = Vec::new();
    let mut tc = 0;
    for c in text.chars().rev() {
        let cw = w(c);
        if tc + cw > tail_budget {
            break;
        }
        tail_chars.push(c);
        tc += cw;
    }
    tail_chars.reverse();
    let tail: String = tail_chars.into_iter().collect();
    format!("{head}\u{2026}{tail}")
}

impl App {
    pub(crate) fn start_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "No session is open.".to_string(),
            "Ctrl+N  new session   ·   Ctrl+Shift+N  new Global general session".to_string(),
            "Ctrl+A  accounts and sign-in".to_string(),
            "Ctrl+E  working directory files".to_string(),
        ];
        if !self.ui.saved_cwd.is_empty() {
            lines.push(format!("Working directory: {}   (Ctrl+E or Files to open)", self.ui.saved_cwd));
        }
        if !self.ui.numbered.is_empty() {
            lines.push("Alt+1 to Alt+9, Ctrl+Tab or a click on the tabs/list opens a session; they keep running when you leave them.".to_string());
        }
        lines
    }

    pub(crate) fn hint_text(&self, width: usize, scale: f32) -> String {
        let cols = (width as f32 / (self.char_w() * scale)) as usize;
        if let Some(text) = self.hover_hint(cols) {
            return text;
        }
        let escape = if self.ui.panel.is_some() {
            Some("Esc back out")
        } else if !self.session.is_empty() {
            Some("Esc session list")
        } else {
            None
        };
        let mut items = vec!["Ctrl+N new session", "Ctrl+A accounts", "Ctrl+Tab next session", "Alt+1-9 session by number"];
        if let Some(e) = escape {
            items.insert(2, e);
        }
        if self.selected_row().map_or(false, |r| !r.approvals.is_empty()) {
            items.insert(0, self.approval_hint());
        }
        if let Some(item) = self.disconnected_hint(cols) {
            items.insert(0, item);
        }
        let mut text = String::new();
        for item in items {
            let next = if text.is_empty() { item.to_string() } else { format!("{text}   {item}") };
            if next.chars().count() > cols {
                break;
            }
            text = next;
        }
        text
    }

    pub(crate) fn menu_json(&self) -> Value {
        let titles: Vec<Value> = TITLES
            .iter()
            .enumerate()
            .map(|(i, t)| json!({"label": t, "rect": self.ui.menu_geom.titles.get(i)}))
            .collect();
        let items: Vec<Value> = match self.ui.menu_open {
            Some(m) => self
                .menu_entries(m)
                .iter()
                .enumerate()
                .map(|(i, e)| json!({"label": e.label, "key": e.key, "enabled": e.reason.is_none(), "reason": e.reason, "rect": self.ui.menu_geom.items.get(i)}))
                .collect(),
            None => Vec::new(),
        };
        json!({"shown": self.ui.menu, "titles": titles, "open": self.ui.menu_open, "selected": self.ui.menu_sel, "items": items})
    }

    pub(crate) fn show_telemetry_bar(&self) -> bool {
        self.ui.workbench && self.ui.menu && !self.session.is_empty() && self.ui.panel.is_none()
    }

    pub(crate) fn focus_active_session_dir(&mut self) {
        let Some(row) = self.selected_row().cloned() else { return };
        if self.ui.sidebar.collapsed {
            self.ui.sidebar.collapsed = false;
            self.ui.composed = None;
        }
        self.sidebar_sync_to_active_session();
        if row.is_global() {
            self.ui.flash = Some("Global general session (no fixed working directory).".into());
            self.dirty = true;
            return;
        }
        self.files_open();
    }

    pub(crate) fn rows_dump_json(&self) -> Vec<Value> {
        self.ui
            .rows
            .iter()
            .map(|r| {
                json!({
                    "session": r.session,
                    "provider": r.provider,
                    "account": r.account,
                    "label": self.label_of(&r.account),
                    "model": r.model,
                    "chosen_model": if r.chosen_model.is_empty() { Value::Null } else { json!(r.chosen_model) },
                    "effort": if r.effort.is_empty() { Value::Null } else { json!(r.effort) },
                    "tokens": r.tokens.map(|(i, o)| json!({"input": i, "output": o})),
                    "global": r.is_global(),
                    "cwd": if r.cwd.is_empty() { Value::Null } else { json!(r.cwd) },
                    "ssh_host": if r.ssh_host.is_empty() { Value::Null } else { json!(r.ssh_host) },
                    "display_name": r.name(),
                    "auto_title": if r.auto_title.is_empty() { Value::Null } else { json!(r.auto_title) },
                    "last_active_at": r.last_active_at,
                    "weekly_remaining_pct": r.weekly_remaining_pct,
                    "short_remaining_pct": r.short_remaining_pct,
                    "weekly_resets_at": r.weekly_resets_at,
                    "short_resets_at": r.short_resets_at,
                    "state": r.state,
                    "generation": r.generation,
                    "state_text": r.state_text(),
                    "activity": r.activity,
                    "approvals": r.approvals.iter().map(|a| json!([a.0, a.1])).collect::<Vec<_>>(),
                    "virtual_display": if r.virtual_display.is_empty() { Value::Null } else { json!(r.virtual_display) },
                    "unread": self.row_unread(r),
                    "worktree": if r.worktree_branch.is_empty() { Value::Null } else { json!({"branch": r.worktree_branch, "repo": r.worktree_repo, "exists": r.worktree_exists}) },
                    "selected": r.session == self.session,
                })
            })
            .collect()
    }

    pub(crate) fn telemetry_json(&self) -> Value {
        let g = &self.ui.telemetry_geom;
        let row = self.selected_row();
        json!({
            "shown": g.shown,
            "model_text": g.model_text,
            "tokens": row.and_then(|r| r.tokens.map(|(i, o)| json!({"input": i, "output": o}))),
            "tokens_text": g.tokens_text,
            "quota_text": g.quota_text,
            "quota_level": if g.quota_level.is_empty() { "unknown" } else { g.quota_level },
            "cwd_text": g.cwd_text,
            "global": g.global,
            "rect": g.rect,
            "cwd_rect": g.cwd_rect,
        })
    }

    fn session_telemetry_parts(&self, row: &Row) -> (String, String, String, &'static str, Color, String, bool) {
        let base_model = if !row.model.is_empty() {
            row.model.clone()
        } else if !row.chosen_model.is_empty() {
            row.chosen_model.clone()
        } else {
            format!("{} (default)", row.provider)
        };
        let model_text = if !row.effort.is_empty() {
            format!("{base_model} ({})", title_case(&row.effort))
        } else {
            base_model
        };

        let tokens_text = match row.tokens {
            Some((inp, out)) => format!("Tok: in:{} out:{}", fmt_tokens(inp), fmt_tokens(out)),
            None => "Tok: in:-- out:--".to_string(),
        };

        let now = self.clock_now();
        let fb_pct = self.ui.quotas.get(&row.account).copied();
        let fb_res = self.ui.quota_resets.get(&row.account).copied();
        let w_pct = row.weekly_remaining_pct.or_else(|| fb_pct.and_then(|q| q.0));
        let s_pct = row.short_remaining_pct.or_else(|| fb_pct.and_then(|q| q.1));
        let w_res = row.weekly_resets_at.or_else(|| fb_res.and_then(|q| q.0));
        let s_res = row.short_resets_at.or_else(|| fb_res.and_then(|q| q.1));

        let mut qparts = Vec::new();
        if let Some(s) = s_pct {
            qparts.push(format!("5h:{s}%{}", fmt_reset(now, s_res)));
        }
        if let Some(w) = w_pct {
            qparts.push(format!("Wk:{w}%{}", fmt_reset(now, w_res)));
        }
        let quota_text = if qparts.is_empty() { "Wk:--".to_string() } else { qparts.join(" \u{00b7} ") };
        let min_pct = match (w_pct, s_pct) {
            (Some(w), Some(s)) => Some(w.min(s)),
            (Some(w), None) => Some(w),
            (None, Some(s)) => Some(s),
            (None, None) => None,
        };
        let (quota_level, quota_col) = match min_pct {
            Some(p) if p < 10 => ("critical", Color::rgb(0xF6, 0x6E, 0x6E)),
            Some(p) if p < 25 => ("low", Color::rgb(0xF5, 0xBD, 0x58)),
            Some(_) => ("ok", Color::rgb(0x6B, 0xDC, 0x9B)),
            None => ("unknown", Color::rgb(0x88, 0x94, 0xA4)),
        };

        let global = row.is_global();
        let cwd_text = if global {
            if !row.ssh_host.is_empty() {
                format!("Global [ssh:{}]", row.ssh_host)
            } else {
                "Global (general)".to_string()
            }
        } else {
            let host_prefix = if row.ssh_host.is_empty() { String::new() } else { format!("ssh:{}:", row.ssh_host) };
            let base = if !row.cwd.is_empty() { row.cwd.as_str() } else { row.worktree_repo.as_str() };
            let branch = if row.worktree_branch.is_empty() {
                String::new()
            } else {
                format!(" ({})", row.worktree_branch)
            };
            format!("{host_prefix}{base}{branch}")
        };

        (model_text, tokens_text, quota_text, quota_level, quota_col, cwd_text, global)
    }

    pub(crate) fn draw_telemetry_bar(
        &mut self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        list_w: usize,
        telem_y: f32,
        hint_y: f32,
    ) {
        self.ui.telemetry_geom = TelemetryGeom::default();
        if !self.show_telemetry_bar() || list_w >= width || hint_y <= telem_y {
            return;
        }
        let Some(row) = self.selected_row().cloned() else { return };
        let (model_text, tokens_text, quota_text, quota_level, quota_col, raw_cwd, global) =
            self.session_telemetry_parts(&row);
        let s = self.ui.scale.max(1.0);
        let char_w = self.char_w() * s;
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;

        let bar_rect = [list_w as f32, telem_y, width as f32, hint_y];
        controls::fill(buf, width, height, bar_rect, 0xFF14_1C26);
        controls::fill(buf, width, height, [bar_rect[0], telem_y, bar_rect[2], telem_y + 1.0], 0xFF28_3444);

        let sep = "  |  ";
        let sep_col = Color::rgb(0x56, 0x64, 0x76);
        let model_col = Color::rgb(0x7C, 0xC4, 0xFF);
        let tok_col = Color::rgb(0xBA, 0xE2, 0xF2);
        let cwd_col = if global {
            Color::rgb(0x5E, 0xDF, 0xC8)
        } else {
            Color::rgb(0xE2, 0xD2, 0x9E)
        };

        let mut x = bar_rect[0] + 6.0 * s;
        let clip_r = (width as f32 - 6.0 * s) as i32;
        x += controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, &model_text, model_col);
        x += controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, sep, sep_col);
        x += controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, &tokens_text, tok_col);
        x += controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, sep, sep_col);
        x += controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, &quota_text, quota_col);
        x += controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, sep, sep_col);

        let rem_px = (width as f32 - 8.0 * s - x).max(0.0);
        let rem_cols = (rem_px / char_w.max(1.0)) as usize;
        let shown_cwd = middle_elide(&raw_cwd, rem_cols.max(10));
        let mut cwd_rect = None;
        if rem_px >= 40.0 * s {
            let cw = controls.text(fs, swash, buf, width, height, x, telem_y + s, clip_r, &shown_cwd, cwd_col);
            let cr = [x, telem_y, (x + cw + 4.0 * s).min(width as f32), hint_y];
            controls.target(cr, Action::FocusSessionDir);
            cwd_rect = Some(cr);
        }

        self.ui.telemetry_geom = TelemetryGeom {
            shown: true,
            rect: Some(bar_rect),
            cwd_rect,
            model_text,
            tokens_text,
            quota_text,
            quota_level,
            cwd_text: raw_cwd,
            global,
        };
    }

    /// Renders a compact right-aligned multi-account quota summary on the bottom status bar when space permits.
    pub(crate) fn draw_global_quota_summary(&mut self, buf: &mut [u32], width: usize, height: usize, hint_y: f32, left_used_px: f32) {
        if !self.ui.workbench || !self.ui.menu || self.ui.quotas.is_empty() {
            return;
        }
        let mut entries: Vec<(String, Option<u8>, Option<u8>)> = self
            .ui
            .quotas
            .iter()
            .map(|(acc, &(w, s))| (self.label_of(acc), w, s))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let parts: Vec<String> = entries
            .into_iter()
            .take(3)
            .filter_map(|(lbl, w, s)| match (s, w) {
                (Some(sv), Some(wv)) => Some(format!("{lbl} 5h:{sv}% Wk:{wv}%")),
                (None, Some(wv)) => Some(format!("{lbl} Wk:{wv}%")),
                (Some(sv), None) => Some(format!("{lbl} 5h:{sv}%")),
                (None, None) => None,
            })
            .collect();
        if parts.is_empty() {
            return;
        }
        let summary = parts.join("  \u{00b7}  ");
        let s = self.ui.scale.max(1.0);
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let tw = controls.width(fs, &summary);
        let x = width as f32 - 8.0 * s - tw;
        if x > left_used_px + 24.0 * s {
            controls.text(fs, swash, buf, width, height, x, hint_y, width as i32, &summary, Color::rgb(0x8E, 0xA8, 0xC4));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_and_reset_formatting_stays_compact_and_exact() {
        assert_eq!(fmt_tokens(0), "0");
        assert_eq!(fmt_tokens(942), "942");
        assert_eq!(fmt_tokens(1_250), "1.2k");
        assert_eq!(fmt_tokens(95_700), "95.7k");
        assert_eq!(fmt_tokens(245_000), "245k");
        assert_eq!(fmt_tokens(2_510_000), "2.51M");

        assert_eq!(fmt_reset(1_000, None), "");
        assert_eq!(fmt_reset(1_000, Some(900)), "");
        assert_eq!(fmt_reset(1_000, Some(1_000 + 45 * 60)), " (45m)");
        assert_eq!(fmt_reset(1_000, Some(1_000 + 3_600 + 16 * 60)), " (1h16m)");
        assert_eq!(fmt_reset(1_000, Some(1_000 + 4 * 86_400 + 9 * 3_600)), " (4d9h)");
    }
}
