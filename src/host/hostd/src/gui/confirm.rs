//! Renaming, closing and deleting the session that is open. Each is a small panel on the machinery of the sign-in and of the new-session form
//! (one panel at a time; the draft is set aside and given back), so the input line is the one text field here too: it carries the new name.
//! Closing a session that is working and deleting any session ask first, and nothing is done until the person says yes. Every action is a
//! menu entry, a shortcut (F2, Ctrl+Shift+W, Ctrl+Delete) and a command of the command line; the host does the work and the window asks it.

use std::time::Duration;

use serde_json::json;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::super::menu::Cmd;
use super::super::App;
use super::{Panel, Step};

const DOUBLE_CLICK: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    Rename,
    Close,
    Delete,
}

/// The session an action is about, as the window knew it when the action was asked for. The host checks the generation again.
#[derive(Clone)]
pub struct Ask {
    pub act: Act,
    pub session: String,
    pub generation: i64,
    pub title: String,
    pub working: bool,
    pub cwd: String, // the folder the person chose; empty for a scratch workspace
}

fn why(error: &str) -> String {
    match error {
        "stale_generation" => "the session has changed since it was shown; try again".into(),
        "no_conversation_yet" => "this session has no conversation to come back to yet".into(),
        "session_exited" => "the session has ended".into(),
        "unknown_session" => "the host does not know it any more".into(),
        "host_unreachable" | "host_timeout" => "the host did not answer".into(),
        other => format!("the host answered {other}"),
    }
}

