//! New session, accounts and sign-in inside the window. Each step is a short list that a key answers (the input line is the
//! one text field: a label, or a code the provider asked for). Everything is a request to the host on a connection of its own;
//! a pasted code goes to the host's login operation and is cleared from the window at once. Nothing here reads a provider's files.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::App;

#[derive(Clone, Copy, PartialEq)]
pub enum Step {
    Provider,
    Account,
    Label,
    Ready,
    Login,
    Accounts,
    Actions,
}

pub struct Panel {
    pub step: Step,
    pub provider: String,
    pub account: String,
    pub notice: String,
    pub accounts: Vec<(String, String, String, bool)>, // id, provider, label, default
    pub login: Value,
    pub confirm: Option<char>,
    pub last_error: String, // the host's raw error word of the last failed step: in the ui dump only
    pub opened: bool, // the browser was opened for this sign-in attempt
    pub stash: String, // what was typed in the input line before the panel took it over
}

/// The window opens the sign-in address by itself unless this is set (read by the window only; the tests set it where no browser is wanted).
fn auto_open_off() -> bool {
    std::env::var_os("LICO_NO_AUTO_OPEN").is_some()
}

impl App {
    fn host_call(&self, request: Value) -> Value {
        self.host_call_within(request, 8)
    }

    /// One request on a connection of its own, waiting at most `seconds` for the reply (the window stands still meanwhile).
    fn host_call_within(&self, request: Value, seconds: u64) -> Value {
        let Ok(mut conn) = UnixStream::connect(&self.socket) else { return json!({"ok": 0, "error": "host_unreachable"}) };
        conn.set_read_timeout(Some(Duration::from_secs(seconds))).ok();
        conn.set_write_timeout(Some(Duration::from_secs(seconds))).ok();
        let mut line = request.to_string();
        line.push('\n');
        if conn.write_all(line.as_bytes()).is_err() {
            return json!({"ok": 0, "error": "host_unreachable"});
        }
        let mut reader = BufReader::new(conn);
        loop {
            let mut reply = String::new();
            match reader.read_line(&mut reply) {
                Ok(n) if n > 0 => {
                    if let Ok(value) = serde_json::from_str::<Value>(&reply) {
                        if value.get("ok").is_some() {
                            return value;
                        }
                    }
                }
                _ => return json!({"ok": 0, "error": "host_timeout"}),
            }
        }
    }

