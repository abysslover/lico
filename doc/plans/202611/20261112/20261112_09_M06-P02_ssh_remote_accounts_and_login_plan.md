# M06-P02 Plan — SSH remote provider accounts (`[ssh: <host>]`) and supervised remote `Sign in` over SSH

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)](../../../milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md)
- Branch: `phase/M06-P02-ssh-remote-accounts-and-login`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

When engineering work executes on an SSH-reachable machine, the CLI coding agent
(`claude` or `codex`) runs on that remote machine and authenticates there
directly against its LLM API provider. Administrators therefore need first-class
SSH remote accounts in `lico` that clearly display **`[ssh: <host>]`** across
`Accounts`, sidebar session-group headings, `New session`, `Account:` actions,
and `Rebind`, allow selecting `Host: local` or any discovered `~/.ssh/config`
host when adding an account, and supervise `claude auth login` / `codex login`
over SSH inside an isolated `0700` account home on the remote host without
copying tokens or logging verification codes.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M06-P02` plan; update `M06` milestone and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Extend `Account` and `accounts.json` (`src/host/hostd/src/accounts.rs`) with optional `ssh_host: Option<String>` (validated via `ssh::valid_ssh_host`); expose `ssh_host` across `account.create`, `account.rename`, `account.list`, `ipc.rs`, `http.rs` (`POST /v1/accounts`), `cli.rs` (`lico account create --provider ... --label ... [--ssh-host HOST]`), and `src/lico_protocol/v1.py` | Rust unit tests, `test_accounts.py`, `test_account_lifecycle.py`, and `test_protocol_conformance.py` pass |
| S03 | Add `src/host/hostd/src/ssh_remote.rs` and extend `src/host/hostd/src/login.rs` (`start` and `signed_in`) so accounts with `ssh_host: Some(host)` run `claude auth login` / `codex login [--device-auth]` / `codex login status` over `BatchMode=yes` `ssh` inside remote `0700` `$HOME/.local/state/lico/accounts/<id>/{home,<provider>}`, streaming the printed sign-in URL to `lico gui` and forwarding pasted verification codes over SSH `stdin` without logging secrets | Rust unit tests and `test_login.py` pass |
| S04 | Update GUI (`gui/ui.rs`, `gui/attn.rs`, `gui/panel.rs`, `gui/admin_ui.rs`, `gui/newsession.rs`) to render `[ssh: <host>]` badges everywhere accounts appear, add `A  add a new account` inside `Accounts`, and allow cycling `Provider` (`Tab`) and `Host: < local | ssh: ... >` (`Up`/`Down` or click) when creating an account; add `tests/test_ssh_accounts.py`, capture manual inspection bundle, write `M06-P02` report, and commit cleanly | `test_ssh_accounts.py`, `test_login_window.py`, `test_size_ratchet.py`, and `test_selection.py` pass |

## Personal data and security

1. Remote account authentication files stay exclusively inside `0700`
   `$HOME/.local/state/lico/accounts/<id>/{home,<provider>}` on the remote host
   (`umask 077`); `lico` never reads, copies, or stores remote provider tokens.
2. Verification codes entered during remote `Sign in` are masked as `*` in the
   GUI, written directly to the SSH child process `stdin`, cleared immediately,
   and never written to logs, registries, `status`, or `ui` dumps.
