//! The whole text behind a card of the host's own (S04b), and the answers to such a card. A card says what an agent asks for in one line; a person who has
//! to allow it reads what it would send, page by page, before they answer. The host keeps the text (`approval.text`); this asks it a page at a time, so that
//! the window holds one page, and shows it cleaned: the text is an agent's, shown and never obeyed.

use serde_json::{json, Value};
use winit::keyboard::NamedKey;

use super::super::editor::{hidden, wide};
use super::sendto::failure;
use super::{Panel, Step};
use crate::gui::App;
use crate::pending::PAGE;

/// Lines of the text shown at a time, and the columns a line is wrapped at.
pub const SHOWN: usize = 14;
pub const WIDE: usize = 96;

const KEYS: &str = "Y allow  N deny  S allow for the session";

/// The text read so far: the page that is shown, and where the pages before it start.
pub struct View {
    pub id: i64,
    pub card: String,
    pub earlier: Vec<usize>,
    pub start: usize,
    pub end: usize,
    pub next: Option<usize>,
    pub total: usize,
    pub top: usize,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Move {
    Line(isize),
    Page(isize),
    Home,
    End,
}

/// One page as the host gave it.
struct Page {
    start: usize,
    end: usize,
    next: Option<usize>,
    total: usize,
    lines: Vec<String>,
}

/// The text of a page as lines of the view: a tab is spaces, what is not drawn is dropped (the escape of a colour code, a direction override), a line
/// is cut at `WIDE` columns and goes on in the next one, and a page that ends at the end of a line has no empty line after it.
fn wrapped(text: &str) -> Vec<String> {
    let mut parts: Vec<&str> = text.split('\n').collect();
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    let mut out = Vec::new();
    for part in parts {
        let (mut line, mut cols) = (String::new(), 0);
        for c in part.chars().flat_map(|c| if c == '\t' { vec![' '; 4] } else { vec![c] }) {
            if c.is_control() || hidden(c) {
                continue;
            }
            let w = if wide(c) { 2 } else { 1 };
            if cols + w > WIDE {
                out.push(std::mem::take(&mut line));
                cols = 0;
            }
            line.push(c);
            cols += w;
        }
        out.push(line);
    }
    out
}

/// What the person is told of an answer that the host took.
fn answered_words(reply: &Value, keep: bool) -> String {
    match (reply["outcome"].as_str(), reply["reason"].as_str()) {
        (Some("done"), _) if keep => "Allowed for the session: the text was sent, and later sends to that session are not asked.".into(),
        (Some("done"), _) => "Allowed: the text was sent.".into(),
        (Some("declined"), _) => "Declined: nothing was sent.".into(),
        (Some("not_done"), reason) => format!("{}: {}.", if keep { "Allowed for the session, but not sent" } else { "Allowed, but not sent" }, failure(reason.unwrap_or("bad_request"))),
        _ => "Answered.".into(),
    }
}

impl App {
    /// The first request of the open session, when it is a question of the host's own (the one Y and N answer, and the only one with a text of ours).
    pub(crate) fn lico_card(&self) -> Option<i64> {
        let row = self.selected_row()?;
        let (id, _) = row.approvals.first()?;
        row.asks.contains(id).then_some(*id)
    }

    /// The keys offered beside the input line for the first request.
    pub(crate) fn approval_hint(&self) -> &'static str {
        if self.lico_card().is_some() {
            "Y allow  N deny  S allow for the session  V read it all"
        } else {
            "Y allow  N deny"
        }
    }

    /// A key of the open session's list while it has a request: true when it was one of the answers (S and V only for a request of the host's own).
    pub(crate) fn answer_key(&mut self, key: &str) -> bool {
        if self.selected_row().map_or(true, |r| r.approvals.is_empty()) {
            return false;
        }
        let ours = self.lico_card().is_some();
        match key.to_ascii_lowercase().as_str() {
            "y" => self.answer_approval(true),
            "n" => self.answer_approval(false),
            "s" if ours => self.answer_card(true, true),
            "v" if ours => self.card_open(),
            _ => return false,
        }
        true
    }

    /// The answer to the first request when it is the host's own.
    pub(crate) fn answer_card(&mut self, allow: bool, keep: bool) {
        if let Some(id) = self.lico_card() {
            self.answer_id(id, allow, keep);
        }
    }