    fn load_accounts(&mut self) {
        let reply = self.host_call(json!({"op": "account.list"}));
        let list: Vec<(String, String, String, bool)> = reply["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| (a["account"].as_str().unwrap_or("").to_string(), a["provider"].as_str().unwrap_or("").to_string(), a["label"].as_str().unwrap_or("").to_string(), a["default"] == json!(true)))
            .collect();
        if let Some(p) = self.ui.panel.as_mut() {
            p.accounts = list;
        }
    }

    fn sync_quiet(&self) {
        super::QUIET_INPUT.store(self.ui.panel.is_some(), std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn panel_open(&mut self, step: Step) {
        let stash = std::mem::take(&mut self.input);
        self.preedit.clear();
        self.ui.panel = Some(Panel { step, provider: String::new(), account: String::new(), notice: String::new(), accounts: Vec::new(), login: Value::Null, confirm: None, last_error: String::new(), opened: false, stash });
        self.sync_quiet();
        if step == Step::Accounts {
            self.load_accounts();
        }
        self.dirty = true;
    }

    pub(crate) fn panel_close(&mut self) {
        if let Some(p) = self.ui.panel.take() {
            if p.step == Step::Login && !p.account.is_empty() {
                self.host_call(json!({"op": "account.login.cancel", "account": p.account})); // leaving the panel ends an unfinished sign-in
            }
            self.input = p.stash;
            self.preedit.clear();
        }
        self.sync_quiet();
        self.dirty = true;
    }

    pub(crate) fn text_mode(&self) -> bool {
        self.ui.panel.as_ref().map_or(false, |p| p.step == Step::Label || (p.step == Step::Login && p.login["needs_code"] == json!(true)))
    }

    /// The sign-in code is being typed: it is drawn as stars and never put in the window's model.
    pub(crate) fn code_entry(&self) -> bool {
        self.ui.panel.as_ref().map_or(false, |p| p.step == Step::Login) && self.text_mode()
    }

    /// What the input line shows: the text itself, or one star per character of a code (at most 64).
    pub(crate) fn shown_input(&self) -> String {
        if self.code_entry() {
            "*".repeat(self.input.chars().count().min(64))
        } else {
            self.input.clone()
        }
    }

    fn account_name(p: &Panel) -> String {
        p.accounts.iter().find(|a| a.0 == p.account).map_or(p.account.clone(), |a| format!("{} / {}", a.1, a.2))
    }

    /// The title and the lines of what the panel shows now, or the first screen when no session is selected.
    pub(crate) fn panel_view(&self) -> Option<(String, Vec<String>)> {
        let Some(p) = self.ui.panel.as_ref() else {
            if self.session.is_empty() && self.ui.rows.is_empty() {
                return Some(("select a session".into(), vec!["Ctrl+N  new session".into(), "Ctrl+A  accounts and sign-in".into()]));
            }
            return None;
        };
        let mut body = Vec::new();
        let title = match p.step {
            Step::Provider => {
                body.extend(["1  codex".to_string(), "2  claude".to_string()]);
                "New session: which provider?".to_string()
            }
            Step::Account => {
                for (i, a) in p.accounts.iter().filter(|a| a.1 == p.provider).enumerate() {
                    body.push(format!("{}  {}{}", i + 1, a.2, if a.3 { "  (default)" } else { "" }));
                }
                body.push("n  a new account".into());
                format!("New {} session: which account?", p.provider)
            }
            Step::Label => {
                body.push("Type a label for the account and press Enter.".into());
                format!("New {} account", p.provider)
            }
            Step::Ready => {
                body.extend(["Enter  start the session".to_string(), "L      sign in to this account first".to_string()]);
                format!("New session: {}", Self::account_name(p))
            }
            Step::Login => {
                let state = p.login["state"].as_str().unwrap_or("starting");
                body.push(format!("state: {state}"));
                if let Some(url) = p.login["url"].as_str() {
                    body.push("Open this address and sign in:".into());
                    body.push(url.to_string());
                    body.push(if auto_open_off() { "Ctrl+Shift+C  copy the address    Ctrl+O  open it in the browser" } else { "Opening the browser. Ctrl+O opens it again, Ctrl+Shift+C copies the address." }.into());
                }
                if p.login["needs_code"] == json!(true) {
                    body.push("The provider asks for a code: paste it below and press Enter.".into());
                } else if p.provider == "codex" && p.login["state"] != json!("succeeded") {
                    body.push("D  use the device-code sign-in instead".into());
                }
                format!("Sign in: {}", Self::account_name(p))
            }
            Step::Accounts => {
                for (i, a) in p.accounts.iter().enumerate() {
                    body.push(format!("{}  {} / {}{}", i + 1, a.1, a.2, if a.3 { "  (default)" } else { "" }));
                }
                if p.accounts.is_empty() {
                    body.push("No account yet: Ctrl+N creates one with a session.".into());
                }
                "Accounts".to_string()
            }
            Step::Actions => {
                body.extend(["S  sign in".to_string(), "F  make it the default for its provider".to_string(), "D  remove it (its sessions stay, unbound)".to_string(), "E  remove it and end its sessions".to_string()]);
                if let Some(s) = p.login["signed_in"].as_bool() {
                    body.push(format!("provider says: {}", if s { "signed in" } else { "not signed in" }));
                } else if p.login.get("signed_in").is_some() {
                    body.push("provider says: unknown (no status command)".into());
                }
                format!("Account: {}", Self::account_name(p))
            }
        };
        if !p.notice.is_empty() {
            body.push(String::new());
            body.push(p.notice.clone());
        }
        body.push(String::new());
        body.push("Esc  back out".into());
        Some((title, body))
    }

    pub(crate) fn panel_json(&self) -> Value {
        match (self.panel_view(), self.ui.panel.as_ref()) {
            (Some((title, body)), Some(p)) => json!({"open": true, "title": title, "body": body, "text_entry": self.text_mode(), "step": p.step as u8, "input": self.shown_input(), "input_len": self.input.chars().count(), "error": p.last_error}),
            _ => Value::Null,
        }
    }

    /// Poll a login in progress: the host says what the provider printed and what it needs.
    pub(crate) fn panel_tick(&mut self) {
        let Some(p) = self.ui.panel.as_ref() else { return };
        if p.step != Step::Login || p.account.is_empty() {
            return;
        }
        let reply = self.host_call_within(json!({"op": "account.login.status", "account": p.account}), 2);
        let finished = reply["state"] == json!("succeeded");
        let mut open_now = false;
        if let Some(p) = self.ui.panel.as_mut() {
            p.login = reply;
            if finished {
                p.step = Step::Ready;
                p.notice = "Signed in.".into();
            } else if p.login["url"].is_string() && !p.opened {
                p.opened = true; // once per attempt, however often the state is polled
                open_now = true;
            }
        }
        if open_now && !auto_open_off() {
            self.login_address(false);
        }
        self.dirty = true;
    }

    fn start_login(&mut self, method: &str) {
        let account = self.ui.panel.as_ref().map(|p| p.account.clone()).unwrap_or_default();
        self.host_call(json!({"op": "account.login.cancel", "account": account}));
        for _ in 0..20 {
            if !matches!(self.host_call(json!({"op": "account.login.status", "account": account}))["state"].as_str(), Some("waiting" | "browser" | "code")) {
                break; // the cancelled login has ended
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let reply = self.host_call(json!({"op": "account.login.start", "account": account, "method": method}));
        if let Some(p) = self.ui.panel.as_mut() {
            p.step = Step::Login;
            p.notice = if reply["ok"] == json!(1) { String::new() } else { format!("Could not start the sign-in: {}", reply["error"].as_str().unwrap_or("unknown")) };
            p.login = json!({"state": "starting"});
            p.opened = false; // a new attempt
        }
    }

    /// A key while a panel is open (or the open-panel chords); true when the panel took it.
    pub(crate) fn panel_key(&mut self, event: &KeyEvent) -> bool {
        let ctrl = self.ui.mods.control_key();
        if self.ui.panel.is_none() {
            if let (true, Key::Character(c)) = (ctrl, &event.logical_key) {
                match c.to_ascii_lowercase().as_str() {
                    "n" => {
                        self.panel_open(Step::Provider);
                        return true;
                    }
                    "a" => {
                        self.panel_open(Step::Accounts);
                        return true;
                    }
                    _ => {}
                }
            }
            return false;
        }
        let text_mode = self.text_mode();
        if event.repeat && !text_mode {
            return true; // a held key is not a second, deliberate press (a confirmation needs two); typing in the input line repeats
        }
        if let (true, Key::Character(c)) = (ctrl, &event.logical_key) {
            // chords work while the input line is taking a code
            match (c.to_lowercase().as_str(), self.ui.mods.shift_key()) {
                ("c", true) => self.login_address(true),
                ("o", _) => self.login_address(false),
                _ => return false,
            }
            self.dirty = true;
            return true;
        }
        let step = self.ui.panel.as_ref().map(|p| p.step).unwrap_or(Step::Provider);
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                match step {
                    Step::Actions | Step::Login | Step::Label | Step::Account | Step::Ready if step != Step::Actions => self.panel_close(),
                    Step::Actions => self.panel_open(Step::Accounts),
                    _ => self.panel_close(),
                }
                true
            }
            Key::Named(NamedKey::Enter) if text_mode => {
                let text = std::mem::take(&mut self.input);
                self.preedit.clear();
                self.submit_text(step, text.trim());
                self.dirty = true;
                true
            }
            Key::Named(NamedKey::Enter) if step == Step::Ready => {
                self.create_session();
                true
            }
            _ if text_mode => false, // typing (and the input method) go to the input line
            Key::Character(c) => {
                let key = c.to_lowercase();
                self.menu_key(step, key.chars().next().unwrap_or(' '));
                self.dirty = true;
                true
            }
            _ => true,
        }
    }

    fn submit_text(&mut self, step: Step, text: &str) {
        if text.is_empty() {
            return;
        }
        match step {
            Step::Label => {
                let provider = self.ui.panel.as_ref().map(|p| p.provider.clone()).unwrap_or_default();
                let reply = self.host_call(json!({"op": "account.create", "provider": provider, "label": text}));
                self.load_accounts();
                if let Some(p) = self.ui.panel.as_mut() {
                    if reply["ok"] == json!(1) {
                        p.account = reply["account"].as_str().unwrap_or("").to_string();
                        p.step = Step::Ready;
                        p.notice.clear();
                    } else {
                        p.notice = format!("Could not create the account: {}", reply["error"].as_str().unwrap_or("unknown"));
                    }
                }
            }
            Step::Login => {
                let account = self.ui.panel.as_ref().map(|p| p.account.clone()).unwrap_or_default();
                let reply = self.host_call(json!({"op": "account.login.input", "account": account, "code": text}));
                if let Some(p) = self.ui.panel.as_mut() {
                    p.login["needs_code"] = json!(false);
                    p.last_error = reply["error"].as_str().unwrap_or("").to_string();
                    p.notice = match (reply["ok"] == json!(1), p.last_error.as_str()) {
                        (true, _) => "Code sent.".into(),
                        (false, "no_code_expected") => "The sign-in did not take the code (no code was expected yet).".into(),
                        _ => "The sign-in did not take the code.".into(),
                    };
                }
            }
            _ => {}
        }
    }

    fn create_session(&mut self) {
        let Some(p) = self.ui.panel.as_ref() else { return };
        let reply = self.host_call(json!({"op": "session.create", "kind": p.provider, "account": p.account}));
        if reply["ok"] == json!(1) {
            let id = reply["session"].as_str().unwrap_or("").to_string();
            self.panel_close();
            self.send("{\"op\":\"status\"}");
            self.ui.scroll = 0;
            self.show(&id, None);
        } else if let Some(p) = self.ui.panel.as_mut() {
            p.notice = format!("The session was not created: {}", reply["error"].as_str().unwrap_or("unknown"));
        }
    }

    fn menu_key(&mut self, step: Step, key: char) {
        match step {
            Step::Provider => {
                let provider = match key {
                    '1' => "codex",
                    '2' => "claude",
                    _ => return,
                };
                self.load_accounts_for(provider);
            }
            Step::Account => {
                let Some(p) = self.ui.panel.as_ref() else { return };
                let mine: Vec<String> = p.accounts.iter().filter(|a| a.1 == p.provider).map(|a| a.0.clone()).collect();
                if key == 'n' {
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.step = Step::Label;
                    }
                } else if let Some(id) = key.to_digit(10).and_then(|d| mine.get((d as usize).wrapping_sub(1))) {
                    let id = id.clone();
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.account = id;
                        p.step = Step::Ready;
                    }
                }
            }
            Step::Ready if key == 'l' => self.start_login("browser"),
            Step::Login => match key {
                'c' => self.login_address(true),
                'o' => self.login_address(false),
                'd' if self.ui.panel.as_ref().map_or(false, |p| p.provider == "codex") => self.start_login("device"),
                _ => {}
            },
            Step::Accounts => {
                let Some(p) = self.ui.panel.as_ref() else { return };
                if let Some(a) = key.to_digit(10).and_then(|d| p.accounts.get((d as usize).wrapping_sub(1))).cloned() {
                    let status = self.host_call(json!({"op": "account.login.status", "account": a.0, "check": true}));
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.account = a.0;
                        p.provider = a.1;
                        p.login = json!({"signed_in": status["signed_in"]});
                        p.step = Step::Actions;
                        p.confirm = None;
                        p.notice.clear();
                    }
                }
            }
            Step::Actions => self.action_key(key),
            _ => {}
        }
    }

