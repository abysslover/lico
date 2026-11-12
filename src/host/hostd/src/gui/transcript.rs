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
pub const PEER_BG: u32 = 0xFF12_2729;
pub const PEER_BORDER: u32 = 0xFF23_4B4E;
pub const PEER_RAIL: u32 = 0xFF38_B2AC;
pub const AGENT_INK: Color = Color::rgb(0xEA, 0xEE, 0xF5);
pub const TOOL_INK: Color = Color::rgb(0x9C, 0xAC, 0xBE);
pub const TOOL_BADGE_INK: Color = Color::rgb(0x64, 0xB5, 0xF6);
pub const CMD_BADGE_INK: Color = Color::rgb(0x4F, 0xD1, 0xB5);
pub const CMD_TEXT_INK: Color = Color::rgb(0xCE, 0xDF, 0xEE);
pub const CMD_OUT_INK: Color = Color::rgb(0xA6, 0xBD, 0xD0);
pub const EXIT_ERR_INK: Color = Color::rgb(0xFF, 0x94, 0x66);
pub const ERROR_INK: Color = Color::rgb(0xF4, 0x8C, 0x6C);
pub const CMD_BG: u32 = 0xFF0B_1016;
pub const CMD_BORDER: u32 = 0xFF24_3342;
pub const CMD_RAIL: u32 = 0xFF3D_6385;
pub const CMD_ERR_BG: u32 = 0xFF25_1517;
pub const CMD_ERR_RAIL: u32 = 0xFFE5_6B55;

pub const STYLE_MASK: u16 = 0x0007;
pub const STYLE_AGENT: u16 = 0;
pub const STYLE_USER: u16 = 1;
pub const STYLE_CMD: u16 = 2;
pub const STYLE_TOOL: u16 = 3;
pub const STYLE_CMD_OUT: u16 = 4;
pub const STYLE_EXIT_ERR: u16 = 5;
pub const STYLE_ERROR: u16 = 6;

pub const ROLE_USER_BG: u16 = 1 << 3;
pub const ROLE_PEER_BG: u16 = 1 << 4;
pub const ROLE_BOX: u16 = 1 << 5;
pub const ROLE_BOX_TOP: u16 = 1 << 6;
pub const ROLE_BOX_BOT: u16 = 1 << 7;
pub const ROLE_ERR_ROW: u16 = 1 << 8;

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
                    | "peer turn started"
                    | "host notice"
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

#[cfg(test)]
pub fn is_hidden_control(text: &str) -> bool {
    classify(text) == LineKind::ControlEvent
}

pub fn is_user_prompt(text: &str) -> bool {
    classify(text) == LineKind::User && super::panel::sendto::header_look(text).is_none()
}

fn is_exit_note(text: &str) -> bool {
    if let Some((note, after)) = text.strip_prefix('[').and_then(|s| s.split_once(']')) {
        (after.is_empty() || after.starts_with(' ')) && (note == "exit 0" || note.starts_with("exit "))
    } else {
        false
    }
}

fn is_tool_error_note(text: &str) -> bool {
    text == "[tool error]" || text.starts_with("[tool error] ")
}

fn starts_new_card(text: &str) -> bool {
    text == "$"
        || text.starts_with("$ ")
        || text == "[tool]"
        || text.starts_with("[tool] ")
        || text == "[patch]"
        || text.starts_with("[patch] ")
}

pub fn default_role(text: &str) -> u16 {
    if is_exit_note(text) {
        if classify(text) == LineKind::ErrorNote {
            STYLE_EXIT_ERR | ROLE_BOX | ROLE_ERR_ROW
        } else {
            STYLE_TOOL | ROLE_BOX
        }
    } else if is_tool_error_note(text) {
        STYLE_EXIT_ERR | ROLE_BOX | ROLE_ERR_ROW
    } else if text == "$" || text.starts_with("$ ") {
        STYLE_CMD | ROLE_BOX
    } else {
        match classify(text) {
            LineKind::User => {
                if matches!(super::panel::sendto::header_look(text), Some(super::panel::sendto::Look::Session(_))) {
                    STYLE_USER | ROLE_PEER_BG
                } else if is_user_prompt(text) {
                    STYLE_USER | ROLE_USER_BG
                } else {
                    STYLE_USER
                }
            }
            LineKind::Tool => STYLE_TOOL | ROLE_BOX,
            LineKind::ControlEvent => STYLE_TOOL,
            LineKind::ErrorNote => STYLE_ERROR,
            LineKind::Agent => STYLE_AGENT,
        }
    }
}

