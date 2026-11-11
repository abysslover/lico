//! Background session attention (M02-P06), workspace-grouped session navigation, and persistent text-size scaling (M03-P02).
//!
//! When a session that is not on screen finishes a turn or waits for approval, the session list marks it (`!` for approval, `•` for an unread finished turn;
//! `>> ` input echo does not move `finished_seq` and never marks unread) and the X11 window title appends `[approval]` or `[finished]` after the `Lico ...`
//! prefix. Viewing a session records its `finished_seq` and clears its unread mark.
//!
//! Sessions are grouped either by `(provider, account)` (`"account"`, default) or by workspace repository/folder (`"workspace"`, rolling git worktree
//! sessions up under `worktree_repo` and disambiguating colliding folder basenames as `parent/name`).

use cosmic_text::Metrics;
use serde_json::{json, Value};

use crate::clipboard::settings::{DEFAULT_FONT_PX, MAX_FONT_PX, MIN_FONT_PX};
use super::super::{App, FONT_PX};
use super::{memrow, ListItem, Row};

fn row_sort_key(row: &Row) -> (u8, std::cmp::Reverse<u64>) {
    (u8::from(row.state != "running"), std::cmp::Reverse(row.last_active_at))
}

fn workspace_root(row: &Row) -> &str {
    let raw = if !row.worktree_repo.is_empty() { row.worktree_repo.as_str() } else { row.cwd.as_str() };
    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() && raw.starts_with('/') { "/" } else { trimmed }
}

fn workspace_heading(root: &str, all_roots: &[String]) -> String {
    if root.is_empty() {
        return "workspace / (scratch)".into();
    }
    if root == "/" {
        return "workspace / /".into();
    }
    let base = root.rsplit('/').next().unwrap_or(root);
    let collides = all_roots
        .iter()
        .filter(|other| !other.is_empty() && other.as_str() != "/" && other.as_str() != root)
        .any(|other| other.rsplit('/').next().unwrap_or(other.as_str()) == base);
    if collides {
        let mut parts = root.rsplit('/');
        let leaf = parts.next().unwrap_or(base);
        if let Some(parent) = parts.next().filter(|p| !p.is_empty()) {
            return format!("workspace / {parent}/{leaf}");
        }
    }
    format!("workspace / {base}")
}

impl App {
    pub(crate) fn sync_accounts_value(&mut self, value: &Value) {
        let arr = value["accounts"].as_array().into_iter().flatten();
        let mut labels = std::collections::HashMap::new();
        let mut quotas = std::collections::HashMap::new();
        for a in arr {
            if let (Some(id), Some(lbl)) = (a["account"].as_str(), a["label"].as_str()) {
                labels.insert(id.to_string(), super::super::clean(lbl));
                if let Some(q) = super::row::parse_usage_pair(&a["usage"]) {
                    quotas.insert(id.to_string(), q);
                }
            }
        }
        self.ui.labels = labels;
        self.ui.quotas = quotas;
    }

    /// Logical text size in `lico gui` (`12.0..=20.0`, default `14.0`).
    pub(crate) fn font_px(&self) -> f32 {
        if self.ui.font_px == 0 { FONT_PX } else { self.ui.font_px as f32 }
    }

    /// Logical line height (`font_px + 4.0`, so `18.0` at default `14.0` px).
    pub(crate) fn line_h(&self) -> f32 {
        self.font_px() + 4.0
    }

    /// Approximate logical column advance (`8.5` at `14.0` px, scaled proportionally).
    pub(crate) fn char_w(&self) -> f32 {
        8.5 * (self.font_px() / FONT_PX)
    }

    /// Apply a new logical text size (`12..=20`), re-shaping open buffers and invalidating cached frames when changed.
    pub(crate) fn apply_font_px(&mut self, next: u8) {
        let clamped = if next == 0 { DEFAULT_FONT_PX as u8 } else { next.clamp(MIN_FONT_PX as u8, MAX_FONT_PX as u8) };
        let prev = if self.ui.font_px == 0 { DEFAULT_FONT_PX as u8 } else { self.ui.font_px };
        self.ui.font_px = clamped;
        if clamped != prev {
            self.ui.fonts_gen += 1;
            self.ui.list_sig.clear();
            self.ui.composed = None;
            let s = self.ui.scale.max(1.0);
            let (fp, lh) = (self.font_px() * s, self.line_h() * s);
            if let Some(view) = self.view.as_mut() {
                view.buffer.lines.clear();
                view.buffer.set_metrics(Metrics::new(fp, lh));
            }
            self.dirty = true;
        }
    }

