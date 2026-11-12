//! Find in conversation (`Ctrl+F`) and search across all sessions (`Ctrl+Shift+F`) in `lico gui` (M02-P06, S03; SR-01, SR-02).
//!
//! No index stays resident in the window or the host: queries ask the host's streaming `session.search` operation on demand, and closing the bar with `Esc`
//! releases the query and match list capacity immediately while leaving the session draft untouched.

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::super::{clean, App};

const MAX_QUERY: usize = 256;
const GUI_SEARCH_LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub session: String,
    pub name: String,
    pub seq: i64,
    pub text: String,
}

#[derive(Default)]
pub struct SearchBar {
    pub open: bool,
    pub scope_all: bool,
    pub query: String,
    pub matches: Vec<Hit>,
    pub index: usize,
    pub total: usize,
    pub pending_jump: Option<(String, i64)>,
}

impl SearchBar {
    pub fn line_kind(&self, open_session: &str, seq: i64) -> Option<bool> {
        if !self.open || self.matches.is_empty() {
            return None;
        }
        if let Some(cur) = self.matches.get(self.index) {
            if cur.session == open_session && cur.seq == seq {
                return Some(true);
            }
        }
        if self.matches.iter().any(|h| h.session == open_session && h.seq == seq) {
            Some(false)
        } else {
            None
        }
    }
}

impl App {
    pub(crate) fn open_search(&mut self, scope_all: bool) {
        let effective_all = scope_all || self.session.is_empty();
        let scope_changed = self.ui.search.open && self.ui.search.scope_all != effective_all;
        self.ui.search.open = true;
        self.ui.search.scope_all = effective_all;
        if scope_changed && !self.ui.search.query.is_empty() {
            self.run_search();
        }
        self.dirty = true;
    }

    pub(crate) fn search_close(&mut self) {
        if !self.ui.search.open && self.ui.search.matches.capacity() == 0 && self.ui.search.query.capacity() == 0 {
            return;
        }
        self.ui.search = SearchBar::default();
        crate::memory::trim_heap();
        self.dirty = true;
    }

    pub(crate) fn search_put(&mut self, text: &str) {
        let mut added = false;
        for c in text.chars() {
            if !c.is_control() && !crate::codex::dropped(c as u32) && self.ui.search.query.chars().count() < MAX_QUERY {
                self.ui.search.query.push(c);
                added = true;
            }
        }
        if added {
            self.run_search();
        }
        self.dirty = true;
    }

