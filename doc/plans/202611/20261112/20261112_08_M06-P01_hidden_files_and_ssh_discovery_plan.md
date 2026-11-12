# M06-P01 Plan — Hidden-file (`.dotfiles`) toggle and `~/.ssh/config` host discovery (`ssh.hosts`, `ssh.check`)

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)](../../../milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md)
- Branch: `phase/M06-P01-hidden-files-and-ssh-discovery`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

Directory listings in `lico 0.2.1` included dot-prefixed entries (`.git`,
`.cargo`, `.config`, `.env.example`, etc.) unconditionally, cluttering everyday
folder navigation, and `lico` had no inventory or connectivity check for hosts
declared in `~/.ssh/config`. `M06-P01` makes dot-prefixed entries hidden by
default with an explicit toggle across GUI, CLI, IPC, and loopback HTTP, splits
`gui/files_ui.rs` cleanly well below the 800-line ceiling, and introduces safe
`~/.ssh/config` host discovery and bounded non-interactive probing (`ssh.hosts`,
`ssh.check`).

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit `M06` milestone and this `M06-P01` plan; update `PLAN.md`, `roadmap.md`, and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Default-off dotfile filtering (`show_hidden: bool`) in `fsview::folder_list`, `settings.rs` (`show_hidden`), CLI (`lico files --hidden`, `lico settings --show-hidden on\|off`), IPC (`folder.list`, `settings.set`), HTTP (`GET /v1/files?cwd=...&hidden=1`), and `src/lico_protocol/v1.py`; split `gui/files_ui.rs` into `gui/files_ui.rs` + `gui/files_render.rs` and add `Hidden: off`/`Hidden: on` toolbar button, `View` menu toggle, and `Ctrl+H` / `.` shortcut | Rust unit tests and `test_workspace_files.py` pass |
| S03 | Create `src/host/hostd/src/ssh.rs` (`--ssh` and `--ssh-config` host flags for hermetic testing, safe `~/.ssh/config` + `Include` parser skipping wildcards and option-like names, bounded 6 s `BatchMode=yes` `ssh.check` probing connectivity and installed `claude`/`codex` binaries); wire `ssh.hosts` and `ssh.check` across `core::OPS`, `ipc.rs`, `http.rs` (`GET /v1/ssh/hosts`, `POST /v1/ssh/check`), `admin.rs` `HTTP_ROUTES`, `cli.rs` (`lico ssh hosts`, `lico ssh check HOST`), and `src/lico_protocol/v1.py` | Rust unit tests, `test_protocol_conformance.py`, and `test_control_parity.py` pass |
| S04 | Add `tests/test_hidden_and_ssh.py` (registered in `tests/testmap.toml`), capture manual inspection bundle `00009_M06-P01_hidden_files_and_ssh_discovery/`, write `M06-P01` report, and commit cleanly | Changed-area tests, `test_size_ratchet.py`, and `test_selection.py` pass |

## Personal data and security

1. SSH host aliases must match `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$` (never
   starting with `-` or containing whitespace or shell metacharacters) and are
   passed after `--` to `Command::new` (`BatchMode=yes`), preventing option
   injection or interactive password hangs.
2. Public repository code, tests, comments, and reports use synthetic test
   aliases only (`gpu-box`, `build-srv`) and never read or record personal host
   configurations during tests.
