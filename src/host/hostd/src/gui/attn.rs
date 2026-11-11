//! Background session attention in `lico gui` (M02-P06, S02; NT-01).
//!
//! When a session that is not on screen finishes a turn or waits for approval, the session list marks it (`!` for approval, `•` for an unread finished turn;
//! `>> ` input echo does not move `finished_seq` and never marks unread) and the X11 window title appends `[approval]` or `[finished]` after the `Lico ...`
//! prefix. Viewing a session records its `finished_seq` and clears its unread mark.

use super::super::App;
use super::{memrow, ListItem, Row};

impl App {
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
        let mut groups: Vec<(String, String)> = Vec::new(); // (provider, account) in order of first appearance
        for row in &self.ui.rows {
            let key = (row.provider.clone(), row.account.clone());
            if !groups.contains(&key) {
                groups.push(key);
            }
        }
        let mut items = Vec::new();
        let mut number = 0usize;
        self.ui.numbered.clear();
        for (provider, account) in &groups {
            items.push(ListItem::Heading(format!("{provider} / {}", self.label_of(account))));
            for row in self.ui.rows.iter().filter(|r| &r.provider == provider && &r.account == account) {
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
                number += 1;
                self.ui.numbered.push(row.session.clone());
                let digit = if number <= 9 { number.to_string() } else { " ".to_string() };
                // a working session shows its clock here: the list changes once a second while a turn is open, and that is what redraws the window
                let (state, age) = row.list_parts(now);
                items.push(ListItem::Row(row.session.clone(), format!("{marker}{digit} {short} {state}{approvals}{wt} {age}"), memrow::cell(row)));
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
    use super::super::Row;

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
}