/// Scan `view_lines[..raw_end]` in raw order (so hidden `[exit 0]` still closes an open `$ <cmd>` output span) and return
/// visible transcript entries `(raw_idx, seq, text, role)` with contiguous per-invocation command/tool card borders (`ROLE_BOX_*`).
pub fn compute_visible_transcript(
    view_lines: &VecDeque<(i64, String)>,
    raw_end: usize,
    show_events: bool,
) -> Vec<(usize, i64, String, u16)> {
    let mut out: Vec<(usize, i64, String, u16)> = Vec::new();
    let mut in_cmd = false;
    let mut in_peer_turn = false;
    let mut in_peer_echo = false;
    let mut in_host_notice = false;
    let is_echo_line = |t: &str| {
        !t.starts_with(crate::leads::TAG)
            && (t == ">>"
                || t.starts_with(">> ")
                || t.strip_prefix('[')
                    .and_then(|s| s.split_once(']'))
                    .is_some_and(|(note, after)| (after.is_empty() || after.starts_with(' ')) && fold_marker(note)))
    };
    for (i, (seq, text)) in view_lines.iter().take(raw_end).enumerate() {
        if !text.starts_with(crate::leads::TAG) {
            if text == "[peer turn started]" {
                in_peer_turn = true;
                in_peer_echo = true;
                in_host_notice = false;
                in_cmd = false;
                if !show_events {
                    continue;
                }
            } else if text == "[host notice]" {
                in_host_notice = true;
                in_cmd = false;
                if !show_events {
                    continue;
                }
            }
        }
        if in_host_notice {
            if is_echo_line(text) {
                if !show_events {
                    continue;
                }
            } else {
                in_host_notice = false;
            }
        }
        if in_peer_turn {
            if in_peer_echo && !is_echo_line(text) {
                in_peer_echo = false;
            }
            if !in_peer_echo && is_user_prompt(text) {
                in_peer_turn = false;
            } else {
                let ends_turn = !text.starts_with(crate::leads::TAG)
                    && (text == "[turn completed]"
                        || text == "[turn interrupted]"
                        || text == "[session closed]"
                        || text == "[provider disconnected]"
                        || text.starts_with("[turn failed]")
                        || text.starts_with("[turn failed "));
                if ends_turn {
                    in_peer_turn = false;
                    in_peer_echo = false;
                    in_cmd = false;
                    if !show_events {
                        continue;
                    }
                } else if !show_events {
                    continue;
                }
            }
        }
        let kind = classify(text);
        if is_exit_note(text) {
            in_cmd = false;
            if !show_events && kind == LineKind::ControlEvent {
                continue;
            }
            let role = if kind == LineKind::ErrorNote {
                STYLE_EXIT_ERR | ROLE_BOX | ROLE_ERR_ROW
            } else {
                STYLE_TOOL | ROLE_BOX
            };
            out.push((i, *seq, text.clone(), role));
            continue;
        }
        if !show_events && kind == LineKind::ControlEvent {
            in_cmd = false;
            continue;
        }
        let role = if text == "$" || text.starts_with("$ ") {
            in_cmd = true;
            STYLE_CMD | ROLE_BOX
        } else if kind == LineKind::Tool {
            if starts_new_card(text) {
                in_cmd = false;
            }
            STYLE_TOOL | ROLE_BOX
        } else if is_tool_error_note(text) {
            in_cmd = false;
            STYLE_EXIT_ERR | ROLE_BOX | ROLE_ERR_ROW
        } else if in_cmd && kind == LineKind::Agent {
            STYLE_CMD_OUT | ROLE_BOX
        } else {
            in_cmd = false;
            default_role(text)
        };
        out.push((i, *seq, text.clone(), role));
    }
    for idx in 0..out.len() {
        if out[idx].3 & ROLE_BOX != 0 {
            let prev_same = idx > 0 && (out[idx - 1].3 & ROLE_BOX != 0) && !starts_new_card(&out[idx].2);
            let next_same = idx + 1 < out.len() && (out[idx + 1].3 & ROLE_BOX != 0) && !starts_new_card(&out[idx + 1].2);
            if !prev_same {
                out[idx].3 |= ROLE_BOX_TOP;
            } else {
                out[idx].3 &= !ROLE_BOX_TOP;
            }
            if !next_same {
                out[idx].3 |= ROLE_BOX_BOT;
            } else {
                out[idx].3 &= !ROLE_BOX_BOT;
            }
        }
    }
    out
}

