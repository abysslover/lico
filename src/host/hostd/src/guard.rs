//! The memory guard's host side (M19-P01): who closed each session. A side table kept while the host runs (a session closed by the person is never revived by the guard; only a
//! session the guard closed is). Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lico_memguard::{ClosedBy, Idle, Level, Limits, LoopGuard, ResumeQueue};
use serde_json::Value;

use crate::core::Shared;

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
}

impl Default for Config {
    fn default() -> Config {
        Config { limits: Limits::default(), auto_close: false, auto_close_off: Vec::new() }
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
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The person sent input to a session (the idle test must not close it while they are typing).
pub fn note_input(session: &str) {
    STATE.lock().unwrap().get_or_insert_with(State::default).typed.insert(session.to_string(), now_ms());
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

/// Called after each `status` reply (no timer of its own: the guard acts when something looks): closes the red idle sessions that qualify, and resumes one guard-closed session when the gap
/// since the last resume has passed.
pub fn tick(shared: &Arc<Shared>, reply: &serde_json::Map<String, Value>) {
    let Some(entries) = reply.get("sessions").and_then(Value::as_array) else { return };
    let state_dir = shared.cfg.registry_dir.parent().map(Path::to_path_buf).unwrap_or_default();
    let cfg = read_config(&state_dir);
    let now = now_ms();
    let (to_close, resume) = {
        let mut g = STATE.lock().unwrap();
        let st = g.get_or_insert_with(State::default);
        let q = st.queue.get_or_insert_with(|| ResumeQueue::new(RESUME_GAP_MS));
        let resume = q.next(now);
        (plan(entries, &cfg, st, now), resume)
    };
    for (id, generation) in to_close {
        let worktree = entries.iter().find(|e| e["session"] == id.as_str()).and_then(|e| (!e["worktree"].is_null()).then_some("keep"));
        if crate::lifecycle::session_close_opts(shared, &id, generation, worktree, false, false).is_ok() {
            record(&id, Some("guard"));
            let mut g = STATE.lock().unwrap();
            let st = g.get_or_insert_with(State::default);
            st.loops.closed(&id, now);
            st.queue.get_or_insert_with(|| ResumeQueue::new(RESUME_GAP_MS)).push(&id, ClosedBy::Guard);
        }
    }
    if let Some(id) = resume {
        // only a session the guard closed is queued; if the person took it up meanwhile it runs already and the host refuses
        if closed_by(&id) == Some("guard") {
            if let Some(found) = crate::core::find(shared, &id) {
                let generation = found.lock().unwrap().generation as i64;
                let _ = crate::core::session_resume(shared, &id, generation);
            }
            clear(&id);
        }
    }
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
