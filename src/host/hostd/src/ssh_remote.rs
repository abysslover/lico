//! Agentless SSH remote execution helpers for isolated remote account sign-in (`M06-P02`)
//! and remote directory / session operations (`M06-P03`).
//!
//! Remote account authentication state lives exclusively on the remote host under
//! `$HOME/.local/state/lico/accounts/<account_id>/{home,<provider>}` created with `umask 077` (`0700`).
//! Real remote `$HOME` binary directories (`~/.local/bin`, `~/.npm-global/bin`, `~/.cargo/bin`, `~/bin`)
//! are added to `PATH` before `HOME` is switched to the isolated account home so user-installed
//! `claude` and `codex` binaries resolve cleanly while keeping credentials isolated per account.

use std::process::Command;

use crate::accounts::provider_variable;
use crate::core::Shared;
use crate::ssh::ssh_base_command;

/// Escape an arbitrary string as a single POSIX shell token (`'...'`).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Validates that an account identifier contains only safe ASCII identifier characters.
pub fn valid_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// Restricts the local `ssh` client's environment to what OpenSSH needs (`HOME`, `PATH`, `LANG`,
/// `USER`, `LOGNAME`, `SSH_AUTH_SOCK`, `KRB5CCNAME`) so `DISPLAY` and unrelated local variables
/// never leak into remote execution.
pub fn sanitize_ssh_env(cmd: &mut Command) {
    cmd.env_clear();
    for key in ["HOME", "PATH", "LANG", "USER", "LOGNAME", "SSH_AUTH_SOCK", "KRB5CCNAME"] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
}

/// Builds the non-interactive `ssh` command that runs `claude auth login` or `codex login [--device-auth]`
/// (or `codex login status`) on `host` inside the remote machine's `0700` isolated account home.
pub fn remote_login_command(
    shared: &Shared,
    host: &str,
    account_id: &str,
    provider: &str,
    words: &[&str],
) -> Result<Command, &'static str> {
    if !valid_account_id(account_id) {
        return Err("bad_request");
    }
    let var = provider_variable(provider).ok_or("bad_request")?;
    let mut cmd = ssh_base_command(shared, host, 8)?;
    sanitize_ssh_env(&mut cmd);

    let mut argv = Vec::new();
    match provider {
        "claude" => {
            argv.push("claude".to_string());
            argv.push("auth".to_string());
        }
        "codex" => {
            argv.push("codex".to_string());
        }
        _ => return Err("bad_request"),
    }
    for w in words {
        argv.push(shell_quote(w));
    }
    let exec_line = argv.join(" ");
    let acct_q = shell_quote(account_id);
    let prov_q = shell_quote(provider);
    let inner = format!(
        "umask 077 && LICO_RH=\"$HOME\" && \
         export PATH=\"$LICO_RH/.local/bin:$LICO_RH/.npm-global/bin:$LICO_RH/.cargo/bin:$LICO_RH/bin:/usr/local/bin:$PATH\" && \
         LICO_AC=\"$LICO_RH/.local/state/lico/accounts/\"{} && \
         mkdir -p \"$LICO_AC/home\" \"$LICO_AC/\"{} && \
         cd \"$LICO_AC/home\" && \
         HOME=\"$LICO_AC/home\" {}=\"$LICO_AC/\"{} NO_COLOR=1 BROWSER=/usr/bin/true exec {}",
        acct_q, prov_q, var, prov_q, exec_line
    );
    let remote_cmd = format!("sh -c {}", shell_quote(&inner));
    cmd.arg(remote_cmd);
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_handles_single_quotes_and_spaces_safely() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("a b'c"), "'a b'\\''c'");
        assert!(valid_account_id("a012345abcdef"));
        assert!(!valid_account_id(""));
        assert!(!valid_account_id("../bad"));
        assert!(!valid_account_id("a;rm"));
    }
}
