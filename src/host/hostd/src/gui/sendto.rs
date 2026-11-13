//! Sending text to another session from the window: the panel (what, to whom, how it is delivered) and the look of the header that the receiving
//! window shows. The host does the work (`transfer.send`, asked over the host socket like the window's other requests, one connection for the call); this asks it and says what came of it.
//! The text of the draft and of the selection is taken when the panel opens, and the draft is cleared only when the host says it was delivered. The third source, the whole conversation
//! of the open session, is not taken here at all: the host reads the program's own record when the person presses Enter, and the window says what that carries before it does.

use std::ops::Range;

use cosmic_text::{AttrsList, AttrsOwned, Color};
use serde_json::json;

use super::super::editor::MAX_TEXT;
use super::super::ui::Row;
use super::super::App;
use super::{Panel, Step};
use crate::transfer::{line_count, lines_word, size_text, INLINE_MAX};

const _: () = assert!(MAX_TEXT == 64 * 1024, "the words below say 64 KiB");

/// How long the window waits for the host to read a program's record and hand it over (a record of up to 1 GiB; the window stands still meanwhile).
const RECORD_WAIT: u64 = 30;

/// The colours of the header of another session's text in the history: the record dimmed, the sender's name lit in mint-teal.
pub const DIM: Color = Color::rgb(0x6A, 0x72, 0x7E);
pub const NAME: Color = Color::rgb(0x5E, 0xEA, 0xD4);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Draft,
    Lines,
    Conversation,
}

/// What the panel holds: the text of both sources as they were when it opened, and what is chosen.
pub struct Outgoing {
    pub from: String,
    pub draft: String,
    pub lines: String,
    pub source: Source,
    pub to: Option<String>,
}

/// What a line of the panel does when it is clicked.
enum Pick {
    Nothing,
    Source(Source),
    To(usize),
    Go,
}

/// The words for what the host answered to a send that it did not carry out.
pub fn failure(error: &str) -> String {
    match error {
        "unknown_session" => "the host does not know that session any more".into(),
        "session_disconnected" => "that session is closed (Ctrl+R in it brings it back)".into(),
        "session_exited" => "that session has ended".into(),
        "busy" => "the host could not take it just now".into(),
        "queue_full" => "that session's input queue is full; try again in a moment".into(),
        "chain_depth" => "the chain of agents is too long; a person has to start it again".into(),
        "too_large" => "the text is too large for the host".into(),
        "no_conversation_yet" => "this session has no conversation yet: its program has not answered once".into(),
        "record_not_found" => "the host did not find the program's record of this session; select the lines you want and send them with Ctrl+Shift+S".into(),
        "record_ambiguous" => "more than one record has the id of this session; select the lines you want and send them with Ctrl+Shift+S".into(),
        "record_format" => format!("the record is not in the layout this Lico reads (Claude Code {}); select the lines you want and send them with Ctrl+Shift+S", crate::provider_record::versions_text()),
        "bad_request" => "the host does not accept that text".into(),
        "send_limit" => "too many sessions that agents made are running; one has to be closed first".into(),
        "unknown_account" => "the account of the session is not there any more".into(),
        "option_unavailable" => "the host refused the session's settings".into(),
        folder if folder.starts_with("folder_") => "the folder of the session is not usable any more".into(),
        "host_unreachable" => "the host did not answer".into(),
        "host_timeout" => "the host did not answer in time; the text may have been delivered, so look at the other session before sending it again".into(),
        other => format!("the host answered {other}"),
    }
}

/// After which answers of a hand-over the host may already have stored the file and told the other session: only when the request was written and no answer came back.
/// Every code the host itself answers is a refusal before anything is stored (`deliver` takes the file away again when the input is refused), and `host_unreachable`
/// is a connection that was not made or a request that could not be written. A new code has to be put in the table of `sendto_tests.rs`, which says which it is.
fn may_have_gone(error: &str) -> bool {
    error == "host_timeout"
}

/// The same for a whole conversation, where a word differs.
pub fn failure_whole(error: &str) -> String {
    match error {
        "too_large" => "the conversation is over 32 MiB; select the lines you want and send them with Ctrl+Shift+S".into(),
        "bad_request" => "the host cannot read the record of this session".into(),
        other => failure(other),
    }
}

