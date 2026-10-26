//! A session that is `disconnected` comes back from the window in two ways: Ctrl+R (and the Session menu) reconnects it and nothing more, and Enter in its box
//! reconnects it and sends what was typed once the host says it is back. The host and the wire do not change: this is a `session.resume`, and then an
//! ordinary `input` with the generation the resume answered. The text waits in `pending_inputs` under the key `reconnect-<session>` (which is also the id of
//! the resume), so it goes with the session when that is deleted, and it is given back when the host is replaced. Tests: `reconnect_tests.rs` (the window
//! with a few lines for a host) and `tests/test_reconnect_window.py` (the window and a host with each simulator).

use super::super::events::emit_ime;
use super::super::panel::sendto::{failure, input_refusal};
use super::super::{jint, jstr, json_escape, App};
use super::super::editor::MAX_TEXT;

const WAITING: &str = "Reconnecting the session...";
const ENDED: &str = "That session has ended and cannot be reopened. Ctrl+N starts a new one. The text is back in the box.";
const NO_CONVERSATION: &str = "That session never got a conversation, so there is nothing to reconnect. Ctrl+N starts a new one. The text is back in the box.";
const ENDED_AGAIN: &str = "The session was reconnected but ended again at once; the provider could not continue that conversation. The text is back in the box.";
const REPLACED: &str = "Not sent: the host was replaced while the session was reconnecting. The text is back in the box.";
/// The permanent line while the open session is disconnected: the long form, and the short one for a narrow window.
const HINT: [&str; 2] = ["Disconnected: Enter reconnects and sends, Ctrl+R reconnects", "Disconnected: Enter reconnects"];

/// The key under which the text of `session` waits for its resume; it is the id of that resume too.
fn key(session: &str) -> String {
    format!("reconnect-{session}")
}

/// What a refused input says under the box. A text sent just after a reconnect (its id ends in `r`) that finds the session closed again says that,
/// because the table's sentence for a closed session sends the person to Enter, which is what they just did.
pub(in super::super) fn input_note(id: &str, error: &str) -> String {
    match error {
        "session_disconnected" if id.ends_with('r') => ENDED_AGAIN.into(),
        _ => input_refusal(error),
    }
}

/// What a refused resume says: a session with no conversation has nothing to come back to; the other codes have the words of the send-to panel.
fn not_reconnected(error: &str) -> String {
    match error {
        "bad_request" => NO_CONVERSATION.into(),
        "stale_generation" => input_refusal(error),
        other => format!("Not sent: the session could not be reconnected: {}. The text is back in the box.", failure(other)),
    }
}

