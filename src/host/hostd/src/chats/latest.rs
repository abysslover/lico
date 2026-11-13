//! The one rule for "the latest chat of this directory and account": the first item of the same list `session.chats` returns
//! (same scope, same sort, same merge with open tabs), of the provider and the account asked for.

use serde_json::{json, Map, Value};

use super::{collect, turns, ChatItem};
use crate::core::Shared;

/// What the creation with `resume_latest` needs to know about the chat it continues.
#[derive(Clone, Debug)]
pub struct Latest {
    pub provider_session: String,
    pub title: String,
    pub turns: Option<u32>,
    pub updated_at: u64,
    pub open_session: Option<String>,
    pub open_state: Option<&'static str>,
}

impl Latest {
    pub fn json(&self) -> Value {
        json!({"provider_session": self.provider_session, "title": self.title, "turns": self.turns, "updated_at": self.updated_at})
    }
}

/// The first of the (newest first) items that is of this provider and this account (`None` account: a session without one).
pub(super) fn pick<'a>(items: &'a [ChatItem], provider: &str, account: Option<&str>) -> Option<&'a ChatItem> {
    items.iter().find(|it| it.provider == provider && it.account.as_deref() == account)
}

/// The latest chat of `cwd` (a folder, local or on `ssh_host`) for `account`, read the way `session.chats` reads it.
pub fn latest_chat(shared: &Shared, provider: &str, account: Option<&str>, cwd: &str, ssh_host: Option<&str>) -> Result<Option<Latest>, &'static str> {
    let mut request = Map::new();
    request.insert("cwd".into(), json!(cwd));
    request.insert("recorded".into(), json!(true)); // a tab that has said nothing yet is not a chat to continue
    if let Some(account) = account {
        request.insert("account".into(), json!(account));
    }
    if let Some(host) = ssh_host {
        request.insert("ssh_host".into(), json!(host));
    }
    let collected = collect(shared, &request)?;
    Ok(pick(&collected.items, provider, account).map(|it| Latest {
        provider_session: it.provider_session.clone(),
        title: it.title.clone(),
        turns: it.file.as_deref().and_then(|f| turns::count(&it.provider, f)),
        updated_at: it.updated_at,
        open_session: it.open_session.clone(),
        open_state: it.open_state,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(provider: &str, account: Option<&str>, sid: &str, updated: u64) -> ChatItem {
        ChatItem {
            provider: provider.into(),
            provider_session: sid.into(),
            account: account.map(str::to_string),
            account_label: None,
            ssh_host: None,
            cwd: Some("/w".into()),
            global: false,
            title: sid.into(),
            updated_at: updated,
            open_session: None,
            open_state: None,
            file: None,
            turns: None,
            recorded: true,
        }
    }

    #[test]
    fn the_pick_is_the_first_item_of_the_provider_and_account_asked_for() {
        let items = vec![item("codex", Some("a1"), "x-new", 9), item("claude", Some("a2"), "c-other", 8), item("claude", Some("a1"), "c-new", 7), item("claude", Some("a1"), "c-old", 1)];
        assert_eq!(pick(&items, "claude", Some("a1")).map(|i| i.provider_session.as_str()), Some("c-new"));
        assert_eq!(pick(&items, "codex", Some("a1")).map(|i| i.provider_session.as_str()), Some("x-new"));
        assert!(pick(&items, "claude", None).is_none(), "a session without an account never takes a chat of an account");
        assert!(pick(&items, "codex", Some("a2")).is_none());
    }
}
