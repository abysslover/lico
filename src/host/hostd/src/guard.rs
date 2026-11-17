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
    /// Minutes of no activity after which a working session is called stalled (0: off).
    pub stall_min: u64,
}

impl Default for Config {
    fn default() -> Config {
        Config { limits: Limits::default(), auto_close: false, auto_close_off: Vec::new(), warn_main: true, main_agent: None, stall_min: lico_memguard::DEFAULT_STALL_MIN }
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
    c.stall_min = v["stall_min"].as_u64().map_or(c.stall_min, |m| m.min(24 * 60));
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
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The person sent input to a session (the idle test must not close it while they are typing).
pub fn note_input(session: &str) {
    STATE.lock().unwrap().get_or_insert_with(State::default).typed.insert(session.to_string(), now_ms());
}

/// The warning for the main agent now, if there is one: (the main agent's session, the text). Only at an idle moment of the main agent (no turn, no approval, the person not typing), once for
/// each red episode of a session (an episode is used up only when its warning is sent) and once for each red episode of the machine; one warning at a time.
fn plan_warning(entries: &[Value], cfg: &Config, st: &mut State, now: u64, machine: Option<(u64, u64)>) -> Option<(String, String)> {
    let main = cfg.main_agent.as_deref()?;
    let level_of = |e: &Value| cfg.limits.level(e["memory"]["max_rss_kb"].as_u64().unwrap_or(0));
    for e in entries {
        if let Some(id) = e["session"].as_str() {
            if level_of(e) != Level::Red {
                st.episodes.warn_main(id, Level::Ok); // the episode of this session ended
            }
        }
    }
    let machine_level = machine.map_or(Level::Ok, |(avail, total)| lico_memguard::machine_level(avail, total));
    if machine_level != Level::Red {
        st.episodes.warn_main("\u{0}machine", Level::Ok);
    }
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
        if id != main && e["state"] == "running" && level_of(e) == Level::Red && st.episodes.warn_main(id, Level::Red) {
            let kb = e["memory"]["max_rss_kb"].as_u64().unwrap_or(0);
            return Some((main.to_string(), lico_memguard::warning_text(id, e["name"].as_str().unwrap_or(""), kb / 1024, cfg.limits.limit_mb)));
        }
    }
    if let (Level::Red, Some((avail, total))) = (machine_level, machine) {
        if st.episodes.warn_main("\u{0}machine", Level::Red) {
            let running = entries.iter().filter(|e| e["state"] == "running").count() as u32;
            return Some((main.to_string(), lico_memguard::machine_warning_text(running, (avail.saturating_mul(100) / total.max(1)) as u32, avail / 1024)));
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

/// The running sessions to close and resume now: red by the largest process, auto-close on for them, idle (no turn, no approval, the person not typing), and not stopped by the loop guard.
fn plan(entries: &[Value], cfg: &Config, st: &mut State, now: u64) -> Vec<(String, i64)> {
    if !cfg.auto_close {
        return Vec::new();
    }
    let mut out = Vec::new();
    for e in entries {
        let (Some(id), Some(generation)) = (e["session"].as_str(), e["generation"].as_i64()) else { continue };
        if e["state"] != "running" || cfg.auto_close_off.iter().any(|o| o == id) || cfg.limits.level(e["memory"]["max_rss_kb"].as_u64().unwrap_or(0)) != Level::Red {
            continue;
        }
        let typing = st.typed.get(id).is_some_and(|t| now.saturating_sub(*t) < TYPING_QUIET_MS);
        let idle = Idle { turn_running: e["activity"] != "idle", approval_pending: e["approvals"].as_array().is_some_and(|a| !a.is_empty()), person_typing: typing };
        if idle.ok() && st.loops.allowed(id, now) {
            out.push((id.to_string(), generation));
        }
    }
    out
}

/// The warning level and the limit the host uses (the owner's `memory.json`, else 400 and 500 MB): sent in every `status` reply so that the window colours by the same numbers.
pub fn limits(state_dir: &Path) -> Limits {
    read_config(state_dir).limits
}

/// The stall time in minutes the host uses (the owner's `stall_min`, else 10).
pub fn stall_min(state_dir: &Path) -> u64 {
    read_config(state_dir).stall_min
}

/// What to do now after a `status` reply (computed here, done by the hubs: this module names no hub, so it stays out of the cycle of the hubs).
pub struct Step {
    pub now: u64,
    /// The running sessions to close with `by: guard`: (id, generation, keep the worktree).
    pub close: Vec<(String, i64, bool)>,
    /// A guard-closed session whose turn has come to be resumed (one at a time, the gap since the last resume has passed).
    pub resume: Option<String>,
    /// A warning for the main agent: (the main agent's session, the text).
    pub warn: Option<(String, String)>,
}

pub fn step(state_dir: &Path, entries: &[Value]) -> Step {
    let cfg = read_config(state_dir);
    let now = now_ms();
    let machine = meminfo();
    let mut g = STATE.lock().unwrap();
    let st = g.get_or_insert_with(State::default);
    let resume = st.queue.get_or_insert_with(|| ResumeQueue::new(RESUME_GAP_MS)).next(now);
    let close = plan(entries, &cfg, st, now).into_iter().map(|(id, generation)| {
        let keep = entries.iter().find(|e| e["session"] == id.as_str()).is_some_and(|e| !e["worktree"].is_null());
        (id, generation, keep)
    });
    let close: Vec<(String, i64, bool)> = close.collect();
    let warn = plan_warning(entries, &cfg, st, now, machine);
    Step { now, close, resume, warn }
}

/// The hub closed `id` for the guard: remember who, count it for the loop guard and queue it for its resume.
pub fn closed_ok(id: &str, now: u64) {
    record(id, Some("guard"));
    let mut g = STATE.lock().unwrap();
    let st = g.get_or_insert_with(State::default);
    st.loops.closed(id, now);
    st.queue.get_or_insert_with(|| ResumeQueue::new(RESUME_GAP_MS)).push(id, ClosedBy::Guard);
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
        let (id, text) = plan_warning(&all, &cfg, &mut st, t, None).expect("a warning");
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
        let (_, text) = plan_warning(&quiet, &cfg, &mut st, t, Some((total / 20, total))).expect("the machine is red");
        assert!(text.contains("1 sessions") && text.contains("5 percent"), "{text}");
        assert!(plan_warning(&quiet, &cfg, &mut st, t, Some((total / 20, total))).is_none(), "once for the episode");
    }

    #[test]
    fn the_stall_time_is_ten_minutes_unless_the_file_says_otherwise_and_is_capped_at_a_day() {
        assert_eq!(Config::default().stall_min, 10);
        assert_eq!(parse_config(r#"{"stall_min": 3}"#).stall_min, 3);
        assert_eq!(parse_config(r#"{"stall_min": 0}"#).stall_min, 0, "0 switches it off");
        assert_eq!(parse_config(r#"{"stall_min": 99999}"#).stall_min, 24 * 60);
        assert_eq!(parse_config(r#"{"stall_min": "x"}"#).stall_min, 10);
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
        assert_eq!(ids(plan(&all, &cfg, &mut st, t)), ["red_idle"]);
        assert!(plan(&all, &Config::default(), &mut st, t).is_empty(), "off by default");
        st.typed.insert("typed".into(), t - 31_000);
        assert!(ids(plan(&all, &cfg, &mut st, t)).contains(&"typed".to_string()), "typing ended 31 s ago");
        st.loops.closed("red_idle", t - 1);
        st.loops.closed("red_idle", t - 2);
        assert!(!ids(plan(&all, &cfg, &mut st, t)).contains(&"red_idle".to_string()), "twice in ten minutes: warn only");
    }
}
