//! `session.create`: a new session of a provider (or the synthetic one) for a person or an agent, on an account, in a folder; with `resume_latest`
//! it continues the latest chat of the folder and account instead (reusing the tab that holds it) and the answer says which it did.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::{find, session_resume, start_session, Reply, Shared};
use crate::session::obj;

pub fn session_create(shared: &Arc<Shared>, kind: &str, rate: u64, history: u64, size: usize, account: Option<&str>, mode: &str, chosen: crate::folders::Settings) -> Reply {
    let mode: &'static str = if mode == "default" { "default" } else { "auto" };
    let asked_latest = chosen.resume_latest;
    if asked_latest && (!matches!(kind, "claude" | "codex") || chosen.worktree_branch.is_some()) {
        return Err("option_unavailable");
    }
    if asked_latest && (chosen.resume_id.is_some() || chosen.cwd.is_none()) {
        return Err("bad_request"); // a chat id and "the latest" exclude each other, and the latest is of a folder
    }
    let mut chosen = crate::ssh_remote::prepare_session(shared, kind, account, chosen)?; // the folder, the model and the effort are checked before anything starts
    chosen.resume_latest = false;
    // a bounded number of sessions, and so of agent processes; concurrent creators reserve a slot first
    let bound = match resolve_account(shared, kind, account) {
        Ok(bound) => bound,
        Err(code) => return Err(code), // refused before anything starts
    };
    let mut resumed: Option<Value> = None;
    let mut note: Option<String> = None;
    let mut others: Option<Value> = None;
    // an explicit chat id is checked the same way: a chat that another program is writing is not resumed from here (SES-07)
    if let (Some(sid), Some(folder), false) = (chosen.resume_id.as_deref(), chosen.cwd.as_deref(), asked_latest) {
        let chat_account = bound.as_ref().map(|(id, _)| id.clone()).or_else(|| crate::native::id_of(kind).map(str::to_string));
        let ssh_host = chat_account.as_deref().and_then(|id| shared.accounts.lock().unwrap().ssh_host(id));
        if crate::chats::live_reason_of(shared, chat_account.as_deref(), folder, ssh_host.as_deref(), sid).ok().flatten().is_some() {
            return Err("chat_running_elsewhere");
        }
    }
    if asked_latest {
        // a session without an account runs in the provider's own default home: its chats are those of the Native entry
        let chat_account = bound.as_ref().map(|(id, _)| id.clone()).or_else(|| crate::native::id_of(kind).map(str::to_string));
        let account_id = chat_account.as_deref();
        let ssh_host = account_id.and_then(|id| shared.accounts.lock().unwrap().ssh_host(id));
        let folder = chosen.cwd.clone().unwrap_or_default();
        match crate::chats::latest_chat(shared, kind, account_id, &folder, ssh_host.as_deref())? {
            None => {
                // nothing to continue: a new session, and where the other chats of the folder are
                let found = crate::chats::others(shared, &folder, ssh_host.as_deref(), account_id).ok();
                note = Some(match found.as_ref().and_then(crate::chats::sentence) {
                    Some(hint) => format!("no earlier chat for this directory and account; created a new one. {hint}"),
                    None => "no earlier chat for this directory and account; created a new one".to_string(),
                });
                others = found.filter(|o| o["total"].as_u64().unwrap_or(0) > 0);
            }
            Some(latest) => {
                if latest.live.is_some() {
                    return Err("chat_running_elsewhere"); // a running external process is never adopted: no silent pick of an older chat either
                }
                resumed = Some(latest.json());
                if let (Some(open), Some(state)) = (latest.open_session.as_deref(), latest.open_state) {
                    if let Some(reply) = reopen(shared, open, state)? {
                        return Ok(with_resumed(reply, Some(&latest.json()), true, None, None));
                    }
                }
                chosen.resume_id = Some(latest.provider_session);
            }
        }
    }
    let reserved = shared.pending_creates.fetch_add(1, Ordering::SeqCst);
    let total = shared.sessions.read().unwrap().len() + reserved;
    let created = if total >= shared.cfg.max_sessions {
        Err("busy")
    } else if kind == "codex" || kind == "claude" {
        crate::folders::start_provider_session(shared, kind, bound, mode, chosen)
    } else {
        start_session(shared, rate > 0, rate, history, size, false)
    };
    shared.pending_creates.fetch_sub(1, Ordering::SeqCst);
    let created = created?;
    let (id, generation) = {
        let s = created.lock().unwrap();
        (s.id.clone(), s.generation)
    };
    let reply = obj(json!({"session": id, "generation": generation}));
    Ok(if asked_latest { with_resumed(reply, resumed.as_ref(), false, note.as_deref(), others) } else { reply })
}

