//! The whole text behind a card of the host's own (S04b), and the answers to such a card. A card says what an agent asks for in one line; a person who has
//! to allow it reads what it would send, page by page, before they answer. The host keeps the text (`approval.text`); this asks it a page at a time, so that
//! the window holds one page, and shows it cleaned: the text is an agent's, shown and never obeyed.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use winit::keyboard::{Key, NamedKey};

use super::super::editor::{hidden, wide};
use super::sendto::failure;
use super::{Panel, Step};
use crate::gui::App;
use crate::pending::PAGE;

/// Lines of the text shown at a time, and the columns a line is wrapped at.
pub const SHOWN: usize = 14;
pub const WIDE: usize = 96;

/// How long a card of the host's own is on screen before a bare key answers it: a key pressed while the card comes up was meant for what the person was typing.
pub const DWELL: Duration = Duration::from_millis(800);

const KEYS: &str = "Y allow  N deny  Shift+S allow for the session";
const KEYS_MAKE: &str = "Y allow  N deny"; // a session is made once for each yes: there is no yes for the session

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

/// What a request of the host's own asks for: a text sent, a session made, or the history of a session read.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Asks {
    Send,
    Make,
    Read,
    RealScreen,
    ScreenTool,
}

impl Asks {
    /// The word for what did not happen when the request ran out of time.
    fn undone(self) -> &'static str {
        match self {
            Asks::Send => "sent",
            Asks::Make => "made",
            Asks::Read => "read",
            Asks::RealScreen | Asks::ScreenTool => "granted",
        }
    }
    pub(crate) fn no_keep(self) -> bool {
        matches!(self, Asks::Make | Asks::RealScreen | Asks::ScreenTool)
    }
}

/// What the person is told of an answer that the host took.
fn answered_words(reply: &Value, keep: bool, asks: Asks) -> String {
    match (reply["outcome"].as_str(), reply["reason"].as_str()) {
        (Some("done"), _) if asks == Asks::RealScreen => "Allowed: real-screen control is active for 15 minutes (Ctrl+Alt+Esc stops).".into(),
        (Some("declined"), _) if asks == Asks::RealScreen => "Declined: real-screen control stays off.".into(),
        (Some("not_done"), reason) if asks == Asks::RealScreen => format!("Allowed, but real-screen control did not start: {}.", failure(reason.unwrap_or("bad_request"))),
        (Some("done"), _) if asks == Asks::ScreenTool => "Allowed: one request was granted.".into(),
        (Some("declined"), _) if asks == Asks::ScreenTool => "Declined: request was refused.".into(),
        (Some("not_done"), reason) if asks == Asks::ScreenTool => format!("Allowed, but not carried out: {}.", failure(reason.unwrap_or("bad_request"))),
        (Some("done"), _) if asks == Asks::Make => "Allowed: the session was made and is working on its first prompt.".into(),
        (Some("declined"), _) if asks == Asks::Make => "Declined: no session was made.".into(),
        (Some("not_done"), reason) if asks == Asks::Make => format!("Allowed, but no session was made: {}.", failure(reason.unwrap_or("bad_request"))),
        (Some("done"), _) if asks == Asks::Read && keep => "Allowed for the session: the records were handed over, and later reads of that history are not asked.".into(),
        (Some("done"), _) if asks == Asks::Read => "Allowed: the records were handed over.".into(),
        (Some("declined"), _) if asks == Asks::Read => "Declined: nothing was read.".into(),
        (Some("not_done"), reason) if asks == Asks::Read => format!("{}: {}.", if keep { "Allowed for the session, but nothing was read" } else { "Allowed, but nothing was read" }, failure(reason.unwrap_or("bad_request"))),
        (Some("done"), _) if keep => "Allowed for the session: the text was sent, and later sends to that session are not asked.".into(),
        (Some("done"), _) => "Allowed: the text was sent.".into(),
        (Some("declined"), _) => "Declined: nothing was sent.".into(),
        (Some("not_done"), reason) => format!("{}: {}.", if keep { "Allowed for the session, but not sent" } else { "Allowed, but not sent" }, failure(reason.unwrap_or("bad_request"))),
        _ => "Answered.".into(),
    }
}

