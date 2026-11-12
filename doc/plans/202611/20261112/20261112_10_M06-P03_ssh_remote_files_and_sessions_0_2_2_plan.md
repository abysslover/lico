# M06-P03 Plan — SSH remote directory browsing, remote `claude`/`codex` session execution & resume, and `0.2.2` patch release

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)](../../../milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md)
- Branch: `phase/M06-P03-ssh-remote-files-and-sessions-0-2-2`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

With `M06-P01` (default-off `.dotfiles` toggle and `~/.ssh/config` host
discovery) and `M06-P02` (`[ssh: <host>]` accounts and supervised remote
`Sign in`) in place, `M06-P03` completes the remote-host workbench and ships
patch version `0.2.2`:

1. Administrators can switch `Files` between `Host: local` and any discovered
   SSH host (`Host: <alias>`), browse remote directories on demand (`F5`
   refresh, `Hidden: off/on`, `Page X / Y` pagination), preview `<= 64 KiB`
   UTF-8 files in-window, and click `+ New here` (`N`) to start a session right
   in that remote directory.
2. Sessions bound to an `[ssh: <host>]` account spawn, stream, approve, collect
   `7d` quota telemetry, switch permission mode, and reconnect (`session.resume`)
   over `BatchMode=yes` SSH inside their remote working directory using the
   remote machine's isolated `0700` account home (`$HOME/.local/state/lico/accounts/<id>`)
   so the remote machine connects directly to its LLM API provider.
3. Patch version advances `0.2.1` -> `0.2.2` with full cold-suite, idle-RSS
   (`< 50.0 MiB`), hidden-growth (`<= +10.0 MiB`), and two-clone
   `suite acceptance` verification before stopping at `M06` G3/G4.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M06-P03` plan; update `M06` milestone and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Extend `src/host/hostd/src/ssh_remote.rs` with bounded (`8 s`, `BatchMode=yes`, process-group) remote `folder_list` (resolving `""` / `"~"` to remote `$HOME`), `folder_read` (`<= 64 KiB` regular-file UTF-8 preview, refusing symlinks/special files), and `folder_git`; wire optional `ssh_host` across `fsview.rs`, `ipc.rs` (`folder.list`, `folder.read`, `folder.git`), `http.rs` (`GET /v1/files`, `GET /v1/files/read`), `cli.rs` (`lico files [--ssh-host HOST]`), `src/lico_protocol/v1.py`, and `gui/files_ui.rs` + `gui/files_render.rs` + `gui/toolbar.rs` (`Host: local` / `Host: <alias>` selector, `H` shortcut, `+ New here` pre-selecting `[ssh: <host>]` account and remote `cwd`) | Rust unit tests and `test_workspace_files.py` pass |
| S03 | Extend `ssh_remote.rs`, `folders.rs`, `core.rs`, `claude.rs`, `codex.rs`, `gui/newsession.rs`, and `gui/ui/row.rs` so `claude` and `codex` sessions bound to an `[ssh: <host>]` account validate/create remote working directories over SSH, spawn and `session.resume` over `ssh` inside remote `0700` `$HOME/.local/state/lico/accounts/<id>/{home,<provider>}`, expose `"ssh_host"` on `status` session entries (`v1.py`), hide local-only `Worktree` in `New session` when an SSH account is selected, and display `ssh:<host>:<cwd>` in session headers | Rust unit tests and `test_ssh_sessions_and_files.py` pass |
| S04 | Bump patch version `0.2.1` -> `0.2.2`, run `cargo test` (`umask 022` and `077`), full cold suite (`suite run --cold`), `idle_memory` (scales 1 & 2) + `hidden_growth`, commit report & docs, run two-clone `suite acceptance`, install `~/bin/lico` `0.2.2`, capture inspection bundle, and stop at `M06` G3/G4 | All release, memory, size-ratchet, and two-clone `sha256` gates pass |

## Personal data and security

1. All remote SSH calls use `ssh_base_command` (`-T -o BatchMode=yes -o ConnectTimeout=... -- <host>`) with `sanitize_ssh_env` and strict `valid_ssh_host` / `shell_quote` validation.
2. Remote non-text/external files show an explicit remote notice rather than invoking local `xdg-open`, and remote credentials never leave `0700` `$HOME/.local/state/lico/accounts/<id>` on the remote machine.