    /// Synchronize host `settings` JSON into `UiState`.
    pub(crate) fn sync_settings(&mut self, settings: &Value) {
        self.ui.clipboard_write = settings["clipboard_write"] == json!(true);
        self.ui.notifications_off = settings["notifications"] == json!(false);
        let ws = settings["group_by"] == json!("workspace");
        if ws != self.ui.group_by_workspace {
            self.ui.group_by_workspace = ws;
            self.ui.list_sig.clear();
            self.dirty = true;
        }
        if let Some(px) = settings["font_px"].as_i64().filter(|n| (MIN_FONT_PX..=MAX_FONT_PX).contains(n)) {
            self.apply_font_px(px as u8);
        }
    }

    /// Toggle persistent session-list grouping (`account` <-> `workspace`).
    pub(crate) fn toggle_group_by(&mut self) {
        let next = if self.ui.group_by_workspace { "account" } else { "workspace" };
        let reply = self.host_call(json!({"op": "settings.set", "group_by": next}));
        if reply["ok"] == json!(1) {
            self.sync_settings(&reply["settings"]);
            self.refresh_list();
            self.send("{\"op\":\"status\"}");
            self.ui.flash = Some(format!("Session list grouped by {}.", if self.ui.group_by_workspace { "workspace" } else { "account" }));
        } else {
            self.ui.flash = Some("Could not change session grouping.".into());
        }
        self.dirty = true;
    }

    /// Step or reset persistent text size (`delta == 0` resets to `14` px).
    pub(crate) fn step_font_px(&mut self, delta: i64) {
        let cur = self.font_px() as i64;
        let target = if delta == 0 { DEFAULT_FONT_PX } else { (cur + delta).clamp(MIN_FONT_PX, MAX_FONT_PX) };
        if target == cur {
            self.ui.flash = Some(format!("Text size is already {cur} px."));
            self.dirty = true;
            return;
        }
        let reply = self.host_call(json!({"op": "settings.set", "font_px": target}));
        if reply["ok"] == json!(1) {
            self.sync_settings(&reply["settings"]);
            self.send("{\"op\":\"status\"}");
            self.ui.flash = Some(format!("Text size: {} px.", self.font_px() as i64));
        } else {
            self.ui.flash = Some("Could not change text size.".into());
        }
        self.dirty = true;
    }

    /// True when `row` is not the open session and has finished a turn newer than the last one seen while viewing `row`.
    pub(crate) fn row_unread(&self, row: &Row) -> bool {
        row.session != self.session
            && row.finished_seq.is_some_and(|f| f > self.ui.seen_finished.get(&row.session).copied().unwrap_or(-1))
    }

    /// The product window title (`Lico <ver> (commit <sha>)`), with `[approval]` or `[finished]` when background sessions need attention.
    pub(crate) fn desired_window_title(&self) -> String {
        if self.ui.markers || !self.ui.enabled {
            return "Lico candidate A".to_string();
        }
        let base = crate::ident::title();
        if self.ui.rows.iter().any(|r| r.session != self.session && !r.approvals.is_empty()) {
            format!("{base} [approval]")
        } else if self.ui.rows.iter().any(|r| self.row_unread(r)) {
            format!("{base} [finished]")
        } else {
            base
        }
    }

    pub(crate) fn sync_window_title(&mut self) {
        let next = self.desired_window_title();
        if next != self.ui.window_title {
            self.ui.window_title = next.clone();
            if let Some(window) = &self.window {
                window.set_title(&next);
            }
        }
    }