/// What a refused input of the open session says under the box: one sentence for the code the host answered, and always that the text is back in the box.
/// These are the codes an `input` can be answered with (`core::input`, the connection's checks of the request and of its size); the words of `failure` are reused.
pub fn input_refusal(error: &str) -> String {
    let sentence = match error {
        "unknown_session" | "session_exited" | "busy" | "too_large" | "bad_request" => failure(error),
        "session_disconnected" => "that session was closed meanwhile; press Enter again to reconnect it and send".into(),
        "stale_generation" => "the session changed meanwhile; look at it and send again".into(),
        "version_unsupported" | "unknown_op" => "this window and the host do not understand each other (they are not the same build); restart the window".into(),
        other => format!("the host answered {other}"),
    };
    format!("Not sent: {sentence}. The text is back in the box.")
}

/// How a source is described on its line: its lines (always for the selection, only when there are several for the draft) and its size.
fn describe(text: &str, lines_always: bool, empty: &str) -> String {
    if text.is_empty() {
        return format!("({empty})");
    }
    let (count, size) = (line_count(text), size_text(text.len()));
    let shape = if lines_always || count > 1 { format!("{}, {size}", lines_word(count)) } else { size };
    if text.len() > MAX_TEXT {
        format!("({shape}: over the limit of {} KiB)", MAX_TEXT / 1024)
    } else {
        format!("({shape})")
    }
}

impl App {
    /// The sessions a text can be sent to, with their numbers (the Alt+digit ones): all but the open one, among the first nine.
    fn others(&self, from: &str) -> Vec<(usize, &Row)> {
        self.ui.numbered.iter().take(9).enumerate().filter(|(_, id)| id.as_str() != from).filter_map(|(i, id)| self.ui.rows.iter().find(|r| &r.session == id).map(|r| (i + 1, r))).collect()
    }

    pub(crate) fn running_peer_count(&self) -> usize {
        if self.session.is_empty() {
            return 0;
        }
        self.others(&self.session).iter().filter(|(_, r)| r.state == "running").count()
    }

    /// The size of the selected lines as they would be sent, without building them.
    fn selected_size(&self) -> usize {
        self.selection_text().map_or(0, |s| s.len())
    }

