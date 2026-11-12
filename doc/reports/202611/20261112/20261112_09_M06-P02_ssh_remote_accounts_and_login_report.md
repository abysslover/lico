# M06-P02 Report — SSH remote provider accounts (`[ssh: <host>]`) and supervised remote `Sign in` over SSH

- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)](../../../milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md)
- Plan: [M06-P02 plan](../../../plans/202611/20261112/20261112_09_M06-P02_ssh_remote_accounts_and_login_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M06-P02-ssh-remote-accounts-and-login`
- Inspection bundle: `202611/20261112/00011_M06-P02_ssh_remote_accounts_and_login/`

## Summary

`M06-P02` introduces SSH remote provider accounts (`ssh_host: Option<String>`)
across IPC, CLI, loopback HTTP, and `lico gui`. Accounts bound to a registered
SSH host display **`[ssh: <host>]`** badges across `Accounts`, sidebar
session-group headings, session headers, `New session`, `Account:` actions,
`Sign in:`, and `Rebind`, offer in-window `Provider` (`Tab` / click) and
`Target: < local | ssh: ... >` (`Up`/`Down` / click) selection when creating an
account, and run supervised `claude auth login` / `codex login [--device-auth]`
/ `codex login status` over `BatchMode=yes` SSH inside isolated `0700`
`$HOME/.local/state/lico/accounts/<id>/{home,<provider>}` directories on the
remote host so remote machines authenticate directly against their LLM API
provider without copying tokens or logging verification codes.

## Delivered changes

1. **S02 — Account `ssh_host` model and control-surface parity (`src/host/hostd/src/accounts.rs`, `admin.rs`, `core.rs`, `ipc.rs`, `http.rs`, `cli.rs`, `src/lico_protocol/v1.py`)**:
   - Extended `Account` (`accounts.rs`, `250` lines) with `ssh_host: Option<String>`, persisted in `0600` `accounts.json` and validated via `ssh::valid_ssh_host` in `Accounts::create_with` (rejecting empty strings, wildcards, and option-shaped strings).
   - Exposed `ssh_host` (`null` for local accounts, `"<alias>"` for SSH remote accounts) across `account.create`, `account.rename`, `account.list`, `POST /v1/accounts`, `lico account create --provider ... --label ... [--ssh-host HOST]`, and `src/lico_protocol/v1.py`.
2. **S03 — Supervised remote `Sign in` over SSH (`src/host/hostd/src/ssh_remote.rs`, `src/host/hostd/src/login.rs`)**:
   - Created `src/host/hostd/src/ssh_remote.rs` (`102` lines): `shell_quote`, `valid_account_id`, `sanitize_ssh_env` (clearing `DISPLAY` and unrelated local environment variables while preserving OpenSSH auth sockets), and `remote_login_command` (exporting real remote `$HOME/{.local/bin,.npm-global/bin,.cargo/bin,bin}` onto `PATH` before switching `HOME` and `CLAUDE_CONFIG_DIR`/`CODEX_HOME` to `0700` `$HOME/.local/state/lico/accounts/<id>/{home,<provider>}`).
   - Updated `login::start` and `login::signed_in` (`login.rs`, `315` lines) to run `ssh_remote::remote_login_command` when an account has `ssh_host: Some(host)`, streaming the sign-in URL to `lico gui` and forwarding masked verification codes over SSH `stdin` without touching local login behavior.
3. **S04 — GUI `[ssh: <host>]` badges, SSH target selector, and verification (`gui/ui.rs`, `gui/attn.rs`, `gui/panel.rs`, `gui/admin_ui.rs`, `gui/newsession.rs`, `tests/test_ssh_accounts.py`)**:
   - Populated `UiState::ssh_hosts` from `account.list` and updated `App::label_of` and `App::acct_display` so `[ssh: <host>]` badges render consistently across `Accounts`, sidebar session-group headings, session headers, `New session`, `Account:`, `Sign in:`, and `Rebind`.
   - Added `A  add a new account (local or SSH remote)` to `Step::Accounts` and `Provider` + `Target` cycling (`Tab`, `Up`/`Down`, and mouse click) to `Step::Label`, keeping `panel.rs` at `689` lines (`<= 800`) and `admin_ui.rs` at `301` lines (`<= 800`).
   - Added `tests/test_ssh_accounts.py` verifying socket/CLI/HTTP parity, `0700` remote directory isolation, zero local token storage, zero verification-code leakage, and end-to-end GUI SSH account creation + remote sign-in.

## Verification

| Check | Result | Gate |
|---|---|---|
| Rust unit tests (`cargo test`) | `585 passed; 0 failed` (`1` ignored; + `2 passed` in `synth_agent`) | `0` failures |
| Changed-area & conformance pytest (`test_ssh_accounts.py`, `test_size_ratchet.py`, `test_selection.py`, `test_no_hidden_characters.py`) | `34 passed` in `10.24 s` | `0` failures |
| Module size ratchets (`core.rs` / `ui.rs` / `panel.rs` / `admin_ui.rs` / `ssh_remote.rs`) | `983` / `1,113` / `689` / `301` / `102` lines | `<= 988` / `<= 1,120` / `<= 800` |

## Gate status

`M06-P02` is complete on `phase/M06-P02-ssh-remote-accounts-and-login`; execution proceeds directly to `M06-P03` (`phase/M06-P03-ssh-remote-files-and-sessions-0-2-2`) under the owner's G1/G2 approval.
