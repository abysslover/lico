//! The memory guard's decisions (M19-P01): pure, no clock, no `/proc`, no display. Times are milliseconds given by the caller; memory is kB as `/proc` gives it.
//! Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Orange,
    Red,
}

/// The warning level and the limit of one agent process, in MB (1 MB = 1024 kB here, as the sizes of `/proc` are read).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub warn_mb: u64,
    pub limit_mb: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { warn_mb: 400, limit_mb: 500 }
    }
}

impl Limits {
    /// A pair that can be used: both above zero and the warning below the limit.
    pub fn valid(&self) -> bool {
        self.warn_mb > 0 && self.warn_mb < self.limit_mb
    }

    /// The level of a session by the RSS (kB) of the largest single process of its tree.
    pub fn level(&self, largest_process_kb: u64) -> Level {
        if largest_process_kb >= self.limit_mb.saturating_mul(1024) {
            Level::Red
        } else if largest_process_kb >= self.warn_mb.saturating_mul(1024) {
            Level::Orange
        } else {
            Level::Ok
        }
    }
}

/// The machine against `MemAvailable`: orange under 20 percent available, red under 10; no total, no judgement.
pub fn machine_level(available_kb: u64, total_kb: u64) -> Level {
    if total_kb == 0 {
        Level::Ok
    } else if available_kb.saturating_mul(100) < total_kb.saturating_mul(10) {
        Level::Red
    } else if available_kb.saturating_mul(100) < total_kb.saturating_mul(20) {
        Level::Orange
    } else {
        Level::Ok
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosedBy {
    Person,
    Guard,
}

impl ClosedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            ClosedBy::Person => "person",
            ClosedBy::Guard => "guard",
        }
    }

    pub fn parse(s: &str) -> Option<ClosedBy> {
        match s {
            "person" => Some(ClosedBy::Person),
            "guard" => Some(ClosedBy::Guard),
            _ => None,
        }
    }
}

/// A question to the person and a warning to the main agent come once for each red episode of a session; leaving red ends the episode.
#[derive(Default)]
pub struct Episodes {
    asked: BTreeSet<String>,
    warned: BTreeSet<String>,
}

impl Episodes {
    pub fn ask(&mut self, id: &str, level: Level) -> bool {
        once(&mut self.asked, id, level)
    }

    pub fn warn_main(&mut self, id: &str, level: Level) -> bool {
        once(&mut self.warned, id, level)
    }

    /// A session that is gone starts anew if it comes back.
    pub fn forget(&mut self, id: &str) {
        self.asked.remove(id);
        self.warned.remove(id);
    }
}

fn once(seen: &mut BTreeSet<String>, id: &str, level: Level) -> bool {
    if level == Level::Red {
        seen.insert(id.to_string())
    } else {
        seen.remove(id);
        false
    }
}

/// The sessions the guard closed and has still to resume: one at a time, `gap_ms` after the last resume, never a session the person closed.
pub struct ResumeQueue {
    gap_ms: u64,
    last_ms: Option<u64>,
    waiting: VecDeque<String>,
}

impl ResumeQueue {
    pub fn new(gap_ms: u64) -> ResumeQueue {
        ResumeQueue { gap_ms, last_ms: None, waiting: VecDeque::new() }
    }

    /// False when the session was closed by the person (it is never revived) or already waits.
    pub fn push(&mut self, id: &str, by: ClosedBy) -> bool {
        if by != ClosedBy::Guard || self.waiting.iter().any(|w| w == id) {
            return false;
        }
        self.waiting.push_back(id.to_string());
        true
    }

    /// The session to resume now, if one waits and the gap since the last resume has passed.
    pub fn next(&mut self, now_ms: u64) -> Option<String> {
        if self.last_ms.is_some_and(|t| now_ms < t.saturating_add(self.gap_ms)) {
            return None;
        }
        let id = self.waiting.pop_front()?;
        self.last_ms = Some(now_ms);
        Some(id)
    }

    /// The person took the session up meanwhile.
    pub fn cancel(&mut self, id: &str) {
        self.waiting.retain(|w| w != id);
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}

/// The auto-close of a session stops when it was closed twice in ten minutes (from then on it only warns).
#[derive(Default)]
pub struct LoopGuard {
    closes: BTreeMap<String, Vec<u64>>,
}

const LOOP_WINDOW_MS: u64 = 10 * 60_000;

impl LoopGuard {
    pub fn closed(&mut self, id: &str, now_ms: u64) {
        self.closes.entry(id.to_string()).or_default().push(now_ms);
    }

    pub fn allowed(&self, id: &str, now_ms: u64) -> bool {
        self.closes.get(id).map_or(0, |v| v.iter().filter(|&&t| now_ms.saturating_sub(t) <= LOOP_WINDOW_MS).count()) < 2
    }
}

/// An idle moment: no turn running, no approval waiting, the person not typing.
#[derive(Clone, Copy, Debug)]
pub struct Idle {
    pub turn_running: bool,
    pub approval_pending: bool,
    pub person_typing: bool,
}

impl Idle {
    pub fn ok(&self) -> bool {
        !self.turn_running && !self.approval_pending && !self.person_typing
    }
}

fn clean_name(name: &str) -> String {
    name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect()
}

/// The fixed warning to the main agent about a session: numbers, the id, a cleaned and capped name, the exact commands; nothing an agent wrote beyond that name.
pub fn warning_text(id: &str, name: &str, mb: u64, limit_mb: u64) -> String {
    format!(
        "Lico memory warning: session {id} (name: \"{}\") has a process using {mb} MB (limit {limit_mb} MB). If it is idle you may end it and continue it cleanly with these commands: `lico session close {id}` then `lico session resume {id}`. Tool jobs that were running in it are lost.",
        clean_name(name)
    )
}

/// The same for the machine: sessions together against the available memory.
pub fn machine_warning_text(sessions: u32, percent_available: u32, available_mb: u64) -> String {
    format!("Lico memory warning: {sessions} sessions are running and only {percent_available} percent of memory is available ({available_mb} MB). End an idle heavy session and continue it later with `lico session close ID` then `lico session resume ID`.")
}

/// The minutes after which a WORKING session that shows nothing is called stalled, unless the owner's setting says otherwise.
pub const DEFAULT_STALL_MIN: u64 = 10;

/// A working session whose last activity is `quiet_secs` ago is stalled from `stall_secs` seconds on (the caller passes `DEFAULT_STALL_MIN * 60` unless the owner's setting says otherwise); an
/// idle session waits for the person and never is; 0 switches the check off.
pub fn stalled(working: bool, quiet_secs: u64, stall_secs: u64) -> bool {
    working && stall_secs > 0 && quiet_secs >= stall_secs
}

/// "last activity N min ago" (minutes up to two hours, then hours).
pub fn quiet_text(quiet_secs: u64) -> String {
    match quiet_secs / 60 {
        0 => "last activity just now".to_string(),
        m if m < 120 => format!("last activity {m} min ago"),
        m => format!("last activity {} h ago", m / 60),
    }
}