/// Apply role-specific ink color to a shaped `AttrsList` while keeping every per-character font span intact.
#[cfg(test)]
pub fn tint(list: AttrsList, text: &str) -> AttrsList {
    tint_role(list, text, default_role(text))
}

pub fn tint_role(list: AttrsList, text: &str, role: u16) -> AttrsList {
    if super::panel::sendto::header_look(text).is_some() {
        return list; // transfer headers keep their dimmed + lit-sender look from `sendto::tint`
    }
    let style = role & STYLE_MASK;
    let col = match style {
        STYLE_USER => USER_INK,
        STYLE_CMD => CMD_TEXT_INK,
        STYLE_TOOL => TOOL_INK,
        STYLE_CMD_OUT => CMD_OUT_INK,
        STYLE_EXIT_ERR => EXIT_ERR_INK,
        STYLE_ERROR => ERROR_INK,
        _ => AGENT_INK,
    };
    let badge = match style {
        STYLE_CMD if text.starts_with("$ ") || text == "$" => Some((1, CMD_BADGE_INK)),
        STYLE_TOOL if text.starts_with("[tool]") => Some((6, TOOL_BADGE_INK)),
        STYLE_TOOL if text.starts_with("[patch]") => Some((7, TOOL_BADGE_INK)),
        STYLE_TOOL if text.starts_with("> ") || text == ">" => Some((1, TOOL_BADGE_INK)),
        _ => None,
    };
    let mut out = AttrsList::new(&list.defaults().color(col));
    for (range, attrs) in list.spans_iter() {
        out.add_span(range.clone(), &attrs.as_attrs().color(col));
    }
    if let Some((end, bcol)) = badge.filter(|&(e, _)| e <= text.len()) {
        out.add_span(0..end, &list.defaults().color(bcol));
    }
    out
}

