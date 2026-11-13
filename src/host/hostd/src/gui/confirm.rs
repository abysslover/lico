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
    CloseTab,
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
    pub worktree_branch: String,
    pub worktree_repo: String,
    pub worktree_exists: bool,
    pub dirty_confirm: bool,
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
            (Act::Close, Some("running")) | (Act::CloseTab | Act::Rename | Act::Delete, _) => None,
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
            _ if ctrl && shift && !alt && crate::gui::key_letter(event) == Some('w') => Act::Close,
            _ => return false,
        };
        if !event.repeat {
            self.ask_open(act);
        }
        true
    }

    /// A click on a row of the list: closes any open panel (giving back any parked draft first), opens the session,
    /// and on a second click soon after (when no panel was just closed) asks for its new name.
    pub(crate) fn row_click(&mut self, id: String) {
        let closed_panel = self.ui.panel.is_some();
        if closed_panel {
            self.panel_close();
        }
        if id != self.session || self.active_call_open() || self.view.is_none() {
            self.switch_to(&id);
        }
        if closed_panel {
            self.ui.clicked = Some((std::time::Instant::now(), id));
            return;
        }
        let double = matches!(&self.ui.clicked, Some((at, last)) if *last == id && at.elapsed() < DOUBLE_CLICK);
        self.ui.clicked = if double { None } else { Some((std::time::Instant::now(), id)) };
        if double {
            self.run_cmd(Cmd::Rename);
        }
    }

    /// Close button (`×`) on a session tab, or `/exit` (`/quit`): dismisses the tab cleanly (stopping any idle process and keeping
    /// resumable conversations in `Past`, or purging a blank 0-turn tab).
    pub(crate) fn close_tab_by_id(&mut self, id: String) {
        if self.ui.panel.is_some() {
            self.panel_close();
        }
        if id != self.session || self.active_call_open() || self.view.is_none() {
            self.switch_to(&id);
        }
        self.ask_open(Act::CloseTab);
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
        let ask = Ask {
            act,
            session: row.session.clone(),
            generation: row.generation,
            title: row.title(40).trim_end().to_string(),
            working: row.holds_a_turn(),
            cwd: row.cwd.clone(),
            worktree_branch: row.worktree_branch.clone(),
            worktree_repo: row.worktree_repo.clone(),
            worktree_exists: row.worktree_exists,
            dirty_confirm: false,
        };
        if matches!(act, Act::Close | Act::CloseTab) && !ask.working && !ask.worktree_exists {
            let prefix = if act == Act::CloseTab { "Close tab" } else { "Close session" };
            return match self.ask_do(&ask, "", None, false) {
                Ok(done) => self.ui.flash = Some(done),
                Err((_, text)) => self.ui.flash = Some(format!("{prefix}: {text}")),
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
    fn ask_do(&mut self, ask: &Ask, name: &str, worktree: Option<&str>, force_dirty: bool) -> Result<String, (String, String)> {
        let mut request = match ask.act {
            Act::Rename => json!({"op": "session.rename", "session": ask.session, "name": name}),
            Act::Close => json!({"op": "session.close", "session": ask.session, "generation": ask.generation}),
            Act::CloseTab => json!({"op": "session.close", "session": ask.session, "generation": ask.generation, "hide_tab": true}),
            Act::Delete => json!({"op": "session.purge", "session": ask.session, "generation": ask.generation}),
        };
        if let Some(policy) = worktree {
            request["worktree"] = json!(policy);
            if force_dirty {
                request["force_dirty"] = json!(true);
            }
        }
        let mut reply = self.host_call(request);
        let mut purged_empty = false;
        if ask.act == Act::CloseTab
            && reply["ok"] != json!(1)
            && matches!(reply["error"].as_str(), Some("no_conversation_yet" | "session_exited"))
        {
            let mut purge_req = json!({"op": "session.purge", "session": ask.session, "generation": ask.generation});
            if let Some(policy) = worktree {
                purge_req["worktree"] = json!(policy);
                if force_dirty {
                    purge_req["force_dirty"] = json!(true);
                }
            }
            reply = self.host_call(purge_req);
            purged_empty = true;
        }
        if reply["ok"] != json!(1) {
            let code = reply["error"].as_str().unwrap_or("unknown").to_string();
            let text = why(&code);
            return Err((code, text));
        }
        if ask.act == Act::Delete {
            self.session_gone(&ask.session);
        } else if ask.act == Act::CloseTab {
            self.dismiss_tab_after(&ask.session, purged_empty);
        }
        self.send("{\"op\":\"status\"}");
        Ok(match (ask.act, worktree) {
            (Act::Rename, _) if name.is_empty() => "The name is taken away.".into(),
            (Act::Rename, _) => "Renamed.".into(),
            (Act::Close, Some("remove")) => "Session closed and worktree removed (branch kept). Ctrl+R brings the session back.".into(),
            (Act::Close, Some("keep")) => "Session closed (worktree kept). Ctrl+R brings it back.".into(),
            (Act::Close, _) => "Session closed. Ctrl+R brings it back.".into(),
            (Act::CloseTab, _) if purged_empty => "Tab closed.".into(),
            (Act::CloseTab, Some("remove")) => "Tab closed and worktree removed (branch kept). Conversation stays in Past.".into(),
            (Act::CloseTab, Some("keep")) => "Tab closed (worktree kept). Conversation stays in Past.".into(),
            (Act::CloseTab, _) => "Tab closed. Conversation stays in Past.".into(),
            (Act::Delete, Some("remove")) => "Session deleted and worktree removed (branch kept).".into(),
            (Act::Delete, Some("keep")) => "Session deleted (worktree kept).".into(),
            (Act::Delete, _) => "Session deleted.".into(),
        })
    }

    fn dismiss_tab_after(&mut self, id: &str, purged: bool) {
        let pos = self.ui.numbered.iter().position(|n| n == id);
        let remaining: Vec<String> = self.ui.numbered.iter().filter(|n| n.as_str() != id).cloned().collect();
        let next_tab = pos.and_then(|i| remaining.get(i).or_else(|| remaining.last()).cloned());
        if purged {
            self.panel_close();
            self.ed.forget(id);
            self.pending_inputs.retain(|_, (session, _)| session != id);
            self.meta.retain(|m| m.id != id);
            self.ui.rows.retain(|r| r.session != id);
            self.ui.numbered.retain(|n| n != id);
        } else {
            self.panel_close();
            if let Some(row) = self.ui.rows.iter_mut().find(|r| r.session == id) {
                row.tab_closed = true;
                row.state = "disconnected".into();
                row.activity.clear();
                row.busy_since = 0;
            }
            if self.tabs_shown() {
                self.ui.numbered.retain(|n| n != id);
            }
        }
        if self.session == id {
            if self.tabs_shown() {
                if let Some(next) = next_tab {
                    self.switch_to(&next);
                } else {
                    self.go_start();
                }
            } else {
                self.go_start();
            }
        }
        self.refresh_list();
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
        match self.ask_do(&ask, "", None, false) {
            Ok(done) => {
                self.panel_close();
                self.ui.flash = Some(done);
            }
            Err((_, text)) => self.notice(format!("Not done: {text}.")),
        }
    }

    fn confirm_worktree(&mut self, policy: &str, force_dirty: bool) {
        let Some(ask) = self.ui.panel.as_ref().and_then(|p| p.ask.clone()) else { return };
        match self.ask_do(&ask, "", Some(policy), force_dirty) {
            Ok(done) => {
                self.panel_close();
                self.ui.flash = Some(done);
            }
            Err((code, _)) if code == "worktree_dirty" => {
                if let Some(p) = self.ui.panel.as_mut() {
                    if let Some(a) = p.ask.as_mut() {
                        a.dirty_confirm = true;
                    }
                    p.notice.clear();
                }
                self.dirty = true;
            }
            Err((_, text)) => self.notice(format!("Not done: {text}.")),
        }
    }

    /// Enter on the name line: the host keeps the name (an empty one takes it away).
    pub(crate) fn rename_submit(&mut self, text: &str) {
        let Some(ask) = self.ui.panel.as_ref().and_then(|p| p.ask.clone()) else { return };
        let outcome = if crate::folders::valid_name(text) {
            self.ask_do(&ask, text, None, false).map_err(|(_, t)| t)
        } else if crate::folders::shows_nothing(text.trim()) {
            Err("the name shows nothing (only characters that are not drawn)".to_string())
        } else {
            Err(format!("a name has at most {} characters and no line break", crate::folders::MAX_NAME))
        };
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

    /// Y (or K/R/D on a worktree session) does it, N keeps the session as it is; any other key does nothing.
    pub(crate) fn confirm_key(&mut self, key: char) {
        let Some((wt, dirty)) = self.ui.panel.as_ref().and_then(|p| p.ask.as_ref().map(|a| (a.worktree_exists, a.dirty_confirm))) else {
            return;
        };
        match (wt, dirty, key) {
            (false, _, 'y') => self.confirm_yes(),
            (true, false, 'k') | (true, true, 'k') => self.confirm_worktree("keep", false),
            (true, false, 'r') => self.confirm_worktree("remove", false),
            (true, true, 'd') => self.confirm_worktree("remove", true),
            (_, _, 'n') => self.panel_close(),
            _ => {}
        }
    }

    /// A click on an action line of the confirmation; false when the line is none of them.
    pub(crate) fn confirm_click(&mut self, line: usize) -> bool {
        let Some((_, body)) = self.panel_view().filter(|_| self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Confirm)) else { return false };
        let Some((wt, dirty)) = self.ui.panel.as_ref().and_then(|p| p.ask.as_ref().map(|a| (a.worktree_exists, a.dirty_confirm))) else {
            return false;
        };
        match (wt, dirty, body.get(line).and_then(|l| l.chars().next())) {
            (false, _, Some('Y')) => self.confirm_yes(),
            (true, _, Some('K')) => self.confirm_worktree("keep", false),
            (true, false, Some('R')) => self.confirm_worktree("remove", false),
            (true, true, Some('D')) => self.confirm_worktree("remove", true),
            (_, _, Some('N')) => self.panel_close(),
            _ => return false,
        }
        true
    }

    /// The title and lines of the rename line and of the confirmations.
    pub(crate) fn ask_view(&self, p: &Panel) -> (String, Vec<String>) {
        let Some(ask) = p.ask.as_ref() else { return (String::new(), Vec::new()) };
        let named = format!("\"{}\"", ask.title);
        if ask.act != Act::Rename && ask.worktree_exists {
            let verb = match ask.act {
                Act::Close => "close session",
                Act::CloseTab => "close tab",
                _ => "delete session",
            };
            if ask.dirty_confirm {
                return (
                    "Uncommitted changes in worktree".into(),
                    vec![
                        format!("The worktree of {named} (branch '{}') has uncommitted or untracked changes.", ask.worktree_branch),
                        format!("Removing it discards uncommitted files in that worktree. Branch '{}' and {} are not touched.", ask.worktree_branch, ask.worktree_repo),
                        String::new(),
                        format!("D  {verb} and remove dirty worktree anyway"),
                        format!("K  {verb} and keep worktree"),
                        "N  cancel".into(),
                    ],
                );
            }
            let heading = if matches!(ask.act, Act::Close | Act::CloseTab) { "Close worktree session" } else { "Delete worktree session" };
            let first = match (ask.act, ask.working) {
                (Act::Close | Act::CloseTab, true) => format!("Stop the work of {named} and close it?"),
                (Act::Close | Act::CloseTab, false) => format!("Close {named}? Its conversation stays; Past / Ctrl+R brings it back."),
                _ => format!("Delete {named}? This cannot be undone."),
            };
            return (
                heading.into(),
                vec![
                    first,
                    format!("Worktree on branch '{}' (main repository {} is not touched).", ask.worktree_branch, ask.worktree_repo),
                    format!("Removing the worktree deletes only Lico's checkout; branch '{}' stays in git.", ask.worktree_branch),
                    String::new(),
                    format!("K  {verb} and keep worktree"),
                    format!("R  {verb} and remove worktree (branch stays)"),
                    "N  cancel".into(),
                ],
            );
        }
        let folder = if ask.cwd.is_empty() { "Its scratch workspace is removed with it.".to_string() } else { format!("The folder {} is not touched.", ask.cwd) };
        match ask.act {
            Act::Rename => ("Rename session".into(), vec![format!("Type the new name of {named} and press Enter."), "An empty name takes the name away. At most 60 characters.".into(), "The host keeps the name: other windows and the command line show it too.".into()]),
            Act::Close | Act::CloseTab => ("Close session".into(), vec![format!("Stop the work of {named} and close it?"), "The turn that is running ends and what it has not said yet is lost.".into(), "The session and its conversation stay; Past / Ctrl+R brings it back.".into(), String::new(), "Y  stop and close".into(), "N  keep it running".into()]),
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
