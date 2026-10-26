//! A session as the list and the header show it: what the host's status says about it, in the words a person reads.

use serde_json::Value;

use super::super::{clean, editor};

#[derive(Clone, Default)]
pub struct Row {
    pub session: String,
    pub provider: String,
    pub account: String,
    pub model: String,
    pub state: String,
    pub state_at: u64,
    pub activity: String,
    pub generation: i64,
    pub approvals: Vec<(i64, String)>,
    pub issue: String,
    pub mode: String,
    pub busy_since: u64,    // the host's start of the open turn (unix seconds), 0 when none
    pub last_event_at: u64, // when the provider last said anything
    pub stalled: bool,      // the host's own judgement: working, nothing to answer, and silent for its stall time
    pub cwd: String,        // the folder the person chose, empty for a scratch workspace
    pub name: String,       // the name the person gave it, or empty
}

pub(crate) fn ago(now: u64, then: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => "<1m".to_string(), // coarse on purpose: a list that redraws every second would never let the client idle
        60..=3599 => format!("{}m", d / 60),
        _ => format!("{}h", d / 3600),
    }
}

/// The clock of an open turn, as a sentence part: "37 s", "2 m 05 s", "1 h 02 m".
pub fn clock_text(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} m {:02} s", secs / 60, secs % 60),
        _ => format!("{} h {:02} m", secs / 3600, secs % 3600 / 60),
    }
}

/// The same for the narrow list: "37s", "2m05s", "1h02m".
pub fn clock_compact(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, secs % 3600 / 60),
    }
}

impl Row {
    /// The row of one entry of the host's status; none for a session without a provider (the synthetic ones are not shown here).
    pub fn of_status(entry: &Value) -> Option<Row> {
        entry["provider"].as_str()?;
        let text = |key: &str| entry[key].as_str().unwrap_or("").to_string();
        let number = |key: &str| entry[key].as_u64().unwrap_or(0);
        let approvals = entry["approvals"].as_array().into_iter().flatten().filter_map(|a| Some((a["approval_id"].as_i64()?, clean(a["prompt"].as_str()?)))).collect();
        Some(Row {
            session: text("session"), provider: text("provider"), account: text("account"), model: clean(&text("model")), state: text("state"), state_at: number("state_at"),
            activity: text("activity"), generation: entry["generation"].as_i64().unwrap_or(1), approvals, issue: text("account_issue"), mode: text("mode"),
            busy_since: number("busy_since"), last_event_at: number("last_event_at"), stalled: entry["stalled"].as_bool().unwrap_or(false),
            cwd: clean(&text("cwd")), name: clean(&text("name")),
        })
    }

    /// What the narrow list calls the session: its name, else the last part of its folder, else the start of its id; cut to `room`
    /// columns (with an ellipsis) and padded to them, so the states stay in a column.
    pub fn title(&self, room: usize) -> String {
        let folder = self.cwd.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        let chosen = [self.name.as_str(), folder].into_iter().find(|t| !t.is_empty()).map_or_else(|| self.session.chars().take(7).collect(), str::to_string);
        let width = |c: char| if editor::wide(c) { 2 } else { 1 };
        let (mut shown, mut columns) = (String::new(), 0);
        let fits = chosen.chars().map(width).sum::<usize>() <= room;
        for c in chosen.chars() {
            if columns + width(c) > if fits { room } else { room - 1 } {
                shown.push('…');
                columns += 1;
                break;
            }
            shown.push(c);
            columns += width(c);
        }
        shown + &" ".repeat(room.saturating_sub(columns))
    }

    /// The short form for the narrow list: the full wording is in the header of the selected session.
    pub fn list_state_text(&self) -> String {
        match self.state_text().as_str() {
            "waiting for approval" => "approval".into(),
            other => other.to_string(),
        }
    }

    /// Seconds the open turn has been running: only while the host says the session works and nothing waits for a person.
    pub fn working_for(&self, now: u64) -> Option<u64> {
        (self.state == "running" && self.activity == "working" && self.approvals.is_empty() && self.busy_since > 0).then(|| now.saturating_sub(self.busy_since))
    }

    /// Seconds without anything from the provider, once the host calls the working session stalled.
    pub fn silent_for(&self, now: u64) -> Option<u64> {
        self.working_for(now).filter(|_| self.stalled).map(|_| now.saturating_sub(self.last_event_at))
    }

    /// The state with the clock of an open turn, for the header.
    pub fn state_with_clock(&self, now: u64) -> String {
        match (self.working_for(now), self.silent_for(now)) {
            (Some(w), Some(q)) => format!("working {}, no output for {}", clock_text(w), clock_text(q)),
            (Some(w), None) => format!("working {}", clock_text(w)),
            _ => self.state_text(),
        }
    }

    /// The state and the age for the narrow list: the clock of an open turn in place of the coarse age.
    pub fn list_parts(&self, now: u64) -> (String, String) {
        match (self.working_for(now), self.silent_for(now)) {
            (_, Some(q)) => ("silent".into(), clock_compact(q)),
            (Some(w), None) => ("working".into(), clock_compact(w)),
            _ => (self.list_state_text(), ago(now, self.state_at)),
        }
    }

    /// What a person reads: the host's state, never a guess (waiting for approval beats working).
    pub fn state_text(&self) -> String {
        if self.state == "running" && !self.approvals.is_empty() {
            return "waiting for approval".into();
        }
        match (self.state.as_str(), self.activity.as_str()) {
            ("running", "working") => "working".into(),
            ("running", "idle") => "idle".into(),
            ("running", _) => "running".into(),
            (other, _) => other.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, cwd: &str) -> Row {
        Row { session: "s0123456789abcdef".into(), name: name.into(), cwd: cwd.into(), ..Default::default() }
    }

    #[test]
    fn the_list_calls_a_session_by_its_name_then_its_folder_then_its_id_and_every_title_has_the_same_width() {
        let columns = |t: &str| t.chars().map(|c| if editor::wide(c) { 2 } else { 1 }).sum::<usize>();
        let titles = [row("api", "/work/x"), row("", "/work/lico/"), row("", ""), row("a very long name indeed", ""), row("한글이름이긴경우", ""), row("", "/")];
        let shown: Vec<String> = titles.iter().map(|r| r.title(9)).collect();
        assert_eq!(shown[..3], ["api      ", "lico     ", "s012345  "]);
        assert_eq!(shown[3], "a very l…");
        assert_eq!(shown[4], "한글이름…");
        assert_eq!(shown[5], "s012345  ", "a folder that is the root has no name to show");
        assert!(shown.iter().all(|t| columns(t) == 9), "{shown:?}");
    }

    #[test]
    fn a_status_entry_gives_the_folder_and_the_name_and_a_synthetic_session_no_row() {
        let entry = json_entry();
        let row = Row::of_status(&entry).unwrap();
        assert_eq!((row.cwd.as_str(), row.name.as_str(), row.provider.as_str()), ("/work/lico", "api", "claude"));
        assert!(Row::of_status(&serde_json::json!({"session": "s1", "state": "running"})).is_none());
    }

    fn json_entry() -> Value {
        serde_json::json!({"session": "s1", "provider": "claude", "cwd": "/work/lico", "name": "api", "state": "running"})
    }
}
