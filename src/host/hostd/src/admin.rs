//! Administrator account, session, and capability control operations (`M03-P01`, `M03-P04`):
//! - `account.rename`: rename an existing account's label atomically without touching credentials or homes.
//! - `session.mode`: switch a provider session's permission mode (`"auto"` <-> `"default"`) safely with
//!   generation checking. Codex applies `s.mode` per turn immediately; an idle running Claude session
//!   restarts cleanly with `--permission-mode` (and `--resume` when a conversation id exists) while busy
//!   or pending-approval turns refuse with `"busy"`.
//! - `capabilities`: canonical machine-readable inventory of protocol version, build identity, operations,
//!   socket-only operations, loopback HTTP routes, MCP agent tools, and host/window limits.

use std::sync::Arc;

use serde_json::json;

use crate::core::{check_generation, find, Reply, Shared};
use crate::session::{obj, State};

/// Operations bound to a stateful Unix-socket subscriber or local file attachment slot; `POST /v1` over HTTP refuses them with `bad_request`.
pub const SOCKET_ONLY_OPS: &[&str] = &[
    "subscribe",
    "unsubscribe",
    "view.attach",
    "clipboard.write",
    "clipboard.done",
    "picture.slot",
];

/// Loopback HTTP routes served by `src/host/hostd/src/http.rs`.
pub const HTTP_ROUTES: &[&str] = &[
    "POST /v1",
    "GET /v1/capabilities",
    "GET /v1/status",
    "GET /v1/resources",
    "GET /v1/settings",
    "POST /v1/settings",
    "GET /v1/accounts",
    "POST /v1/accounts",
    "POST /v1/accounts/<id>/rename",
    "GET /v1/sessions",
    "POST /v1/sessions",
    "GET /v1/sessions/<id>",
    "GET /v1/sessions/<id>/snapshot",
    "GET /v1/sessions/<id>/events",
    "GET /v1/sessions/<id>/history",
    "POST /v1/sessions/<id>/input",
    "POST /v1/sessions/<id>/mode",
    "POST /v1/sessions/<id>/rebind",
    "GET /v1/search",
    "GET /v1/files",
    "GET /v1/files/read",
    "GET /v1/ssh/hosts",
    "POST /v1/ssh/check",
    "POST /v1/approvals/<id>/response",
    "POST /v1/views",
    "DELETE /v1/views/<view>",
];

pub fn capabilities(shared: &Shared) -> Reply {
    let mcp_tools: Vec<&'static str> = crate::mcp::TOOLS.iter().map(|t| t.name).collect();
    Ok(obj(json!({
        "protocol": 1,
        "version": env!("CARGO_PKG_VERSION"),
        "commit": env!("LICO_BUILT_COMMIT"),
        "build": crate::core::build_id(),
        "ops": crate::core::OPS,
        "socket_only_ops": SOCKET_ONLY_OPS,
        "http_routes": HTTP_ROUTES,
        "mcp_tools": mcp_tools,
        "limits": {
            "max_request": crate::core::MAX_REQUEST,
            "max_input_text": crate::ipc::MAX_INPUT_TEXT,
            "max_sessions": shared.cfg.max_sessions,
            "retain": shared.cfg.retain,
            "font_px_min": crate::clipboard::settings::MIN_FONT_PX,
            "font_px_max": crate::clipboard::settings::MAX_FONT_PX,
        },
    })))
}

pub fn account_rename(shared: &Shared, account: &str, label: &str) -> Reply {
    if !crate::ipc::valid_label(&json!(label)) {
        return Err("bad_request");
    }
    let updated = shared.accounts.lock().unwrap().rename(account, label)?;
    Ok(obj(json!({"account": updated.id, "provider": updated.provider, "label": updated.label})))
}

pub fn session_mode(shared: &Arc<Shared>, session: &str, generation: i64, mode: &str) -> Reply {
    let next_mode: &'static str = match mode {
        "auto" => "auto",
        "default" => "default",
        _ => return Err("bad_request"),
    };
    let found = find(shared, session).ok_or("unknown_session")?;
    let (id, provider, was_running, prev_mode, changed) = {
        let mut s = found.lock().unwrap();
        check_generation(&s, Some(generation))?;
        if s.provider.is_empty() {
            return Err("bad_request");
        }
        if s.state == State::Exited {
            return Err("session_exited");
        }
        if s.state == State::Running && (s.busy || !s.approvals.is_empty()) {
            return Err("busy");
        }
        if s.mode == next_mode {
            return Ok(obj(json!({"session": s.id, "generation": s.generation, "mode": s.mode})));
        }
        let prev_mode = s.mode;
        let was_running = s.state == State::Running;
        if s.provider == "claude" && was_running {
            s.stop_process();
            crate::tools::forget(&s.id);
            s.set_state(State::Disconnected);
            s.set_busy(false);
            s.input_tx = None;
            s.claude = None;
            s.drop_program_approvals();
            s.mode = next_mode;
            s.generation += 1;
        } else {
            s.mode = next_mode;
            let note = format!("[permission mode: {next_mode}]");
            if s.provider == "claude" {
                crate::claude::note(&mut s, &note);
            } else {
                crate::codex::note(&mut s, &note);
            }
        }
        (s.id.clone(), s.provider, was_running, prev_mode, true)
    };
    if changed && provider == "claude" && was_running {
        if let Err(code) = crate::claude::restart_after_change(shared, &found) {
            let mut s = found.lock().unwrap();
            s.generation -= 1;
            s.mode = prev_mode;
            s.set_state(State::Disconnected);
            return Err(code);
        }
        let mut s = found.lock().unwrap();
        crate::claude::note(&mut s, &format!("[permission mode: {next_mode}]"));
    }
    crate::registry::save(shared);
    let out_gen = found.lock().unwrap().generation;
    Ok(obj(json!({"session": id, "generation": out_gen, "mode": next_mode})))
}

#[cfg(test)]
mod tests {
    use crate::accounts::Accounts;
    use crate::handoff::{mode_of, Scratch};

    #[test]
    fn renaming_an_account_preserves_id_provider_default_and_private_permissions() {
        let scratch = Scratch::new("account_rename");
        let dir = scratch.0.join("accounts");
        let mut accounts = Accounts::load(&dir).unwrap();
        let created = accounts.create("claude", "Initial").unwrap();
        let renamed = accounts.rename(&created.id, "Team Admin").unwrap();
        assert_eq!(renamed.id, created.id);
        assert_eq!(renamed.provider, "claude");
        assert_eq!(renamed.label, "Team Admin");
        assert_eq!(mode_of(&dir.join("accounts.json")), 0o600);

        let reloaded = Accounts::load(&dir).unwrap();
        assert_eq!(reloaded.get(&created.id).unwrap().label, "Team Admin");
        assert_eq!(reloaded.default_for("claude").unwrap().id, created.id);
        assert_eq!(accounts.rename("a000000", "Ghost").err(), Some("unknown_account"));
    }

    #[test]
    fn socket_only_ops_are_a_strict_subset_of_protocol_ops() {
        for op in super::SOCKET_ONLY_OPS {
            assert!(crate::core::OPS.contains(op), "missing in core::OPS: {op}");
        }
        assert!(crate::core::OPS.contains(&"capabilities"));
        assert!(!super::SOCKET_ONLY_OPS.contains(&"capabilities"));
        assert!(super::HTTP_ROUTES.contains(&"POST /v1"));
        assert!(super::HTTP_ROUTES.contains(&"GET /v1/capabilities"));
    }
}
