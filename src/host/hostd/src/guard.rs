//! The memory guard's host side (M19-P01): who closed each session. A side table kept while the host runs (a session closed by the person is never revived by the guard; only a
//! session the guard closed is). Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use lico_memguard::{ClosedBy, Episodes, Idle, Level, Limits, LoopGuard, ResumeQueue};
use serde_json::Value;


static CLOSED_BY: Mutex<BTreeMap<String, &'static str>> = Mutex::new(BTreeMap::new());

/// Records who closed `session`: `person` (the default) or `guard`.
pub fn record(session: &str, by: Option<&str>) {
    let by = if by == Some("guard") { "guard" } else { "person" };
    CLOSED_BY.lock().unwrap().insert(session.to_string(), by);
}

/// The session runs again (or is gone): nobody closed it now.
pub fn clear(session: &str) {
    CLOSED_BY.lock().unwrap().remove(session);
}

pub fn closed_by(session: &str) -> Option<&'static str> {
    CLOSED_BY.lock().unwrap().get(session).copied()
}


// ---- the auto-close (Slice B): OFF unless the owner's file says so ---------------------------------------------------------------------------------------

/// How long after the person's last input to a session it still counts as being typed to.
const TYPING_QUIET_MS: u64 = 30_000;
/// The gap between two resumes by the guard.
const RESUME_GAP_MS: u64 = 20_000;
/// After the warning to the main agent the auto-close waits this long (the main agent may deal with it itself); a close for the machine is also paced by it (one at a time).
const GRACE_MS: u64 = 120_000;
/// The key of the machine's own red episode in the tables below (no session has this id).
const MACHINE_KEY: &str = "\u{0}machine";
/// When the main agent could be warned but has not been yet (it is busy), the auto-close holds this long, then closes anyway: a heavy session must not wait for ever.
const WARN_WAIT_MS: u64 = 300_000;
/// A warning that could not be delivered is tried again after this long.
const RETRY_MS: u64 = 30_000;

/// The owner's settings, `memory.json` of the state folder (an owner-only plain file): `{"warn_mb": 400, "limit_mb": 500, "auto_close": false, "auto_close_off": ["session id"]}`. No file, or a file
/// that cannot be used, means the defaults and no auto-close.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub limits: Limits,
    pub auto_close: bool,
    pub auto_close_off: Vec<String>,
    /// The warning to the main agent (ON by default, one switch) and the session the person marked as the main agent (`main_agent`).
    pub warn_main: bool,
    pub main_agent: Option<String>,
    /// Seconds of no activity after which a working session is called stalled (0: off). The owner's file gives minutes (`stall_min`); `stall_s` (seconds) wins when both are there and lets a test or a quick check use seconds.
    pub stall_secs: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config { limits: Limits::default(), auto_close: false, auto_close_off: Vec::new(), warn_main: true, main_agent: None, stall_secs: lico_memguard::DEFAULT_STALL_MIN * 60 }
    }
}

pub fn parse_config(text: &str) -> Config {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Config::default() };
    let mut c = Config::default();
    let limits = Limits { warn_mb: v["warn_mb"].as_u64().unwrap_or(c.limits.warn_mb), limit_mb: v["limit_mb"].as_u64().unwrap_or(c.limits.limit_mb) };
    if limits.valid() {
        c.limits = limits;
    }
    c.auto_close = v["auto_close"].as_bool() == Some(true);
    c.stall_secs = v["stall_min"].as_u64().map_or(c.stall_secs, |m| m.min(24 * 60) * 60);
    c.stall_secs = v["stall_s"].as_u64().map_or(c.stall_secs, |s| s.min(24 * 3600));
    c.warn_main = v["warn_main"].as_bool() != Some(false);
    c.main_agent = v["main_agent"].as_str().filter(|s| !s.is_empty() && s.len() <= 64).map(str::to_string);
    c.auto_close_off = v["auto_close_off"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).take(64).collect();
    c
}