    /// Why a text cannot be sent now (the menu greys the entry with this), if it cannot.
    pub(crate) fn send_reason(&self) -> Option<&'static str> {
        if self.selected_row().is_none() {
            return Some("no session is open");
        }
        if self.ui.numbered.len() < 2 {
            return Some("needs two sessions");
        }
        if !self.others(&self.session).iter().any(|(_, r)| r.state == "running") {
            return Some("no other session is running");
        }
        if !self.ed.text().is_empty() {
            return None;
        }
        match self.selected_size() {
            0 => Some("nothing to send: type in the input box or select lines"),
            n if n > MAX_TEXT => Some("the selected lines are over 64 KiB: select fewer"),
            _ => None,
        }
    }

    /// Whether the host has a record to read for the session: only a Claude session has one of the layout it knows.
    fn has_record(&self, id: &str) -> bool {
        self.ui.rows.iter().find(|r| r.session == id).is_some_and(|r| r.provider == "claude")
    }

    /// Why a conversation cannot be handed over now (the menu greys the entry with this), if it cannot.
    pub(crate) fn hand_reason(&self) -> Option<&'static str> {
        if self.selected_row().is_none() {
            return Some("no session is open");
        }
        if !self.has_record(&self.session) {
            return Some("only a Claude session has a record to hand over");
        }
        if self.ui.numbered.len() < 2 {
            return Some("needs two sessions");
        }
        if !self.others(&self.session).iter().any(|(_, r)| r.state == "running") {
            return Some("no other session is running");
        }
        None
    }

    /// The menu entry and Ctrl+Shift+S: the panel opens, or the first row says why not.
    pub(crate) fn send_ask(&mut self) {
        if let Some(reason) = self.send_reason() {
            self.ui.flash = Some(format!("Send to session: {reason}"));
            self.dirty = true;
            return;
        }
        self.send_open(None);
    }

    /// The menu entry "Hand over conversation...": the same panel, with the whole conversation chosen.
    pub(crate) fn hand_ask(&mut self) {
        if let Some(reason) = self.hand_reason() {
            self.ui.flash = Some(format!("Hand over conversation: {reason}"));
            self.dirty = true;
            return;
        }
        self.send_open(Some(Source::Conversation));
    }

    fn send_open(&mut self, chosen: Option<Source>) {
        let from = self.session.clone();
        let (draft, lines) = (self.ed.text().to_string(), self.selection_text().unwrap_or_default());
        let source = chosen.unwrap_or(if !lines.is_empty() && lines.len() <= MAX_TEXT { Source::Lines } else { Source::Draft });
        let to = match self.others(&from).as_slice() {
            [(_, only)] if only.state == "running" => Some(only.session.clone()), // with one other session there is nothing to choose
            _ => None,
        };
        self.panel_open(Step::Send); // the box is the panel's now; the draft is given back when it closes
        if let Some(p) = self.ui.panel.as_mut() {
            p.send = Some(Outgoing { from, draft, lines, source, to });
        }
    }

    fn send_name(&self, id: &str) -> String {
        self.ui.rows.iter().find(|r| r.session == id).map_or_else(|| id.chars().take(7).collect(), Row::name)
    }

    /// The lines of the panel, each with what a click on it does.
    fn send_rows(&self, s: &Outgoing) -> Vec<(String, Pick)> {
        let mark = |on: bool| if on { "[x]" } else { "[ ]" };
        let mut rows = vec![
            ("What to send".to_string(), Pick::Nothing),
            (format!("  {} D  Draft text {}", mark(s.source == Source::Draft), describe(&s.draft, false, "the input box is empty")), Pick::Source(Source::Draft)),
            (format!("  {} L  Selected lines {}", mark(s.source == Source::Lines), describe(&s.lines, true, "none selected")), Pick::Source(Source::Lines)),
            (format!("  {} C  Whole conversation {}", mark(s.source == Source::Conversation), if self.has_record(&s.from) { "(the host reads the program's record)" } else { "(only a Claude session has a record)" }), Pick::Source(Source::Conversation)),
            ("To".to_string(), Pick::Nothing),
        ];
        for (number, row) in self.others(&s.from) {
            let state = if row.state == "disconnected" { "closed".to_string() } else { row.state_text() };
            rows.push((format!("  {} {}  {}  {}  {}", mark(s.to.as_deref() == Some(row.session.as_str())), number, row.title(24).trim_end(), row.provider, state), Pick::To(number)));
        }
        let (whole, text) = (s.source == Source::Conversation, if s.source == Source::Draft { &s.draft } else { &s.lines });
        let how = if whole {
            "as a file. The host reads the program's record when you press Enter. Nothing is cut up to 32 MiB.".to_string()
        } else if text.len() <= INLINE_MAX {
            format!("inline ({} is at most {} KiB)", size_text(text.len()), INLINE_MAX / 1024)
        } else {
            format!("as a file ({} is more than {} KiB). Nothing is cut.", size_text(text.len()), INLINE_MAX / 1024)
        };
        rows.push((String::new(), Pick::Nothing));
        rows.push((format!("Delivery  {how}"), Pick::Nothing));
        if whole {
            rows.push(("Careful   The whole conversation goes to another session. It may hold what you or a tool typed:".to_string(), Pick::Nothing));
            rows.push(("          keys, passwords, other people's data. To send only part of it, choose L.".to_string(), Pick::Nothing));
        }
        let to = s.to.as_ref().map(|id| self.send_name(id));
        rows.push((
            match (&to, text.len() <= INLINE_MAX && !whole) {
                (None, _) => "          Pick a session to see where it goes.".to_string(),
                (Some(to), true) => format!("          The text goes into '{to}' as a {} that says it came from '{}'.", if s.source == Source::Draft { "message" } else { "selection" }, self.send_name(&s.from)),
                (Some(to), false) => format!("          Saved in the handoff folder of '{to}'; it gets the path and the first lines."),
            },
            Pick::Nothing,
        ));
        rows.push((String::new(), Pick::Nothing));
        let verb = if whole { "Hand over" } else { "Send" };
        rows.push((to.map_or(format!("Enter  {verb} (pick a session first)"), |to| format!("Enter  {verb} to '{to}'")), Pick::Go));
        rows
    }

    pub(crate) fn send_view(&self, p: &Panel) -> (String, Vec<String>) {
        let Some(s) = p.send.as_ref() else { return (String::new(), Vec::new()) };
        ("Send to session".into(), self.send_rows(s).into_iter().map(|r| r.0).collect())
    }

    fn send_notice(&mut self, text: String) {
        if let Some(p) = self.ui.panel.as_mut() {
            p.notice = text;
        }
        self.dirty = true;
    }

    /// D and L choose the source, a digit the receiver; each says what is wrong with a choice it refuses.
    pub(crate) fn send_key(&mut self, key: char) {
        match key {
            'd' => self.send_source(Source::Draft),
            'l' => self.send_source(Source::Lines),
            'c' => self.send_source(Source::Conversation),
            '1'..='9' => self.send_pick(key as usize - '0' as usize),
            _ => {}
        }
    }

    fn send_source(&mut self, source: Source) {
        let Some(s) = self.ui.panel.as_ref().and_then(|p| p.send.as_ref()) else { return };
        let refusal = match source {
            Source::Draft if s.draft.is_empty() => Some("The input box was empty when this opened: there is no draft to send.".to_string()),
            Source::Lines if s.lines.is_empty() => Some("No lines were selected when this opened.".to_string()),
            Source::Lines if s.lines.len() > MAX_TEXT => Some(format!("The selected lines are {}: one input holds at most {} KiB.", size_text(s.lines.len()), MAX_TEXT / 1024)),
            Source::Conversation if !self.has_record(&s.from) => Some("Only a Claude session has a record the host can read: send the draft or selected lines instead.".to_string()),
            _ => None,
        };
        match refusal {
            Some(why) => self.send_notice(why),
            None => {
                if let Some(s) = self.ui.panel.as_mut().and_then(|p| p.send.as_mut()) {
                    s.source = source;
                }
                self.send_notice(String::new());
            }
        }
    }

    fn send_pick(&mut self, number: usize) {
        let Some(from) = self.ui.panel.as_ref().and_then(|p| p.send.as_ref()).map(|s| s.from.clone()) else { return };
        let picked = match self.ui.numbered.get(number - 1).map(String::as_str) {
            None => Err(format!("There is no session {number} in the list.")),
            Some(id) if id == from => Err("That is the open session: pick another.".to_string()),
            Some(id) => match self.ui.rows.iter().find(|r| r.session == id) {
                None => Err(format!("There is no session {number} in the list.")),
                Some(r) if r.state == "disconnected" => Err(format!("'{}' is closed: Ctrl+R there brings it back.", r.name())),
                Some(r) if r.state != "running" => Err(format!("'{}' has ended.", r.name())),
                Some(r) => Ok(r.session.clone()),
            },
        };
        match picked {
            Ok(id) => {
                if let Some(s) = self.ui.panel.as_mut().and_then(|p| p.send.as_mut()) {
                    s.to = Some(id);
                }
                self.send_notice(String::new());
            }
            Err(why) => self.send_notice(why),
        }
    }

    /// A click on a line of the panel: what its key does. False for a line that is only a word.
    pub(crate) fn send_click(&mut self, line: usize) -> bool {
        let Some(s) = self.ui.panel.as_ref().and_then(|p| p.send.as_ref()) else { return false };
        match self.send_rows(s).into_iter().nth(line).map(|r| r.1) {
            Some(Pick::Source(Source::Draft)) => self.send_key('d'),
            Some(Pick::Source(Source::Lines)) => self.send_key('l'),
            Some(Pick::Source(Source::Conversation)) => self.send_key('c'),
            Some(Pick::To(number)) => self.send_pick(number),
            Some(Pick::Go) => self.send_go(),
            Some(Pick::Nothing) | None => return false,
        }
        true
    }

    /// Enter: the host is asked. The draft goes only when it is delivered; otherwise the panel stays with the choices, the text and the reason.
    pub(crate) fn send_go(&mut self) {
        let Some(s) = self.ui.panel.as_ref().and_then(|p| p.send.as_ref()) else { return };
        let Some(to) = s.to.clone() else { return self.send_notice("Pick the session to send to: its number, or a click on its line.".into()) };
        let (source, from) = (s.source, s.from.clone());
        let whole = source == Source::Conversation;
        let (kind, text) = match source {
            Source::Draft => ("text", s.draft.clone()),
            Source::Lines => ("lines", s.lines.clone()),
            Source::Conversation => ("conversation", String::new()),
        };
        let reply = if whole {
            self.host_call_within(json!({"op": "transfer.send", "from": from, "to": to, "kind": kind}), RECORD_WAIT)
        } else {
            self.host_call(json!({"op": "transfer.send", "from": from, "to": to, "kind": kind, "text": text}))
        };
        if reply["ok"] != json!(1) {
            let why = reply["error"].as_str().unwrap_or("unknown");
            if whole && may_have_gone(why) {
                return self.send_notice("The host has not said yet whether it handed the conversation over; look at the other session before you hand it over again.".into());
            }
            let kept = if whole { "Nothing was sent." } else { "The text is kept." };
            return self.send_notice(format!("Not {}: {}. {kept}", if whole { "handed over" } else { "sent" }, if whole { failure_whole(why) } else { failure(why) }));
        }
        let how = if reply["delivery"] == json!("file") { "as a file" } else { "inline" };
        let state = reply["receiver"].as_str().unwrap_or("idle").replace('_', " ");
        let size = size_text(reply["size"].as_u64().unwrap_or(text.len() as u64) as usize);
        let said = format!("{} '{}' ({how}, {size}); it was {state}.", if whole { "Handed over the conversation to" } else { "Sent to" }, self.send_name(&to));
        self.panel_close(); // gives the draft back to the box ...
        if source == Source::Draft {
            self.ed.clear(); // ... and a draft that was delivered is not a draft any more
        }
        self.ui.flash = Some(said);
    }
}

