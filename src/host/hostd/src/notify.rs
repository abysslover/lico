//! Desktop notifications for background sessions (M02-P06, S02; NT-02, SF-02, SF-03).
//!
//! Uses `notify-send` only when it exists on `PATH` (or `LICO_NOTIFY_BIN` in tests) with zero new dependency.
//! Controlled by `notifications` in `settings.json` (default `true`).
//! Carries only the sanitized session label (`'name' finished` or `'name' needs approval`) — never transcript lines, approval prompt commands, or credentials.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::clipboard::settings::{notifications_enabled, settings_path};
use crate::core::Shared;

const LABEL_CHARS: usize = 40;
const NOTIFY_WAIT: Duration = Duration::from_secs(3);

/// True when any connected `lico gui` window is currently viewing `session_id`.
pub fn has_gui_view(shared: &Shared, session_id: &str) -> bool {
    shared.views.lock().unwrap().values().any(|v| v.gui_seq.is_some() && v.session == session_id)
}

/// Called after dropping the session lock when a running session's turn finishes.
pub fn on_turn_finished(shared: &Shared, session_id: &str, name: Option<&str>, cwd: Option<&str>) {
    if !has_gui_view(shared, session_id) {
        fire(&shared.cfg.handoff_dir, session_id, name, cwd, "finished");
    }
}

/// Called after dropping the session lock when a session adds a pending approval.
pub fn on_approval_needed(shared: &Shared, session_id: &str, name: Option<&str>, cwd: Option<&str>) {
    if !has_gui_view(shared, session_id) {
        fire(&shared.cfg.handoff_dir, session_id, name, cwd, "needs approval");
    }
}

pub fn format_body(session_id: &str, name: Option<&str>, cwd: Option<&str>, event: &str) -> String {
    let raw = crate::transfer::label(session_id, name, cwd);
    let label: String = raw
        .chars()
        .filter(|c| !crate::codex::dropped(*c as u32))
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(LABEL_CHARS)
        .collect();
    format!("'{label}' {event}")
}

fn notify_bin() -> OsString {
    std::env::var_os("LICO_NOTIFY_BIN").filter(|v| !v.is_empty()).unwrap_or_else(|| OsString::from("notify-send"))
}

pub fn fire(handoff_dir: &Path, session_id: &str, name: Option<&str>, cwd: Option<&str>, event: &'static str) {
    if !notifications_enabled(&settings_path(handoff_dir)) {
        return;
    }
    let body = format_body(session_id, name, cwd, event);
    let bin = notify_bin();
    std::thread::spawn(move || {
        run_once(&bin, &body);
    });
}

fn run_once(bin: &OsString, body: &str) {
    let mut cmd = Command::new(bin);
    cmd.args(["--app-name=Lico", "Lico", body]).env_clear().stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    for key in ["PATH", "LANG", "DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"] {
        if let Some(val) = std::env::var_os(key) {
            cmd.env(key, val);
        }
    }
    let Ok(mut child) = cmd.spawn() else { return }; // `notify-send` is optional: missing binary is silently ignored
    let deadline = Instant::now() + NOTIFY_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn notification_body_uses_only_sanitized_label_and_missing_or_disabled_helper_is_clean() {
        let body = format_body("s1234567890", Some("api\nsecret"), Some("/work/api"), "finished");
        assert_eq!(body, "'api secret' finished");
        assert!(!body.contains("s1234567890"));

        // Missing helper binary: returns cleanly without panicking or leaving children.
        run_once(&OsString::from("/nonexistent/lico_no_such_notify_send"), "'api' finished");

        // Enabled vs disabled in settings.json with a tiny shell double.
        let dir = std::env::temp_dir().join(format!("lico_notify_{}", crate::core::random_hex(6)));
        let handoff = dir.join("handoff");
        std::fs::create_dir_all(&handoff).unwrap();
        let out = dir.join("calls.txt");
        let bin = dir.join("mock_notify.sh");
        std::fs::write(&bin, format!("#!/bin/sh\nprintf '%s|%s|%s\\n' \"$1\" \"$2\" \"$3\" >> '{}'\n", out.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(notifications_enabled(&settings_path(&handoff)), "notifications default to on");
        run_once(&OsString::from(&bin), "'docs' needs approval");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "--app-name=Lico|Lico|'docs' needs approval\n");

        crate::clipboard::settings::set_settings(&handoff, None, Some(false)).unwrap();
        assert!(!notifications_enabled(&settings_path(&handoff)));
        fire(&handoff, "s1111111", Some("docs"), None, "finished");
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(std::fs::read_to_string(&out).unwrap().lines().count(), 1, "nothing fired while disabled");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