impl App {
    // ------------------------------------------------------------------ Ctrl+R
    /// Why the selected session cannot be resumed now (None: it can). Only a session that was disconnected can, and not while Enter has a resume under way.
    pub(crate) fn resume_reason(&self) -> Option<&'static str> {
        match self.selected_row().map(|r| r.state.as_str()) {
            None => Some("no session is open"),
            Some("disconnected") if self.pending_inputs.contains_key(&key(&self.session)) => Some("the session is being reconnected"),
            Some("disconnected") => None,
            Some("running") => Some("the session is running"),
            Some(_) => Some("the session has ended"),
        }
    }

    /// Ctrl+R and the Session menu: the host starts the provider again for the open session, with a new generation. The reply comes with the id
    /// `resume` (an error is shown on the first row); the list shows the new state at the next status.
    pub(crate) fn resume_session(&mut self) {
        if let Some(reason) = self.resume_reason() {
            self.ui.flash = Some(format!("Resume session: {reason}"));
            self.dirty = true;
            return;
        }
        let Some(row) = self.selected_row() else { return };
        let msg = format!("{{\"op\":\"session.resume\",\"session\":\"{}\",\"generation\":{},\"id\":\"resume\"}}", self.session, row.generation);
        self.send(&msg);
        self.ui.flash = Some("Resuming the session...".to_string());
        self.dirty = true;
    }

    /// The answer to the resume request: what the person is told.
    pub(crate) fn on_resume_reply(&mut self, line: &str) -> bool {
        if jstr(line, "id") != Some("resume") {
            return false;
        }
        self.ui.flash = Some(if line.contains("\"ok\":1") {
            "Session resumed.".to_string()
        } else {
            match jstr(line, "error") {
                Some("bad_request") => "Resume session: the host does not know a conversation to resume yet.".to_string(),
                Some("stale_generation") => "Resume session: the session changed; try again.".to_string(),
                Some(other) => format!("Resume session: {other}"),
                None => "Resume session: no answer".to_string(),
            }
        });
        self.send("{\"op\":\"status\"}");
        self.dirty = true;
        true
    }

    // ------------------------------------------------------------------ Enter
    /// One `input` for `session` at `generation`, with a request id of its own (`gui-<process>-<n>`, an `r` after the number when the session was
    /// reconnected for it); the text waits under that id until the host has answered, and goes back to its box if the host refuses it.
    pub(crate) fn send_input(&mut self, session: &str, generation: i64, text: String, after_reconnect: bool) {
        self.sent_count += 1;
        let request_id = format!("gui-{}-{}{}", std::process::id(), self.sent_count, if after_reconnect { "r" } else { "" });
        let msg = format!(
            "{{\"op\":\"input\",\"id\":\"{request_id}\",\"session\":\"{session}\",\"generation\":{generation},\"request_id\":\"{request_id}\",\"text\":\"{}\"}}",
            json_escape(&text)
        );
        self.pending_inputs.insert(request_id, (session.to_string(), text));
        self.send(&msg);
    }

    /// A text the host did not take goes back in front of the draft of its session (wherever that is) and, when that session is open, a line says why.
    pub(crate) fn give_back_to(&mut self, session: &str, text: &str, why: String) {
        let current = self.session == session;
        self.ed.give_back(session, current, text);
        if current {
            self.ed.say(why);
        }
        emit_ime("send_failed", text);
    }

    /// Enter in the box of the open session while the session is not running. A disconnected one is asked to resume and the text waits for the answer; an ended
    /// one cannot come back, and the text stays. True: the key was taken here. An empty box, a text over the limit and a running session are the ordinary path.
    pub(crate) fn reconnect_instead(&mut self) -> bool {
        let text = self.ed.text();
        if text.trim().is_empty() || text.len() > MAX_TEXT {
            return false;
        }
        let Some((state, generation)) = self.selected_row().map(|r| (r.state.clone(), r.generation)) else { return false };
        let held = key(&self.session);
        match state.as_str() {
            "exited" => self.ed.say(ENDED),
            "disconnected" if self.pending_inputs.contains_key(&held) => self.ed.say(WAITING), // one resume per session, the text held once
            "disconnected" => {
                let Some(text) = self.ed.prepare_send(&self.session) else { return false };
                emit_ime("sent", &text);
                emit_ime("text", self.ed.text());
                self.pending_inputs.insert(held.clone(), (self.session.clone(), text));
                self.send(&format!("{{\"op\":\"session.resume\",\"session\":\"{}\",\"generation\":{generation},\"id\":\"{held}\"}}", self.session));
                self.ed.say(WAITING);
            }
            _ => return false,
        }
        self.dirty = true;
        true
    }

    /// The answer to the resume that Enter asked for: the held text goes as one input with the generation of the answer, to the session it was typed in
    /// (the person may have opened another since); a refusal gives it back with a sentence. A resume is never asked for twice for one Enter.
    pub(crate) fn on_reconnect_reply(&mut self, line: &str) -> bool {
        let Some(id) = jstr(line, "id").filter(|id| id.starts_with("reconnect-")) else { return false };
        let Some((session, text)) = self.pending_inputs.remove(id) else { return true }; // the text went with the first answer, or the host was replaced
        self.ed.unsay(WAITING);
        self.send("{\"op\":\"status\"}");
        match jint(line, "generation").filter(|_| line.contains("\"ok\":1")) {
            Some(generation) => self.send_input(&session, generation, text, true),
            None => self.give_back_to(&session, &text, not_reconnected(jstr(line, "error").unwrap_or("refused"))),
        }
        self.dirty = true;
        true
    }

    /// The host was replaced: the texts that waited for a resume were never sent and come back; what was in flight is let go, as it always was.
    pub(crate) fn release_held(&mut self) {
        let held: Vec<String> = self.pending_inputs.keys().filter(|k| k.starts_with("reconnect-")).cloned().collect();
        for id in held {
            if let Some((session, text)) = self.pending_inputs.remove(&id) {
                self.give_back_to(&session, &text, REPLACED.into());
            }
        }
        self.pending_inputs.clear();
    }

    /// The first item of the permanent line while the open session is disconnected, the longest form that fits `cols`.
    pub(crate) fn disconnected_hint(&self, cols: usize) -> Option<&'static str> {
        if self.selected_row()?.state != "disconnected" {
            return None;
        }
        HINT.into_iter().find(|h| h.chars().count() <= cols)
    }
}

#[cfg(test)]
#[path = "reconnect_tests.rs"]
mod tests;
