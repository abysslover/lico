//! A click on a block of the conversation selects all of it, and a second click folds it (`M24-P11c`, the owner's words of 2026-11-24).
//!
//! The fold of the cards of commands and tools lives in `tool_fold.rs`: a card is folded to begin with and the keys that are in the set are the ones the person opened. Here it is the other way
//! round: a block of the person, of a peer or of the agent is open to begin with, and the set holds the keys `(session, seq of the first line)` of the blocks the person folded. A folded block is one
//! line of the visible transcript, `▸ the first words  / 12 lines`, with the raw index, the seq and the colour of the block's first line, so that its time (P11b) is the first line's.

use std::collections::HashSet;

use super::blocks::{block_span, kind_of, Kind};
use super::tool_fold::{clip_chars, PILL_HEAD_CHARS};
use super::transcript::{ROLE_BLOCK_FOLDED, ROLE_BOX_BOT, ROLE_BOX_TOP};
use super::App;

type Entry = (usize, i64, String, u16);

/// The most blocks kept folded: past it the set starts again (as the set of the opened cards does).
pub(super) const MAX_FOLDED: usize = 512;

/// What the click needs to remember, and the blocks that are folded.
#[derive(Default)]
pub struct State {
    /// `(session, seq of the first line)` of every block the person folded.
    pub folded: HashSet<(String, i64)>,
    /// The selection as it was when the button went down (a press puts the pressed line in its place, so that a drag can start there); the first or the second click is told by it.
    pub before: Option<(usize, usize)>,
}

impl State {
    /// For the state dump: how many blocks of `session` are folded, and the selection.
    pub fn json(&self, session: &str, sel: Option<(usize, usize)>) -> serde_json::Value {
        serde_json::json!({"folded": self.folded.iter().filter(|(s, _)| s == session).count(), "sel": sel})
    }
}

/// Whether the lines of one block have anything to fold: two lines or more, or one that is longer than the pill is wide; and a block that is folded already, to be opened.
pub(super) fn foldable(block: &[Entry]) -> bool {
    block.len() >= 2 || block.first().is_some_and(|line| line.3 & ROLE_BLOCK_FOLDED != 0 || line.2.trim().chars().count() > PILL_HEAD_CHARS)
}

/// The line a folded block shows: `▸ ` and the first words, then how many lines are folded.
pub(super) fn pill(head: &str, lines: usize) -> String {
    let head = clip_chars(head.trim(), PILL_HEAD_CHARS);
    format!("\u{25b8} {head}  / {lines} line{}", if lines == 1 { "" } else { "s" })
}

/// The visible lines with the blocks that are in `folded` (for `session`) made one line each.
pub(super) fn apply(entries: Vec<Entry>, folded: &HashSet<(String, i64)>, session: &str) -> Vec<Entry> {
    if folded.is_empty() {
        return entries;
    }
    let roles: Vec<u16> = entries.iter().map(|line| line.3).collect();
    let mut key = (session.to_string(), 0i64);
    let mut out = Vec::with_capacity(entries.len());
    let mut at = 0;
    while at < entries.len() {
        let last = block_span(&roles, at).map_or(at, |span| span.1);
        let block = &entries[at..=last];
        let kind = kind_of(block[0].3);
        key.1 = block[0].1;
        if matches!(kind, Kind::User | Kind::Peer | Kind::Response) && foldable(block) && folded.contains(&key) {
            let (raw, seq, ref head, role) = block[0];
            let borders = if kind == Kind::Peer { ROLE_BOX_TOP | ROLE_BOX_BOT } else { 0 };
            out.push((raw, seq, pill(head, block.len()), role | ROLE_BLOCK_FOLDED | borders));
        } else {
            out.extend_from_slice(block);
        }
        at = last + 1;
    }
    out
}

impl App {
    /// A stationary click on the line `raw_idx` of the transcript; true when it was a click on a block of the person, a peer or the agent (and is handled).
    /// A block that is not the selection becomes the selection, the whole of it (and is not copied); the block that is the selection is folded, or unfolded when it is folded.
    pub(crate) fn try_block_click(&mut self, raw_idx: usize) -> bool {
        if self.ui.show_events || self.active_call_open() || self.ui.panel.is_some() || self.session.is_empty() {
            return false;
        }
        let total = self.view.as_ref().map_or(0, |view| view.lines.len());
        let Some((vis, first, last)) = self.block_of(raw_idx, total) else { return false };
        let mut range = (vis[first].0, vis[last].0);
        if self.ui.block_fold.before == Some(range) && foldable(&vis[first..=last]) {
            let key = (self.session.clone(), vis[first].1);
            if !self.ui.block_fold.folded.remove(&key) {
                if self.ui.block_fold.folded.len() >= MAX_FOLDED {
                    self.ui.block_fold.folded.clear();
                }
                self.ui.block_fold.folded.insert(key);
            }
            if let Some((now, a, b)) = self.block_of(range.0, total) {
                range = (now[a].0, now[b].0); // the block as it shows now
            }
        }
        self.ui.sel = Some(range);
        self.ui.composed = None;
        self.dirty = true;
        true
    }

    /// The visible lines, and the first and last of those of the block of the person, a peer or the agent that holds the line `raw_idx`.
    fn block_of(&self, raw_idx: usize, total: usize) -> Option<(Vec<Entry>, usize, usize)> {
        let vis = self.visible_transcript_lines(total);
        let pos = vis.iter().position(|line| line.0 == raw_idx)?;
        let roles: Vec<u16> = vis.iter().map(|line| line.3).collect();
        if !matches!(kind_of(roles[pos]), Kind::User | Kind::Peer | Kind::Response) {
            return None;
        }
        let (first, last) = block_span(&roles, pos)?;
        Some((vis, first, last))
    }
}

#[cfg(test)]
#[path = "block_fold_tests.rs"]
mod tests;