impl App {
    /// Why an action cannot be asked for now (the menu greys it with this), if it cannot.
    pub(crate) fn ask_reason(&self, act: Act) -> Option<&'static str> {
        match (act, self.selected_row().map(|r| r.state.as_str())) {
            (_, None) => Some("no session is open"),
            (Act::Close, Some("running")) | (Act::Rename | Act::Delete, _) => None,
            (Act::Close, Some("disconnected")) => Some("the session is already closed"),
            (Act::Close, Some(_)) => Some("the session has ended"),
        }
    }

    /// F2, Ctrl+Shift+W and Ctrl+Delete while no panel is open; true when the key was taken.
    pub(crate) fn lifecycle_key(&mut self, event: &KeyEvent) -> bool {
        let (ctrl, shift, alt) = (self.ui.mods.control_key(), self.ui.mods.shift_key(), self.ui.mods.alt_key());
        let act = match &event.logical_key {
            Key::Named(NamedKey::F2) if !ctrl && !alt => Act::Rename,
            Key::Named(NamedKey::Delete) if ctrl && !alt => Act::Delete,
            Key::Character(c) if ctrl && shift && !alt && c.eq_ignore_ascii_case("w") => Act::Close,
            _ => return false,
        };
        if !event.repeat {
            self.ask_open(act);
        }
        true
    }

    /// A click on a row of the list: it opens the session, and a second click on it soon after asks for its new name.
    pub(crate) fn row_click(&mut self, id: String) {
        if self.ui.panel.is_some() {
            return; // a panel owns the input line: leaving for another session would carry its text along
        }
        if id != self.session {
            self.switch_to(&id);
        }
        let double = matches!(&self.ui.clicked, Some((at, last)) if *last == id && at.elapsed() < DOUBLE_CLICK);
        self.ui.clicked = if double { None } else { Some((std::time::Instant::now(), id)) };
        if double {
            self.run_cmd(Cmd::Rename);
        }
    }

    /// Start an action on the open session: a name is asked for, a delete and the stopping of a working turn are asked about, and closing
    /// an idle session is done at once (nothing is lost: the conversation stays and resumes).
    pub(crate) fn ask_open(&mut self, act: Act) {
        if let Some(reason) = self.ask_reason(act) {
            self.ui.flash = Some(format!("{act:?} session: {reason}"));
            self.dirty = true;
            return;
        }
        let Some(row) = self.selected_row() else { return };
        let ask = Ask { act, session: row.session.clone(), generation: row.generation, title: row.title(40).trim_end().to_string(), working: row.holds_a_turn(), cwd: row.cwd.clone() };
        if act == Act::Close && !ask.working {
            return match self.ask_do(&ask, "") {
                Ok(done) => self.ui.flash = Some(done),
                Err(why) => self.ui.flash = Some(format!("Close session: {why}")),
            };
        }
        let name = row.name.clone();
        self.panel_open(if act == Act::Rename { Step::Rename } else { Step::Confirm });
        if let Some(p) = self.ui.panel.as_mut() {
            p.ask = Some(ask);
        }
        if act == Act::Rename {
            self.ed.put(&name);
        }
    }

    /// The host does it; the window follows: a deleted session leaves the list, the screen and the draft box, a closed one stays.
    fn ask_do(&mut self, ask: &Ask, name: &str) -> Result<String, String> {
        let request = match ask.act {
            Act::Rename => json!({"op": "session.rename", "session": ask.session, "name": name}),
            Act::Close => json!({"op": "session.close", "session": ask.session, "generation": ask.generation}),
            Act::Delete => json!({"op": "session.purge", "session": ask.session, "generation": ask.generation}),
        };
        let reply = self.host_call(request);
        if reply["ok"] != json!(1) {
            return Err(why(reply["error"].as_str().unwrap_or("unknown")));
        }
        if ask.act == Act::Delete {
            self.session_gone(&ask.session);
        }
        self.send("{\"op\":\"status\"}");
        Ok(match ask.act {
            Act::Rename if name.is_empty() => "The name is taken away.".into(),
            Act::Rename => "Renamed.".into(),
            Act::Close => "Session closed. Ctrl+R brings it back.".into(),
            Act::Delete => "Session deleted.".into(),
        })
    }

    /// The host deleted a session: the screen leaves it if it was shown, and the list, the draft box and the bookkeeping of the window forget it.
    fn session_gone(&mut self, id: &str) {
        self.panel_close(); // the draft of the open session comes back to the box, and goes with the session below
        if self.session == id {
            self.go_start();
        }
        self.ed.forget(id);
        self.pending_inputs.retain(|_, (session, _)| session != id); // a late refusal must not make a draft for it again
        self.meta.retain(|m| m.id != id);
        self.ui.rows.retain(|r| r.session != id);
        self.ui.numbered.retain(|n| n != id);
    }

    /// Y in the confirmation: do what was asked.
    fn confirm_yes(&mut self) {
        let Some(ask) = self.ui.panel.as_ref().and_then(|p| p.ask.clone()) else { return };
        match self.ask_do(&ask, "") {
            Ok(done) => {
                self.panel_close();
                self.ui.flash = Some(done);
            }
            Err(why) => self.notice(format!("Not done: {why}.")),
        }
    }

    /// Enter on the name line: the host keeps the name (an empty one takes it away).
    pub(crate) fn rename_submit(&mut self, text: &str) {
        let Some(ask) = self.ui.panel.as_ref().and_then(|p| p.ask.clone()) else { return };
        let outcome = if crate::folders::valid_name(text) { self.ask_do(&ask, text) } else { Err(format!("a name has at most {} characters and no line break", crate::folders::MAX_NAME)) };
        match outcome {
            Ok(done) => {
                self.panel_close();
                self.ui.flash = Some(done);
            }
            Err(why) => {
                self.ed.put(text); // what was typed stays to be corrected
                self.notice(format!("Not renamed: {why}."));
            }
        }
    }

    fn notice(&mut self, text: String) {
        if let Some(p) = self.ui.panel.as_mut() {
            p.notice = text;
        }
    }

    /// Y does it, N keeps the session as it is; any other key does nothing.
    pub(crate) fn confirm_key(&mut self, key: char) {
        match key {
            'y' => self.confirm_yes(),
            'n' => self.panel_close(),
            _ => {}
        }
    }

    /// A click on the Y or N line of the confirmation; false when the line is neither.
    pub(crate) fn confirm_click(&mut self, line: usize) -> bool {
        let Some((_, body)) = self.panel_view().filter(|_| self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Confirm)) else { return false };
        match body.get(line).map(|l| l.chars().next()) {
            Some(Some('Y')) => self.confirm_yes(),
            Some(Some('N')) => self.panel_close(),
            _ => return false,
        }
        true
    }

    /// The title and lines of the rename line and of the confirmations.
    pub(crate) fn ask_view(&self, p: &Panel) -> (String, Vec<String>) {
        let Some(ask) = p.ask.as_ref() else { return (String::new(), Vec::new()) };
        let named = format!("\"{}\"", ask.title);
        let folder = if ask.cwd.is_empty() { "Its scratch workspace is removed with it.".to_string() } else { format!("The folder {} is not touched.", ask.cwd) };
        match ask.act {
            Act::Rename => ("Rename session".into(), vec![format!("Type the new name of {named} and press Enter."), "An empty name takes the name away. At most 60 characters.".into(), "The host keeps the name: other windows and the command line show it too.".into()]),
            Act::Close => ("Close session".into(), vec![format!("Stop the work of {named} and close it?"), "The turn that is running ends and what it has not said yet is lost.".into(), "The session and its conversation stay; Ctrl+R brings it back.".into(), String::new(), "Y  stop and close".into(), "N  keep it running".into()]),
            Act::Delete => {
                let running = if ask.working { "A turn is running: it is stopped first." } else { "Its process, if any, is stopped first." };
                ("Delete session".into(), vec![format!("Delete {named}? This cannot be undone."), format!("{running} Its record is removed for good."), folder, String::new(), "Y  delete it".into(), "N  keep it".into()])
            }
        }
    }
}

#[cfg(test)]
#[path = "confirm_tests.rs"]
mod tests;
