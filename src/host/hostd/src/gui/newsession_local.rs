//! The Local provider of the new-session form (M16-P07): the one look for model servers (only when the person asks on the Server row), what it found, the form on Local from
//! "+ New chat", and the way back to the other providers for the forms that name a folder. Kept in its own file so that the form stays under the size limit.

use super::*;

impl App {
    /// The form on Local with the given server and model (from + New chat's Options...): the one server is offered at once, no look is made.
    pub(crate) fn form_choose_local(&mut self, addr: &str, api: &str, model: &str) {
        let server = Server { addr: addr.into(), api: api.into(), models: vec![model.into()], wants_key: false, remembered: true };
        self.with_form(|p, f| {
            (p.provider, f.local, f.focus) = ("local".into(), true, Field::Server);
            p.account.clear();
            (f.servers, f.kept, f.server, f.local_model) = (vec![server.clone()], Some(server), 0, model.into());
        });
        self.form_resume();
    }

    /// The forms that name a folder or a host are for the other providers: a form that opened on Local (the last choice) goes to the first provider with an account.
    pub(super) fn form_leave_local(&mut self) {
        self.with_form(|p, f| {
            if f.local {
                f.local = false;
                p.provider = PROVIDERS.iter().copied().filter(|k| *k != "local").find(|k| p.accounts.iter().any(|a| a.1 == *k)).unwrap_or(PROVIDERS[0]).to_string();
                p.account = p.first_account();
            }
        });
    }

    /// The one look for model servers (`llm.discover`: on this machine, read-only), only when the person asks on the Server row. It runs off the window's thread; its answer is
    /// `on_local_scan`. What it finds stays in the form.
    pub(super) fn form_scan(&mut self) {
        if self.ui.panel.as_ref().and_then(|p| p.form.as_ref()).map_or(true, |f| f.scan_req != 0) {
            return;
        }
        if let Some(p) = self.ui.panel.as_mut() {
            p.notice = "Looking for model servers on this machine...".into();
        }
        self.dirty = true;
        let has_proxy = self.ui.proxy.is_some();
        let req_id = self.spawn_host_call(json!({"op": "llm.discover", "all_users": false}), 14, |req_id| super::super::AsyncJob::LocalScan { req_id });
        if has_proxy {
            self.with_form(|_, f| f.scan_req = req_id); // (without an event loop the answer has already come)
        }
    }

    pub(crate) fn on_local_scan(&mut self, req_id: u64, reply: Value) {
        let has_proxy = self.ui.proxy.is_some();
        self.with_form(|p, f| {
            if has_proxy && f.scan_req != req_id {
                return;
            }
            f.scan_req = 0;
            if reply["ok"] != json!(1) {
                p.notice = format!("Could not look for servers: {}", reply["error"].as_str().unwrap_or("unknown"));
                return;
            }
            let found = local_rows::merge(local_rows::servers(&reply), f.kept.as_ref());
            let (chosen, model) = (f.servers.get(f.server).map(|s| s.addr.clone()), f.local_model.clone());
            f.server = chosen.and_then(|a| found.iter().position(|s| s.addr == a)).unwrap_or(0);
            f.servers = found;
            f.local_model = model;
            keep_model(f);
            if f.servers.is_empty() {
                f.local_model.clear();
            }
            f.scanned = true;
            let n = f.servers.len();
            p.notice = if n == 0 { "No model server was found on this machine.".into() } else { format!("Found {n} server{}.", if n == 1 { "" } else { "s" }) };
        });
        self.dirty = true;
    }
}

/// After the server changed: the model stays when the new server lists it, else the server's first.
pub(super) fn keep_model(f: &mut Form) {
    if let Some(s) = f.servers.get(f.server) {
        if !s.models.contains(&f.local_model) {
            f.local_model = s.models.first().cloned().unwrap_or_default();
        }
    }
}

