//! Closing a session: its provider process ends and the session stays, disconnected, with its log, its folder and its provider
//! session, so that `session.resume` brings it back. (Deleting is `session.purge`.)

use std::sync::Arc;

use serde_json::json;

use crate::core::{check_generation, find, Reply, Shared};
use crate::session::{obj, State};

/// End the process of a session that is running. A session that is already disconnected is closed (the answer is the same, so a
/// second click or a second command does no harm); a session that cannot be resumed (its program has not
/// told its own session id yet) is refused as `no_conversation_yet`, as closing it would be deleting it.
pub fn session_close_opts(
    shared: &Arc<Shared>,
    session: &str,
    generation: i64,
    worktree: Option<&str>,
    force_dirty: bool,
    hide_tab: bool,
) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let (id, out_gen, repo, remove_wt) = {
        let mut s = found.lock().unwrap();
        check_generation(&s, Some(generation))?;
        if (s.codex_thread.is_none() && s.driver.is_none() && crate::driver::get(s.provider).is_none()) || (hide_tab && !s.has_conversation()) {
            return Err("no_conversation_yet"); // the program has not told its session id (or a blank 0-turn tab was dismissed): a close now would be a delete
        }
        if s.state == State::Exited {
            return Err("session_exited");
        }
        let remove_wt = crate::worktree::check_before_remove(shared, &s, worktree, force_dirty)?;
        if hide_tab && !s.tab_closed {
            s.tab_closed = true;
            crate::registry::mark_dirty();
        }
        if s.state == State::Running {
            // the same steps as the provider reader takes when the process ends by itself, here taken first and by the host
            s.stop_process();
            crate::tools::forget(&s.id); // the tool connection of the process that ended is dead at once, not when its reader notices
            crate::vdisplay::stop(&s.id);
            crate::computer::real::revoke(&s.id);
            s.set_state(State::Disconnected);
            s.set_busy(false);
            s.input_tx = None;
            s.claude = None; // its approvals and unanswered requests are stale now
            s.codex = None;
            if let Some(mut link) = s.driver.take() {
                link.stop();
            }
            s.drop_program_approvals();
            if s.provider == "claude" {
                crate::claude::note(&mut s, "[session closed]");
            } else {
                crate::codex::note(&mut s, "[session closed]");
            }
        }
        (s.id.clone(), s.generation, s.settings.worktree_repo.clone(), remove_wt)
    };
    if remove_wt {
        crate::worktree::remove_checked(shared, repo.as_deref(), &id);
    }
    Ok(obj(json!({"session": id, "generation": out_gen})))
}

/// After each `status` reply: what the memory guard decided (the memory guard's module is a leaf; the hub calls are made here). Closes the red idle sessions that qualify (when the owner switched the
/// auto-close on), resumes one guard-closed session when its turn has come, and warns the main agent.
pub fn guard_tick(shared: &Arc<Shared>, reply: &serde_json::Map<String, serde_json::Value>) {
    let Some(entries) = reply.get("sessions").and_then(serde_json::Value::as_array) else { return };
    let Some(state_dir) = shared.cfg.registry_dir.parent() else { return };
    let step = crate::guard::step(state_dir, entries);
    if let Some((main, text)) = step.warn {
        let _ = crate::core::input(shared, &main, None, &format!("guard-{}", step.now), &text, crate::session::Origin::Host, None);
    }
    for (id, generation, keep_worktree) in step.close {
        if session_close_opts(shared, &id, generation, keep_worktree.then_some("keep"), false, false).is_ok() {
            crate::guard::closed_ok(&id, step.now);
        }
    }
    if let Some(id) = step.resume {
        // only a session the guard closed is queued; one the person took up meanwhile runs already and the host refuses
        if crate::guard::closed_by(&id) == Some("guard") {
            if let Some(found) = find(shared, &id) {
                let generation = found.lock().unwrap().generation as i64;
                let _ = crate::core::session_resume(shared, &id, generation);
            }
            crate::guard::clear(&id);
        }
    }
}

