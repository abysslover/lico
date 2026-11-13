//! Collapsible per-invocation tool, shell command, and patch cards in session transcripts (`M09-P01`).
//!
//! By default (`!show_events` and outside synthetic Call Room views), each contiguous `ROLE_BOX`
//! invocation (`[tool] ...`, `$ ...`, `[patch] ...` + output / error lines) folds into a 1-line
//! clickable summary pill (`▸ ...`). A stationary mouse click on a collapsed pill expands it to
//! `▾ ...` + full body, and a stationary click on an expanded card folds it back up while leaving
//! mouse drag-selection and auto-copy intact.

use std::collections::HashSet;

use super::transcript::{
    compute_visible_transcript, ROLE_BOX, ROLE_BOX_BOT, ROLE_BOX_TOP, ROLE_ERR_ROW,
    ROLE_TOOL_FOLDED, ROLE_TOOL_HEAD, STYLE_EXIT_ERR, STYLE_TOOL_FOLD, STYLE_TOOL_HEAD,
};
use super::App;

const MAX_EXPANDED: usize = 512;
const PILL_HEAD_CHARS: usize = 84;

fn clip_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        text.to_string()
    } else {
        let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}\u{2026}", kept.trim_end())
    }
}

pub fn format_folded_pill(head_text: &str, span_len: usize, has_err: bool) -> String {
    let head = clip_chars(head_text.trim(), PILL_HEAD_CHARS);
    if has_err {
        if span_len > 1 {
            format!("\u{25b8} {head}  \u{00b7} error ({span_len} lines)")
        } else {
            format!("\u{25b8} {head}  \u{00b7} error")
        }
    } else if span_len > 1 {
        format!("\u{25b8} {head}  \u{00b7} {span_len} lines")
    } else {
        format!("\u{25b8} {head}")
    }
}

fn find_box_end(entries: &[(usize, i64, String, u16)], start: usize) -> usize {
    let mut end = start;
    while end + 1 < entries.len()
        && (entries[end].3 & ROLE_BOX_BOT == 0)
        && (entries[end + 1].3 & ROLE_BOX != 0)
        && (entries[end + 1].3 & ROLE_BOX_TOP == 0)
    {
        end += 1;
    }
    end
}

pub fn apply(
    entries: Vec<(usize, i64, String, u16)>,
    expanded: &HashSet<(String, i64)>,
    session: &str,
) -> Vec<(usize, i64, String, u16)> {
    let mut out = Vec::with_capacity(entries.len());
    let mut i = 0usize;
    while i < entries.len() {
        if entries[i].3 & ROLE_BOX == 0 {
            out.push(entries[i].clone());
            i += 1;
            continue;
        }
        let end = find_box_end(&entries, i);
        let (head_raw, head_seq, ref head_text, head_role) = entries[i];
        let span = &entries[i..=end];
        let has_err = span.iter().any(|e| e.3 & ROLE_ERR_ROW != 0);
        if expanded.contains(&(session.to_string(), head_seq)) {
            let exp_head = format!("\u{25be} {head_text}");
            let mut r = if head_role & ROLE_ERR_ROW != 0 {
                STYLE_EXIT_ERR | ROLE_ERR_ROW
            } else {
                STYLE_TOOL_HEAD
            } | ROLE_BOX | ROLE_BOX_TOP | ROLE_TOOL_HEAD;
            if i == end {
                r |= ROLE_BOX_BOT;
            }
            out.push((head_raw, head_seq, exp_head, r));
            for item in &entries[i + 1..=end] {
                out.push(item.clone());
            }
        } else {
            let pill = format_folded_pill(head_text, span.len(), has_err);
            let style = if has_err {
                STYLE_EXIT_ERR | ROLE_ERR_ROW
            } else {
                STYLE_TOOL_FOLD
            };
            out.push((
                head_raw,
                head_seq,
                pill,
                style | ROLE_BOX | ROLE_BOX_TOP | ROLE_BOX_BOT | ROLE_TOOL_FOLDED,
            ));
        }
        i = end + 1;
    }
    out
}

