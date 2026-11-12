//! In-window account rename, session rebind, and permission-mode toggle (`M03-P01`).

use serde_json::json;

use super::super::{clean, App};
use super::confirm::{Act, Ask};
use super::{Panel, Step};

impl App {
    pub(super) fn acct_display(&self, a: &(String, String, String, bool)) -> String {
        self.ui.ssh_hosts.get(&a.0).map_or_else(|| a.2.clone(), |h| format!("{} [ssh: {h}]", a.2))
    }

    pub(super) fn account_name(&self, p: &Panel) -> String {
        p.accounts
            .iter()
            .find(|a| a.0 == p.account)
            .map_or_else(|| self.label_of(&p.account), |a| format!("{} / {}", a.1, self.acct_display(a)))
    }

    pub(super) fn load_ssh_hosts(&mut self) {
        let hosts: Vec<String> = if self.socket.is_empty() {
            Vec::new()
        } else {
            let reply = self.host_call_within(json!({"op": "ssh.hosts"}), 2);
            reply["hosts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|h| h["host"].as_str().map(clean))
                .filter(|s| !s.is_empty())
                .collect()
        };
        if let Some(p) = self.ui.panel.as_mut() {
            p.ssh_hosts = hosts;
        }
    }

    pub(super) fn cycle_label_host(&mut self, by: isize) {
        let Some(p) = self.ui.panel.as_mut() else { return };
        if p.ssh_hosts.is_empty() {
            p.new_ssh_host = None;
            return;
        }
        let mut choices: Vec<Option<String>> = vec![None];
        choices.extend(p.ssh_hosts.iter().cloned().map(Some));
        let cur = choices.iter().position(|c| *c == p.new_ssh_host).unwrap_or(0) as isize;
        let idx = (cur + by).rem_euclid(choices.len() as isize) as usize;
        p.new_ssh_host = choices[idx].clone();
        self.dirty = true;
    }

    pub(super) fn toggle_label_provider(&mut self) {
        if let Some(p) = self.ui.panel.as_mut() {
            p.provider = if p.provider == "codex" { "claude".into() } else { "codex".into() };
            self.dirty = true;
        }
    }

    pub(super) fn open_add_account_from_accounts(&mut self) {
        self.load_ssh_hosts();
        let _ = self.ed.take();
        self.preedit.clear();
        if let Some(p) = self.ui.panel.as_mut() {
            if p.provider.is_empty() {
                p.provider = "claude".into();
            }
            p.new_ssh_host = None;
            p.step = Step::Label;
            p.notice.clear();
        }
        self.dirty = true;
    }

    pub(super) fn accounts_or_label_click(&mut self, line: usize) -> bool {
        let Some(p) = self.ui.panel.as_ref() else { return false };
        match p.step {
            Step::Label => {
                if p.form.is_none() {
                    match line {
                        0 => {
                            self.toggle_label_provider();
                            return true;
                        }
                        1 => {
                            self.cycle_label_host(1);
                            return true;
                        }
                        _ => return false,
                    }
                } else if line == 0 {
                    self.cycle_label_host(1);
                    return true;
                }
                false
            }
            Step::Accounts => {
                let n = p.accounts.len();
                if line < n && line < 9 {
                    let ch = char::from_digit((line + 1) as u32, 10).unwrap_or(' ');
                    self.panel_choice(Step::Accounts, ch);
                    return true;
                }
                let add_row = if n == 0 { 2 } else { n + 1 };
                if line == add_row {
                    self.open_add_account_from_accounts();
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Start renaming the currently selected account inside `Step::Actions`.
    pub(super) fn account_rename_start(&mut self) {
        let current = self
            .ui
            .panel
            .as_ref()
            .and_then(|p| p.accounts.iter().find(|a| a.0 == p.account).map(|a| a.2.clone()))
            .unwrap_or_default();
        let _ = self.ed.take();
        self.preedit.clear();
        self.ed.put(&current);
        if let Some(p) = self.ui.panel.as_mut() {
            p.step = Step::AccountRename;
            p.notice.clear();
        }
    }

    /// Submit a new account label on `Enter` in `Step::AccountRename`.
    pub(super) fn account_rename_submit(&mut self, text: &str) {
        let account = self.ui.panel.as_ref().map(|p| p.account.clone()).unwrap_or_default();
        if !crate::ipc::valid_label(&json!(text)) {
            self.ed.put(text);
            if let Some(p) = self.ui.panel.as_mut() {
                p.notice = "Not renamed: a label has 1 to 40 visible characters and no line break.".into();
            }
            return;
        }
        let reply = self.host_call(json!({"op": "account.rename", "account": account, "label": text}));
        if reply["ok"] == json!(1) {
            self.ui.labels.insert(account, clean(text));
            self.load_accounts();
            self.send("{\"op\":\"account.list\"}");
            self.refresh_list();
            if let Some(p) = self.ui.panel.as_mut() {
                p.step = Step::Actions;
                p.last_error.clear();
                p.notice = "Renamed.".into();
            }
        } else {
            self.ed.put(text);
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            if let Some(p) = self.ui.panel.as_mut() {
                p.last_error = err.clone();
                p.notice = format!("Could not rename the account: {err}");
            }
        }
    }

    /// Why the open session cannot be rebound to another account right now (`None`: it can).
    pub(crate) fn rebind_reason(&self) -> Option<&'static str> {
        match self.selected_row() {
            None => Some("no session is open"),
            Some(r) if r.provider.is_empty() => Some("not a provider session"),
            Some(r) if r.state == "running" => Some("close the session first"),
            Some(r) if r.state != "disconnected" => Some("the session has ended"),
            Some(_) => None,
        }
    }

    /// Open `Step::Rebind` for the open disconnected session (`Session > Rebind account...` / `Ctrl+Shift+B`).
    pub(crate) fn open_rebind(&mut self) {
        if let Some(reason) = self.rebind_reason() {
            self.ui.flash = Some(format!("Rebind account: {reason}"));
            self.dirty = true;
            return;
        }
        let Some(row) = self.selected_row() else { return };
        let (session, generation, provider, current_account, title) = (
            row.session.clone(),
            row.generation,
            row.provider.clone(),
            row.account.clone(),
            row.title(40).trim_end().to_string(),
        );
        self.panel_open(Step::Rebind);
        self.load_accounts();
        if let Some(p) = self.ui.panel.as_mut() {
            p.accounts.retain(|a| a.1 == provider);
            p.provider = provider;
            p.account = current_account;
            p.ask = Some(Ask {
                act: Act::Rename,
                session,
                generation,
                title,
                working: false,
                cwd: String::new(),
                worktree_branch: String::new(),
                worktree_repo: String::new(),
                worktree_exists: false,
                dirty_confirm: false,
            });
        }
    }

    pub(super) fn rebind_view(&self, p: &Panel) -> (String, Vec<String>) {
        let mut lines = Vec::new();
        let label = p.ask.as_ref().map_or("session", |a| a.title.as_str());
        lines.push(format!("Choose a {} account for '{label}' (starts a fresh provider thread):", p.provider));
        if p.accounts.is_empty() {
            lines.push(format!("No {} account exists yet (Ctrl+N creates one).", p.provider));
        } else {
            for (i, a) in p.accounts.iter().enumerate().take(9) {
                let def = if a.3 { "  (default)" } else { "" };
                let cur = if a.0 == p.account { "  (current)" } else { "" };
                lines.push(format!("{}  {} / {}{def}{cur}", i + 1, a.1, self.acct_display(a)));
            }
        }
        ("Rebind session to account".into(), lines)
    }

    pub(super) fn rebind_choice(&mut self, key: char) {
        let Some((session, generation, target)) = self.ui.panel.as_ref().and_then(|p| {
            let ask = p.ask.as_ref()?;
            let idx = key.to_digit(10)?.checked_sub(1)? as usize;
            let acc = p.accounts.get(idx)?;
            Some((ask.session.clone(), ask.generation, acc.0.clone()))
        }) else {
            return;
        };
        let reply = self.host_call(json!({"op": "session.rebind", "session": session, "generation": generation, "account": target}));
        if reply["ok"] == json!(1) {
            if let Some(g) = reply["generation"].as_i64() {
                self.generation = g;
            }
            self.panel_close();
            self.send("{\"op\":\"status\"}");
            self.send("{\"op\":\"account.list\"}");
            self.ui.flash = Some("Account changed; session resumed.".into());
        } else {
            let err = reply["error"].as_str().unwrap_or("unknown").to_string();
            if let Some(p) = self.ui.panel.as_mut() {
                p.last_error = err.clone();
                p.notice = match err.as_str() {
                    "busy" | "no_conversation_yet" => "Not rebound: this session has no conversation to move yet, or is still running.".into(),
                    "stale_generation" => "Not rebound: the session changed since this panel opened; reopen it and try again.".into(),
                    other => format!("Not rebound: {other}"),
                };
            }
        }
    }

    /// Why the open session's permission mode cannot be switched right now (`None`: it can).
    pub(crate) fn mode_reason(&self) -> Option<&'static str> {
        match self.selected_row() {
            None => Some("no session is open"),
            Some(r) if r.provider.is_empty() => Some("not a provider session"),
            Some(r) if r.state == "exited" => Some("the session has ended"),
            Some(r) if r.holds_a_turn() => Some("a turn is open"),
            Some(_) => None,
        }
    }

    /// Toggle the open session's permission mode (`"auto"` <-> `"default"`, `Ctrl+Shift+M` / `Session` menu).
    pub(crate) fn toggle_session_mode(&mut self) {
        if let Some(reason) = self.mode_reason() {
            self.ui.flash = Some(format!("Permission mode: {reason}"));
            self.dirty = true;
            return;
        }
        let Some((session, generation, next)) = self.selected_row().map(|r| {
            let next = if r.mode == "default" { "auto" } else { "default" };
            (r.session.clone(), r.generation, next)
        }) else {
            return;
        };
        let reply = self.host_call(json!({"op": "session.mode", "session": session, "generation": generation, "mode": next}));
        if reply["ok"] == json!(1) {
            let out_gen = reply["generation"].as_i64().unwrap_or(generation);
            self.generation = out_gen;
            if let Some(row) = self.ui.rows.iter_mut().find(|r| r.session == session) {
                row.mode = next.to_string();
                row.generation = out_gen;
            }
            self.send("{\"op\":\"status\"}");
            self.ui.flash = Some(if next == "auto" {
                "Permission mode: auto (permissions skipped).".into()
            } else {
                "Permission mode: asks for approval.".into()
            });
        } else {
            let err = reply["error"].as_str().unwrap_or("unknown");
            self.ui.flash = Some(format!("Permission mode: {err}"));
        }
        self.dirty = true;
    }
}
