//! `M24-P11f`: the status of a session stands at the bottom of the window and nowhere else, and the conversation has a border of its own under the tabs.
//!
//! The top line of a conversation said `claude | Work | claude-sonnet | idle 14:03:09 | mode: auto (permissions skipped) | ... | dir: ... | 5h 80%`: one more
//! thin bar of the tone of the tab bar over the first line, saying again what the strip at the bottom says (model, tokens, quota, folder). It is gone: the
//! conversation starts under a border, and what only the top line said (the provider, the account, the state with its clock, the mode, an account issue, and
//! the host's notice and the one-line messages) stands at the left of the strip above the path bar. A view that has a title or a control of its own at the top
//! (a panel, the browser, a terminal, the Call Room) keeps its line.

use cosmic_text::Color;

use super::super::App;
use super::row::Row;

/// The rule under the tab bar, in logical pixels, and the margin of the conversation's own ground under the rule.
const RULE: f32 = 2.0;
const MARGIN: f32 = 4.0;
/// The tone of the rule: lighter than the bars above it, so that the chat reads as a pane of its own.
pub(crate) const RULE_TONE: u32 = 0xFF3A_4B63;
/// The state part of the strip, in the light ink of the path bar; a notice or a message in a warm one.
pub(crate) const STATE_INK: Color = Color::rgb(0xD6, 0xDE, 0xE8);
pub(crate) const WARM_INK: Color = Color::rgb(0xFF, 0xC8, 0x6B);

/// The height of the border between the bars (menu, icons, tabs) and the conversation, in pixels at scale `s`: none when there are no bars.
pub(crate) fn edge(menu: bool, s: f32) -> f32 {
    if menu {
        (RULE + MARGIN) * s
    } else {
        0.0
    }
}

/// The two rows of the rule: from the line where the tab bar ends, `RULE` logical pixels.
pub(crate) fn rule_rows(y: f32, s: f32) -> [f32; 2] {
    [y, y + RULE * s]
}

/// The permission mode in a word, as the strip says it; none for a mode the host did not report.
pub(crate) fn mode_word(mode: &str) -> Option<&'static str> {
    match mode {
        "auto" => Some("auto"),
        "default" => Some("asks"),
        _ => None,
    }
}

/// What the top line said of the session, without what the strip says: `provider | account | state with its clock | mode | account issue`; a local
/// session has no account and no provider mode.
pub(crate) fn state_part(row: &Row, label: &str, now: u64) -> String {
    let state = row.state_with_clock(now);
    if row.provider == "local" {
        return format!("Local | {state}");
    }
    let mode = mode_word(&row.mode).map_or_else(String::new, |m| format!(" | mode {m}"));
    format!("{} | {label} | {state}{mode}{}", row.provider, issue_part(row))
}

fn issue_part(row: &Row) -> String {
    if row.issue.is_empty() {
        String::new()
    } else {
        format!(" | account issue: {}", row.issue)
    }
}

/// The forms of the state part, the longest first: the whole, without the mode, and the state alone (with the issue, which is never left out). A strip
/// that is too narrow for the whole takes a shorter one, so that the folder keeps its room.
pub(crate) fn state_forms(row: &Row, label: &str, now: u64) -> Vec<String> {
    let state = row.state_with_clock(now);
    let mut forms = vec![state_part(row, label, now)];
    let mut push = |form: String| {
        if forms.last() != Some(&form) {
            forms.push(form);
        }
    };
    if row.provider != "local" {
        push(format!("{} | {label} | {state}{}", row.provider, issue_part(row)));
    }
    push(format!("{state}{}", issue_part(row)));
    forms
}

/// The separator between the parts of the strip and the columns it takes, and the columns the folder keeps at least.
pub(crate) const SEP: &str = "  |  ";
pub(crate) const SEP_COLS: usize = 5;
pub(crate) const CWD_COLS: usize = 20;

/// The longest form that leaves `extra` columns for the notices after it within `room` columns (a form and its separator count); the shortest when none does.
pub(crate) fn pick_form(forms: &[String], extra: usize, room: usize) -> &str {
    let cost = |form: &str| super::display_cols(form) + SEP_COLS + extra;
    forms.iter().find(|f| cost(f) <= room).or(forms.last()).map_or("", String::as_str)
}

/// The form of the state part for a strip `strip_px` wide at scale `s` whose columns are `char_w` pixels: the strip keeps 12 units at its ends; the model, the
/// tokens and the quota (`parts`, in columns; none for a session the host does not list) take theirs with their separators and the folder keeps `CWD_COLS`;
/// the notices take their columns before the state takes any.
pub(crate) fn strip_form<'a>(forms: &'a [String], notices: &[String], strip_px: f32, s: f32, char_w: f32, parts: Option<[usize; 3]>) -> &'a str {
    let room = ((strip_px - 12.0 * s) / char_w.max(1.0)) as usize;
    let rest = parts.map_or(0, |p| p.iter().sum::<usize>() + 3 * SEP_COLS + CWD_COLS);
    let extra: usize = notices.iter().map(|n| super::display_cols(n) + SEP_COLS).sum();
    pick_form(forms, extra, room.saturating_sub(rest))
}

impl App {
    /// True when the conversation of this window has no line of status at its top: the window has its bars and a workbench, and no panel (the start
    /// page is one, with its title), browser, terminal or Call Room has the top line for its own.
    pub(crate) fn status_at_bottom(&self, panel: bool) -> bool {
        self.ui.workbench && self.ui.menu && !panel && self.browser_header().is_none() && self.terminal_header().is_none() && self.active_call_header().is_none()
    }

    /// The forms of the state part of the strip for the selected session, the longest first; a session the host does not list says so.
    pub(crate) fn status_forms(&self) -> Vec<String> {
        match self.selected_row() {
            Some(r) => state_forms(r, &self.row_label(r), self.clock_now()),
            None if self.session.is_empty() => Vec::new(),
            None => vec![format!("{} | not listed", self.session)],
        }
    }

    /// The host's notice and the one-line message, in the order the top line had them.
    pub(crate) fn notice_list(&self) -> Vec<String> {
        [self.ui.host_notice.as_ref(), self.ui.flash.as_ref()].into_iter().flatten().cloned().collect()
    }

    /// The rule under the tab bar, from the list's edge to the window's: `y` is the line where the tab bar ends.
    pub(crate) fn draw_chat_edge(&mut self, buf: &mut [u32], width: usize, height: usize, list_w: usize, y: f32, s: f32) {
        if self.ui.menu && list_w < width {
            let [y0, y1] = rule_rows(y, s);
            super::super::controls::fill(buf, width, height, [list_w as f32, y0, width as f32, y1], RULE_TONE);
        }
    }
}

#[cfg(test)]
#[path = "statusline_tests.rs"]
mod statusline_tests;
