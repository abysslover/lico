//! The rooms the host reports (`M08-P02`), kept for the room tabs and the mark `⇄` on a session's tab. `M22-P03`: the left sidebar has no Calls tab any more: a room is seen between the
//! agents that talk to each other (the mark on the tab of each of its two sessions; a click on it opens the room), and the person starts one with `@name message` in the input, or an
//! agent does with its tool. The host, `call.*`, REST and the tools are not touched.

use serde_json::{json, Value};

use super::App;

#[derive(Clone, Debug, PartialEq)]
pub struct CallSummaryItem {
    pub call_id: String,
    pub title: String,
    pub session_a: String,
    pub session_b: String,
    pub provider_a: String,
    pub provider_b: String,
    pub label_a: String,
    pub label_b: String,
    pub status: String,
    pub active: bool,
    pub paused: bool,
    pub owner_authorized: bool,
    pub round: u32,
    pub max_rounds: u32,
    pub entry_count: usize,
    pub updated_at: u64,
    pub last_speaker: String,
    pub last_preview: String,
}

#[derive(Default)]
pub struct CallsSidebarState {
    pub summaries: Vec<CallSummaryItem>,
}

fn parse_call_summaries(value: &Value) -> Vec<CallSummaryItem> {
    value["calls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|it| {
            let call_id = it["call_id"].as_str()?.to_string();
            let session_a = it["session_a"].as_str()?.to_string();
            let session_b = it["session_b"].as_str()?.to_string();
            let label_a = it["label_a"].as_str().unwrap_or(&session_a).to_string();
            let label_b = it["label_b"].as_str().unwrap_or(&session_b).to_string();
            let title = it["title"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{label_a} \u{00b7} {label_b}"));
            Some(CallSummaryItem {
                call_id,
                title,
                session_a,
                session_b,
                provider_a: it["provider_a"].as_str().unwrap_or("claude").to_string(),
                provider_b: it["provider_b"].as_str().unwrap_or("codex").to_string(),
                label_a,
                label_b,
                status: it["status"].as_str().unwrap_or("idle").to_string(),
                active: it["active"] == json!(true),
                paused: it["paused"] == json!(true),
                owner_authorized: it["owner_authorized"] == json!(true),
                round: it["round"].as_u64().unwrap_or(0) as u32,
                max_rounds: it["max_rounds"].as_u64().unwrap_or(4) as u32,
                entry_count: it["entry_count"].as_u64().unwrap_or(0) as usize,
                updated_at: it["updated_at"].as_u64().unwrap_or(0),
                last_speaker: it["last_speaker"].as_str().unwrap_or("").to_string(),
                last_preview: it["last_preview"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

impl CallsSidebarState {
    /// The room a session is in, for the mark on its tab: the one that is running, else the most recently written. None when it is in no room.
    pub fn room_of(&self, session: &str) -> Option<&CallSummaryItem> {
        self.summaries
            .iter()
            .filter(|c| c.session_a == session || c.session_b == session)
            .max_by_key(|c| (c.active, c.updated_at))
    }
}

impl App {
    pub(crate) fn sync_calls_from_status(&mut self, value: &Value) {
        let next = parse_call_summaries(value);
        let changed = next != self.ui.sidebar.calls.summaries;
        self.ui.sidebar.calls.summaries = next;
        if changed {
            self.dirty = true;
        }
        self.sync_open_calls_after_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: &str, a: &str, b: &str, active: bool, updated_at: u64) -> CallSummaryItem {
        CallSummaryItem {
            call_id: id.into(), title: format!("{a} \u{00b7} {b}"), session_a: a.into(), session_b: b.into(), provider_a: "claude".into(), provider_b: "codex".into(),
            label_a: a.into(), label_b: b.into(), status: "idle".into(), active, paused: false, owner_authorized: true, round: 0, max_rounds: 4, entry_count: 0, updated_at,
            last_speaker: String::new(), last_preview: String::new(),
        }
    }

    #[test]
    fn a_session_in_a_room_has_that_room_and_one_in_none_has_none() {
        let state = CallsSidebarState { summaries: vec![room("c1", "s1", "s2", false, 10)] };
        assert_eq!(state.room_of("s1").map(|c| c.call_id.as_str()), Some("c1"));
        assert_eq!(state.room_of("s2").map(|c| c.call_id.as_str()), Some("c1"), "both sessions of the room carry the mark");
        assert!(state.room_of("s3").is_none());
    }

    #[test]
    fn with_two_rooms_the_running_one_wins_then_the_newer() {
        let state = CallsSidebarState { summaries: vec![room("old", "s1", "s2", false, 10), room("new", "s1", "s3", false, 20), room("live", "s1", "s4", true, 5)] };
        assert_eq!(state.room_of("s1").map(|c| c.call_id.as_str()), Some("live"));
        let idle = CallsSidebarState { summaries: vec![room("old", "s1", "s2", false, 10), room("new", "s1", "s3", false, 20)] };
        assert_eq!(idle.room_of("s1").map(|c| c.call_id.as_str()), Some("new"));
    }
}