fn read_config(state_dir: &Path) -> Config {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let path = state_dir.join("memory.json");
    let Ok(m) = std::fs::metadata(&path) else { return Config::default() };
    if !m.is_file() || m.permissions().mode() & 0o077 != 0 || m.len() > 4096 || m.uid() != unsafe { getuid() } {
        return Config::default();
    }
    std::fs::read_to_string(path).map_or_else(|_| Config::default(), |t| parse_config(&t))
}

extern "C" {
    fn getuid() -> u32;
}

#[derive(Default)]
struct State {
    queue: Option<ResumeQueue>,
    loops: LoopGuard,
    typed: BTreeMap<String, u64>,
    episodes: Episodes,
    /// When the main agent was first warned (the warning was tried) about a session or the machine: the auto-close waits from then.
    warned: BTreeMap<String, u64>,
    /// When the warning was last tried (a failed delivery is tried again after `RETRY_MS`).
    tried: BTreeMap<String, u64>,
    /// Since when a red session (or the machine) waits for a warning that could not be sent yet.
    waiting: BTreeMap<String, u64>,
    /// The session chosen for the machine's sake, and when the last one was really closed for it (one at a time, a grace time apart).
    machine_pick: Option<String>,
    machine_closed: Option<u64>,
    /// The memory in MB of a session the guard closed, for the note that tells the person after it resumed.
    notes: BTreeMap<String, u64>,
}

impl State {
    /// The hub really closed `id` for the guard.
    fn mark_closed(&mut self, id: &str, now: u64) {
        if self.machine_pick.as_deref() == Some(id) {
            self.machine_pick = None;
            self.machine_closed = Some(now); // the next close for the machine's sake only after another grace time
        }
        self.loops.closed(id, now);
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The person sent input to a session (the idle test must not close it while they are typing).
pub fn note_input(session: &str) {
    STATE.lock().unwrap().get_or_insert_with(State::default).typed.insert(session.to_string(), now_ms());
}

/// The warning for the main agent now, if there is one: (the main agent's session, the text, the key of the episode it belongs to). Only at an idle moment of the main agent (no turn, no approval, the person not typing), once for
/// each red episode of a session (an episode is used up only when its warning is sent) and once for each red episode of the machine; one warning at a time.
fn plan_warning(entries: &[Value], cfg: &Config, st: &mut State, now: u64, machine: Option<(u64, u64)>) -> Option<(String, String, String)> {
    let main = cfg.main_agent.as_deref()?;
    let level_of = |e: &Value| cfg.limits.level(e["memory"]["max_rss_kb"].as_u64().unwrap_or(0));
    for e in entries {
        if let Some(id) = e["session"].as_str() {
            if level_of(e) != Level::Red {
                st.episodes.warn_main(id, Level::Ok); // the episode of this session ended
                st.warned.remove(id);
                st.tried.remove(id);
            }
        }
    }
    let machine_level = machine.map_or(Level::Ok, |(avail, total)| lico_memguard::machine_level(avail, total));
    if machine_level != Level::Red {
        st.episodes.warn_main(MACHINE_KEY, Level::Ok);
        st.warned.remove(MACHINE_KEY);
        st.tried.remove(MACHINE_KEY);
    }
    let retry = |st: &State, key: &str| st.tried.get(key).is_some_and(|t| now.saturating_sub(*t) < RETRY_MS);
    if !cfg.warn_main {
        return None;
    }
    let me = entries.iter().find(|e| e["session"] == main)?;
    let typing = st.typed.get(main).is_some_and(|t| now.saturating_sub(*t) < TYPING_QUIET_MS);
    let idle = Idle { turn_running: me["activity"] != "idle", approval_pending: me["approvals"].as_array().is_some_and(|a| !a.is_empty()), person_typing: typing };
    if me["state"] != "running" || !idle.ok() {
        return None;
    }
    for e in entries {
        let Some(id) = e["session"].as_str() else { continue };
        if id != main && e["state"] == "running" && level_of(e) == Level::Red && !retry(st, id) && st.episodes.warn_main(id, Level::Red) {
            let kb = e["memory"]["max_rss_kb"].as_u64().unwrap_or(0);
            st.warned.entry(id.to_string()).or_insert(now);
            st.tried.insert(id.to_string(), now);
            return Some((main.to_string(), lico_memguard::warning_text(id, e["name"].as_str().unwrap_or(""), kb / 1024, cfg.limits.limit_mb), id.to_string()));
        }
    }
    if let (Level::Red, Some((avail, total))) = (machine_level, machine) {
        if !retry(st, MACHINE_KEY) && st.episodes.warn_main(MACHINE_KEY, Level::Red) {
            st.warned.entry(MACHINE_KEY.to_string()).or_insert(now);
            st.tried.insert(MACHINE_KEY.to_string(), now);
            let running = entries.iter().filter(|e| e["state"] == "running").count() as u32;
            return Some((main.to_string(), lico_memguard::machine_warning_text(running, (avail.saturating_mul(100) / total.max(1)) as u32, avail / 1024), MACHINE_KEY.to_string()));
        }
    }
    None
}

/// `MemAvailable` and `MemTotal` of this machine in kB.
fn meminfo() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)?.split_whitespace().next()?.parse::<u64>().ok());
    Some((get("MemAvailable:")?, get("MemTotal:")?))
}

