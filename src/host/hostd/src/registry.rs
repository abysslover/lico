//! The session registry: which provider sessions exist, their account binding, provider session id and
//! last known state, in one owner-only file replaced atomically. After a restart it brings every session
//! back as `disconnected` with its history; a provider process is never assumed to have survived.
//! A registry that cannot be read, or that names something that cannot be right, stops the host.

use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::core::Shared;

static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::Relaxed)
}

pub struct Entry {
    pub session: String,
    pub provider: String,
    pub generation: u64,
    pub account: Option<String>,
    pub credential_generation: u64,
    pub provider_session: Option<String>,
    pub state_at: u64,
    pub account_removed: bool, // the account it was bound to was removed with its sessions detached
}

fn valid_session_id(id: &str) -> bool {
    id.len() == 17 && id.starts_with('s') && id[1..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn read(dir: &Path) -> Result<Vec<Value>, String> {
    match std::fs::read_to_string(dir.join("sessions.json")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("cannot read sessions.json: {e}")),
        Ok(text) => {
            let value = serde_json::from_str::<Value>(&text).map_err(|e| format!("sessions.json is not valid: {e}"))?;
            value["sessions"].as_array().cloned().ok_or_else(|| "sessions.json has no list of sessions".to_string())
        }
    }
}

/// The ids the registry names, read leniently (used before anything on disk is touched).
pub fn registered_ids(dir: &Path) -> Result<HashSet<String>, String> {
    Ok(read(dir)?.iter().filter_map(|e| e["session"].as_str().map(str::to_string)).collect())
}

/// Parse and check every entry; anything that cannot be right is an error, never silently dropped.
pub fn load(dir: &Path, accounts: &crate::accounts::Accounts) -> Result<Vec<Entry>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in read(dir)? {
        let session = value["session"].as_str().unwrap_or("").to_string();
        if !valid_session_id(&session) || !seen.insert(session.clone()) {
            return Err(format!("sessions.json names a session id that cannot be right: {session:?}"));
        }
        let provider = value["provider"].as_str().unwrap_or("").to_string();
        if provider != "codex" && provider != "claude" {
            return Err(format!("sessions.json names an unknown provider for {session}"));
        }
        let generation = value["generation"].as_u64().filter(|g| *g >= 1).ok_or_else(|| format!("sessions.json has no valid generation for {session}"))?;
        let mut account = value["account"].as_str().map(str::to_string);
        let mut account_removed = false;
        if let Some(id) = &account {
            match accounts.get(id) {
                Some(a) if a.provider == provider => {}
                Some(_) => return Err(format!("sessions.json binds {session} to an account of another provider")),
                None => { // removed with its sessions detached: the session stays, unbound, and says so until it is rebound
                    account = None;
                    account_removed = true;
                }
            }
        }
        let provider_session = value["provider_session"].as_str().map(str::to_string);
        if provider_session.as_ref().map_or(false, |p| p.is_empty() || p.len() > 64) {
            return Err(format!("sessions.json has an invalid provider session id for {session}"));
        }
        out.push(Entry {
            session, provider, generation, account, credential_generation: value["credential_generation"].as_u64().unwrap_or(0),
            provider_session, state_at: value["state_at"].as_u64().unwrap_or(0), account_removed,
        });
    }
    Ok(out)
}

/// Write the registry of every live provider session (a purged session is not in it).
pub fn save(shared: &Shared) {
    static SAVING: std::sync::Mutex<()> = std::sync::Mutex::new(()); // one writer at a time: they share the temporary file
    let _one = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    DIRTY.store(false, Ordering::Relaxed);
    let sessions: Vec<_> = shared.sessions.read().unwrap().clone();
    let entries: Vec<Value> = sessions
        .iter()
        .filter_map(|s| {
            let s = s.lock().unwrap();
            if s.provider.is_empty() || s.purged {
                return None;
            }
            Some(json!({
                "session": s.id, "provider": s.provider, "generation": s.generation, "account": s.account,
                "credential_generation": s.credential_generation, "provider_session": s.codex_thread, "state_at": s.state_at,
            }))
        })
        .collect();
    let dir = &shared.cfg.registry_dir;
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    let temporary = dir.join(format!(".sessions.{}.tmp", std::process::id()));
    let written = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temporary).and_then(|mut out| {
        out.write_all(json!({"sessions": entries}).to_string().as_bytes())?;
        out.sync_all()
    });
    if written.and_then(|_| std::fs::rename(&temporary, dir.join("sessions.json"))).is_err() {
        DIRTY.store(true, Ordering::Relaxed); // try again at the next tick rather than forget a session
    }
}

/// Bring the registered sessions back after a restart, as `disconnected`.
pub fn recover(shared: &Arc<Shared>) -> Result<usize, String> {
    let entries = {
        let accounts = shared.accounts.lock().unwrap();
        load(&shared.cfg.registry_dir, &accounts)?
    };
    let count = entries.len();
    for entry in entries {
        let (mut session, report) = crate::session::Session::recovered(&entry, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window);
        if entry.account_removed {
            session.account_issue = Some("account_removed".to_string());
        }
        if report.torn_bytes > 0 || report.segments_set_aside > 0 {
            session.append(format!("[log recovered: {} byte(s) of a damaged tail and {} segment(s) set aside]", report.torn_bytes, report.segments_set_aside).into_bytes());
        }
        shared.sessions.write().unwrap().push(Arc::new(std::sync::Mutex::new(session)));
    }
    Ok(count)
}
