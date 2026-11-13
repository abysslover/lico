//! The provider's own default home as a visible `Native` choice (`M11-P01`). The default home is where Claude Code (`CLAUDE_CONFIG_DIR`, else `~/.claude`) and Codex
//! (`CODEX_HOME`, else `~/.codex`) keep what the person did before Lico: its chats and its login. Native is a virtual entry with the reserved ids `native-claude` and
//! `native-codex`: nothing is stored, nothing is copied, linked or decrypted (AUTH-08), the provider runs in place with its own login, and Lico reads only what it reads
//! for any account: a directory listing of the transcripts and their first lines, never a credential file. It is open only when the person chose it (the setting
//! `native_claude` / `native_codex`, `on` or `off`), or when nothing else exists to choose from (no managed account of that provider: the first run, now named).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::core::Shared;

pub const CLAUDE_ID: &str = "native-claude";
pub const CODEX_ID: &str = "native-codex";

/// How many transcript files a count looks at at the most (the same bound the chat scan has).
const MAX_COUNTED: usize = 2500;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    On,
    Off,
    Ask,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::On => "on",
            State::Off => "off",
            State::Ask => "ask",
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

/// The state that counts: the choice, else open when no managed account of the provider exists (nothing else to choose, as on a first run), else not answered yet.
pub fn state(shared: &Shared, provider: &str) -> State {
    match chosen(shared, provider) {
        Some("on") => State::On,
        Some(_) => State::Off,
        None if shared.accounts.lock().unwrap().any_for(provider) => State::Ask,
        None => State::On,
    }
}

pub fn enabled(shared: &Shared, provider: &str) -> bool {
    state(shared, provider) == State::On
}

/// How many chats the default home holds: transcript files counted from a listing, none opened, none of the credential files looked at. `None` when there is no such home.
pub fn probe(shared: &Shared, provider: &str) -> Option<usize> {
    let home = home(shared, provider)?;
    let mut left = MAX_COUNTED;
    let mut found = 0;
    if provider == "claude" {
        for project in std::fs::read_dir(home.join("projects")).ok()?.flatten() {
            if is_dir(&project.path()) {
                found += count_dir(&project.path(), 0, 0, &mut left);
            }
        }
    } else {
        found = count_dir(&home.join("sessions"), 0, 5, &mut left);
    }
    Some(found)
}

fn is_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_dir())
}

/// Transcript files (`*.jsonl`) in a folder and, up to `depth` levels down, below it; a link is never followed.
fn count_dir(dir: &Path, level: usize, depth: usize, left: &mut usize) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut found = 0;
    for entry in entries.flatten() {
        if *left == 0 {
            break;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
        if meta.file_type().is_dir() && level < depth {
            found += count_dir(&entry.path(), level + 1, depth, left);
        } else if meta.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "jsonl") {
            *left -= 1;
            found += 1;
        }
    }
    found
}

/// The counts of the last look, so that the window asking every few seconds does not list the folders every time.
fn counted(shared: &Shared, provider: &str) -> Option<usize> {
    static LAST: std::sync::Mutex<Vec<(String, std::time::Instant, Option<usize>)>> = std::sync::Mutex::new(Vec::new());
    let mut last = LAST.lock().unwrap();
    if let Some((_, at, n)) = last.iter().find(|(p, at, _)| p == provider && at.elapsed().as_secs() < 30) {
        let _ = at;
        return *n;
    }
    let n = probe(shared, provider);
    last.retain(|(p, _, _)| p != provider);
    last.push((provider.to_string(), std::time::Instant::now(), n));
    n
}

/// The Native entries for `account.list`: the managed accounts stay in their own array. The chats are counted only while the person has not answered (that is
/// what the first-run question shows), and no more often than every 30 seconds.
pub fn entries(shared: &Shared) -> Vec<Value> {
    ["claude", "codex"]
        .into_iter()
        .map(|provider| {
            let state = state(shared, provider);
            json!({
                "account": id_of(provider), "provider": provider, "label": label(shared, provider), "state": state.word(),
                "chosen": chosen(shared, provider), "chats": if state == State::Ask { counted(shared, provider) } else { None },
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
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lico-native-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    #[test]
    fn the_count_lists_transcripts_only_and_never_opens_a_credential_file_or_follows_a_link() {
        let root = scratch("count");
        let home = root.join(".claude");
        fs::create_dir_all(home.join("projects/a")).unwrap();
        fs::create_dir_all(home.join("projects/b")).unwrap();
        for name in ["projects/a/one.jsonl", "projects/a/two.jsonl", "projects/b/three.jsonl", "projects/a/notes.txt"] {
            fs::write(home.join(name), "{}\n").unwrap();
        }
        let secret = home.join(".credentials.json");
        fs::write(&secret, "{\"canary\":1}").unwrap();
        fs::set_permissions(&secret, std::os::unix::fs::PermissionsExt::from_mode(0)).unwrap();
        std::os::unix::fs::symlink(root.join("elsewhere"), home.join("projects/linked")).unwrap();
        let mut left = MAX_COUNTED;
        let mut found = 0;
        for project in fs::read_dir(home.join("projects")).unwrap().flatten() {
            if is_dir(&project.path()) {
                found += count_dir(&project.path(), 0, 0, &mut left);
            }
        }
        assert_eq!(found, 3);
        fs::set_permissions(&secret, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_count_of_codex_goes_down_the_dated_folders_and_stops_at_its_bound() {
        let root = scratch("codex");
        fs::create_dir_all(root.join("sessions/2026/11/12")).unwrap();
        fs::write(root.join("sessions/2026/11/12/rollout-a.jsonl"), "{}\n").unwrap();
        fs::write(root.join("sessions/2026/11/rollout-b.jsonl"), "{}\n").unwrap();
        let mut left = MAX_COUNTED;
        assert_eq!(count_dir(&root.join("sessions"), 0, 5, &mut left), 2);
        let mut few = 1;
        assert_eq!(count_dir(&root.join("sessions"), 0, 5, &mut few), 1, "the bound ends the count");
        let _ = fs::remove_dir_all(&root);
    }
}