/// The running sessions to close and resume now: red by the largest process, auto-close on for them, idle (no turn, no approval, the person not typing), not stopped by the loop guard, and not
/// within the grace after the warning to the main agent. When the machine is red as well, the heaviest idle session that is not the main agent (by the RSS of its whole tree) is added, one
/// at a time and at most one every grace time.
fn plan(entries: &[Value], cfg: &Config, st: &mut State, now: u64, machine: Option<(u64, u64)>) -> Vec<(String, i64)> {
    if !cfg.auto_close {
        return Vec::new();
    }
    let holding = |st: &State, key: &str| st.warned.get(key).is_some_and(|t| now.saturating_sub(*t) < GRACE_MS);
    // With a main agent that could be warned (it runs, the switch is on) a heavy session is not closed before the warning has been tried: it waits for an idle moment of the main agent, for at most WARN_WAIT_MS.
    let warnable = cfg.warn_main && cfg.main_agent.as_deref().is_some_and(|m| entries.iter().any(|e| e["session"] == m && e["state"] == "running"));
    let waits_for_warning = |st: &mut State, key: &str| -> bool {
        if !warnable || cfg.main_agent.as_deref() == Some(key) || st.warned.contains_key(key) {
            return false;
        }
        let since = *st.waiting.entry(key.to_string()).or_insert(now);
        now.saturating_sub(since) < WARN_WAIT_MS
    };
    for e in entries {
        if let Some(id) = e["session"].as_str() {
            if cfg.limits.level(e["memory"]["max_rss_kb"].as_u64().unwrap_or(0)) != Level::Red {
                st.waiting.remove(id); // the episode ended
            }
        }
    }
    let ok_to_close = |st: &State, e: &Value, id: &str| -> bool {
        let typing = st.typed.get(id).is_some_and(|t| now.saturating_sub(*t) < TYPING_QUIET_MS);
        let idle = Idle { turn_running: e["activity"] != "idle", approval_pending: e["approvals"].as_array().is_some_and(|a| !a.is_empty()), person_typing: typing };
        e["state"] == "running" && !cfg.auto_close_off.iter().any(|o| o == id) && idle.ok() && st.loops.allowed(id, now)
    };
    let mut out: Vec<(String, i64)> = Vec::new();
    for e in entries {
        let (Some(id), Some(generation)) = (e["session"].as_str(), e["generation"].as_i64()) else { continue };
        if cfg.limits.level(e["memory"]["max_rss_kb"].as_u64().unwrap_or(0)) != Level::Red || holding(st, id) || !ok_to_close(st, e, id) || waits_for_warning(st, id) {
            continue;
        }
        out.push((id.to_string(), generation));
    }
    let machine_red = machine.is_some_and(|(avail, total)| lico_memguard::machine_level(avail, total) == Level::Red);
    if !machine_red {
        st.waiting.remove(MACHINE_KEY);
    }
    let paced = st.machine_closed.is_some_and(|t| now.saturating_sub(t) < GRACE_MS);
    if machine_red && !holding(st, MACHINE_KEY) && !paced && !waits_for_warning(st, MACHINE_KEY) {
        let heaviest = entries
            .iter()
            .filter_map(|e| Some((e, e["session"].as_str()?, e["generation"].as_i64()?)))
            .filter(|(e, id, _)| Some(*id) != cfg.main_agent.as_deref() && !out.iter().any(|(o, _)| o == id) && ok_to_close(st, e, id))
            .max_by_key(|(e, _, _)| e["memory"]["rss_kb"].as_u64().or(e["memory"]["max_rss_kb"].as_u64()).unwrap_or(0));
        if let Some((_, id, generation)) = heaviest {
            out.push((id.to_string(), generation));
            st.machine_pick = Some(id.to_string()); // the pace starts when the close has really happened (`closed_ok`)
        }
    }
    out
}

