//! The discovery on an ssh host (`M12-P02`): the script of `remote_script.rs` run through the bounded remote runner (`ssh_remote::run_ssh_bounded`: batch mode, the process
//! group killed at the deadline, the output capped), with its own time budget (the 4 s default of a remote operation is too short for many ports). Every way it can fail is
//! a word, never a hang: the host cannot be reached, it has no `python3`, it outlasts the budget, it answers with more text than the cap, it is not Linux or its process table
//! cannot be read (an answer with `supported: false`, which is not a failure).

use std::time::{Duration, Instant};

use serde_json::Value;

use crate::core::{Reply, Shared};

/// The time the whole ssh call may take; the script keeps `INNER_SECS` of it for itself. `LICO_LLM_SSH_SECS` shortens it (for the tests only).
fn call_secs() -> u64 {
    static SECS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *SECS.get_or_init(|| std::env::var("LICO_LLM_SSH_SECS").ok().and_then(|v| v.parse().ok()).filter(|n| (1..=120).contains(n)).unwrap_or(20))
}

const OUTPUT_CAP: usize = 128 * 1024;

/// The command run on the remote machine: python3 must be there, and the script gets its arguments.
fn command(root: &str, all_users: bool, inner_secs: u64) -> String {
    let q = crate::ssh_remote::shell_quote;
    format!(
        "command -v python3 >/dev/null 2>&1 || {{ echo LICO_ERR=no_python3; exit 0; }}; python3 -c {} {} {} {}",
        q(super::remote_script::SCRIPT), q(root), if all_users { "'1'" } else { "'0'" }, q(&inner_secs.to_string())
    )
}

/// The reply of the script as the host gives it: the line `LICO_JSON=` of its output, with the host's name put in.
fn parse(stdout: &str, host: &str) -> Reply {
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(code) = line.strip_prefix("LICO_ERR=") {
            return Err(if code.trim() == "no_python3" { "remote_python_missing" } else { "remote_failed" });
        }
        if let Some(raw) = line.strip_prefix("LICO_JSON=") {
            return match serde_json::from_str::<Value>(raw) {
                Ok(Value::Object(mut reply)) if reply.get("endpoints").is_some_and(Value::is_array) && reply.get("supported").is_some_and(Value::is_boolean) => {
                    reply.insert("host".into(), Value::String(host.to_string()));
                    Ok(reply)
                }
                _ => Err("remote_failed"),
            };
        }
    }
    Err("remote_failed")
}

/// `llm.discover` for the ssh host `host`.
pub fn discover(shared: &Shared, host: &str, all_users: bool) -> Reply {
    if !crate::ssh::valid_ssh_host(host) {
        return Err("bad_request");
    }
    // the process table of the remote machine: `/proc`, or, for a test, a folder of its own for this host under the folder the host was given
    let root = match super::set_root() {
        Some(base) => base.join(host).to_string_lossy().into_owned(),
        None => "/proc".to_string(),
    };
    let secs = call_secs();
    let inner = secs.saturating_sub(4).max(1);
    let started = Instant::now();
    let (ok, stdout) = match crate::ssh_remote::run_ssh_bounded(shared, host, &command(&root, all_users, inner), OUTPUT_CAP, secs) {
        Ok(done) => done,
        // the runner kills the call at the deadline and reports it like an unreachable host: told apart by the time it took
        Err(_) if started.elapsed() >= Duration::from_secs(secs) => return Err("remote_timeout"),
        Err(code) => return Err(code),
    };
    if stdout.len() >= OUTPUT_CAP {
        return Err("remote_output_too_large");
    }
    if !ok && !stdout.contains("LICO_") {
        return Err("remote_failed");
    }
    parse(&stdout, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_checks_for_python_and_passes_the_arguments_quoted() {
        let c = command("/proc", true, 16);
        assert!(c.starts_with("command -v python3 >/dev/null 2>&1 || { echo LICO_ERR=no_python3; exit 0; }; python3 -c '"), "{}", &c[..80]);
        assert!(c.ends_with(" '/proc' '1' '16'"), "{}", &c[c.len() - 30..]);
    }

    #[test]
    fn the_output_is_read_by_its_marker_and_the_host_is_put_in() {
        let ok = parse("noise\nLICO_JSON={\"supported\":true,\"endpoints\":[],\"host\":null}\n", "gpu-box").unwrap();
        assert_eq!(ok["host"], "gpu-box");
        assert_eq!(parse("LICO_ERR=no_python3\n", "h").unwrap_err(), "remote_python_missing");
        assert_eq!(parse("LICO_ERR=other\n", "h").unwrap_err(), "remote_failed");
        assert_eq!(parse("", "h").unwrap_err(), "remote_failed");
        assert_eq!(parse("LICO_JSON={\"supported\":true}\n", "h").unwrap_err(), "remote_failed", "no endpoints: not an answer");
        assert_eq!(parse("LICO_JSON=not json\n", "h").unwrap_err(), "remote_failed");
    }

    #[test]
    fn the_script_says_it_does_not_know_a_machine_that_is_not_linux() {
        let wrapped = format!("import sys; sys.platform = 'freebsd14'\n{}", super::super::remote_script::SCRIPT);
        let out = std::process::Command::new("python3").args(["-c", &wrapped, "/proc", "0", "5"]).output().expect("python3");
        let text = String::from_utf8_lossy(&out.stdout);
        let reply = parse(&text, "bsd").unwrap();
        assert_eq!((reply["supported"].as_bool(), reply["reason"].as_str(), reply["os"].as_str()), (Some(false), Some("os_unknown"), Some("freebsd14")), "{text}");
    }
}
