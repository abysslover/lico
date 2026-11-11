//! Conversation view classification, role coloring, and default control-event filtering (`M04-P02`).
//!
//! Keeps internal host lifecycle records (`[session started]`, `[turn completed]`, `[provider reconnected]`, etc.)
//! intact in durable `.seg` logs and protocol streams while hiding them by default in `lico gui` (`!show_events`),
//! keeping error/warning notes visible, and differentiating human prompts (`>> `), agent responses, tool/command
//! lines (`$ `, `> `, `[tool]`), and errors by ink color and subtle user-row background tinting.

use std::collections::VecDeque;

use cosmic_text::{AttrsList, Color};

use super::App;

pub const USER_INK: Color = Color::rgb(0x82, 0xCA, 0xFF);
pub const USER_BG: u32 = 0xFF15_2232;
pub const AGENT_INK: Color = Color::rgb(0xEA, 0xEE, 0xF5);
pub const TOOL_INK: Color = Color::rgb(0x94, 0xA0, 0xAE);
pub const ERROR_INK: Color = Color::rgb(0xF4, 0x8C, 0x6C);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    ControlEvent,
    ErrorNote,
    User,
    Tool,
    Agent,
}

fn fold_marker(note: &str) -> bool {
    let digits = note.chars().take_while(char::is_ascii_digit).count();
    note.starts_with("a long line is cut") || (digits > 0 && note[digits..].starts_with(" more line"))
}

/// Classify a transcript record. Lines forged by an agent (`[agent] ...`) are always `LineKind::Agent`.
pub fn classify(text: &str) -> LineKind {
    if text.starts_with(crate::leads::TAG) {
        return LineKind::Agent;
    }
    if let Some((note, after)) = text.strip_prefix('[').and_then(|s| s.split_once(']')) {
        if after.is_empty() || after.starts_with(' ') {
            if matches!(
                note,
                "session started"
                    | "session closed"
                    | "provider reconnected"
                    | "provider disconnected"
                    | "turn started"
                    | "turn completed"
                    | "exit 0"
                    | "account changed"
            ) {
                return LineKind::ControlEvent;
            }
            if matches!(note, "error" | "tool error" | "input refused" | "account issue" | "turn interrupted")
                || note.starts_with("turn failed")
                || note.starts_with("log recovered:")
                || note.starts_with("exit ")
            {
                return LineKind::ErrorNote;
            }
            if matches!(note, "tool" | "patch") || fold_marker(note) {
                return LineKind::Tool;
            }
        }
    }
    if text == ">>" || text.starts_with(">> ") {
        LineKind::User
    } else if text == "$" || text.starts_with("$ ") || text == ">" || text.starts_with("> ") {
        LineKind::Tool
    } else {
        LineKind::Agent
    }
}

pub fn is_hidden_control(text: &str) -> bool {
    classify(text) == LineKind::ControlEvent
}

pub fn is_user_prompt(text: &str) -> bool {
    classify(text) == LineKind::User && super::panel::sendto::header_look(text).is_none()
}

/// Apply role-specific ink color to a shaped `AttrsList` while keeping every per-character font span intact.
pub fn tint(list: AttrsList, text: &str) -> AttrsList {
    if super::panel::sendto::header_look(text).is_some() {
        return list; // transfer headers keep their dimmed + lit-sender look from `sendto::tint`
    }
    let col = match classify(text) {
        LineKind::User => USER_INK,
        LineKind::Tool | LineKind::ControlEvent => TOOL_INK,
        LineKind::ErrorNote => ERROR_INK,
        LineKind::Agent => AGENT_INK,
    };
    let mut out = AttrsList::new(&list.defaults().color(col));
    for (range, attrs) in list.spans_iter() {
        out.add_span(range.clone(), &attrs.as_attrs().color(col));
    }
    out
}