    /// The answer to one request, asked of the host on a connection of its own: what is said of it is what the host did, and the request is dropped from
    /// the row only when the host took the answer (or says it is gone), so that one that was not answered is asked again.
    fn answer_id(&mut self, id: i64, allow: bool, keep: bool) {
        let generation = self.selected_row().map_or(1, |r| r.generation);
        let mut request = json!({"op": "approval.answer", "session": self.session, "generation": generation, "approval_id": id, "answer": if allow { "allow" } else { "deny" }});
        if keep {
            request["keep"] = json!(true);
        }
        let reply = self.host_call(request);
        if reply["ok"] == json!(1) {
            return self.card_gone(id, answered_words(&reply, keep));
        }
        match reply["error"].as_str().unwrap_or("bad_request") {
            "unknown_approval" => self.card_gone(id, format!("Request {id} is not waiting any more.")),
            "expired" => self.card_gone(id, format!("Request {id} ran out of time: nothing was sent.")),
            "host_timeout" => self.card_say(format!("Not answered: {}.", failure("host_timeout"))),
            other => self.card_say(format!("Not answered: {}; the request waits.", failure(other))),
        }
    }

    /// A line for the person: in the panel when one is open, else beside the input line.
    fn card_say(&mut self, text: String) {
        match self.ui.panel.as_mut() {
            Some(p) => p.notice = text,
            None => self.ui.flash = Some(text),
        }
        self.dirty = true;
    }

