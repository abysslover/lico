//! `Native` in the window (`M11-P01`): the name of the provider's own default home beside the input target and the Settings menu entries. `M22-P03`: the first-run question
//! `Use your existing Claude/Codex login and its chats?` is gone; the provider's own home is on by default (`M22-P02`: the host never says `ask`), and the Settings menu
//! turns it off.

use serde_json::{json, Value};

use super::ui::Row;
use super::{clean, App};

/// One Native entry as `account.list` reports it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeEntry {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub state: String,
    pub chosen: String, // `on`, `off`, or empty while not answered
    pub chats: Option<u64>,
}

/// The Native entries of an `account.list` answer.
pub(super) fn parse_entries(list: &Value) -> Vec<NativeEntry> {
    list["native"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| NativeEntry {
            id: n["account"].as_str().unwrap_or("").to_string(),
            provider: n["provider"].as_str().unwrap_or("").to_string(),
            label: clean(n["label"].as_str().unwrap_or("")),
            state: n["state"].as_str().unwrap_or("").to_string(),
            chosen: n["chosen"].as_str().unwrap_or("").to_string(),
            chats: n["chats"].as_u64(),
        })
        .collect()
}

fn provider_name(provider: &str) -> &'static str {
    if provider == "codex" { "Codex" } else { "Claude Code" }
}

impl App {
    /// The account of a row as the person reads it: the managed account's label, `Native (~/.claude)` for a session that runs in the provider's own default home
    /// (AUTH-06: never `no account` for a session that does run somewhere), else `no account`.
    pub(crate) fn row_label(&self, row: &Row) -> String {
        if row.account.is_empty() && !row.native.is_empty() {
            row.native.clone()
        } else {
            self.label_of(&row.account)
        }
    }

    /// An `account.list` answer arrived: keep the Native entries (the Settings menu and the account line read them).
    pub(crate) fn native_seen(&mut self, list: &Value) {
        self.ui.sidebar.native_entries = parse_entries(list);
    }

    /// The Settings menu entry of a provider: `Native Claude Code login (~/.claude): on`.
    pub(crate) fn native_menu_label(&self, provider: &str) -> String {
        match self.ui.sidebar.native_entries.iter().find(|e| e.provider == provider) {
            Some(e) => format!("Use existing {} login ({}): {}", provider_name(provider), e.label.trim_start_matches("Native (").trim_end_matches(')'), if e.state == "on" { "on" } else if e.chosen == "off" { "off" } else { "not chosen" }),
            None => format!("Use existing {} login: ?", provider_name(provider)),
        }
    }

    pub(crate) fn toggle_native(&mut self, provider: &str) {
        let on = self.ui.sidebar.native_entries.iter().find(|e| e.provider == provider).is_some_and(|e| e.chosen == "on" || (e.chosen.is_empty() && e.state == "on"));
        let reply = self.host_call(json!({"op": "settings.set", format!("native_{provider}"): if on { "off" } else { "on" }}));
        self.ui.flash = Some(if reply["ok"] == json!(1) {
            self.send("{\"op\":\"account.list\"}");
            self.chats_sync_if_open(true);
            format!("{} login and chats: {}.", provider_name(provider), if on { "off" } else { "on" })
        } else {
            "Could not change that setting.".into()
        });
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entries_are_read_from_the_account_list() {
        let list = json!({"native": [{"account": "native-claude", "provider": "claude", "label": "Native (~/.claude)", "state": "ask", "chosen": null, "chats": 4}]});
        let got = parse_entries(&list);
        assert_eq!(got, vec![NativeEntry { id: "native-claude".into(), provider: "claude".into(), label: "Native (~/.claude)".into(), state: "ask".into(), chosen: String::new(), chats: Some(4) }]);
        assert!(parse_entries(&json!({"accounts": []})).is_empty());
    }
}
