//! The memory guard's host side (M19-P01): who closed each session. A side table kept while the host runs (a session closed by the person is never revived by the guard; only a
//! session the guard closed is). Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

use std::collections::BTreeMap;
use std::sync::Mutex;

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
}
