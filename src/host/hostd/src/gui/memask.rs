//! The memory guard in the window (M19-P01, requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner): a click on a
//! RED session (its largest process at 500 MB or more) asks once for each red episode whether to end it and resume it, in one step, to a clean process. Orange asks nothing.

use std::sync::Mutex;

use lico_memguard::{Episodes, Level};
use serde_json::json;

use super::confirm::{Act, Ask};
use super::super::ui::row::memrow;
use super::super::App;

static EPISODES: Mutex<Option<Episodes>> = Mutex::new(None);

/// True once for each red episode of `id`; any other level ends the episode.
fn first_red(id: &str, level: Level) -> bool {
    EPISODES.lock().unwrap().get_or_insert_with(Episodes::default).ask(id, level)
}

/// The question's text: the heading and the lines (Y and N lines are the actions of the confirmation panel).
pub(super) fn view(ask: &Ask) -> (String, Vec<String>) {
    let limit = memrow::limits().limit_mb;
    (
        "Memory".into(),
        vec![
            format!("This session uses {} MB (limit {limit}). End it and continue? The conversation resumes; tool jobs that were running are lost.", ask.mb),
            String::new(),
            "Y  end and resume".into(),
            "N  cancel".into(),
        ],
    )
}

impl App {
    /// A click on a row of the list opened the session: if it is red and this is the first look in this red episode, ask.
    pub(crate) fn memory_question(&mut self, id: &str) {
        let Some(level) = self.ui.rows.iter().find(|r| r.session == id).map(memrow::level) else { return };
        if first_red(id, level) {
            self.ask_open(Act::MemoryResume);
        }
    }

    /// "End and resume": the host closes the session (the guard's closing: nothing but the guard resumes such a session) and resumes it. A provider that has no conversation to come
    /// back to is refused by the close itself, before anything is ended, and is said plainly.
    pub(super) fn memory_resume(&mut self, ask: &Ask) -> Result<String, (String, String)> {
        let mut close = json!({"op": "session.close", "session": ask.session, "generation": ask.generation, "by": "guard"});
        if !ask.worktree_repo.is_empty() {
            close["worktree"] = json!("keep");
        }
        let reply = self.host_call(close);
        if reply["ok"] != json!(1) {
            let code = reply["error"].as_str().unwrap_or("unknown").to_string();
            let text = if code == "no_conversation_yet" { "this session has no conversation to come back to, so it was not ended".to_string() } else { format!("the host answered {code}") };
            return Err((code, text));
        }
        let generation = reply["generation"].as_i64().unwrap_or(ask.generation);
        let resumed = self.host_call(json!({"op": "session.resume", "session": ask.session, "generation": generation}));
        self.send("{\"op\":\"status\"}");
        if resumed["ok"] != json!(1) {
            let code = resumed["error"].as_str().unwrap_or("unknown").to_string();
            return Err((code, "the session was ended but could not be resumed; Ctrl+R brings it back".to_string()));
        }
        Ok(format!("Ended and resumed: the session used {} MB.", ask.mb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_comes_once_for_each_red_episode_and_never_for_orange() {
        assert!(!first_red("m_s1", Level::Orange));
        assert!(first_red("m_s1", Level::Red));
        assert!(!first_red("m_s1", Level::Red));
        assert!(!first_red("m_s1", Level::Ok));
        assert!(first_red("m_s1", Level::Red), "a new episode");
    }

    #[test]
    fn the_question_names_the_size_the_limit_and_what_is_lost() {
        let ask = Ask { act: Act::MemoryResume, session: "s".into(), generation: 1, title: "t".into(), working: false, cwd: String::new(), worktree_branch: String::new(), worktree_repo: String::new(), worktree_exists: false, dirty_confirm: false, mb: 612 };
        let (heading, lines) = view(&ask);
        assert_eq!(heading, "Memory");
        assert!(lines[0].contains("612 MB (limit 500). End it and continue? The conversation resumes; tool jobs that were running are lost."), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Y  end and resume")) && lines.iter().any(|l| l.starts_with("N  cancel")));
    }
}