pub fn card_head_seq_for_raw(entries: &[(usize, i64, String, u16)], raw_idx: usize) -> Option<i64> {
    let pos = entries.iter().position(|e| e.0 == raw_idx && (e.3 & ROLE_BOX != 0))?;
    let mut start = pos;
    while start > 0
        && (entries[start].3 & ROLE_BOX_TOP == 0)
        && (entries[start - 1].3 & ROLE_BOX != 0)
        && (entries[start - 1].3 & ROLE_BOX_BOT == 0)
    {
        start -= 1;
    }
    Some(entries[start].1)
}

impl App {
    pub(crate) fn try_toggle_tool_card(&mut self, raw_idx: usize) -> bool {
        if self.ui.show_events || self.active_call_open() || self.ui.panel.is_some() || self.session.is_empty() {
            return false;
        }
        let head_seq = {
            let Some(view) = self.view.as_ref() else { return false };
            let vis = compute_visible_transcript(&view.lines, view.lines.len(), false);
            card_head_seq_for_raw(&vis, raw_idx)
        };
        let Some(seq) = head_seq else { return false };
        let key = (self.session.clone(), seq);
        if !self.ui.expanded_tools.remove(&key) {
            if self.ui.expanded_tools.len() >= MAX_EXPANDED {
                self.ui.expanded_tools.clear();
            }
            self.ui.expanded_tools.insert(key);
        }
        self.ui.sel = None;
        self.ui.composed = None;
        self.dirty = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn tool_and_command_cards_collapse_by_default_and_expand_when_toggled_by_head_seq() {
        let raw: VecDeque<(i64, String)> = [
            ">> 안녕하세요",
            "[tool] mcp__lico__send",
            "> [to sf87442] 날씨 알려줘",
            "> {\"sent\":true}",
            "응답을 기다리는 중입니다.",
            "$ cargo test -q",
            "test result: FAILED",
            "[exit 1]",
            "[patch] 2 change(s)",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, s)| ((i + 10) as i64, s.to_string()))
        .collect();

        let base = compute_visible_transcript(&raw, raw.len(), false);
        let mut expanded = HashSet::new();
        let folded = apply(base.clone(), &expanded, "s1");
        let texts: Vec<&str> = folded.iter().map(|e| e.2.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                ">> 안녕하세요",
                "\u{25b8} [tool] mcp__lico__send  \u{00b7} 3 lines",
                "응답을 기다리는 중입니다.",
                "\u{25b8} $ cargo test -q  \u{00b7} error (3 lines)",
                "\u{25b8} [patch] 2 change(s)",
            ]
        );
        assert_ne!(folded[1].3 & ROLE_TOOL_FOLDED, 0);
        assert_ne!(folded[3].3 & ROLE_ERR_ROW, 0);

        // Toggle `[tool] mcp__lico__send` (raw_idx = 1 -> seq = 11)
        let seq = card_head_seq_for_raw(&base, 1).unwrap();
        assert_eq!(seq, 11);
        expanded.insert(("s1".to_string(), seq));
        let opened = apply(base.clone(), &expanded, "s1");
        let opened_texts: Vec<&str> = opened.iter().map(|e| e.2.as_str()).collect();
        assert_eq!(
            opened_texts,
            vec![
                ">> 안녕하세요",
                "\u{25be} [tool] mcp__lico__send",
                "> [to sf87442] 날씨 알려줘",
                "> {\"sent\":true}",
                "응답을 기다리는 중입니다.",
                "\u{25b8} $ cargo test -q  \u{00b7} error (3 lines)",
                "\u{25b8} [patch] 2 change(s)",
            ]
        );
        assert_ne!(opened[1].3 & ROLE_TOOL_HEAD, 0);
        // Clicking any line inside the expanded card resolves back to the same `head_seq = 11`!
        assert_eq!(card_head_seq_for_raw(&base, 2), Some(11));
        assert_eq!(card_head_seq_for_raw(&base, 3), Some(11));
        assert_eq!(card_head_seq_for_raw(&base, 4), None);
    }
}
