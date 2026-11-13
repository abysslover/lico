//! "+ New chat" (`M10-P03`): the row above the parent row of the Directories tab and the small centered modal behind it. The person chooses an
//! account, then `Resume` (the default) or `New`. `Resume` shows `Last Session : <provider> / <YYYY-MM-DD HH:MM> / <title> (<N> turns)` for the viewed
//! directory and that account, read through the host's one rule (`session.chats` with `recorded` and `limit` 1), and continues that chat by creating with
//! `resume_latest`, which the host resolves again at that moment; with no earlier chat it opens a new one and says so. `New` starts a fresh conversation with the
//! quick-start defaults (mode auto). `Options...` opens the full new-session form for the same directory.
//! Keys: Up and Down the account, Left and Right Resume or New, Enter, Esc, `o` for Options.

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::controls::{Action, Rect};
use super::panel::AsyncJob;
use super::sidebar_chats::chats_error_text;
use super::App;

/// How many accounts the modal lists at once (it scrolls with the selection beyond this).
pub(super) const MAX_ROWS: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Acct {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Last {
    pub provider: String,
    pub updated_at: u64,
    pub title: String,
    pub turns: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Preview {
    Idle,
    Loading,
    Nothing,
    Found(Last),
    Failed(String),
}

/// Where the modal drew its parts the last time (the UI dump and the tests click on them).
#[derive(Clone, Default)]
pub(crate) struct Geom {
    pub rows: Vec<(usize, Rect)>,
    pub resume: Option<Rect>,
    pub new: Option<Rect>,
    pub ok: Option<Rect>,
    pub cancel: Option<Rect>,
    pub options: Option<Rect>,
}

pub(crate) struct NewChat {
    pub folder: String,
    pub ssh_host: Option<String>,
    pub accounts: Vec<Acct>,
    pub sel: usize,
    pub resume: bool,
    pub preview: Preview,
    pub error: Option<String>,
    pub req: u64,
    pub busy: bool,
    pub geom: Geom,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Press {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Options,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Outcome {
    Stay,
    Changed,
    Confirm,
    Cancel,
    Options,
}

impl NewChat {
    pub(super) fn press(&mut self, key: Press) -> Outcome {
        match key {
            Press::Up if self.sel > 0 => {
                self.sel -= 1;
                Outcome::Changed
            }
            Press::Down if self.sel + 1 < self.accounts.len() => {
                self.sel += 1;
                Outcome::Changed
            }
            Press::Left if !self.resume => {
                self.resume = true;
                Outcome::Changed
            }
            Press::Right if self.resume => {
                self.resume = false;
                Outcome::Changed
            }
            Press::Enter if !self.accounts.is_empty() => Outcome::Confirm,
            Press::Esc => Outcome::Cancel,
            Press::Options => Outcome::Options,
            _ => Outcome::Stay,
        }
    }

    /// The line under the choice: the last session, or why there is none.
    pub(super) fn info_line(&self) -> String {
        if let Some(error) = &self.error {
            return error.clone();
        }
        if self.accounts.is_empty() {
            return "No account for this location. Use Options... to add one.".into();
        }
        if !self.resume {
            return "New starts a fresh conversation in this directory.".into();
        }
        match &self.preview {
            Preview::Idle => String::new(),
            Preview::Loading => "Looking for the latest chat...".into(),
            Preview::Nothing => "No earlier chat in this directory for this account; Resume will open a new one.".into(),
            Preview::Found(last) => last_session_line(&last.provider, last.updated_at, crate::transfer::utc_offset(last.updated_at as i64), &last.title, last.turns),
            Preview::Failed(why) => format!("Could not look for chats: {why}"),
        }
    }
}

/// Why the button cannot be used now, if it cannot.
pub(super) fn gate(unreachable: bool, global: bool, folder: &str) -> Result<(), &'static str> {
    if unreachable {
        Err("that SSH host is unreachable")
    } else if global {
        Err("Global has no folder: open a directory first")
    } else if folder.is_empty() {
        Err("no directory is shown")
    } else {
        Ok(())
    }
}

/// The accounts of the `account.list` answer that can work in the viewed place: Claude and Codex accounts of that SSH host, or the local ones.
pub(super) fn accounts_for(list: &Value, host: Option<&str>) -> Vec<Acct> {
    list["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| matches!(a["provider"].as_str(), Some("claude" | "codex")))
        .filter(|a| a["ssh_host"].as_str().filter(|h| !h.is_empty()) == host)
        .map(|a| Acct {
            id: a["account"].as_str().unwrap_or("").to_string(),
            provider: a["provider"].as_str().unwrap_or("").to_string(),
            label: a["label"].as_str().unwrap_or("").to_string(),
            default: a["default"] == json!(true),
        })
        .collect()
}

/// The account the modal starts on: the first one marked default, else the first.
pub(super) fn default_index(accounts: &[Acct]) -> usize {
    accounts.iter().position(|a| a.default).unwrap_or(0)
}

/// `Last Session : claude / 2026-11-13 20:41 / Fix pagination (12 turns)`; the time is the local one (`offset` seconds east of UTC), `?` when the turns are not known.
pub(super) fn last_session_line(provider: &str, updated_at: u64, offset: i64, title: &str, turns: Option<u64>) -> String {
    let c = crate::transfer::civil(updated_at as i64, offset);
    let turns = match turns {
        Some(1) => "1 turn".to_string(),
        Some(n) => format!("{n} turns"),
        None => "? turns".to_string(),
    };
    format!("Last Session : {provider} / {:04}-{:02}-{:02} {:02}:{:02} / {title} ({turns})", c.year, c.month, c.day, c.hour, c.minute)
}

impl App {
    /// Why the "+ New chat" row cannot be used now, if it cannot.
    pub(crate) fn newchat_gate(&self) -> Result<(), &'static str> {
        let sb = &self.ui.sidebar;
        gate(sb.unreachable, sb.global_scope, &sb.cwd)
    }

    /// A click on the row (or its key): opens the modal for the viewed directory.
    pub(crate) fn sidebar_new_chat_open(&mut self) {
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.filter_focused = false;
        if let Err(reason) = self.newchat_gate() {
            self.ui.flash = Some(format!("+ New chat: {reason}."));
            self.dirty = true;
            return;
        }
        let reply = self.host_call(json!({"op": "account.list"}));
        let (folder, host) = (self.ui.sidebar.cwd.clone(), self.ui.sidebar.ssh_host.clone());
        let accounts = accounts_for(&reply, host.as_deref());
        let sel = default_index(&accounts);
        self.ui.sidebar.newchat = Some(NewChat { folder, ssh_host: host, accounts, sel, resume: true, preview: Preview::Idle, error: None, req: 0, busy: false, geom: Geom::default() });
        self.newchat_preview();
        self.dirty = true;
    }

    /// Asks the host for the latest chat of the folder and the chosen account (only while `Resume` is chosen).
    fn newchat_preview(&mut self) {
        let Some(nc) = self.ui.sidebar.newchat.as_mut() else { return };
        nc.error = None;
        if !nc.resume || nc.accounts.is_empty() {
            nc.preview = Preview::Idle;
            return;
        }
        let account = nc.accounts[nc.sel].id.clone();
        let mut request = json!({"op": "session.chats", "cwd": nc.folder, "account": account, "recorded": true, "limit": 1});
        if let Some(host) = &nc.ssh_host {
            request["ssh_host"] = json!(host);
        }
        nc.preview = Preview::Loading;
        let tag = account.clone();
        let req_id = self.spawn_host_call(request, 7, move |req_id| AsyncJob::NewChatLast { req_id, account: tag });
        if let Some(nc) = self.ui.sidebar.newchat.as_mut() {
            nc.req = req_id;
        }
    }

    /// Creates the session for the choice: `Resume` asks the host to continue the latest chat (it decides again, now), `New` starts fresh.
    fn newchat_confirm(&mut self) {
        let Some(nc) = self.ui.sidebar.newchat.as_mut() else { return };
        if nc.busy || nc.accounts.is_empty() {
            return;
        }
        let account = nc.accounts[nc.sel].clone();
        let mut request = json!({"op": "session.create", "kind": account.provider, "account": account.id, "mode": "auto", "cwd": nc.folder});
        if nc.resume {
            request["resume_latest"] = json!(true);
        }
        let created_cwd = if nc.ssh_host.is_some() { None } else { Some(nc.folder.clone()) };
        let resume = nc.resume;
        nc.busy = true;
        nc.error = None;
        let req_id = self.spawn_host_call(request, 8, move |req_id| AsyncJob::NewChatStart { req_id, created_cwd, resume });
        if let Some(nc) = self.ui.sidebar.newchat.as_mut() {
            nc.req = req_id;
        }
        self.dirty = true;
    }

    /// `Options...`: the full new-session form for the same directory, on the chosen account.
    fn newchat_options(&mut self) {
        let Some(nc) = self.ui.sidebar.newchat.take() else { return };
        self.newchat_gone();
        let chosen = nc.accounts.get(nc.sel).cloned();
        self.form_open_at_host(nc.folder, nc.ssh_host);
        if let (Some(account), Some(p)) = (chosen, self.ui.panel.as_mut()) {
            if p.accounts.iter().any(|a| a.0 == account.id) {
                p.provider = account.provider;
                p.account = account.id;
                p.new_ssh_host = None;
            }
        }
        self.dirty = true;
    }

    pub(crate) fn newchat_close(&mut self) {
        self.ui.sidebar.newchat = None;
        self.newchat_gone();
    }

    /// The modal is gone: its click areas go with it at once, so that a click before the next paint reaches what is beneath, not a dialog that is no longer there.
    fn newchat_gone(&mut self) {
        let ours = |a: &Action| matches!(a, Action::ModalBackdrop | Action::NewChatAccount(_) | Action::NewChatMode(_) | Action::NewChatOk | Action::NewChatCancel | Action::NewChatOptions);
        self.ui.controls.hits.retain(|h| !ours(&h.action));
        self.dirty = true;
    }

    fn newchat_apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Stay => {}
            Outcome::Changed => self.newchat_preview(),
            Outcome::Confirm => self.newchat_confirm(),
            Outcome::Cancel => self.newchat_close(),
            Outcome::Options => self.newchat_options(),
        }
        self.dirty = true;
    }

    /// The keys of the modal while it is open (every plain key is taken, so nothing is typed behind it); a chord with Ctrl or Alt goes on to the window.
    pub(crate) fn newchat_key(&mut self, event: &KeyEvent) -> bool {
        if self.ui.sidebar.newchat.is_none() || self.ui.mods.control_key() || self.ui.mods.alt_key() {
            return false;
        }
        let press = match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => Some(Press::Up),
            Key::Named(NamedKey::ArrowDown) => Some(Press::Down),
            Key::Named(NamedKey::ArrowLeft) => Some(Press::Left),
            Key::Named(NamedKey::ArrowRight) => Some(Press::Right),
            Key::Named(NamedKey::Enter) => Some(Press::Enter),
            Key::Named(NamedKey::Escape) => Some(Press::Esc),
            Key::Character(c) if c.as_str().eq_ignore_ascii_case("o") => Some(Press::Options),
            _ => None,
        };
        if let Some(press) = press.filter(|p| !(event.repeat && matches!(p, Press::Enter | Press::Esc | Press::Options))) {
            let outcome = self.ui.sidebar.newchat.as_mut().map_or(Outcome::Stay, |nc| nc.press(press));
            self.newchat_apply(outcome);
        }
        true
    }

    /// A click on the row or on a part of the modal; true when it was one of ours.
    pub(crate) fn newchat_click(&mut self, action: &Action) -> bool {
        match action {
            Action::SidebarNewChat => self.sidebar_new_chat_open(),
            Action::NewChatAccount(i) => {
                let outcome = self.ui.sidebar.newchat.as_mut().map_or(Outcome::Stay, |nc| {
                    if *i < nc.accounts.len() && nc.sel != *i {
                        nc.sel = *i;
                        Outcome::Changed
                    } else {
                        Outcome::Stay
                    }
                });
                self.newchat_apply(outcome);
            }
            Action::NewChatMode(resume) => {
                let outcome = self.ui.sidebar.newchat.as_mut().map_or(Outcome::Stay, |nc| nc.press(if *resume { Press::Left } else { Press::Right }));
                self.newchat_apply(outcome);
            }
            Action::NewChatOk => self.newchat_apply(Outcome::Confirm),
            Action::NewChatCancel => self.newchat_apply(Outcome::Cancel),
            Action::NewChatOptions => self.newchat_apply(Outcome::Options),
            _ => return false,
        }
        true
    }

    /// The answers of the host to the preview and to the creation.
    pub(crate) fn on_newchat_async(&mut self, job: AsyncJob, reply: Value) {
        let has_proxy = self.ui.proxy.is_some();
        match job {
            AsyncJob::NewChatLast { req_id, account } => {
                let Some(nc) = self.ui.sidebar.newchat.as_mut() else { return };
                if (has_proxy && nc.req != req_id) || nc.accounts.get(nc.sel).map(|a| a.id.as_str()) != Some(account.as_str()) {
                    return; // an answer for a choice that has changed since
                }
                nc.preview = if reply["ok"] != json!(1) {
                    Preview::Failed(chats_error_text(reply["error"].as_str().unwrap_or("unknown")))
                } else if let Some(item) = reply["items"].as_array().and_then(|items| items.first()) {
                    Preview::Found(Last {
                        provider: item["provider"].as_str().unwrap_or("").to_string(),
                        updated_at: item["updated_at"].as_u64().unwrap_or(0),
                        title: super::clean(item["title"].as_str().unwrap_or("")),
                        turns: item["turns"].as_u64(),
                    })
                } else {
                    Preview::Nothing
                };
            }
            AsyncJob::NewChatStart { req_id, created_cwd, resume } => {
                if self.ui.sidebar.newchat.as_ref().map_or(false, |nc| has_proxy && nc.req != req_id) {
                    return;
                }
                if reply["ok"] != json!(1) {
                    let why = chats_error_text(reply["error"].as_str().unwrap_or("unknown"));
                    if let Some(nc) = self.ui.sidebar.newchat.as_mut() {
                        nc.busy = false;
                        nc.error = Some(format!("Could not open the chat: {why}"));
                    }
                } else {
                    let id = reply["session"].as_str().unwrap_or("").to_string();
                    let said = if reply["resumed"] == json!(true) {
                        if reply["already_open"] == json!(true) { "Switched to the open chat.".to_string() } else { format!("Continued: {}.", super::clean(reply["resumed_chat"]["title"].as_str().unwrap_or("chat"))) }
                    } else if resume {
                        "No earlier chat here; opened a new one.".to_string()
                    } else {
                        "Opened a new chat.".to_string()
                    };
                    self.ui.sidebar.newchat = None;
                    self.newchat_gone();
                    self.send("{\"op\":\"status\"}");
                    self.ui.scroll = 0;
                    self.show(&id, None);
                    self.ui.tab_ensure_visible = true;
                    self.persist_workspace(created_cwd.as_deref(), Some("session"), Some(&id));
                    self.ui.flash = Some(said);
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    /// The state of the row and the modal for the UI dump.
    pub(super) fn newchat_json(&self) -> Value {
        let Some(nc) = &self.ui.sidebar.newchat else { return Value::Null };
        let rect_of = |r: &Option<Rect>| r.map_or(Value::Null, |r| json!(r));
        let accounts: Vec<Value> = nc
            .accounts
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let rect = nc.geom.rows.iter().find(|(row, _)| *row == i).map(|(_, r)| *r);
                json!({"id": a.id, "label": a.label, "provider": a.provider, "default": a.default, "selected": i == nc.sel, "rect": rect})
            })
            .collect();
        json!({
            "open": true, "folder": nc.folder, "ssh_host": nc.ssh_host, "accounts": accounts, "selected": nc.sel, "resume": nc.resume,
            "last_line": nc.info_line(), "busy": nc.busy,
            "rects": {"resume": rect_of(&nc.geom.resume), "new": rect_of(&nc.geom.new), "ok": rect_of(&nc.geom.ok), "cancel": rect_of(&nc.geom.cancel), "options": rect_of(&nc.geom.options)},
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts(n: usize) -> Vec<Acct> {
        (0..n).map(|i| Acct { id: format!("a{i}"), provider: if i % 2 == 0 { "claude" } else { "codex" }.into(), label: format!("L{i}"), default: i == 1 }).collect()
    }

    fn modal(n: usize) -> NewChat {
        NewChat { folder: "/w".into(), ssh_host: None, accounts: accounts(n), sel: 0, resume: true, preview: Preview::Idle, error: None, req: 0, busy: false, geom: Geom::default() }
    }

    #[test]
    fn the_last_session_line_reads_like_the_launchers_with_local_time_and_a_question_mark_for_unknown_turns() {
        assert_eq!(last_session_line("claude", 1_000_000_000, 0, "Fix pagination", Some(12)), "Last Session : claude / 2001-09-09 01:46 / Fix pagination (12 turns)");
        assert_eq!(last_session_line("codex", 1_000_000_000, 9 * 3600, "Fix pagination", Some(1)), "Last Session : codex / 2001-09-09 10:46 / Fix pagination (1 turn)");
        assert_eq!(last_session_line("claude", 1_000_000_000, 0, "t", None), "Last Session : claude / 2001-09-09 01:46 / t (? turns)");
    }

    #[test]
    fn the_button_says_why_it_cannot_be_used() {
        assert!(gate(false, false, "/w").is_ok());
        assert!(gate(true, false, "/w").unwrap_err().contains("unreachable"));
        assert!(gate(false, true, "/w").unwrap_err().contains("Global"));
        assert!(gate(false, false, "").unwrap_err().contains("no directory"));
    }

    #[test]
    fn up_and_down_choose_the_account_left_and_right_choose_resume_or_new_and_enter_and_esc_end_it() {
        let mut m = modal(3);
        assert_eq!(m.press(Press::Up), Outcome::Stay);
        assert_eq!((m.press(Press::Down), m.sel), (Outcome::Changed, 1));
        assert_eq!((m.press(Press::Down), m.sel), (Outcome::Changed, 2));
        assert_eq!((m.press(Press::Down), m.sel), (Outcome::Stay, 2));
        assert_eq!((m.press(Press::Up), m.sel), (Outcome::Changed, 1));
        assert_eq!(m.press(Press::Left), Outcome::Stay, "Resume is already chosen");
        assert_eq!((m.press(Press::Right), m.resume), (Outcome::Changed, false));
        assert_eq!(m.press(Press::Right), Outcome::Stay);
        assert_eq!((m.press(Press::Left), m.resume), (Outcome::Changed, true));
        assert_eq!(m.press(Press::Enter), Outcome::Confirm);
        assert_eq!(m.press(Press::Esc), Outcome::Cancel);
        assert_eq!(m.press(Press::Options), Outcome::Options);
        assert_eq!(modal(0).press(Press::Enter), Outcome::Stay, "no account, nothing to confirm");
    }

    #[test]
    fn the_modal_starts_on_the_default_account_and_lists_only_the_accounts_of_the_viewed_place() {
        let list = json!({"accounts": [
            {"account": "l1", "provider": "claude", "label": "Local", "default": false, "ssh_host": null},
            {"account": "l2", "provider": "codex", "label": "LocalX", "default": true, "ssh_host": null},
            {"account": "r1", "provider": "claude", "label": "Remote", "default": true, "ssh_host": "gpu"},
            {"account": "s1", "provider": "synthetic", "label": "No", "default": false, "ssh_host": null}]});
        let local = accounts_for(&list, None);
        assert_eq!(local.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["l1", "l2"]);
        assert_eq!(default_index(&local), 1);
        let remote = accounts_for(&list, Some("gpu"));
        assert_eq!(remote.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["r1"]);
        assert_eq!(default_index(&accounts_for(&list, Some("none"))), 0);
    }

    #[test]
    fn the_line_under_the_choice_says_what_is_going_on() {
        let mut m = modal(2);
        m.preview = Preview::Loading;
        assert!(m.info_line().starts_with("Looking"));
        m.preview = Preview::Nothing;
        assert!(m.info_line().starts_with("No earlier chat"));
        m.resume = false;
        assert!(m.info_line().starts_with("New starts"));
        m.error = Some("Could not open the chat: busy".into());
        assert_eq!(m.info_line(), "Could not open the chat: busy");
        assert!(modal(0).info_line().contains("No account"));
    }
}
