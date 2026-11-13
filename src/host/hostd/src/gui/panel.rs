//! New session, accounts and sign-in inside the window. A step is the new-session form (newsession.rs) or a short list that a key answers
//! (the input line is the one text field: a label, a code the provider asked for, or the folder of the form). Everything is a request to the host on a connection of its own;
//! a pasted code goes to the host's login operation and is cleared from the window at once. Nothing here reads a provider's files.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::App;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Step {
    NewSession,
    Label,
    Login,
    Accounts,
    Actions,
    Keys,
    About,
    Rename,  // the new name of a session, on the input line
    Confirm, // a question about a session that Y answers
    Send,    // text of the box or lines of the screen, for another session (sendto.rs)
    Card,    // the whole text behind a card of the host's own (cardview.rs)
    AccountRename, // a new label for an account, on the input line (admin_ui.rs)
    Rebind,        // choose another account of the same provider for a closed session (admin_ui.rs)
    Files,         // on-demand working-directory file viewer (files_ui.rs)
}

pub(crate) use super::jobs::{AsyncDone, AsyncJob};

pub struct Panel {
    pub step: Step,
    pub provider: String,
    pub account: String,
    pub notice: String,
    pub accounts: Vec<(String, String, String, bool)>, // id, provider, label, default
    pub new_ssh_host: Option<String>, // None = local, Some(alias) = SSH remote target when adding an account
    pub ssh_hosts: Vec<String>,       // discovered SSH host aliases from `ssh.hosts`
    pub pending_check_req: u64,       // non-zero while checking SSH Codex login status asynchronously
    pub login: Value,
    pub confirm: Option<char>,
    pub last_error: String, // the host's raw error word of the last failed step: in the ui dump only
    pub mode: &'static str, // the mode a new session starts in: "auto" (permissions skipped) or "default" (asks for approval)
    pub opened: bool, // the browser was opened for this sign-in attempt
    pub stash: super::editor::Parked, // what was typed in the input line before the panel took it over
    pub form: Option<newsession::Form>, // the new-session form: what it holds stays while the label or the sign-in is asked for
    pub ask: Option<confirm::Ask>, // the session that a rename or a confirmation is about
    pub send: Option<sendto::Outgoing>, // what the send-to-session panel holds
    pub card: Option<cardview::View>, // the text of a request, as far as it was read
    pub files: Option<files_ui::Explorer>, // on-demand working-directory viewer
}

/// The window opens the sign-in address by itself unless this is set (read by the window only; the tests set it where no browser is wanted).
fn auto_open_off() -> bool {
    std::env::var_os("LICO_NO_AUTO_OPEN").is_some()
}

/// How long the window waits for an ordinary reply of the host.
const CALL_SECONDS: u64 = 8;