impl App {
    /// Active approval target row: the open session first, or (in `lico gui` workbench tab mode) any open tab waiting for approval.
    pub(crate) fn approval_row_any_panel(&self) -> Option<&super::super::ui::Row> {
        self.selected_row()
            .filter(|r| !r.approvals.is_empty())
            .or_else(|| {
                if self.tabs_shown() {
                    self.ui.rows.iter().find(|r| !r.tab_closed && !r.approvals.is_empty())
                } else {
                    None
                }
            })
    }

    /// Approval row shown as a window modal (hidden while a full-pane panel such as `Step::Card` or `Step::Accounts` is open).
    pub(crate) fn modal_approval_row(&self) -> Option<&super::super::ui::Row> {
        if self.ui.panel.is_some() {
            None
        } else {
            self.approval_row_any_panel()
        }
    }

    /// The first request of the active approval session, when it is a question of the host's own (the one Y and N answer, and the only one with a text of ours).
    pub(crate) fn lico_card(&self) -> Option<i64> {
        let row = self.approval_row_any_panel()?;
        let (id, _) = row.approvals.first()?;
        row.asks.contains(id).then_some(*id)
    }

    /// The instant the dwell counts to (a test sets it).
    fn card_now(&self) -> Instant {
        self.ui.clock.unwrap_or_else(Instant::now)
    }

    /// A frame with the first request was shown: the dwell of that request counts from the first such frame.
    pub(crate) fn card_drawn(&mut self) {
        let Some(target_session) = self.approval_row_any_panel().map(|r| r.session.clone()) else {
            self.ui.card_seen = None;
            return;
        };
        let Some(id) = self.lico_card() else {
            self.ui.card_seen = None;
            return;
        };
        if self.ui.card_seen.as_ref().is_some_and(|(session, seen, _)| *session == target_session && *seen == id) {
            return;
        }
        self.ui.card_seen = Some((target_session, id, self.card_now()));
    }

    /// A session was selected: its card has not been on screen yet.
    pub(crate) fn card_forget(&mut self) {
        self.ui.card_seen = None;
    }