    pub(crate) fn run_search(&mut self) {
        let q = self.ui.search.query.clone();
        if q.is_empty() {
            self.ui.search.matches = Vec::new();
            self.ui.search.index = 0;
            self.ui.search.total = 0;
            self.ui.search.pending_jump = None;
            self.dirty = true;
            return;
        }
        if self.socket.is_empty() {
            self.dirty = true;
            return;
        }
        let mut req = json!({"op": "session.search", "query": q, "limit": GUI_SEARCH_LIMIT});
        if !self.ui.search.scope_all && !self.session.is_empty() {
            req["session"] = json!(self.session);
        }
        let reply = self.host_call_within(req, 3);
        if reply["ok"] == json!(1) {
            let hits: Vec<Hit> = reply["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    Some(Hit {
                        session: m["session"].as_str()?.to_string(),
                        name: clean(m["name"].as_str().unwrap_or("")),
                        seq: m["seq"].as_i64()?,
                        text: clean(m["text"].as_str().unwrap_or("")),
                    })
                })
                .collect();
            self.ui.search.total = reply["total"].as_u64().unwrap_or(hits.len() as u64) as usize;
            // Start on the newest match in a conversation search, or the first match in all-sessions search.
            self.ui.search.index = if !self.ui.search.scope_all && !hits.is_empty() { hits.len() - 1 } else { 0 };
            self.ui.search.matches = hits;
            if !self.ui.search.matches.is_empty() {
                self.jump_to_current_hit();
            }
        }
        self.dirty = true;
    }

    pub(crate) fn search_step(&mut self, delta: isize) {
        let len = self.ui.search.matches.len();
        if len == 0 {
            if !self.ui.search.query.is_empty() {
                self.run_search();
            }
            return;
        }
        let next = (self.ui.search.index as isize + delta).rem_euclid(len as isize) as usize;
        self.ui.search.index = next;
        self.jump_to_current_hit();
        self.dirty = true;
    }

    pub(crate) fn jump_to_current_hit(&mut self) {
        let Some(hit) = self.ui.search.matches.get(self.ui.search.index).cloned() else { return };
        if hit.session != self.session {
            self.ui.search.pending_jump = Some((hit.session.clone(), hit.seq));
            self.switch_to(&hit.session);
        } else {
            self.ui.search.pending_jump = None;
            self.jump_to_seq_in_open_session(hit.seq);
        }
    }

    pub(crate) fn apply_pending_search_jump(&mut self) {
        let Some((target, seq)) = self.ui.search.pending_jump.clone() else { return };
        if target == self.session {
            self.ui.search.pending_jump = None;
            self.jump_to_seq_in_open_session(seq);
        }
    }

    pub(crate) fn jump_to_seq_in_open_session(&mut self, seq: i64) {
        let already = self.view.as_ref().is_some_and(|v| v.lines.iter().any(|(s, _)| *s == seq));
        if !already && !self.socket.is_empty() && !self.session.is_empty() {
            let reply = self.host_call_within(json!({"op": "history", "session": self.session, "before": seq.saturating_add(25), "limit": 80}), 3);
            if reply["ok"] == json!(1) {
                let entries: Vec<(i64, String)> = reply["lines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| Some((e[0].as_i64()?, clean(e[1].as_str()?))))
                    .collect();
                if !entries.is_empty() {
                    self.ui.oldest_known = reply["oldest"].as_i64().unwrap_or(0);
                    self.ui.history_pending = false;
                    self.ui.behind = entries.last().is_some_and(|(last_seq, _)| *last_seq < self.cursor);
                    if let Some(view) = self.view.as_mut() {
                        view.lines = entries.into_iter().collect();
                    }
                }
            }
        }
        if let Some(view) = self.view.as_ref() {
            let len = view.lines.len();
            if let Some(pos) = view.lines.iter().position(|(s, _)| *s == seq) {
                let target_end = (pos + 4).min(len);
                self.ui.scroll = len.saturating_sub(target_end);
            }
        }
        self.dirty = true;
    }

    /// Key handling for `Ctrl+F` / `Ctrl+Shift+F` and for the open search bar; true when taken.
    pub(crate) fn search_key(&mut self, event: &KeyEvent) -> bool {
        if self.ui.panel.is_some() {
            return false;
        }
        let (ctrl, shift, alt) = (self.ui.mods.control_key(), self.ui.mods.shift_key(), self.ui.mods.alt_key());
        let letter = super::super::key_letter(event);
        if ctrl && !alt && letter == Some('f') {
            if !event.repeat {
                if shift {
                    self.open_search(true);
                } else if self.session.is_empty() {
                    self.open_search(true);
                } else {
                    self.open_search(false);
                }
            }
            return true;
        }
        if !self.ui.search.open {
            return false;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.search_close();
                true
            }
            Key::Named(NamedKey::Tab) if !ctrl && !alt => {
                if !self.session.is_empty() {
                    self.ui.search.scope_all = !self.ui.search.scope_all;
                    self.run_search();
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::F3) => {
                self.search_step(if shift { -1 } else { 1 });
                true
            }
            Key::Named(NamedKey::ArrowDown) if !ctrl && !alt => {
                self.search_step(1);
                true
            }
            Key::Named(NamedKey::ArrowUp) if !ctrl && !alt => {
                self.search_step(-1);
                true
            }
            Key::Named(NamedKey::Backspace) if !ctrl && !alt => {
                if self.ui.search.query.pop().is_some() {
                    self.run_search();
                }
                self.dirty = true;
                true
            }
            _ if ctrl && !shift && !alt && letter == Some('u') => {
                if !self.ui.search.query.is_empty() {
                    self.ui.search.query.clear();
                    self.run_search();
                }
                true
            }
            Key::Named(NamedKey::PageUp | NamedKey::PageDown | NamedKey::End) => false,
            _ if ctrl && shift && !alt && letter == Some('c') => false,
            _ if ctrl || alt => true,
            _ => {
                let typed = event.text.as_deref().filter(|t| !t.is_empty()).or_else(|| match &event.logical_key {
                    Key::Character(c) => Some(c.as_str()),
                    Key::Named(NamedKey::Space) => Some(" "),
                    _ => None,
                });
                if let Some(t) = typed {
                    self.search_put(t);
                }
                true
            }
        }
    }

    pub(crate) fn search_bar_line(&self) -> Option<String> {
        if !self.ui.search.open {
            return None;
        }
        let scope = if self.ui.search.scope_all { "Search all sessions" } else { "Find in conversation" };
        let count = if self.ui.search.query.is_empty() {
            "type to search".to_string()
        } else if self.ui.search.matches.is_empty() {
            "0 matches".to_string()
        } else {
            let cur = &self.ui.search.matches[self.ui.search.index];
            let suffix = if self.ui.search.total > self.ui.search.matches.len() {
                format!(" (first {} of {})", self.ui.search.matches.len(), self.ui.search.total)
            } else {
                String::new()
            };
            if self.ui.search.scope_all {
                format!("{} / {}{}  [{} #{}]", self.ui.search.index + 1, self.ui.search.matches.len(), suffix, cur.name, cur.seq)
            } else {
                format!("{} / {}{}", self.ui.search.index + 1, self.ui.search.matches.len(), suffix)
            }
        };
        let tab_hint = if self.session.is_empty() { "" } else { "   Tab: scope" };
        Some(format!("{scope}: {}_   ({count})   Enter/Shift+Enter: next/prev{tab_hint}   Esc: close", self.ui.search.query))
    }

    /// Returns `Some(true)` for the active match line in `session`, `Some(false)` for another match in `session`, else `None`.
    #[cfg(test)]
    pub(crate) fn search_line_kind(&self, seq: i64) -> Option<bool> {
        self.ui.search.line_kind(&self.session, seq)
    }

    pub(crate) fn search_json(&self) -> Value {
        if !self.ui.search.open {
            return Value::Null;
        }
        let current = self.ui.search.matches.get(self.ui.search.index).map(|h| json!({"session": h.session, "name": h.name, "seq": h.seq, "text": h.text}));
        json!({
            "open": true,
            "scope": if self.ui.search.scope_all { "all" } else { "session" },
            "query": self.ui.search.query,
            "count": self.ui.search.matches.len(),
            "total": self.ui.search.total,
            "index": self.ui.search.index,
            "current": current,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::scroll_oracle::app;
    use super::*;

    #[test]
    fn search_bar_keeps_session_draft_untouched_and_releases_buffers_on_close() {
        let mut a = app(1.0, false);
        a.ed.put("important draft");
        a.open_search(false);
        a.search_put("needle");
        a.ui.search.matches = vec![
            Hit { session: "session-s0".into(), name: "api".into(), seq: 10, text: "first needle".into() },
            Hit { session: "session-s0".into(), name: "api".into(), seq: 42, text: "second needle".into() },
        ];
        a.ui.search.total = 2;
        a.ui.search.index = 0;
        assert_eq!(a.ed.text(), "important draft");
        assert_eq!(a.search_line_kind(10), Some(true));
        assert_eq!(a.search_line_kind(42), Some(false));
        assert_eq!(a.search_line_kind(99), None);

        a.search_step(1);
        assert_eq!(a.ui.search.index, 1);
        assert_eq!(a.search_line_kind(42), Some(true));

        a.search_close();
        assert!(!a.ui.search.open);
        assert_eq!(a.ui.search.query.capacity(), 0);
        assert_eq!(a.ui.search.matches.capacity(), 0);
        assert_eq!(a.ed.text(), "important draft");
    }
}
