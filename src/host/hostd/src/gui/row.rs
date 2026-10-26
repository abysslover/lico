//! A session as the list and the header show it: what the host's status says about it, in the words a person reads.

use serde_json::Value;

use super::super::{clean, editor};

#[path = "memrow.rs"]
pub(crate) mod memrow;

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
    pub memory_kb: u64,     // what the agent and all its descendants hold of their own (RssAnon + RssShmem), 0 until the host says
    pub rss_kb: u64,        // the same processes' VmRSS
    pub processes: u32,
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
            memory_kb: entry["memory"]["private_kb"].as_u64().unwrap_or(0), rss_kb: entry["memory"]["rss_kb"].as_u64().unwrap_or(0), processes: entry["memory"]["processes"].as_u64().unwrap_or(0) as u32,
        })
    }

    /// What a session is called: its name, else the last part of its folder, else the start of its id.
    pub fn name(&self) -> String {
        let folder = self.cwd.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        [self.name.as_str(), folder].into_iter().find(|t| !t.is_empty()).map_or_else(|| self.session.chars().take(7).collect(), str::to_string)
    }

    /// What the narrow list calls the session: its name, else the last part of its folder, else the start of its id; cut to `room`
    /// columns (with an ellipsis) and padded to them, so the states stay in a column.
    pub fn title(&self, room: usize) -> String {
        let chosen = self.name();
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

    /// A turn is open: the host says the session works, or a request of its turn waits for a person. The work an interruption would cut short.
    pub fn holds_a_turn(&self) -> bool {
        self.state == "running" && (self.activity == "working" || !self.approvals.is_empty())
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

    fn working(busy_since: u64, last_event_at: u64, stalled: bool) -> Row {
        Row {
            session: "s".into(), provider: "claude".into(), account: String::new(), model: "m".into(), state: "running".into(), state_at: 100, activity: "working".into(),
            generation: 1, approvals: Vec::new(), issue: String::new(), mode: String::new(), busy_since, last_event_at, stalled, ..Default::default()
        }
    }

    #[test]
    fn the_clock_of_an_open_turn_is_read_in_seconds_then_minutes_then_hours() {
        let spoken: Vec<String> = [0, 7, 59, 60, 125, 3599, 3600, 7385].iter().map(|n| clock_text(*n)).collect();
        assert_eq!(spoken, ["0 s", "7 s", "59 s", "1 m 00 s", "2 m 05 s", "59 m 59 s", "1 h 00 m", "2 h 03 m"]);
        let narrow: Vec<String> = [0, 7, 59, 60, 125, 3599, 3600, 7385].iter().map(|n| clock_compact(*n)).collect();
        assert_eq!(narrow, ["0s", "7s", "59s", "1m00s", "2m05s", "59m59s", "1h00m", "2h03m"]);
    }

    #[test]
    fn a_working_session_shows_its_clock_and_a_silent_one_says_so_in_the_header_and_the_list() {
        let row = working(1000, 1037, false);
        assert_eq!(row.state_with_clock(1037), "working 37 s");
        assert_eq!(row.list_parts(1037), ("working".to_string(), "37s".to_string()));
        let quiet = working(1000, 1000, true);
        assert_eq!(quiet.state_with_clock(1071), "working 1 m 11 s, no output for 1 m 11 s");
        assert_eq!(quiet.list_parts(1071), ("silent".to_string(), "1m11s".to_string()));
        // a stall flag from the host means nothing unless the session works
        let mut idle = working(0, 0, true);
        idle.activity = "idle".into();
        assert_eq!(idle.state_with_clock(5000), "idle");
        assert_eq!(idle.list_parts(5000), ("idle".to_string(), ago(5000, 100)));
    }

    #[test]
    fn a_session_that_waits_for_a_person_has_no_clock_and_no_stall() {
        let mut row = working(1000, 1000, true);
        row.approvals = vec![(3, "run: ls".into())];
        assert_eq!(row.working_for(1100), None);
        assert_eq!(row.silent_for(1100), None);
        assert_eq!(row.state_with_clock(1100), "waiting for approval");
        assert_eq!(row.list_parts(1100).0, "approval");
    }

    #[test]
    fn a_status_without_the_clock_fields_is_a_session_that_does_not_work() {
        let mut row = working(0, 0, false);
        row.activity = "idle".into();
        assert_eq!((row.working_for(9), row.silent_for(9)), (None, None));
        let mut unreported = working(0, 0, false); // the host said working but gave no start: no clock is made up
        unreported.busy_since = 0;
        assert_eq!(unreported.working_for(9), None);
    }
}
