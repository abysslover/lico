//! Administrator account and session control operations (`M03-P01`):
//! - `account.rename`: rename an existing account's label atomically without touching credentials or homes.
//! - `session.mode`: switch a provider session's permission mode (`"auto"` <-> `"default"`) safely with
//!   generation checking. Codex applies `s.mode` per turn immediately; an idle running Claude session
//!   restarts cleanly with `--permission-mode` (and `--resume` when a conversation id exists) while busy
//!   or pending-approval turns refuse with `"busy"`.

use std::sync::Arc;

use serde_json::json;

use crate::core::{check_generation, find, Reply, Shared};
use crate::session::{obj, State};

pub fn account_rename(shared: &Shared, account: &str, label: &str) -> Reply {
    if !crate::ipc::valid_label(&json!(label)) {
        return Err("bad_request");
    }
    let updated = shared.accounts.lock().unwrap().rename(account, label)?;
    Ok(obj(json!({"account": updated.id, "provider": updated.provider, "label": updated.label})))
}

pub fn session_mode(shared: &Arc<Shared>, session: &str, generation: i64, mode: &str) -> Reply {
    let next_mode: &'static str = match mode {
        "auto" => "auto",
        "default" => "default",
        _ => return Err("bad_request"),
    };
    let found = find(shared, session).ok_or("unknown_session")?;
    let (id, provider, was_running, prev_mode, changed) = {
        let mut s = found.lock().unwrap();
        check_generation(&s, Some(generation))?;
        if s.provider.is_empty() {
            return Err("bad_request");
        }
        if s.state == State::Exited {
            return Err("session_exited");
        }
        if s.state == State::Running && (s.busy || !s.approvals.is_empty()) {
            return Err("busy");
        }
        if s.mode == next_mode {
            return Ok(obj(json!({"session": s.id, "generation": s.generation, "mode": s.mode})));
        }
        let prev_mode = s.mode;
        let was_running = s.state == State::Running;
        if s.provider == "claude" && was_running {
            s.stop_process();
            crate::tools::forget(&s.id);
            s.set_state(State::Disconnected);
            s.set_busy(false);
            s.input_tx = None;
            s.claude = None;
            s.drop_program_approvals();
            s.mode = next_mode;
            s.generation += 1;
        } else {
            s.mode = next_mode;
            let note = format!("[permission mode: {next_mode}]");
            if s.provider == "claude" {
                crate::claude::note(&mut s, &note);
            } else {
                crate::codex::note(&mut s, &note);
            }
        }
        (s.id.clone(), s.provider, was_running, prev_mode, true)
    };
    if changed && provider == "claude" && was_running {
        if let Err(code) = crate::claude::restart_after_change(shared, &found) {
            let mut s = found.lock().unwrap();
            s.generation -= 1;
            s.mode = prev_mode;
            s.set_state(State::Disconnected);
            return Err(code);
        }
        let mut s = found.lock().unwrap();
        crate::claude::note(&mut s, &format!("[permission mode: {next_mode}]"));
    }
    crate::registry::save(shared);
    let out_gen = found.lock().unwrap().generation;
    Ok(obj(json!({"session": id, "generation": out_gen, "mode": next_mode})))
}

#[cfg(test)]
mod tests {
    use crate::accounts::Accounts;
    use crate::handoff::{mode_of, Scratch};

    #[test]
    fn renaming_an_account_preserves_id_provider_default_and_private_permissions() {
        let scratch = Scratch::new("account_rename");
        let dir = scratch.0.join("accounts");
        let mut accounts = Accounts::load(&dir).unwrap();
        let created = accounts.create("claude", "Initial").unwrap();
        let renamed = accounts.rename(&created.id, "Team Admin").unwrap();
        assert_eq!(renamed.id, created.id);
        assert_eq!(renamed.provider, "claude");
        assert_eq!(renamed.label, "Team Admin");
        assert_eq!(mode_of(&dir.join("accounts.json")), 0o600);

        let reloaded = Accounts::load(&dir).unwrap();
        assert_eq!(reloaded.get(&created.id).unwrap().label, "Team Admin");
        assert_eq!(reloaded.default_for("claude").unwrap().id, created.id);
        assert_eq!(accounts.rename("a000000", "Ghost").err(), Some("unknown_account"));
    }
}
