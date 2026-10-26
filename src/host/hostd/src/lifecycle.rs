//! Closing a session: its provider process ends and the session stays, disconnected, with its log, its folder and its provider
//! session, so that `session.resume` brings it back. (Deleting is `session.purge`.)

use std::sync::Arc;

use serde_json::json;

use crate::core::{check_generation, find, Reply, Shared};
use crate::session::{obj, State};

/// End the process of a session that is running. A session that is already disconnected is closed (the answer is the same, so a
/// second click or a second command does no harm); a session that cannot be resumed (its program has not
/// told its own session id yet) is refused as `no_conversation_yet`, as closing it would be deleting it.
pub fn session_close(shared: &Arc<Shared>, session: &str, generation: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let mut s = found.lock().unwrap();
    check_generation(&s, Some(generation))?;
    if s.codex_thread.is_none() {
        return Err("no_conversation_yet"); // the program has not told its session id yet: a close now would be a delete
    }
    match s.state {
        State::Exited => return Err("session_exited"),
        State::Disconnected => {}
        State::Running => {
            // the same steps as the provider reader takes when the process ends by itself, here taken first and by the host
            s.stop_process();
            s.set_state(State::Disconnected);
            s.set_busy(false);
            s.input_tx = None;
            s.claude = None; // its approvals and unanswered requests are stale now
            s.codex = None;
            s.approvals.clear();
            if s.provider == "claude" {
                crate::claude::note(&mut s, "[session closed]");
            } else {
                crate::codex::note(&mut s, "[session closed]");
            }
        }
    }
    Ok(obj(json!({"session": s.id, "generation": s.generation})))
}