/// The warning level and the limit the host uses (the owner's `memory.json`, else 400 and 500 MB): sent in every `status` reply so that the window colours by the same numbers.
pub fn limits(state_dir: &Path) -> Limits {
    read_config(state_dir).limits
}

/// The stall time in seconds the host uses (the owner's `stall_min` or `stall_s`, else 10 minutes).
pub fn stall_secs(state_dir: &Path) -> u64 {
    read_config(state_dir).stall_secs
}

/// What to do now after a `status` reply (computed here, done by the hubs: this module names no hub, so it stays out of the cycle of the hubs).
pub struct Step {
    pub now: u64,
    /// The running sessions to close with `by: guard`: (id, generation, keep the worktree).
    pub close: Vec<(String, i64, bool)>,
    /// A guard-closed session whose turn has come to be resumed (one at a time, the gap since the last resume has passed).
    pub resume: Option<String>,
    /// A warning for the main agent: (the main agent's session, the text, the key of its episode: give it to `warn_failed` when it could not be delivered).
    pub warn: Option<(String, String, String)>,
}

pub fn step(state_dir: &Path, entries: &[Value]) -> Step {
    let cfg = read_config(state_dir);
    let now = now_ms();
    let machine = meminfo();
    let mut g = STATE.lock().unwrap();
    let st = g.get_or_insert_with(State::default);
    let resume = st.queue.get_or_insert_with(|| ResumeQueue::new(RESUME_GAP_MS)).next(now);
    let warn = plan_warning(entries, &cfg, st, now, machine); // first: the grace of the auto-close runs from the warning
    let close = plan(entries, &cfg, st, now, machine).into_iter().map(|(id, generation)| {
        let keep = entries.iter().find(|e| e["session"] == id.as_str()).is_some_and(|e| !e["worktree"].is_null());
        (id, generation, keep)
    });
    let close: Vec<(String, i64, bool)> = close.collect();
    Step { now, close, resume, warn }
}

/// The hub closed `id` for the guard: remember who, count it for the loop guard and queue it for its resume.
pub fn closed_ok(id: &str, now: u64, mb: u64) {
    record(id, Some("guard"));
    let mut g = STATE.lock().unwrap();
    let st = g.get_or_insert_with(State::default);
    st.notes.insert(id.to_string(), mb);
    st.mark_closed(id, now);
    st.queue.get_or_insert_with(|| ResumeQueue::new(RESUME_GAP_MS)).push(id, ClosedBy::Guard);
}

/// The warning of this episode could not be delivered: the episode is open again and the warning is tried again after `RETRY_MS` (the grace runs from the first try).
pub fn warn_failed(key: &str) {
    let mut g = STATE.lock().unwrap();
    g.get_or_insert_with(State::default).episodes.warn_main(key, Level::Ok);
}

/// The guard's resume of `id` did not work: its note is dropped (the session stays closed, marked as closed by the guard, for the person to resume).
pub fn resume_failed(id: &str) {
    take_note(id);
}

/// The line the session shows after the guard closed it and resumed it.
pub fn auto_note(mb: u64) -> String {
    format!("[memory {mb} MB: closed and resumed automatically]")
}