    /// The login address, on the person's request only: onto the clipboard, or to the browser.
    fn login_address(&mut self, copy: bool) {
        let Some(url) = self.ui.panel.as_ref().and_then(|p| p.login["url"].as_str().map(str::to_string)) else { return };
        if copy {
            self.copy_text(url);
        } else {
            let started = match std::process::Command::new("xdg-open").arg(&url).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
                Ok(mut child) => {
                    std::thread::spawn(move || {
                        let _ = child.wait(); // reaped, so it does not stay a zombie
                    });
                    true
                }
                Err(_) => false,
            };
            if let (false, Some(p)) = (started, self.ui.panel.as_mut()) {
                p.notice = "xdg-open is not available: copy the address (Ctrl+Shift+C) and open it yourself.".into();
            }
        }
    }

    fn load_accounts_for(&mut self, provider: &str) {
        self.load_accounts();
        if let Some(p) = self.ui.panel.as_mut() {
            p.provider = provider.to_string();
            p.step = Step::Account;
        }
    }

    fn action_key(&mut self, key: char) {
        let Some(p) = self.ui.panel.as_ref() else { return };
        let account = p.account.clone();
        let pending = p.confirm;
        if !matches!(key, 'd' | 'e') {
            if let Some(p) = self.ui.panel.as_mut() {
                p.confirm = None; // any other key withdraws a pending removal
            }
        }
        match key {
            's' => self.start_login("browser"),
            'f' => {
                let reply = self.host_call(json!({"op": "account.default", "account": account}));
                self.load_accounts();
                if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = if reply["ok"] == json!(1) { "Now the default.".into() } else { format!("Not changed: {}", reply["error"].as_str().unwrap_or("unknown")) };
                }
            }
            'd' | 'e' => {
                let confirmed = pending == Some(key);
                let sessions = if key == 'd' { "detach" } else { "end" };
                if !confirmed {
                    let affected = self.host_call(json!({"op": "account.affected", "account": account}));
                    if affected["ok"] != json!(1) {
                        if let Some(p) = self.ui.panel.as_mut() {
                            p.notice = "Could not list the sessions bound to it: nothing was removed.".into();
                        }
                        return;
                    }
                    let names: Vec<String> = affected["sessions"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(|s| s.chars().take(7).collect())).collect();
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.confirm = Some(key);
                        p.notice = format!("Sessions bound to it: {}. Press {} again to {}.", if names.is_empty() { "none".to_string() } else { names.join(", ") }, key.to_ascii_uppercase(), if key == 'd' { "remove it (they stay, unbound)" } else { "remove it and end them" });
                    }
                    return;
                }
                let reply = self.host_call(json!({"op": "account.remove", "account": account, "sessions": sessions}));
                if reply["ok"] == json!(1) {
                    self.panel_open(Step::Accounts);
                    if let Some(p) = self.ui.panel.as_mut() {
                        p.notice = "Removed. The provider's own login was not revoked.".into();
                    }
                } else if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = format!("Not removed: {}", reply["error"].as_str().unwrap_or("unknown"));
                }
            }
            _ => {}
        }
    }
}