/// The tab that already holds the latest chat: a running one is shown again (a closed tab is reopened), a disconnected one is resumed in place;
/// any other state means the chat is to be continued by a new session (`None`).
fn reopen(shared: &Arc<Shared>, session: &str, state: &str) -> Result<Option<serde_json::Map<String, Value>>, &'static str> {
    let found = find(shared, session).ok_or("unknown_session")?;
    match state {
        "running" => {
            let mut s = found.lock().unwrap();
            if s.tab_closed {
                s.tab_closed = false;
                crate::registry::mark_dirty();
            }
            Ok(Some(obj(json!({"session": s.id, "generation": s.generation}))))
        }
        "disconnected" => {
            let generation = found.lock().unwrap().generation;
            session_resume(shared, session, generation as i64).map(Some)
        }
        _ => Ok(None),
    }
}

/// The answer to a creation that asked for the latest chat: whether one was continued, which, whether its tab was already open, or why not.
fn with_resumed(mut reply: serde_json::Map<String, Value>, chat: Option<&Value>, already_open: bool, note: Option<&str>, others: Option<Value>) -> serde_json::Map<String, Value> {
    reply.insert("resumed".into(), json!(chat.is_some()));
    if let Some(chat) = chat {
        reply.insert("resumed_chat".into(), chat.clone());
    }
    if already_open {
        reply.insert("already_open".into(), json!(true));
    }
    if let Some(note) = note {
        reply.insert("note".into(), json!(note));
    }
    if let Some(others) = others {
        reply.insert("others".into(), others);
    }
    reply
}

/// The account a new session is bound to: the one asked for, else the provider's default, else none
/// (a session without an account runs with the host's own provider home, as before accounts existed).
fn resolve_account(shared: &Shared, kind: &str, requested: Option<&str>) -> Result<Option<(String, u64)>, &'static str> {
    // the provider's own default home (`Native`) is asked for by its reserved id, and only of its own provider and only once it is open
    if let Some(id) = requested.filter(|id| crate::native::provider_of(id).is_some()) {
        return if crate::native::provider_of(id) != Some(kind) {
            Err("bad_request")
        } else if crate::native::enabled(shared, kind) {
            Ok(None)
        } else {
            Err("native_off")
        };
    }
    let native_off = matches!(kind, "claude" | "codex") && crate::native::state(shared, kind) == crate::native::State::Off;
    let accounts = shared.accounts.lock().unwrap();
    match (kind, requested) {
        ("codex" | "claude", Some(id)) => match accounts.get(id) {
            Some(a) if a.provider == kind => Ok(Some((a.id.clone(), a.credential_generation))),
            _ => Err("unknown_account"),
        },
        ("codex" | "claude", None) => match accounts.default_for(kind) {
            Some(a) => Ok(Some((a.id.clone(), a.credential_generation))),
            // accounts exist but none is the default: never fall back to an unbound session (the host's own provider home)
            None if accounts.any_for(kind) => Err("unknown_account"),
            None if native_off => Err("native_off"), // the person turned the default home off and there is no other account
            None => Ok(None),
        },
        (_, Some(_)) => Err("bad_request"),
        (_, None) => Ok(None),
    }
}