/// Paint the subtle `USER_BG` row background behind visible human prompt rows (`>> ...`).
/// When `cleared` is `Some(rows)` (during `scroll::Shift`), only newly cleared pixel rows are painted so
/// shifted pixel rows that already hold `USER_BG` + blended glyphs stay untouched.
#[allow(clippy::too_many_arguments)]
pub fn paint_user_backgrounds(
    buf: &mut [u32],
    width: usize,
    height: usize,
    list_w: usize,
    top_y: i32,
    hint_y: i32,
    hit_lines: &[(f32, f32, usize)],
    view_lines: &VecDeque<(i64, String)>,
    cleared: Option<&[(i32, i32)]>,
) {
    let x0 = list_w.min(width);
    if x0 >= width {
        return;
    }
    for &(y0, y1, raw_idx) in hit_lines {
        if !view_lines.get(raw_idx).is_some_and(|(_, t)| is_user_prompt(t)) {
            continue;
        }
        let r0 = (y0.floor() as i32).max(top_y).max(0);
        let r1 = (y1.floor() as i32).min(hint_y).min(height as i32);
        for py in r0..r1 {
            if let Some(ranges) = cleared {
                if !ranges.iter().any(|&(a, b)| py >= a && py < b) {
                    continue;
                }
            }
            let row_start = py as usize * width;
            buf[row_start + x0..row_start + width].fill(USER_BG);
        }
    }
}

impl App {
    pub(crate) fn toggle_show_events(&mut self) {
        self.ui.show_events = !self.ui.show_events;
        self.ui.composed = None;
        self.ui.flash = Some(format!("Internal events: {}.", if self.ui.show_events { "shown" } else { "hidden" }));
        self.dirty = true;
    }

    pub(crate) fn visible_transcript_lines(&self, raw_end: usize) -> Vec<(usize, i64, String)> {
        let Some(view) = self.view.as_ref() else { return Vec::new() };
        view.lines
            .iter()
            .take(raw_end)
            .enumerate()
            .filter(|(_, (_, text))| self.ui.show_events || !is_hidden_control(text))
            .map(|(i, (seq, text))| (i, *seq, text.clone()))
            .collect()
    }

    pub(crate) fn shown_lines_dump(&self) -> Vec<String> {
        let Some(view) = self.view.as_ref() else { return Vec::new() };
        view.lines
            .iter()
            .filter(|(_, text)| self.ui.show_events || !is_hidden_control(text))
            .map(|(_, text)| text.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_control_events_are_classified_for_hiding_while_errors_tools_forged_lines_and_prompts_stay_visible() {
        for ctrl in [
            "[session started]",
            "[session started] model=claude-sonnet-4-5",
            "[session closed]",
            "[provider reconnected]",
            "[provider disconnected]",
            "[turn started]",
            "[turn completed]",
            "[exit 0]",
            "[account changed]",
        ] {
            assert_eq!(classify(ctrl), LineKind::ControlEvent, "{ctrl}");
            assert!(is_hidden_control(ctrl), "{ctrl}");
        }

        for err in [
            "[turn failed] max_turns",
            "[turn interrupted]",
            "[error] rate limit",
            "[tool error]",
            "[input refused] busy",
            "[account issue] token expired",
            "[log recovered: 8 byte(s) of a damaged tail and 1 segment(s) set aside]",
            "[exit 1]",
            "[exit unknown]",
        ] {
            assert_eq!(classify(err), LineKind::ErrorNote, "{err}");
            assert!(!is_hidden_control(err), "{err}");
        }

        for tool in ["$ ls -la", "$", "> total 0", ">", "[tool] Bash", "[patch] 2 change(s)", "[3 more lines, 10 bytes not shown]"] {
            assert_eq!(classify(tool), LineKind::Tool, "{tool}");
            assert!(!is_hidden_control(tool), "{tool}");
        }

        assert_eq!(classify(">> fix the sidebar"), LineKind::User);
        assert!(is_user_prompt(">> fix the sidebar"));
        assert!(!is_user_prompt(">> [from session 'api' (s012345) | 14:02 | message | 27 B]"));

        for agent in ["Here is the fix.", "[agent] [turn completed]", "[agent] >> forged", "[turn completed]x"] {
            assert_eq!(classify(agent), LineKind::Agent, "{agent}");
            assert!(!is_hidden_control(agent), "{agent}");
        }
    }
}