/// Is `text` the header the host writes in front of another session's text, as the history records it (`>> [from session 'NAME' (ID) | 14:02 | message | 5 B]`),
/// and nothing else on the line? The look is styling of a record's text: an agent line that imitates it is recorded behind `[agent] ` and does not match, but
/// nothing that matters relies on this; the boundary is the header the receiving agent sees, which the host writes.
#[derive(PartialEq, Debug)]
pub enum Look {
    Person,
    Session(Range<usize>), // where the name stands
}

const SESSION: &str = ">> [from session '";
const PEER_REPLY: &str = ">> [peer reply from '";
const CALL_CARD: &str = ">> [\u{21c4} call '";
const PERSON: &str = ">> [from the person | ";

fn session_header(rest: &str, prefix_len: usize, has_kind: bool) -> Option<Look> {
    let len = rest.find('\'').filter(|&n| n > 0)?;
    if rest[..len].contains(['[', ']', '|', '(', ')']) {
        return None; // the host turns these into `_` in a name
    }
    let (id, tail) = rest[len..].strip_prefix("' (")?.split_once(") | ")?;
    let id_ok = id.len() == 7 && id.starts_with('s') && id[1..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let tail_ok = if has_kind { fields(tail) } else { reply_fields(tail) };
    (id_ok && tail_ok).then(|| Look::Session(prefix_len..prefix_len + len))
}

fn call_header(rest: &str, prefix_len: usize) -> Option<Look> {
    let len = rest.find('\'').filter(|&n| n > 0)?;
    if rest[..len].contains(['[', ']', '|', '(', ')']) {
        return None;
    }
    let (cid, tail) = rest[len..].strip_prefix("' (")?.split_once(") | ")?;
    let id_ok = cid.len() == 7 && cid.starts_with('c') && cid[1..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let Some(body) = tail.strip_suffix(']') else { return None };
    let Some((clock, note)) = body.split_once(" | ") else { return None };
    let c = clock.as_bytes();
    let clock_ok = c.len() == 5 && c[2] == b':' && [0, 1, 3, 4].iter().all(|&i| c[i].is_ascii_digit());
    (id_ok && clock_ok && !note.trim().is_empty()).then(|| Look::Session(prefix_len..prefix_len + len))
}

pub fn header_look(text: &str) -> Option<Look> {
    if let Some(rest) = text.strip_prefix(SESSION) {
        session_header(rest, SESSION.len(), true)
    } else if let Some(rest) = text.strip_prefix(PEER_REPLY) {
        session_header(rest, PEER_REPLY.len(), false)
    } else if let Some(rest) = text.strip_prefix(CALL_CARD) {
        call_header(rest, CALL_CARD.len())
    } else {
        text.strip_prefix(PERSON).filter(|tail| fields(tail)).map(|_| Look::Person)
    }
}

/// Extract the 7-char Call Room id (`cXXXXXX`) from a `>> [⇄ call ...]` or `>> [peer reply ... | call cXXXXXX ...]` line.
#[allow(dead_code)]
pub fn extract_call_id7(text: &str) -> Option<&str> {
    if let Some(rest) = text.strip_prefix(CALL_CARD) {
        let len = rest.find('\'')?;
        let (cid, _) = rest[len..].strip_prefix("' (")?.split_once(") | ")?;
        if cid.len() == 7 && cid.starts_with('c') && cid[1..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Some(cid);
        }
    } else if text.starts_with(PEER_REPLY) {
        if let Some(after) = text.rsplit_once(" | call ").map(|(_, r)| r) {
            let cid = after.split_whitespace().next()?;
            if cid.len() == 7 && cid.starts_with('c') && cid[1..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
                return Some(cid);
            }
        }
    }
    None
}

/// `14:02 | message | 5 B]` or `... | selection | 3.1 KiB, 24 lines]` (or `conversation`), and the line ends there.
fn fields(tail: &str) -> bool {
    let Some(body) = tail.strip_suffix(']') else { return false };
    let parts: Vec<&str> = body.split(" | ").collect();
    let [clock, kind, size] = parts.as_slice() else { return false };
    let c = clock.as_bytes();
    c.len() == 5 && c[2] == b':' && [0, 1, 3, 4].iter().all(|&i| c[i].is_ascii_digit()) && matches!(*kind, "message" | "selection" | "conversation") && size_shape(size)
}

/// `14:02 | 9 B]` or `14:02 | 9 B | call c012345 round 1/4]` for `[peer reply from 'NAME' (ID) | ...]`.
fn reply_fields(tail: &str) -> bool {
    let Some(body) = tail.strip_suffix(']') else { return false };
    let parts: Vec<&str> = body.split(" | ").collect();
    let (clock, size, call_seg) = match parts.as_slice() {
        [clock, size] => (*clock, *size, None),
        [clock, size, call_seg] => (*clock, *size, Some(*call_seg)),
        _ => return false,
    };
    let c = clock.as_bytes();
    let clock_ok = c.len() == 5 && c[2] == b':' && [0, 1, 3, 4].iter().all(|&i| c[i].is_ascii_digit());
    let call_ok = call_seg.is_none_or(|seg| {
        let mut w = seg.split_whitespace();
        matches!((w.next(), w.next(), w.next(), w.next()), (Some("call"), Some(cid), Some("round"), Some(rm))
            if cid.len() == 7 && cid.starts_with('c') && cid[1..].bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            && rm.split_once('/').is_some_and(|(r, m)| !r.is_empty() && !m.is_empty() && r.bytes().all(|b| b.is_ascii_digit()) && m.bytes().all(|b| b.is_ascii_digit())))
    });
    clock_ok && size_shape(size) && call_ok
}

/// What `size_text` and `lines_word` write: `27 B`, `3.1 KiB`, `1.0 MiB`, each with `, 24 lines` (or `, 1 line`) after it for a selection.
fn size_shape(size: &str) -> bool {
    let (bytes, lines) = size.split_once(", ").map_or((size, None), |(b, l)| (b, Some(l)));
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let Some((number, unit)) = bytes.split_once(' ') else { return false };
    let number_ok = number.split_once('.').map_or_else(|| digits(number), |(whole, tenth)| digits(whole) && digits(tenth));
    number_ok && matches!(unit, "B" | "KiB" | "MiB" | "GiB") && lines.is_none_or(|l| l.strip_suffix(" lines").or_else(|| l.strip_suffix(" line")).is_some_and(digits))
}

/// The attributes of a line, in the look of a header when it is one: every character dimmed, the name lit, the font of each character kept.
pub fn tint(list: AttrsList, text: &str) -> AttrsList {
    let Some(look) = header_look(text) else { return list };
    let mut out = AttrsList::new(&list.defaults().color(DIM));
    for (range, attrs) in list.spans_iter() {
        out.add_span(range.clone(), &attrs.as_attrs().color(DIM));
    }
    if let Look::Session(name) = look {
        for (i, c) in text[name.clone()].char_indices() {
            let at = name.start + i;
            let kept = AttrsOwned::new(&out.get_span(at));
            out.add_span(at..at + c.len_utf8(), &kept.as_attrs().color(NAME));
        }
    }
    out
}

#[cfg(test)]
#[path = "sendto_tests.rs"]
pub(super) mod tests;
