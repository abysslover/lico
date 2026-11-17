//! The provider's own default home as a visible `Native` choice (`M11-P01`). The default home is where Claude Code (`CLAUDE_CONFIG_DIR`, else `~/.claude`) and Codex
//! (`CODEX_HOME`, else `~/.codex`) keep what the person did before Lico: its chats and its login. Native is a virtual entry with the reserved ids `native-claude` and
//! `native-codex`: nothing is stored, nothing is copied, linked or decrypted (AUTH-08), the provider runs in place with its own login, and Lico reads only what it reads
//! for any account: a directory listing of the transcripts and their first lines, never a credential file. It is open unless the person turned it off (the setting
//! `native_claude` / `native_codex`, `on` or `off`; `ask` takes a choice back): since `M22-P02` the chats of the provider's own home are listed by default, with no question first.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::core::Shared;

pub const CLAUDE_ID: &str = "native-claude";
pub const CODEX_ID: &str = "native-codex";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    On,
    Off,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::On => "on",
            State::Off => "off",
        }
    }
}

pub fn id_of(provider: &str) -> Option<&'static str> {
    match provider {
        "claude" => Some(CLAUDE_ID),
        "codex" => Some(CODEX_ID),
        _ => None,
    }
}

/// The provider a reserved id is the Native entry of.
pub fn provider_of(id: &str) -> Option<&'static str> {
    match id {
        CLAUDE_ID => Some("claude"),
        CODEX_ID => Some("codex"),
        _ => None,
    }
}

/// The default home the provider program uses when the host gives it no account: the one `claude.rs` and `codex.rs` start it in.
pub fn home(shared: &Shared, provider: &str) -> Option<PathBuf> {
    match provider {
        "claude" => crate::provider_record::home_of(shared, None),
        "codex" => crate::chats::unbound_codex_home(),
        _ => None,
    }
}

/// A path as people write it: under the home folder it starts with `~/`.
fn shown(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from).filter(|h| h.is_absolute());
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if !rest.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// The fixed name the entry always has, beside the target and in every list: `Native (~/.claude)`.
pub fn label(shared: &Shared, provider: &str) -> String {
    let fallback = if provider == "codex" { "~/.codex" } else { "~/.claude" };
    format!("Native ({})", home(shared, provider).map_or_else(|| fallback.to_string(), |h| shown(&h)))
}

/// What the person chose, if they did: `on` or `off` (the settings file keeps nothing else).
pub fn chosen(shared: &Shared, provider: &str) -> Option<&'static str> {
    crate::clipboard::settings::native_value(&shared.cfg.handoff_dir, provider)
}

/// The state that counts: the person's `off`, else on (the default since `M22-P02`; `chosen` is `on` or `off` and nothing else).
pub fn state(shared: &Shared, provider: &str) -> State {
    match chosen(shared, provider) {
        Some("off") => State::Off,
        _ => State::On,
    }
}

pub fn enabled(shared: &Shared, provider: &str) -> bool {
    state(shared, provider) == State::On
}

/// The counts of the last look, so that the window asking every few seconds does not list the folders every time.
fn counted(shared: &Shared, provider: &str) -> Option<usize> {
    static LAST: std::sync::Mutex<Vec<(String, std::time::Instant, Option<usize>)>> = std::sync::Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap();
    if let Some((_, at, n)) = last.iter().find(|(p, at, _)| p == provider && at.elapsed().as_secs() < 30) {
        let _ = at;
        return *n;
    }
    let n = crate::chats::count_of(shared, provider);
    last.retain(|(p, _, _)| p != provider);
    last.push((provider.to_string(), std::time::Instant::now(), n));
    n
}

/// The Native entries for `account.list`: the managed accounts stay in their own array. The chats of a home that is on are counted from the chat index, and no more
/// often than every 30 seconds.
pub fn entries(shared: &Shared) -> Vec<Value> {
    ["claude", "codex"]
        .into_iter()
        .map(|provider| {
            let state = state(shared, provider);
            json!({
                "account": id_of(provider), "provider": provider, "label": label(shared, provider), "state": state.word(),
                "chosen": chosen(shared, provider), "chats": if state == State::On { counted(shared, provider) } else { None },
            })
        })
        .collect()
}

/// The answer of `account.list`: the managed accounts and, beside them, the Native entries.
pub fn account_list_reply(shared: &Shared, entries_of_accounts: Vec<Value>) -> crate::core::Reply {
    Ok(crate::session::obj(json!({"accounts": entries_of_accounts, "native": entries(shared)})))
}

/// A session without an account runs in the default home: its status line carries the Native label, so the person sees where it runs (AUTH-06).
pub fn annotate(shared: &Shared, entry: &mut Value, s: &crate::session::Session) {
    if s.account.is_none() && matches!(s.provider, "claude" | "codex") {
        entry["native"] = json!(label(shared, s.provider));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reserved_ids_map_to_their_provider_and_nothing_else_does() {
        assert_eq!((provider_of(CLAUDE_ID), provider_of(CODEX_ID), provider_of("a0123456789ab")), (Some("claude"), Some("codex"), None));
        assert_eq!((id_of("claude"), id_of("codex"), id_of("synthetic")), (Some(CLAUDE_ID), Some(CODEX_ID), None));
    }

    #[test]
    fn a_path_under_the_home_folder_is_shown_with_a_tilde() {
        let home = std::env::var_os("HOME").map(PathBuf::from).expect("HOME");
        assert_eq!(shown(&home.join(".claude")), "~/.claude");
        assert_eq!(shown(Path::new("/elsewhere/claude-home")), "/elsewhere/claude-home");
    }
}