/// One request on a connection of its own, waiting at most `seconds` for the reply.
pub(crate) fn call_socket_within(socket: &str, request: Value, seconds: u64) -> Value {
    let Ok(mut conn) = UnixStream::connect(socket) else { return json!({"ok": 0, "error": "host_unreachable"}) };
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

impl App {
    pub(crate) fn host_call(&self, request: Value) -> Value {
        self.host_call_within(request, CALL_SECONDS)
    }

    /// One request on a connection of its own, waiting at most `seconds` for the reply (the window stands still meanwhile).
    pub(crate) fn host_call_within(&self, request: Value, seconds: u64) -> Value {
        call_socket_within(&self.socket, request, seconds)
    }

    /// Non-blocking host call on a background worker thread, delivering `Ev::Async(AsyncDone)` back to the event loop.
    pub(crate) fn spawn_host_call(&mut self, request: Value, seconds: u64, make_job: impl FnOnce(u64) -> AsyncJob) -> u64 {
        self.ui.async_seq = self.ui.async_seq.wrapping_add(1).max(1);
        let req_id = self.ui.async_seq;
        let job = make_job(req_id);
        if let Some(proxy) = self.ui.proxy.clone() {
            let socket = self.socket.clone();
            std::thread::spawn(move || {
                let reply = call_socket_within(&socket, request, seconds);
                let _ = proxy.send_event(super::Ev::Async(AsyncDone { job, reply }));
            });
        } else {
            let reply = call_socket_within(&self.socket, request, seconds);
            self.on_async_done(AsyncDone { job, reply });
        }
        req_id
    }

    pub(crate) fn on_async_done(&mut self, done: AsyncDone) {
        match done.job {
            AsyncJob::FilesDir { .. } | AsyncJob::FilesPreview { .. } => self.on_files_async(done.job, done.reply),
            AsyncJob::AccountCheck { req_id, account } => {
                if let Some(p) = self
                    .ui
                    .panel
                    .as_mut()
                    .filter(|p| p.step == Step::Actions && p.account == account && p.pending_check_req == req_id)
                {
                    p.pending_check_req = 0;
                    p.login = json!({"signed_in": done.reply["signed_in"]});
                    self.dirty = true;
                }
            }
            AsyncJob::SessionStart { req_id, created_cwd } => self.on_form_start_done(req_id, created_cwd, done.reply),
            AsyncJob::SidebarDir { .. } => self.on_sidebar_async(done.job, done.reply),
            AsyncJob::SidebarNavSave => self.on_sidebar_nav_saved(done.reply),
            AsyncJob::SidebarChats { .. } | AsyncJob::ChatResume { .. } => self.on_chats_async(done.job, done.reply),
            AsyncJob::NewChatLast { .. } | AsyncJob::NewChatStart { .. } => self.on_newchat_async(done.job, done.reply),
            AsyncJob::LlmFind { .. } => self.on_llm_async(done.job, done.reply),
            AsyncJob::CallGet { .. } | AsyncJob::CallSay { .. } | AsyncJob::CallStart | AsyncJob::CallAction => self.on_call_async(done.job, done.reply),
        }
    }

    fn load_accounts(&mut self) {
        let reply = self.host_call(json!({"op": "account.list"}));
        self.sync_accounts_value(&reply);
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
        super::events::QUIET_INPUT.store(self.ui.panel.is_some(), std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn panel_open(&mut self, step: Step) {
        self.search_close();
        self.panel_close(); // one panel at a time: the one that was open gives back the draft it took and ends its sign-in
        (self.ui.sel, self.ui.dragging, self.ui.drag_origin, self.ui.drag_moved) = (None, false, None, false);
        let stash = self.ed.take();
        self.preedit.clear();
        self.ui.panel = Some(Panel { step, provider: String::new(), account: String::new(), notice: String::new(), accounts: Vec::new(), new_ssh_host: None, ssh_hosts: Vec::new(), pending_check_req: 0, login: Value::Null, confirm: None, last_error: String::new(), mode: "auto", opened: false, stash, form: None, ask: None, send: None, card: None, files: None });
        self.sync_quiet();
        if step == Step::Accounts {
            self.load_accounts();
        }
        self.dirty = true;
    }

    pub(crate) fn panel_close(&mut self) {
        let closed = self.ui.panel.is_some();
        (self.ui.sel, self.ui.dragging, self.ui.drag_origin, self.ui.drag_moved) = (None, false, None, false);
        if let Some(p) = self.ui.panel.take() {
            if p.step == Step::Login && !p.account.is_empty() {
                self.host_call(json!({"op": "account.login.cancel", "account": p.account})); // leaving the panel ends an unfinished sign-in
            }
            self.ed.restore(p.stash);
            self.preedit.clear();
        }
        if closed {
            crate::memory::trim_heap();
        }
        self.sync_quiet();
        self.dirty = true;
    }

    pub(crate) fn text_mode(&self) -> bool {
        self.ui.panel.as_ref().map_or(false, |p| matches!(p.step, Step::Label | Step::Rename | Step::AccountRename) || p.text_field() || (p.step == Step::Login && p.login["needs_code"] == json!(true)))
    }

    /// The sign-in code is being typed: it is drawn as stars and never put in the window's model.
    pub(crate) fn code_entry(&self) -> bool {
        self.ui.panel.as_ref().map_or(false, |p| p.step == Step::Login) && self.text_mode()
    }

    /// What the input line shows: the text itself, or one star per character of a code (at most 64).
    pub(crate) fn shown_input(&self) -> String {
        if self.code_entry() {
            "*".repeat(self.ed.text().chars().count().min(64))
        } else {
            self.ed.text().to_string()
        }
    }

    /// The title and the lines of what the panel shows now, or the first screen when no session is selected.
    pub(crate) fn panel_view(&self) -> Option<(String, Vec<String>)> {
        let Some(p) = self.ui.panel.as_ref() else {
            if self.session.is_empty() && !self.active_call_open() {
                return Some(("select a session".into(), self.start_lines()));
            }
            return None;
        };
        let mut body = Vec::new();
        let title = match p.step {
            Step::NewSession => {
                body.extend(self.form_lines(p));
                "New session".to_string()
            }
            Step::Label => {
                if p.form.is_none() {
                    body.push(format!("Provider: < {} >   (Tab or click to switch)", p.provider));
                }
                let host_txt = p.new_ssh_host.as_deref().map_or_else(
                    || "local (this machine)".to_string(),
                    |h| format!("ssh: {h} (authenticates & runs on {h})"),
                );
                if p.ssh_hosts.is_empty() {
                    body.push(format!("Target:   < {host_txt} >   (no hosts in ~/.ssh/config)"));
                } else {
                    body.push(format!("Target:   < {host_txt} >   (Up/Down or click to switch SSH host)"));
                }
                body.push(String::new());
                body.push("Type a label for the account and press Enter.".into());
                match &p.new_ssh_host {
                    Some(h) => format!("New {} account [ssh: {h}]", p.provider),
                    None => format!("New {} account", p.provider),
                }
            }
            Step::Login => {
                let state = p.login["state"].as_str().unwrap_or("starting");
                body.push(format!("state: {state}"));
                if let Some(h) = self.ui.ssh_hosts.get(&p.account) {
                    body.push(format!("host: ssh:{h} (runs login on {h}; remote connects directly to provider)"));
                }
                if let Some(url) = p.login["url"].as_str() {
                    body.push("Open this address and sign in:".into());
                    body.push(url.to_string());
                    body.push(if auto_open_off() { "Ctrl+C  copy the address    Ctrl+O  open it in the browser" } else { "Opening the browser. Ctrl+O opens it again, Ctrl+C copies the address." }.into());
                }
                if p.login["needs_code"] == json!(true) {
                    body.push("The provider asks for a code: paste it below and press Enter.".into());
                } else if p.provider == "codex" && p.login["state"] != json!("succeeded") {
                    body.push("D  use the device-code sign-in instead".into());
                }
                format!("Sign in: {}", self.account_name(p))
            }
            Step::Accounts => {
                for (i, a) in p.accounts.iter().enumerate() {
                    let q = self.ui.quotas.get(&a.0).map(|&(w, s)| super::ui::row::format_quota_detail(w, s)).filter(|s| !s.is_empty()).map(|s| format!("  [{s}]")).unwrap_or_default();
                    body.push(format!("{}  {} / {}{}{q}", i + 1, a.1, self.acct_display(a), if a.3 { "  (default)" } else { "" }));
                }
                if p.accounts.is_empty() {
                    body.push("No account yet: press A to add one, or Ctrl+N with a session.".into());
                }
                body.push(String::new());
                body.push("A  add a new account (local or SSH remote)".into());
                "Accounts".to_string()
            }
            Step::Send => {
                let (heading, lines) = self.send_view(p);
                body.extend(lines);
                heading
            }
            Step::Card => {
                let (heading, lines) = self.card_view(p);
                body.extend(lines);
                heading
            }
            Step::Rename | Step::Confirm => {
                let (heading, lines) = self.ask_view(p);
                body.extend(lines);
                heading
            }
            Step::AccountRename => {
                body.push("Type a new label for the account and press Enter.".into());
                format!("Rename account: {}", self.account_name(p))
            }
            Step::Rebind => {
                let (heading, lines) = self.rebind_view(p);
                body.extend(lines);
                heading
            }
            Step::Files => {
                let (heading, lines) = self.files_view(p);
                body.extend(lines);
                heading
            }
            Step::Keys => {
                body.extend([
                    "Ctrl+N new session   Ctrl+A accounts   Ctrl+E working directory files (F5 refresh)   Ctrl+Q quit", "F10 or Alt+F, Alt+S, Alt+V, Alt+H  the menus (arrows, Enter, Esc)", "Ctrl+Tab, Ctrl+Shift+Tab  next, previous session   Alt+1 to Alt+9  session by number",
                    "Esc (empty input line)  back to the start screen", "Y allow, N deny the pending approval (input line empty); S allow for the session, V read it all (a request of Lico's own)", "Ctrl+V pastes text, or a PNG picture into a Claude session (Esc removes it)   Ctrl+Shift+P captures the screen into a Claude session   Shift+Insert or the middle button pastes the selected text   Ctrl+C (or Ctrl+Shift+C) copy selected lines or prompt text   Ctrl+X cut prompt selection   Ctrl+Z, Ctrl+Y undo and redo   Ctrl+U clear the line",
                    "Page Up, Page Down, End  scroll the output   Ctrl+F  find in conversation   Ctrl+Shift+F  search all sessions",
                    "Ctrl+Shift+G  group by account or workspace   Ctrl+=, Ctrl+-, Ctrl+0  text size (12 to 20 px)",
                    "F2 rename   Ctrl+Shift+W close   Ctrl+Delete delete   Ctrl+Shift+B rebind account   Ctrl+Shift+M toggle permission mode",
                    "Ctrl+Shift+S  send the input box, or the selected lines, to another session",
                ].map(String::from));
                "Keys".to_string()
            }
            Step::About => {
                body.extend([crate::ident::window_line(), "One program: this window, the host and the command line (`lico licenses` prints the licenses).".to_string()]);
                "About Lico".to_string()
            }
            Step::Actions => {
                body.extend(["S  sign in".to_string(), "R  rename this account".to_string(), "F  make it the default for its provider".to_string(), "D  remove it (its sessions stay, unbound)".to_string(), "E  remove it and end its sessions".to_string()]);
                if let Some(h) = self.ui.ssh_hosts.get(&p.account) {
                    body.push(format!("host: ssh:{h} (authenticates on {h}; connects directly to provider)"));
                }
                if p.login["checking"] == json!(true) {
                    body.push("provider says: checking over ssh...".into());
                } else if let Some(s) = p.login["signed_in"].as_bool() {
                    body.push(format!("provider says: {}", if s { "signed in" } else { "not signed in" }));
                } else if p.login.get("signed_in").is_some() {
                    body.push("provider says: unknown (no status command)".into());
                }
                if let Some(&(w, s)) = self.ui.quotas.get(&p.account) {
                    let q = super::ui::row::format_quota_detail(w, s);
                    if !q.is_empty() {
                        body.push(format!("quota: {q}"));
                    }
                }
                format!("Account: {}", self.account_name(p))
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
            (Some((title, body)), Some(p)) => json!({"open": true, "title": title, "body": body, "text_entry": self.text_mode(), "step": p.step as u8, "input": self.shown_input(), "input_len": self.ed.text().chars().count(), "error": p.last_error, "form": self.form_json(), "files": self.files_json()}),
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
                p.step = if p.form.is_some() { Step::NewSession } else { Step::Actions }; // back to where the sign-in was asked for
                p.notice = "Signed in.".into();
            } else if p.login["url"].is_string() && !p.opened {
                p.opened = true; // once per attempt, however often the state is polled
                open_now = true;
            }
        }
        if open_now && !auto_open_off() {
            self.login_address(false);
        }
        if finished && self.ui.panel.as_ref().map_or(false, |p| p.form.is_some()) {
            self.form_signed_in();
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
            if self.lifecycle_key(event) {
                return true;
            }
            if ctrl {
                if let Some(ch) = super::key_letter(event) {
                    match ch {
                        'n' => {
                            if !event.repeat {
                                if self.ui.mods.shift_key() {
                                    self.form_open_global();
                                } else {
                                    self.form_open();
                                }
                            }
                            return true;
                        }
                        'a' => {
                            self.panel_open(Step::Accounts);
                            return true;
                        }
                        'r' => {
                            self.resume_session();
                            return true;
                        }
                        's' if self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.send_ask();
                            }
                            return true;
                        }
                        'b' if self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.open_rebind();
                            }
                            return true;
                        }
                        'm' if self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.toggle_session_mode();
                            }
                            return true;
                        }
                        'g' if self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.toggle_group_by();
                            }
                            return true;
                        }
                        'e' if self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.toggle_show_events();
                            }
                            return true;
                        }
                        'e' if !self.ui.mods.alt_key() => {
                            if !event.repeat {
                                self.files_open();
                            }
                            return true;
                        }
                        'h' if !self.ui.mods.alt_key() && !self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.toggle_hidden_files();
                            }
                            return true;
                        }
                        _ => {}
                    }
                }
                if let Key::Character(c) = &event.logical_key {
                    match c.as_str() {
                        "=" | "+" if !self.ui.mods.alt_key() => {
                            if !event.repeat {
                                self.step_font_px(1);
                            }
                            return true;
                        }
                        "-" if !self.ui.mods.alt_key() && !self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.step_font_px(-1);
                            }
                            return true;
                        }
                        "0" if !self.ui.mods.alt_key() && !self.ui.mods.shift_key() => {
                            if !event.repeat {
                                self.step_font_px(0);
                            }
                            return true;
                        }
                        _ => {}
                    }
                }
            }
            return false;
        }
        if self.nav_key(event) {
            return true;
        }
        if self.form_key(event) || self.files_key(event) {
            return true;
        }
        let text_mode = self.text_mode();
        if event.repeat && !text_mode {
            return true; // a held key is not a second, deliberate press (a confirmation needs two); typing in the input line repeats
        }
        if let (true, Some(ch)) = (ctrl, super::key_letter(event)) {
            // chords work while the input line is taking a code
            let on_login = self.ui.panel.as_ref().is_some_and(|p| p.step == Step::Login);
            match ch {
                'c' if on_login && self.ui.sel.is_none() => self.login_address(true),
                'o' if on_login => self.login_address(false),
                _ => return false,
            }
            self.dirty = true;
            return true;
        }
        let step = self.ui.panel.as_ref().map(|p| p.step).unwrap_or(Step::NewSession);
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                match step {
                    Step::Actions | Step::Label => self.panel_open(Step::Accounts),
                    Step::AccountRename => {
                        let _ = self.ed.take();
                        self.preedit.clear();
                        if let Some(p) = self.ui.panel.as_mut() {
                            p.step = Step::Actions;
                            p.notice.clear();
                        }
                    }
                    _ => self.panel_close(),
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) if step == Step::Label => {
                self.cycle_label_host(-1);
                true
            }
            Key::Named(NamedKey::ArrowDown) if step == Step::Label => {
                self.cycle_label_host(1);
                true
            }
            Key::Named(NamedKey::Tab) if step == Step::Label => {
                self.toggle_label_provider();
                true
            }
            Key::Named(NamedKey::Enter) if step == Step::Send => {
                self.send_go();
                true
            }
            Key::Named(key) if step == Step::Card && self.card_named(key) => true,
            Key::Named(NamedKey::Enter) if text_mode => {
                let text = self.ed.take().text();
                self.preedit.clear();
                self.submit_text(step, text.trim());
                self.dirty = true;
                true
            }
            _ if text_mode => false, // typing (and the input method) go to the input line
            _ => {
                let choice = super::key_letter_cased(event, self.ui.mods.shift_key()).or_else(|| match &event.logical_key {
                    Key::Character(c) => c.chars().next(),
                    _ => None,
                });
                if let Some(ch) = choice {
                    let ch = if step == Step::Card { ch } else { ch.to_ascii_lowercase() };
                    self.panel_choice(step, ch);
                    self.dirty = true;
                }
                true
            }
        }
    }

    /// A click on a line of a panel: the Y or N of a question, a choice of the send panel; false when the line is neither.
    pub(crate) fn panel_click(&mut self, line: usize) -> bool {
        self.confirm_click(line) || self.send_click(line) || self.files_click(line) || self.accounts_or_label_click(line)
    }

    fn submit_text(&mut self, step: Step, text: &str) {
        if text.is_empty() && step != Step::Rename {
            return; // an empty name is a name taken away; the other lines have nothing to send
        }
        match step {
            Step::Rename => self.rename_submit(text),
            Step::AccountRename => self.account_rename_submit(text),
            Step::Label => {
                let (provider, ssh_host, from_form) = self
                    .ui
                    .panel
                    .as_ref()
                    .map(|p| (p.provider.clone(), p.new_ssh_host.clone(), p.form.is_some()))
                    .unwrap_or_default();
                let mut req = json!({"op": "account.create", "provider": provider, "label": text});
                if let Some(h) = &ssh_host {
                    req["ssh_host"] = json!(h);
                }
                let reply = self.host_call(req);
                self.load_accounts();
                self.refresh_list();
                if reply["ok"] == json!(1) {
                    let new_id = reply["account"].as_str().unwrap_or("").to_string();
                    if from_form {
                        self.form_account_added(new_id);
                    } else if let Some(p) = self.ui.panel.as_mut() {
                        p.account = new_id;
                        p.step = Step::Actions;
                        p.confirm = None;
                        p.login = json!({});
                        p.notice = "Account added. Press S to sign in.".into();
                    }
                } else if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = format!("Could not create the account: {}", reply["error"].as_str().unwrap_or("unknown"));
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

    pub(super) fn panel_choice(&mut self, step: Step, key: char) {
        match step {
            Step::Login => match key {
                'c' => self.login_address(true),
                'o' => self.login_address(false),
                'd' if self.ui.panel.as_ref().map_or(false, |p| p.provider == "codex") => self.start_login("device"),
                _ => {}
            },
            Step::Accounts => {
                if key == 'a' {
                    self.open_add_account_from_accounts();
                    return;
                }
                let Some(p) = self.ui.panel.as_ref() else { return };
                if let Some(a) = key.to_digit(10).and_then(|d| p.accounts.get((d as usize).wrapping_sub(1))).cloned() {
                    if self.ui.ssh_hosts.contains_key(&a.0) && a.1 == "codex" {
                        let acct_id = a.0.clone();
                        if let Some(p) = self.ui.panel.as_mut() {
                            p.account = a.0;
                            p.provider = a.1;
                            p.login = json!({"checking": true});
                            p.step = Step::Actions;
                            p.confirm = None;
                            p.notice.clear();
                        }
                        let req = json!({"op": "account.login.status", "account": acct_id.clone(), "check": true});
                        let has_proxy = self.ui.proxy.is_some();
                        let req_id = self.spawn_host_call(req, 6, move |req_id| AsyncJob::AccountCheck { req_id, account: acct_id });
                        if has_proxy {
                            if let Some(p) = self.ui.panel.as_mut() {
                                p.pending_check_req = req_id;
                            }
                        }
                    } else {
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
            }
            Step::Actions => self.action_key(key),
            Step::Confirm => self.confirm_key(key),
            Step::Send => self.send_key(key),
            Step::Card => self.card_key(key),
            Step::Rebind => self.rebind_choice(key),
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
                p.notice = "xdg-open is not available: copy the address (Ctrl+C) and open it yourself.".into();
            }
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
            'r' => self.account_rename_start(),
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

#[path = "admin_ui.rs"]
mod admin_ui;
#[path = "cardview.rs"]
pub(super) mod cardview;
#[path = "confirm.rs"]
pub(super) mod confirm;
#[path = "files_host.rs"]
mod files_host;
#[path = "files_render.rs"]
mod files_render;
#[path = "files_ui.rs"]
pub(super) mod files_ui;
#[path = "newsession.rs"]
mod newsession;
#[path = "sendto.rs"]
pub(super) mod sendto;

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