/// The memory (MB) the guard saw when it closed `id`, once: for the note after the resume.
pub fn take_note(id: &str) -> Option<u64> {
    STATE.lock().unwrap().get_or_insert_with(State::default).notes.remove(id)
}

/// `MemAvailable` and `MemTotal` in kB for the `status` reply (the window shows the machine's line and warns before a new session from it).
pub fn machine_memory() -> Option<(u64, u64)> {
    meminfo()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_closed_a_session_is_recorded_shown_and_cleared_when_it_resumes() {
        record("g_s1", Some("guard"));
        record("g_s2", None);
        record("g_s3", Some("anything else"));
        assert_eq!((closed_by("g_s1"), closed_by("g_s2"), closed_by("g_s3"), closed_by("g_none")), (Some("guard"), Some("person"), Some("person"), None), "only the exact word guard is the guard");
        clear("g_s1");
        assert_eq!(closed_by("g_s1"), None);
    }

    #[test]
    fn session_close_takes_who_closed_it_and_nothing_else_new() {
        let map = |v: serde_json::Value| v.as_object().cloned().unwrap();
        let ok = |v: serde_json::Value| crate::ipc::valid_request("session.close", &map(v));
        assert!(ok(serde_json::json!({"op": "session.close", "session": "s1", "generation": 1})));
        assert!(ok(serde_json::json!({"op": "session.close", "session": "s1", "generation": 1, "by": "guard"})));
        assert!(ok(serde_json::json!({"op": "session.close", "session": "s1", "generation": 1, "by": "person", "hide_tab": true})));
        assert!(!ok(serde_json::json!({"op": "session.close", "session": "s1", "generation": 1, "by": "robot"})));
        assert!(!ok(serde_json::json!({"op": "session.close", "session": "s1", "generation": 1, "by": 1})));
        assert!(!crate::ipc::valid_request("session.resume", &map(serde_json::json!({"op": "session.resume", "session": "s1", "generation": 1, "by": "guard"}))), "only close takes it");
    }

    fn entry(id: &str, kb: u64, activity: &str, approvals: usize) -> Value {
        serde_json::json!({"session": id, "generation": 3, "state": "running", "activity": activity, "approvals": vec![1; approvals], "memory": {"max_rss_kb": kb}})
    }

    #[test]
    fn the_main_agent_is_warned_once_for_each_red_episode_only_when_it_is_idle_and_never_with_a_name_it_did_not_get_cleaned() {
        let cfg = Config { main_agent: Some("main".into()), ..Config::default() };
        let mut st = State::default();
        let t = 5_000_000;
        let mut heavy = entry("w1", 520 * 1024, "working", 0);
        heavy["name"] = serde_json::json!("builder\u{1b}[31m");
        let mut all = vec![entry("main", 50 * 1024, "idle", 0), heavy];
        let (id, text, key) = plan_warning(&all, &cfg, &mut st, t, None).expect("a warning");
        assert_eq!(key, "w1");
        assert_eq!(id, "main");
        assert!(text.contains("w1") && text.contains("520 MB") && text.contains("lico session close w1") && !text.contains('\u{1b}'), "{text}");
        assert!(plan_warning(&all, &cfg, &mut st, t, None).is_none(), "once for the episode");
        all[1] = entry("w1", 100 * 1024, "working", 0);
        assert!(plan_warning(&all, &cfg, &mut st, t, None).is_none());
        all[1] = entry("w1", 520 * 1024, "working", 0);
        assert!(plan_warning(&all, &cfg, &mut st, t, None).is_some(), "a new episode after it left red");
        // not while the main agent works, asks or is typed to; the episode is kept for later
        let mut st = State::default();
        for busy in [entry("main", 1, "working", 0), entry("main", 1, "idle", 1)] {
            all[0] = busy;
            assert!(plan_warning(&all, &cfg, &mut st, t, None).is_none());
        }
        all[0] = entry("main", 1, "idle", 0);
        st.typed.insert("main".into(), t - 1_000);
        assert!(plan_warning(&all, &cfg, &mut st, t, None).is_none(), "the person is typing to the main agent");
        assert!(plan_warning(&all, &cfg, &mut st, t + 40_000, None).is_some(), "later, at an idle moment, it is still sent");
        // switched off, or no main agent marked
        assert!(plan_warning(&all, &Config { warn_main: false, ..cfg.clone() }, &mut State::default(), t, None).is_none());
        assert!(plan_warning(&all, &Config::default(), &mut State::default(), t, None).is_none());
        // the machine
        let quiet = vec![entry("main", 1, "idle", 0)];
        let total = 20 * 1024 * 1024;
        let mut st = State::default();
        assert!(plan_warning(&quiet, &cfg, &mut st, t, Some((total / 4, total))).is_none());
        let (_, text, _) = plan_warning(&quiet, &cfg, &mut st, t, Some((total / 20, total))).expect("the machine is red");
        assert!(text.contains("1 sessions") && text.contains("5 percent"), "{text}");
        assert!(plan_warning(&quiet, &cfg, &mut st, t, Some((total / 20, total))).is_none(), "once for the episode");
    }

    fn entry_rss(id: &str, max_kb: u64, rss_kb: u64, activity: &str) -> Value {
        serde_json::json!({"session": id, "generation": 3, "state": "running", "activity": activity, "approvals": [], "memory": {"max_rss_kb": max_kb, "rss_kb": rss_kb}})
    }

    #[test]
    fn after_the_warning_to_the_main_agent_the_auto_close_waits_two_minutes_and_without_a_main_agent_it_does_not_wait() {
        let cfg = Config { auto_close: true, main_agent: Some("main".into()), ..Config::default() };
        let all = vec![entry("main", 50 * 1024, "idle", 0), entry("w1", 520 * 1024, "idle", 0)];
        let ids = |p: Vec<(String, i64)>| p.into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        let mut st = State::default();
        let t = 9_000_000;
        assert!(plan_warning(&all, &cfg, &mut st, t, None).is_some(), "the warning goes first");
        assert!(plan(&all, &cfg, &mut st, t, None).is_empty(), "and the close waits for the grace");
        assert!(plan(&all, &cfg, &mut st, t + GRACE_MS - 1, None).is_empty());
        assert_eq!(ids(plan(&all, &cfg, &mut st, t + GRACE_MS, None)), ["w1"], "still red after two minutes: closed");
        // no main agent marked, nobody to warn: no grace
        let none = Config { main_agent: None, ..cfg.clone() };
        assert_eq!(ids(plan(&all, &none, &mut State::default(), t, None)), ["w1"]);
        // the session left red: the next episode is warned again and waits again
        let mut st = State::default();
        assert!(plan_warning(&all, &cfg, &mut st, t, None).is_some());
        let calm = vec![entry("main", 1, "idle", 0), entry("w1", 100 * 1024, "idle", 0)];
        assert!(plan_warning(&calm, &cfg, &mut st, t + 10, None).is_none());
        assert!(plan_warning(&all, &cfg, &mut st, t + 20, None).is_some(), "a new episode");
        assert!(plan(&all, &cfg, &mut st, t + 21, None).is_empty());
    }

    #[test]
    fn a_heavy_session_is_not_closed_before_the_main_agent_could_be_warned_but_not_for_ever_and_a_failed_delivery_is_tried_again() {
        let cfg = Config { auto_close: true, main_agent: Some("main".into()), ..Config::default() };
        let ids = |p: Vec<(String, i64)>| p.into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        let t = 7_000_000;
        // the main agent works: no warning can go, and the close holds (it would otherwise close the session with no warning and no grace)
        let busy = vec![entry("main", 50 * 1024, "working", 0), entry("w1", 520 * 1024, "idle", 0)];
        let mut st = State::default();
        assert!(plan_warning(&busy, &cfg, &mut st, t, None).is_none());
        assert!(plan(&busy, &cfg, &mut st, t, None).is_empty(), "held for the warning");
        assert!(plan(&busy, &cfg, &mut st, t + WARN_WAIT_MS - 1, None).is_empty());
        assert_eq!(ids(plan(&busy, &cfg, &mut st, t + WARN_WAIT_MS, None)), ["w1"], "five minutes without an idle moment of the main agent: closed anyway");
        // the main agent is not running at all: nobody to wait for
        let gone = vec![serde_json::json!({"session": "main", "generation": 1, "state": "closed", "activity": "idle", "approvals": [], "memory": {}}), entry("w1", 520 * 1024, "idle", 0)];
        assert_eq!(ids(plan(&gone, &cfg, &mut State::default(), t, None)), ["w1"]);
        // the switch for the warning off: no wait either
        assert_eq!(ids(plan(&busy, &Config { warn_main: false, ..cfg.clone() }, &mut State::default(), t, None)), ["w1"]);
        // the episode ended while it waited: the next one waits from its own start
        let calm = vec![entry("main", 50 * 1024, "working", 0), entry("w1", 100 * 1024, "idle", 0)];
        assert!(plan(&calm, &cfg, &mut st, t + WARN_WAIT_MS + 10, None).is_empty());
        assert!(plan(&busy, &cfg, &mut st, t + WARN_WAIT_MS + 20, None).is_empty(), "a new episode waits again");
        // a delivery that failed: the episode is open again, tried again after RETRY_MS, and the grace runs from the first try
        let idle = vec![entry("main", 50 * 1024, "idle", 0), entry("w1", 520 * 1024, "idle", 0)];
        let mut st = State::default();
        let (_, _, key) = plan_warning(&idle, &cfg, &mut st, t, None).expect("the first try");
        st.episodes.warn_main(&key, Level::Ok); // what `warn_failed` does
        assert!(plan_warning(&idle, &cfg, &mut st, t + RETRY_MS - 1, None).is_none(), "not at every tick");
        assert!(plan_warning(&idle, &cfg, &mut st, t + RETRY_MS, None).is_some(), "again after the retry time");
        assert!(plan(&idle, &cfg, &mut st, t + GRACE_MS - 1, None).is_empty());
        assert_eq!(ids(plan(&idle, &cfg, &mut st, t + GRACE_MS, None)), ["w1"], "the grace counts from the first try: a main agent that never answers does not hold it for ever");
    }

    #[test]
    fn a_red_machine_closes_the_heaviest_idle_session_that_is_not_the_main_agent_one_at_a_time_two_minutes_apart() {
        let cfg = Config { auto_close: true, main_agent: Some("main".into()), ..Config::default() };
        let total = 20 * 1024 * 1024;
        let red = Some((total / 20, total));
        let all = vec![
            entry_rss("main", 300 * 1024, 900 * 1024, "idle"),
            entry_rss("a", 200 * 1024, 300 * 1024, "idle"),
            entry_rss("b", 300 * 1024, 600 * 1024, "idle"),
            entry_rss("busy", 300 * 1024, 2000 * 1024, "working"),
        ];
        let ids = |p: Vec<(String, i64)>| p.into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        let mut st = State::default();
        let t = 4_000_000;
        assert!(plan_warning(&all, &cfg, &mut st, t, red).is_some(), "the machine warning goes first");
        assert!(plan(&all, &cfg, &mut st, t, red).is_empty(), "grace");
        assert_eq!(ids(plan(&all, &cfg, &mut st, t + GRACE_MS, red)), ["b"], "the heaviest IDLE session that is not the main agent");
        assert_eq!(ids(plan(&all, &cfg, &mut st, t + GRACE_MS + 1_000, red)), ["b"], "until the close has really happened it is still the one chosen");
        st.mark_closed("b", t + GRACE_MS + 1_000);
        assert!(plan(&all, &cfg, &mut st, t + GRACE_MS + 2_000, red).is_empty(), "one at a time");
        let mut again = all.clone();
        again[2] = entry_rss("b", 300 * 1024, 100 * 1024, "idle");
        assert_eq!(ids(plan(&again, &cfg, &mut st, t + 2 * GRACE_MS + 1_000, red)), ["a"], "two minutes after the close, the next heaviest");
        // a healthy machine, or the switch off: nothing from the machine
        assert!(plan(&all, &cfg, &mut State::default(), t, Some((total / 2, total))).is_empty());
        assert!(plan(&all, &Config { auto_close: false, ..cfg.clone() }, &mut State::default(), t, red).is_empty());
        // the loop guard still holds
        let mut st = State::default();
        st.loops.closed("b", t - 1);
        st.loops.closed("b", t - 2);
        assert!(plan_warning(&all, &cfg, &mut st, t, red).is_some());
        assert_eq!(ids(plan(&all, &cfg, &mut st, t + GRACE_MS, red)), ["a"], "b was closed twice in ten minutes: not b, and never the main agent");
    }

    #[test]
    fn the_note_after_an_automatic_close_names_the_memory_and_is_given_once() {
        assert_eq!(auto_note(612), "[memory 612 MB: closed and resumed automatically]");
        closed_ok("note_s1", 5, 612);
        assert_eq!(take_note("note_s1"), Some(612));
        assert_eq!(take_note("note_s1"), None);
        assert_eq!(take_note("note_none"), None);
    }

    #[test]
    fn the_stall_time_is_ten_minutes_unless_the_file_says_otherwise_and_is_capped_at_a_day() {
        assert_eq!(Config::default().stall_secs, 600);
        assert_eq!(parse_config(r#"{"stall_min": 3}"#).stall_secs, 180);
        assert_eq!(parse_config(r#"{"stall_min": 0}"#).stall_secs, 0, "0 switches it off");
        assert_eq!(parse_config(r#"{"stall_min": 99999}"#).stall_secs, 24 * 3600);
        assert_eq!(parse_config(r#"{"stall_min": "x"}"#).stall_secs, 600);
        assert_eq!(parse_config(r#"{"stall_s": 3}"#).stall_secs, 3, "seconds");
        assert_eq!(parse_config(r#"{"stall_min": 3, "stall_s": 7}"#).stall_secs, 7, "seconds win");
        assert_eq!(parse_config(r#"{"stall_s": 999999}"#).stall_secs, 24 * 3600);
    }

    #[test]
    fn the_settings_file_gives_limits_and_the_switch_and_anything_unusable_means_the_defaults_and_no_auto_close() {
        assert_eq!(parse_config("nonsense"), Config::default());
        assert!(!Config::default().auto_close);
        let c = parse_config(r#"{"warn_mb": 300, "limit_mb": 600, "auto_close": true, "auto_close_off": ["s9"]}"#);
        assert_eq!((c.limits, c.auto_close, c.auto_close_off), (Limits { warn_mb: 300, limit_mb: 600 }, true, vec!["s9".to_string()]));
        assert_eq!(parse_config(r#"{"warn_mb": 700, "limit_mb": 600}"#).limits, Limits::default(), "an unusable pair keeps the defaults");
        assert!(!parse_config(r#"{"auto_close": "yes"}"#).auto_close, "only the true of JSON switches it on");
    }

    #[test]
    fn only_a_red_idle_session_that_may_be_auto_closed_is_planned() {
        let on = Config { auto_close: true, ..Config::default() };
        let mut st = State::default();
        let red = 520 * 1024;
        let t = 1_000_000;
        let ids = |p: Vec<(String, i64)>| p.into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        let all = vec![entry("red_idle", red, "idle", 0), entry("orange", 450 * 1024, "idle", 0), entry("working", red, "working", 0), entry("asking", red, "idle", 1), entry("typed", red, "idle", 0), entry("off", red, "idle", 0)];
        st.typed.insert("typed".into(), t - 1_000);
        let cfg = Config { auto_close_off: vec!["off".into()], ..on.clone() };
        assert_eq!(ids(plan(&all, &cfg, &mut st, t, None)), ["red_idle"]);
        assert!(plan(&all, &Config::default(), &mut st, t, None).is_empty(), "off by default");
        st.typed.insert("typed".into(), t - 31_000);
        assert!(ids(plan(&all, &cfg, &mut st, t, None)).contains(&"typed".to_string()), "typing ended 31 s ago");
        st.loops.closed("red_idle", t - 1);
        st.loops.closed("red_idle", t - 2);
        assert!(!ids(plan(&all, &cfg, &mut st, t, None)).contains(&"red_idle".to_string()), "twice in ten minutes: warn only");
    }
}
