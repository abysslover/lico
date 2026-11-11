//! Time-bounded real-screen control grants (`CU-05`, `CU-06`, `SF-01`, `SF-05`, `SF-06`).
//!
//! Real-screen control is OFF by default and never persistent across restarts:
//! - Every grant expires automatically after `DEFAULT_SECS` (`900 s` = 15 minutes; overridable in
//!   tests via `LICO_REAL_SCREEN_S`).
//! - `stop_op` (`lico real-screen stop`, `Ctrl+Alt+Esc`, or the crimson `[ Stop now ]` button in
//!   `lico gui`) revokes real-screen control immediately.
//! - Closing or purging a session, or stopping the host, revokes its grant as well.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use crate::core::{self, Reply, Shared};
use crate::session::obj;
use crate::transfer;

pub const DEFAULT_SECS: u64 = 15 * 60;

#[derive(Clone, Debug)]
struct Grant {
    until: Instant,
}

fn table() -> MutexGuard<'static, BTreeMap<String, Grant>> {
    static TABLE: OnceLock<Mutex<BTreeMap<String, Grant>>> = OnceLock::new();
    TABLE.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner())
}

pub fn window_duration() -> Duration {
    let secs = std::env::var("LICO_REAL_SCREEN_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| s >= 1)
        .unwrap_or(DEFAULT_SECS);
    Duration::from_secs(secs)
}

/// Grant `session` time-bounded real-screen control starting at `now`.
pub fn grant_at(session: &str, now: Instant, dur: Duration) -> u64 {
    table().insert(session.to_string(), Grant { until: now + dur });
    dur.as_secs()
}

/// Remaining whole seconds (>= 1) of `session`'s real-screen grant at `now`, removing expired entries.
pub fn seconds_left_at(session: &str, now: Instant) -> Option<u64> {
    let mut map = table();
    map.retain(|_, g| g.until > now);
    map.get(session).map(|g| g.until.saturating_duration_since(now).as_secs().max(1))
}

/// Revoke `session`'s grant; true if one was active.
pub fn revoke(session: &str) -> bool {
    table().remove(session).is_some()
}

pub fn stop_all() {
    table().clear();
}

/// Active real-screen grants for `status["real_screen"]`, with the human-readable session label and countdown.
pub fn status_json(shared: &Shared) -> Value {
    let now = Instant::now();
    let active: Vec<(String, u64)> = {
        let mut map = table();
        map.retain(|_, g| g.until > now);
        map.iter().map(|(sid, g)| (sid.clone(), g.until.saturating_duration_since(now).as_secs().max(1))).collect()
    };
    let mut out = Vec::new();
    for (sid, secs) in active {
        let Some(found) = core::find(shared, &sid) else {
            revoke(&sid);
            continue;
        };
        let s = found.lock().unwrap();
        if s.purged || s.state != crate::session::State::Running {
            drop(s);
            revoke(&sid);
            continue;
        }
        let name: String = transfer::label(&s.id, s.settings.name.as_deref(), s.settings.cwd.as_deref())
            .chars()
            .take(crate::folders::MAX_NAME)
            .collect();
        out.push(json!({"session": sid, "name": name, "seconds_left": secs}));
    }
    Value::Array(out)
}

/// Socket op `real_screen.stop`: revokes one session's grant (when `session` is given) or all active grants.
pub fn stop_op(shared: &Shared, session: Option<&str>) -> Reply {
    let now = Instant::now();
    let stopped = match session.filter(|s| !s.is_empty()) {
        Some(sid) => {
            let found = core::find(shared, sid).ok_or("unknown_session")?;
            let active = seconds_left_at(sid, now).is_some() && revoke(sid);
            if active {
                let mut s = found.lock().unwrap();
                if s.provider == "claude" {
                    crate::claude::note(&mut s, "[lico: real-screen control stopped by the person]");
                }
            }
            usize::from(active)
        }
        None => {
            let ids: Vec<String> = {
                let mut map = table();
                map.retain(|_, g| g.until > now);
                let keys: Vec<String> = map.keys().cloned().collect();
                map.clear();
                keys
            };
            for sid in &ids {
                if let Some(found) = core::find(shared, sid) {
                    let mut s = found.lock().unwrap();
                    if s.provider == "claude" {
                        crate::claude::note(&mut s, "[lico: real-screen control stopped by the person]");
                    }
                }
            }
            ids.len()
        }
    };
    Ok(obj(json!({"stopped": stopped})))
}

/// `lico real-screen stop [--session ID]`
pub fn cli(args: &[String], socket: &str) -> ! {
    let die = |code: i32, msg: &str| -> ! {
        eprintln!("{msg}");
        std::process::exit(code);
    };
    if socket.is_empty() {
        die(1, "lico real-screen needs a host socket (--socket PATH or $LICO_SOCKET)");
    }
    match args {
        [sub] if sub == "stop" => {
            crate::cli::expect_ok_pub(crate::cli::call_once(socket, json!({"op": "real_screen.stop"})));
            std::process::exit(0);
        }
        [sub, flag, sid] if sub == "stop" && flag == "--session" => {
            crate::cli::expect_ok_pub(crate::cli::call_once(socket, json!({"op": "real_screen.stop", "session": sid})));
            std::process::exit(0);
        }
        _ => die(1, "usage: lico real-screen stop [--session ID]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_counts_down_expires_cleanly_and_revokes_at_once() {
        let base = Instant::now();
        let sid = format!("rs-test-{}", core::random_hex(4));
        assert_eq!(seconds_left_at(&sid, base), None);
        assert_eq!(grant_at(&sid, base, Duration::from_secs(900)), 900);
        assert_eq!(seconds_left_at(&sid, base + Duration::from_secs(120)), Some(780));
        assert_eq!(seconds_left_at(&sid, base + Duration::from_secs(900)), None);
        grant_at(&sid, base, Duration::from_secs(60));
        assert!(revoke(&sid) && !revoke(&sid));
        assert_eq!(seconds_left_at(&sid, base + Duration::from_secs(1)), None);
    }
}