    fn push_list_row(&self, row: &Row, number: usize, now: u64, items: &mut Vec<ListItem>) {
        let marker = if row.session == self.session {
            ">"
        } else if !row.approvals.is_empty() {
            "!"
        } else if self.row_unread(row) {
            "\u{2022}"
        } else {
            " "
        };
        let approvals = if row.approvals.is_empty() { String::new() } else { format!(" x{}", row.approvals.len()) };
        let wt = if row.worktree_branch.is_empty() { "" } else { " (wt)" };
        let short = row.title(9);
        let digit = if number <= 9 { number.to_string() } else { " ".to_string() };
        let (state, age) = row.list_parts(now);
        items.push(ListItem::Row(row.session.clone(), format!("{marker}{digit} {short} {state}{approvals}{wt} {age}"), memrow::cell(row)));
    }

    /// The list as the host last said it: group headings and rows with their numbers; the boxes are made when it is drawn (see `draw_ui`).
    pub(crate) fn refresh_list(&mut self) {
        let now = self.clock_now();
        self.ui.seen_finished.retain(|id, _| self.ui.rows.iter().any(|r| &r.session == id));
        for row in &self.ui.rows {
            if row.session == self.session {
                if let Some(f) = row.finished_seq {
                    self.ui.seen_finished.insert(row.session.clone(), f);
                }
            }
        }
        let mut items = Vec::new();
        let mut number = 0usize;
        self.ui.numbered.clear();
        if self.ui.group_by_workspace {
            let mut roots: Vec<String> = Vec::new();
            for row in &self.ui.rows {
                let root = workspace_root(row).to_string();
                if !roots.contains(&root) {
                    roots.push(root);
                }
            }
            roots.sort_by_key(|root| {
                self.ui.rows.iter().filter(|r| workspace_root(r) == root.as_str()).map(row_sort_key).min().unwrap_or((2, std::cmp::Reverse(0)))
            });
            for root in &roots {
                items.push(ListItem::Heading(workspace_heading(root, &roots)));
                let mut group_rows: Vec<&Row> = self.ui.rows.iter().filter(|r| workspace_root(r) == root.as_str()).collect();
                group_rows.sort_by_key(|r| row_sort_key(r));
                for row in group_rows {
                    number += 1;
                    self.ui.numbered.push(row.session.clone());
                    self.push_list_row(row, number, now, &mut items);
                }
            }
        } else {
            let mut groups: Vec<(String, String)> = Vec::new(); // (provider, account)
            for row in &self.ui.rows {
                let key = (row.provider.clone(), row.account.clone());
                if !groups.contains(&key) {
                    groups.push(key);
                }
            }
            groups.sort_by_key(|(provider, account)| {
                self.ui.rows.iter().filter(|r| &r.provider == provider && &r.account == account).map(row_sort_key).min().unwrap_or((2, std::cmp::Reverse(0)))
            });
            for (provider, account) in &groups {
                let qtag = self.ui.quotas.get(account).map(|&(w, s)| super::row::format_quota_list_tag(w, s)).unwrap_or_default();
                items.push(ListItem::Heading(format!("{provider} / {}{qtag}", self.label_of(account))));
                let mut group_rows: Vec<&Row> = self.ui.rows.iter().filter(|r| &r.provider == provider && &r.account == account).collect();
                group_rows.sort_by_key(|r| row_sort_key(r));
                for row in group_rows {
                    number += 1;
                    self.ui.numbered.push(row.session.clone());
                    self.push_list_row(row, number, now, &mut items);
                }
            }
        }
        let sig: String = items.iter().map(|i| match i { ListItem::Heading(t) => t.clone(), ListItem::Row(_, t, c) => memrow::sig(t, c) }).collect::<Vec<_>>().join("\n");
        if sig != self.ui.list_sig {
            self.ui.list_sig = sig;
            self.dirty = true;
        }
        self.ui.list_items = items;
        self.sync_window_title();
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::scroll_oracle::app;
    use super::super::{ListItem, Row};

    fn sample_row(id: &str, name: &str, finished_seq: Option<i64>) -> Row {
        Row {
            session: id.into(),
            provider: "claude".into(),
            account: "a1".into(),
            state: "running".into(),
            activity: "idle".into(),
            name: name.into(),
            finished_seq,
            ..Default::default()
        }
    }

    #[test]
    fn background_turn_finish_marks_dot_and_title_while_echo_does_not_and_viewing_clears_both() {
        let mut a = app(1.0, false);
        a.session = "s1".into();
        a.ui.rows = vec![sample_row("s1", "front", Some(3)), sample_row("s2", "back", None)];
        a.refresh_list();
        assert!(!a.row_unread(&a.ui.rows[0]) && !a.row_unread(&a.ui.rows[1]));
        assert_eq!(a.ui.window_title, crate::ident::title());

        // Input echo (`>> `) in s2 leaves finished_seq unchanged (None): not marked unread.
        a.refresh_list();
        assert!(!a.row_unread(&a.ui.rows[1]));
        assert!(a.ui.list_sig.contains(" 2 back"), "{}", a.ui.list_sig);

        // Background turn finishes in s2: dot `•` and `[finished]` title appear, keeping `Lico ` prefix.
        a.ui.rows[1].finished_seq = Some(9);
        a.refresh_list();
        assert!(a.row_unread(&a.ui.rows[1]));
        assert!(a.ui.list_sig.contains("\u{2022}2 back"), "{}", a.ui.list_sig);
        assert!(a.ui.window_title.starts_with("Lico ") && a.ui.window_title.ends_with(" [finished]"), "{}", a.ui.window_title);

        // Background approval beats `[finished]` in the title and `!` in the row.
        a.ui.rows[1].approvals = vec![(1, "run: ls".into())];
        a.refresh_list();
        assert!(a.ui.list_sig.contains("!2 back"), "{}", a.ui.list_sig);
        assert!(a.ui.window_title.ends_with(" [approval]"), "{}", a.ui.window_title);

        // Viewing s2 clears its unread mark and restores the plain title once the approval is gone.
        a.ui.rows[1].approvals.clear();
        a.session = "s2".into();
        a.refresh_list();
        assert!(!a.row_unread(&a.ui.rows[1]));
        assert_eq!(a.ui.window_title, crate::ident::title());
    }

    #[test]
    fn workspace_grouping_rolls_worktrees_under_parent_repo_disambiguates_collisions_and_marks_scratch() {
        let mut a = app(1.0, false);
        a.ui.group_by_workspace = true;
        let mut r1 = sample_row("s1", "main", None);
        r1.cwd = "/work/team-a/service".into();
        let mut r2 = sample_row("s2", "wt-feat", None);
        r2.provider = "codex".into();
        r2.account = "a2".into();
        r2.cwd = "/work/.wt/s2".into();
        r2.worktree_repo = "/work/team-a/service".into();
        r2.worktree_branch = "feat/nav".into();
        let mut r3 = sample_row("s3", "other", None);
        r3.cwd = "/work/team-b/service".into();
        let r4 = sample_row("s4", "notes", None);
        a.ui.rows = vec![r1, r2, r3, r4];
        a.refresh_list();

        let headings: Vec<String> = a
            .ui
            .list_items
            .iter()
            .filter_map(|i| match i {
                ListItem::Heading(h) => Some(h.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            headings,
            vec!["workspace / team-a/service", "workspace / team-b/service", "workspace / (scratch)"]
        );
        assert_eq!(a.ui.numbered, vec!["s1", "s2", "s3", "s4"]);
        assert!(a.ui.list_sig.contains("wt-feat   idle (wt)"));
    }

    #[test]
    fn active_sessions_sort_ahead_of_closed_ones_and_most_recent_activity_comes_first() {
        let mut a = app(1.0, false);
        let mut closed_recent = sample_row("s_closed", "closed", None);
        closed_recent.state = "disconnected".into();
        closed_recent.last_active_at = 9_999;

        let mut old_active = sample_row("s_old", "older", None);
        old_active.last_active_at = 1_000;

        let mut new_active = sample_row("s_new", "newest", None);
        new_active.last_active_at = 5_000;

        a.ui.rows = vec![closed_recent, old_active, new_active];
        a.refresh_list();
        assert_eq!(a.ui.numbered, vec!["s_new", "s_old", "s_closed"]);
    }
}