    /// After a frame was presented: a request on screen starts (or keeps) its count; one that is not (gone, or under another panel) is forgotten, so that it is
    /// new to the person when it shows again. The view of the text (V) is the card on screen.
    pub(crate) fn card_frame(&mut self, presented: bool) {
        if !presented {
            return;
        }
        if self.ui.bar.is_some() {
            self.card_drawn();
        } else if !self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Card) {
            self.card_forget();
        }
    }

    /// The window's test clock (stdin `clock ...`): `freeze` stops the instant that the dwell counts to, `advance MS` moves it on, anything else lets it run.
    pub(crate) fn card_clock(&mut self, word: Option<&str>, ms: Option<&str>) {
        match (word, ms.and_then(|n| n.parse::<u64>().ok())) {
            (Some("freeze"), _) => self.ui.clock = Some(Instant::now()),
            (Some("advance"), Some(ms)) => self.ui.clock = self.ui.clock.map(|t| t + Duration::from_millis(ms)),
            _ => self.ui.clock = None,
        }
        self.dirty = true;
    }

    /// Whether the first request is a card of the host's own that has been on screen for the dwell.
    pub(crate) fn card_ready(&self) -> bool {
        let Some(target_session) = self.approval_row_any_panel().map(|r| r.session.as_str()) else { return false };
        let Some(id) = self.lico_card() else { return false };
        self.ui.card_seen.as_ref().is_some_and(|(session, seen, at)| session == target_session && *seen == id && self.card_now().saturating_duration_since(*at) >= DWELL)
    }

    /// Whether the first request may be answered now: the question of the program at once, a card of the host's own once it has been on screen for the dwell.
    pub(crate) fn answers_live(&self) -> bool {
        self.lico_card().is_none() || self.card_ready()
    }

    /// After a second (the tick that is there anyway; no timer of its own): buttons that were drawn dimmed are drawn again once the dwell is over.
    pub(crate) fn card_tick(&mut self) {
        if self.ui.controls.dimmed && self.answers_live() {
            self.dirty = true;
        }
    }

    /// The keys offered beside the input line for the first request.
    pub(crate) fn approval_hint(&self) -> &'static str {
        if self.lico_card().is_some() {
            if self.makes_card() { "Y allow  N deny  V read it all" } else { "Y allow  N deny  Shift+S allow for the session  V read it all" }
        } else {
            "Y allow  N deny"
        }
    }

    /// Whether the first request has no session-long allowance (`create`, real-screen `computer`, or one-shot `screentools`).
    fn makes_card(&self) -> bool {
        self.lico_card().is_some_and(|id| self.card_asks(id).no_keep())
    }

    /// What request `id` asks for.
    pub(crate) fn card_asks(&self, id: i64) -> Asks {
        match self.approval_row_any_panel().or_else(|| self.selected_row()) {
            Some(r) if r.makes.contains(&id) => Asks::Make,
            Some(r) if r.reads.contains(&id) => Asks::Read,
            Some(r) if r.real_screens.contains(&id) => Asks::RealScreen,
            Some(r) if r.screen_tools.contains(&id) => Asks::ScreenTool,
            _ => Asks::Send,
        }
    }

    /// A key while a request is active: `Y` / `Allow` grants session-long permission when supported (`!no_keep()`), `N` denies, `Shift+S` also allows for the session, `V` reads all.
    pub(crate) fn answer_key(&mut self, key: &str) -> bool {
        if self.approval_row_any_panel().is_none() {
            return false;
        }
        let ours = self.lico_card().is_some();
        if ours && !self.card_ready() {
            return false;
        }
        match key {
            "y" | "Y" => self.answer_approval(true),
            "n" | "N" => self.answer_approval(false),
            "S" if ours && !self.makes_card() && self.ui.mods.shift_key() => self.answer_card(true, true),
            "v" | "V" if ours => self.card_open(),
            _ => return false,
        }
        true
    }

    pub(crate) fn answer_approval(&mut self, allow: bool) {
        let Some(row) = self.approval_row_any_panel().cloned() else { return };
        let Some((id, _)) = row.approvals.first().cloned() else { return };
        if self.lico_card() == Some(id) {
            if self.card_ready() {
                let keep = allow && !self.card_asks(id).no_keep();
                self.answer_card(allow, keep);
            }
            return;
        }
        let msg = format!(
            "{{\"op\":\"approval.answer\",\"session\":\"{}\",\"generation\":{},\"approval_id\":{},\"answer\":\"{}\"}}",
            row.session, row.generation, id, if allow { "allow" } else { "deny" }
        );
        self.send(&msg);
        if let Some(r) = self.ui.rows.iter_mut().find(|r| r.session == row.session) {
            r.approvals.remove(0);
        }
        self.refresh_list();
    }

    /// Y allows and N denies the active pending approval while the input line is empty and no panel is open.
    pub(crate) fn approval_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        if self.ui.panel.is_some() || !self.ed.is_empty() || !self.preedit.is_empty() || event.repeat {
            return false;
        }
        if self.ui.mods.control_key() || self.ui.mods.alt_key() || self.approval_row_any_panel().is_none() {
            return false;
        }
        let key_buf;
        let key_str = if let Some(ch) = crate::gui::key_letter_cased(event, self.ui.mods.shift_key()) {
            key_buf = ch.to_string();
            Some(key_buf.as_str())
        } else if let Key::Character(c) = &event.logical_key {
            Some(c.as_str())
        } else {
            None
        };
        match key_str {
            Some(c) if self.answer_key(c) => {}
            _ => return false,
        }
        self.dirty = true;
        true
    }

    /// The answer to the first request when it is the host's own.
    pub(crate) fn answer_card(&mut self, allow: bool, keep: bool) {
        if let Some(id) = self.lico_card() {
            self.answer_id(id, allow, keep);
        }
    }

    /// The answer to one request, asked of the host on a connection of its own.
    fn answer_id(&mut self, id: i64, allow: bool, keep: bool) {
        let target_row = self.approval_row_any_panel().or_else(|| self.selected_row());
        let target_session = target_row.map(|r| r.session.clone()).unwrap_or_else(|| self.session.clone());
        let generation = target_row.map_or(1, |r| r.generation);
        let asks = self.card_asks(id);
        let keep = keep && !asks.no_keep();
        let mut request = json!({"op": "approval.answer", "session": target_session, "generation": generation, "approval_id": id, "answer": if allow { "allow" } else { "deny" }});
        if keep {
            request["keep"] = json!(true);
        }
        let reply = self.host_call(request);
        if reply["ok"] == json!(1) {
            return self.card_gone_for(&target_session, id, answered_words(&reply, keep, asks));
        }
        match reply["error"].as_str().unwrap_or("bad_request") {
            "unknown_approval" => self.card_gone_for(&target_session, id, format!("Request {id} is not waiting any more.")),
            "expired" => self.card_gone_for(&target_session, id, format!("Request {id} ran out of time: nothing was {}.", asks.undone())),
            "host_timeout" => {
                self.card_say("The host has not said yet whether it took your answer; the list shows it when it has.".into());
                self.send("{\"op\":\"status\"}");
            }
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

    fn card_gone_for(&mut self, target_session: &str, id: i64, said: String) {
        if let Some(row) = self.ui.rows.iter_mut().find(|r| r.session == target_session) {
            row.approvals.retain(|a| a.0 != id);
            row.asks.retain(|a| *a != id);
            row.makes.retain(|a| *a != id);
            row.reads.retain(|a| *a != id);
            row.real_screens.retain(|a| *a != id);
            row.screen_tools.retain(|a| *a != id);
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
        let cur_session = self.session.clone();
        self.card_gone_for(&cur_session, id, format!("Request {id} is not waiting any more."));
        None
    }

    /// One page of the text, asked from `offset`; what the host refuses is said and nothing changes.
    fn card_fetch(&mut self, id: i64, offset: usize) -> Option<Page> {
        let generation = self.selected_row()?.generation;
        let cur_session = self.session.clone();
        let asks = self.card_asks(id);
        let reply = self.host_call(json!({"op": "approval.text", "session": cur_session, "generation": generation, "approval_id": id, "offset": offset}));
        let (Some(text), Some(total), true) = (reply["text"].as_str(), reply["total"].as_u64(), reply["ok"] == json!(1)) else {
            match reply["error"].as_str().unwrap_or("bad_request") {
                "unknown_approval" => self.card_gone_for(&cur_session, id, format!("Request {id} is not waiting any more.")),
                "expired" => self.card_gone_for(&cur_session, id, format!("Request {id} ran out of time: nothing was {}.", asks.undone())),
                other => self.card_say(format!("Not shown: {}.", failure(other))),
            }
            return None;
        };
        Some(Page { start: offset, end: offset + text.len(), next: reply["next"].as_u64().map(|n| n as usize), total: total as usize, lines: wrapped(text) })
    }

    /// V (or Read Full button): the text behind the first request, from its start.
    pub(crate) fn card_open(&mut self) {
        let Some(id) = self.lico_card() else { return };
        if !self.card_ready() {
            return;
        }
        let Some(target_session) = self.approval_row_any_panel().map(|r| r.session.clone()) else { return };
        if target_session != self.session {
            self.switch_to(&target_session);
        }
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
        let keys = if self.card_asks(v.id).no_keep() { KEYS_MAKE } else { KEYS };
        lines.push(format!("{keys}   Up Down PgUp PgDn Home End  move"));
        (format!("Request {}", v.id), lines)
    }

    /// A letter in the view answers the request that is shown (and not whichever one comes first by then).
    pub(crate) fn card_key(&mut self, key: char) {
        let Some(id) = self.card_current() else { return };
        let keep_default = !self.card_asks(id).no_keep();
        match key {
            'y' | 'Y' => self.answer_id(id, true, keep_default),
            'n' | 'N' => self.answer_id(id, false, false),
            'S' if self.ui.mods.shift_key() && keep_default => self.answer_id(id, true, true),
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