    /// The request does not wait any more (answered, expired, or answered somewhere else): the row forgets it, the view is closed, and the person is told.
    fn card_gone(&mut self, id: i64, said: String) {
        if let Some(row) = self.ui.rows.iter_mut().find(|r| r.session == self.session) {
            row.approvals.retain(|a| a.0 != id);
            row.asks.retain(|a| *a != id);
        }
        if self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Card) {
            self.panel_close();
        }
        self.ui.flash = Some(said);
        self.refresh_list();
        self.dirty = true;
    }

    /// The request the view is of, if it still waits; if not, the view is closed.
    fn card_current(&mut self) -> Option<i64> {
        let id = self.ui.panel.as_ref().filter(|p| p.step == Step::Card).and_then(|p| p.card.as_ref()).map(|v| v.id)?;
        if self.selected_row().is_some_and(|r| r.approvals.iter().any(|a| a.0 == id)) {
            return Some(id);
        }
        self.card_gone(id, format!("Request {id} is not waiting any more."));
        None
    }

    /// One page of the text, asked from `offset`; what the host refuses is said and nothing changes.
    fn card_fetch(&mut self, id: i64, offset: usize) -> Option<Page> {
        let generation = self.selected_row()?.generation;
        let reply = self.host_call(json!({"op": "approval.text", "session": self.session, "generation": generation, "approval_id": id, "offset": offset}));
        let (Some(text), Some(total), true) = (reply["text"].as_str(), reply["total"].as_u64(), reply["ok"] == json!(1)) else {
            match reply["error"].as_str().unwrap_or("bad_request") {
                "unknown_approval" => self.card_gone(id, format!("Request {id} is not waiting any more.")),
                "expired" => self.card_gone(id, format!("Request {id} ran out of time: nothing was sent.")),
                other => self.card_say(format!("Not shown: {}.", failure(other))),
            }
            return None;
        };
        Some(Page { start: offset, end: offset + text.len(), next: reply["next"].as_u64().map(|n| n as usize), total: total as usize, lines: wrapped(text) })
    }

    /// V: the text behind the first request, from its start.
    pub(crate) fn card_open(&mut self) {
        let Some(id) = self.lico_card() else { return };
        let Some(card) = self.selected_row().and_then(|r| r.approvals.iter().find(|a| a.0 == id)).map(|a| a.1.clone()) else { return };
        let Some(page) = self.card_fetch(id, 0) else { return };
        self.panel_open(Step::Card);
        if let Some(p) = self.ui.panel.as_mut() {
            p.card = Some(View { id, card, earlier: Vec::new(), start: 0, end: page.end, next: page.next, total: page.total, top: 0, lines: page.lines });
        }
    }

    /// The pages of the text: the ones before this, this, and as many as the size says come after (a page is cut at a character, so the count is a
    /// least one more than this page's number while one follows).
    fn pages(v: &View) -> usize {
        let here = v.earlier.len() + 1;
        if v.next.is_some() { v.total.div_ceil(PAGE).max(here + 1) } else { here }
    }

    pub(crate) fn card_view(&self, p: &Panel) -> (String, Vec<String>) {
        let Some(v) = p.card.as_ref() else { return ("Request".into(), Vec::new()) };
        let mut lines = vec![v.card.clone(), format!("bytes {} to {} of {} (page {} of {})", v.start, v.end, v.total, v.earlier.len() + 1, Self::pages(v)), String::new()];
        lines.extend(v.lines.iter().skip(v.top).take(SHOWN).cloned());
        lines.push(String::new());
        lines.push(format!("{KEYS}   Up Down PgUp PgDn Home End  move"));
        (format!("Request {}", v.id), lines)
    }

    /// A letter in the view answers the request that is shown (and not whichever one comes first by then).
    pub(crate) fn card_key(&mut self, key: char) {
        let Some(id) = self.card_current() else { return };
        match key {
            'y' => self.answer_id(id, true, false),
            'n' => self.answer_id(id, false, false),
            's' => self.answer_id(id, true, true),
            _ => {}
        }
    }

    /// Installs a page that was read: where it starts is what the pages after it will come back to.
    fn card_install(&mut self, page: Page, earlier: Vec<usize>, at_end: bool) {
        let Some(v) = self.ui.panel.as_mut().and_then(|p| p.card.as_mut()) else { return };
        (v.earlier, v.start, v.end, v.next, v.total) = (earlier, page.start, page.end, page.next, page.total);
        v.top = if at_end { page.lines.len().saturating_sub(SHOWN) } else { 0 };
        v.lines = page.lines;
        self.dirty = true;
    }

    pub(crate) fn card_move(&mut self, by: Move) {
        let Some(id) = self.card_current() else { return };
        let Some(v) = self.ui.panel.as_ref().and_then(|p| p.card.as_ref()) else { return };
        let (top, max, start, next, mut earlier) = (v.top, v.lines.len().saturating_sub(SHOWN), v.start, v.next, v.earlier.clone());
        let by = match by {
            Move::Line(d) => d,
            Move::Page(d) => d * SHOWN as isize,
            Move::Home if start == 0 => -(top as isize),
            Move::Home => {
                if let Some(page) = self.card_fetch(id, 0) {
                    self.card_install(page, Vec::new(), false);
                }
                return;
            }
            Move::End => {
                let (mut last, mut at, mut following) = (None, start, next);
                while let Some(n) = following.filter(|n| *n > at) {
                    let Some(page) = self.card_fetch(id, n) else { return };
                    earlier.push(at);
                    (at, following) = (n, page.next);
                    last = Some(page);
                }
                match last {
                    Some(page) => self.card_install(page, earlier, true),
                    None => self.card_set_top(max),
                }
                return;
            }
        };
        if by > 0 && top < max {
            self.card_set_top((top + by as usize).min(max));
        } else if by > 0 {
            if let Some(n) = next.filter(|n| *n > start) {
                if let Some(page) = self.card_fetch(id, n) {
                    earlier.push(start);
                    self.card_install(page, earlier, false);
                }
            }
        } else if by < 0 && top > 0 {
            self.card_set_top(top.saturating_sub(by.unsigned_abs()));
        } else if by < 0 {
            if let Some(back) = earlier.pop() {
                if let Some(page) = self.card_fetch(id, back) {
                    self.card_install(page, earlier, true);
                }
            }
        }
    }

    fn card_set_top(&mut self, top: usize) {
        if let Some(v) = self.ui.panel.as_mut().and_then(|p| p.card.as_mut()) {
            v.top = top;
        }
        self.dirty = true;
    }

    /// The keys that move the view; true when the key was one of them.
    pub(crate) fn card_named(&mut self, key: &NamedKey) -> bool {
        let by = match key {
            NamedKey::ArrowDown => Move::Line(1),
            NamedKey::ArrowUp => Move::Line(-1),
            NamedKey::PageDown => Move::Page(1),
            NamedKey::PageUp => Move::Page(-1),
            NamedKey::Home => Move::Home,
            NamedKey::End => Move::End,
            _ => return false,
        };
        self.card_move(by);
        true
    }
}

#[cfg(test)]
#[path = "cardview_tests.rs"]
mod tests;