/// Paint user prompt backgrounds (`USER_BG`), inter-session peer banners (`PEER_BG`), and bordered command/tool execution cards (`CMD_BG`).
/// During `scroll::Shift` (`cleared == Some(rows)`), only newly cleared pixel rows are painted and card borders test unclipped `raw_r0`/`raw_r1`
/// so shifted pixels match full repaints pixel-for-pixel.
#[allow(clippy::too_many_arguments)]
pub fn paint_transcript_decorations(
    buf: &mut [u32],
    width: usize,
    height: usize,
    list_w: usize,
    scale: f32,
    top_y: i32,
    hint_y: i32,
    hit_lines: &[(f32, f32, usize, u16)],
    cleared: Option<&[(i32, i32)]>,
) {
    let x0 = list_w.min(width);
    if x0 >= width {
        return;
    }
    let s = scale.max(1.0);
    let pad_l = (1.0 * s).round() as usize;
    let pad_r = (2.0 * s).round() as usize;
    let rail_w = (2.0 * s).round().max(2.0) as usize;
    let bx0 = (x0 + pad_l).min(width);
    let bx1 = width.saturating_sub(pad_r).max(bx0);
    for &(y0, y1, _raw_idx, role) in hit_lines {
        if role & (ROLE_USER_BG | ROLE_PEER_BG | ROLE_BOX) == 0 {
            continue;
        }
        let raw_r0 = y0.floor() as i32;
        let raw_r1 = y1.floor() as i32;
        let r0 = raw_r0.max(top_y).max(0);
        let r1 = raw_r1.min(hint_y).min(height as i32);
        for py in r0..r1 {
            if let Some(ranges) = cleared {
                if !ranges.iter().any(|&(a, b)| py >= a && py < b) {
                    continue;
                }
            }
            let row_start = py as usize * width;
            if role & ROLE_USER_BG != 0 {
                buf[row_start + x0..row_start + width].fill(USER_BG);
            } else if role & ROLE_PEER_BG != 0 {
                if bx0 < bx1 {
                    let is_edge = py == raw_r0 || py == raw_r1 - 1;
                    buf[row_start + bx0..row_start + bx1].fill(if is_edge { PEER_BORDER } else { PEER_BG });
                    let rx1 = (bx0 + rail_w).min(bx1);
                    buf[row_start + bx0..row_start + rx1].fill(PEER_RAIL);
                    if bx1 > bx0 + 1 {
                        buf[row_start + bx1 - 1] = PEER_BORDER;
                    }
                }
            } else if role & ROLE_BOX != 0 && bx0 < bx1 {
                let err_row = role & ROLE_ERR_ROW != 0;
                let top_edge = (role & ROLE_BOX_TOP != 0) && py == raw_r0;
                let bot_edge = (role & ROLE_BOX_BOT != 0) && py == raw_r1 - 1;
                let fill_col = if top_edge || bot_edge {
                    CMD_BORDER
                } else if err_row {
                    CMD_ERR_BG
                } else {
                    CMD_BG
                };
                buf[row_start + bx0..row_start + bx1].fill(fill_col);
                let rx1 = (bx0 + rail_w).min(bx1);
                buf[row_start + bx0..row_start + rx1].fill(if err_row { CMD_ERR_RAIL } else { CMD_RAIL });
                if bx1 > bx0 + 1 {
                    buf[row_start + bx1 - 1] = CMD_BORDER;
                }
            }
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

    pub(crate) fn visible_transcript_lines(&self, raw_end: usize) -> Vec<(usize, i64, String, u16)> {
        let Some(view) = self.view.as_ref() else { return Vec::new() };
        compute_visible_transcript(&view.lines, raw_end, self.ui.show_events)
    }

    pub(crate) fn shown_lines_dump(&self) -> Vec<String> {
        let Some(view) = self.view.as_ref() else { return Vec::new() };
        compute_visible_transcript(&view.lines, view.lines.len(), self.ui.show_events)
            .into_iter()
            .map(|(_, _, text, _)| text)
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

    #[test]
    fn command_cards_stdout_boundaries_exit_errors_mcp_tool_cards_and_peer_banners_are_grouped_per_invocation() {
        let raw: VecDeque<(i64, String)> = [
            ">> 지금은 lico 다른 메신저와 대화 할 수 있나?",
            "$ /bin/bash -lc 'cat SKILL.md'",
            "skill header line 1",
            "skill header line 2",
            "[exit 0]",
            "현재 제게 보이는 맥락에는 다른 세션 대화가 들어 있지 않아요.",
            "$ /bin/bash -lc 'lico --help'",
            "$ /bin/bash -lc 'lico session list'",
            "[exit 1]",
            "[tool] mcp__lico__sessions",
            "> {\"sessions\":[{\"id\":\"se76be5808152e8c8\",\"kind\":\"codex\"}]}",
            "[tool] mcp__lico__send",
            "[tool error]",
            "> denied by user",
            ">> [from session 'codex_initial' (se76be5) | 14:05 | message | 18 B]",
            ">> 안녕 Claude! 잘 들리니?",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, s)| (i as i64, s.to_string()))
        .collect();

        let vis = compute_visible_transcript(&raw, raw.len(), false);
        let by_text = |needle: &str| vis.iter().find(|(_, _, t, _)| t == needle).expect(needle).3;

        // User prompt vs incoming peer session banner
        assert_eq!(by_text(">> 지금은 lico 다른 메신저와 대화 할 수 있나?"), STYLE_USER | ROLE_USER_BG);
        assert_eq!(by_text(">> [from session 'codex_initial' (se76be5) | 14:05 | message | 18 B]"), STYLE_USER | ROLE_PEER_BG);

        // First `$ ...` command + stdout lines form one card; hidden `[exit 0]` closes `in_cmd` so prose stays unboxed!
        assert_eq!(by_text("$ /bin/bash -lc 'cat SKILL.md'"), STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP);
        assert_eq!(by_text("skill header line 1"), STYLE_CMD_OUT | ROLE_BOX);
        assert_eq!(by_text("skill header line 2"), STYLE_CMD_OUT | ROLE_BOX | ROLE_BOX_BOT);
        assert!(vis.iter().all(|(_, _, t, _)| t != "[exit 0]"));
        assert_eq!(by_text("현재 제게 보이는 맥락에는 다른 세션 대화가 들어 있지 않아요."), STYLE_AGENT);

        // Back-to-back `$ ...` invocations split into separate cards, and `[exit 1]` gets error row + bottom border
        assert_eq!(by_text("$ /bin/bash -lc 'lico --help'"), STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP | ROLE_BOX_BOT);
        assert_eq!(by_text("$ /bin/bash -lc 'lico session list'"), STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP);
        assert_eq!(by_text("[exit 1]"), STYLE_EXIT_ERR | ROLE_BOX | ROLE_BOX_BOT | ROLE_ERR_ROW);

        // Consecutive `[tool] ...` invocations also split into distinct cards, with `[tool error]` highlighted
        assert_eq!(by_text("[tool] mcp__lico__sessions"), STYLE_TOOL | ROLE_BOX | ROLE_BOX_TOP);
        assert_eq!(by_text("> {\"sessions\":[{\"id\":\"se76be5808152e8c8\",\"kind\":\"codex\"}]}"), STYLE_TOOL | ROLE_BOX | ROLE_BOX_BOT);
        assert_eq!(by_text("[tool] mcp__lico__send"), STYLE_TOOL | ROLE_BOX | ROLE_BOX_TOP);
        assert_eq!(by_text("[tool error]"), STYLE_EXIT_ERR | ROLE_BOX | ROLE_ERR_ROW);
        assert_eq!(by_text("> denied by user"), STYLE_TOOL | ROLE_BOX | ROLE_BOX_BOT);

        let base = AttrsList::new(&cosmic_text::Attrs::new());
        assert_eq!(tint(base.clone(), "$ echo hi").defaults().color_opt, Some(CMD_TEXT_INK));
        assert_eq!(tint_role(base, "skill header line 1", by_text("skill header line 1")).defaults().color_opt, Some(CMD_OUT_INK));
    }

    #[test]
    fn multiline_peer_turn_and_host_notice_echo_are_hidden_until_turn_completes_unless_events_shown() {
        let raw: VecDeque<(i64, String)> = [
            ">> 사용자가 직접 질문한 항목",
            "사람 질문에 대한 답변입니다.",
            "[turn completed]",
            "[peer turn started]",
            ">> [from session 'ClaudeMain' (sd220fe) | 05:31 | message | 121 B]",
            ">> 안녕하세요 Codex! 첫 줄입니다.",
            ">> 두 번째 줄입니다.",
            "[turn started]",
            "반갑습니다 Claude!",
            "[turn completed]",
            "[host notice]",
            ">> [peer reply from 'CodexWorker' (s999999) | 05:31 | 24 B]",
            ">> 반갑습니다 Claude!",
            "Codex로부터 답장이 도착했습니다.",
            "[turn completed]",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, s)| (i as i64, s.to_string()))
        .collect();

        let hidden: Vec<String> = compute_visible_transcript(&raw, raw.len(), false).into_iter().map(|(_, _, t, _)| t).collect();
        assert_eq!(
            hidden,
            vec![
                ">> 사용자가 직접 질문한 항목",
                "사람 질문에 대한 답변입니다.",
                "Codex로부터 답장이 도착했습니다.",
            ]
        );
        let shown = compute_visible_transcript(&raw, raw.len(), true);
        assert_eq!(shown.len(), raw.len());
    }
}
