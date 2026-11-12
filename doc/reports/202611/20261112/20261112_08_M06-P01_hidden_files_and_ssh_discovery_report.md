# M06-P01 Report — Hidden-file (`.dotfiles`) toggle and `~/.ssh/config` host discovery (`ssh.hosts`, `ssh.check`)

- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)](../../../milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md)
- Plan: [M06-P01 plan](../../../plans/202611/20261112/20261112_08_M06-P01_hidden_files_and_ssh_discovery_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M06-P01-hidden-files-and-ssh-discovery`
- Inspection bundle: `202611/20261112/00010_M06-P01_hidden_files_and_ssh_discovery/`

## Summary

`M06-P01` implements default-off hidden-file (`.dotfiles`) filtering and explicit
visibility toggling across the GUI (`Files` toolbar, `View` menu, `Ctrl+H` / `.`),
CLI (`lico files --hidden`, `lico settings --show-hidden on|off`), IPC
(`folder.list`, `settings.set`), and loopback HTTP (`GET /v1/files?cwd=...&hidden=1`),
splits `gui/files_ui.rs` cleanly into `gui/files_ui.rs` (`551` lines) and
`gui/files_render.rs` (`250` lines), and adds `src/host/hostd/src/ssh.rs` (`667`
lines) for safe `~/.ssh/config` (`Include`-aware) host discovery (`ssh.hosts`)
and bounded 6 s non-interactive connectivity and remote provider probing
(`ssh.check`).

## Delivered changes

1. **S02 — Default-off hidden-file filtering and GUI/CLI/IPC/HTTP parity (`src/host/hostd/src/fsview.rs`, `src/host/hostd/src/clipboard/settings.rs`, `src/host/hostd/src/gui/files_ui.rs`, `src/host/hostd/src/gui/files_render.rs`, `src/host/hostd/src/gui/toolbar.rs`, `src/host/hostd/src/gui/menu.rs`)**:
   - Updated `fsview::folder_list(shared, cwd, show_hidden)` (`451` lines) so `.` and `..` are always excluded, dot-prefixed entries (`name.starts_with('.')`) are skipped when `show_hidden == false`, and `"show_hidden": show_hidden` is returned in `folder.list` replies.
   - Persisted optional `show_hidden` in `0600` `settings.json` (default `false`; omitted from `status.settings` when `false`, emitted as `true` when enabled) and exposed `--show-hidden on|off` in `lico settings`.
   - Split `gui/files_ui.rs` into `gui/files_ui.rs` (`551` lines) and `gui/files_render.rs` (`250` lines), adding `Hidden: off` / `Hidden: on` (`id: "toggle_hidden"`) to the `Files` toolbar, `Hidden files: hidden` / `Hidden files: shown` (`Ctrl+H`) to `View`, and `Ctrl+H` / `.` keyboard toggling in `Files`.
   - Added `--hidden` / `-a` to `lico files` and `&hidden=1` to `GET /v1/files`.
2. **S03 — Safe `~/.ssh/config` host discovery and bounded `ssh.check` (`src/host/hostd/src/ssh.rs`, `core.rs`, `ipc.rs`, `http.rs`, `admin.rs`, `cli.rs`, `src/lico_protocol/v1.py`)**:
   - Created `src/host/hostd/src/ssh.rs` (`667` lines; `--ssh` and `--ssh-config` host overrides for hermetic tests):
     - `valid_ssh_host(s)` enforces `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$` (never starting with `-`, whitespace, or metacharacters) and `ssh_base_command` passes `-- <host>` with `-T -o BatchMode=yes -o ConnectTimeout=...`.
     - `ssh_hosts(shared)` reads `~/.ssh/config` and `Include` globs with `O_NOFOLLOW | O_NONBLOCK` (`<= 256 KiB` per file, depth `<= 4`, `<= 32` files, cycle-guarded), skipping wildcards (`*`, `?`), negations (`!`), and invalid aliases while applying `Host *` `User`/`Port` fallbacks.
     - `ssh_check(shared, host)` runs a bounded `6 s` `BatchMode=yes` probe in its own process group (`kill(-pid, 9)` on timeout), returning `reachable`, remote `home`, and installed `claude`/`codex` paths (`providers`).
   - Exposed `ssh.hosts` and `ssh.check` across `core::OPS`, IPC, loopback HTTP (`GET /v1/ssh/hosts`, `POST /v1/ssh/check`), `admin.rs` `HTTP_ROUTES`, CLI (`lico ssh hosts`, `lico ssh check HOST`), and `src/lico_protocol/v1.py`.
3. **S04 — Automated verification and manual inspection bundle (`tests/test_hidden_and_ssh.py`, `tests/testmap.toml`)**:
   - Added `tests/test_hidden_and_ssh.py` covering default-off dotfile filtering and parity across IPC/CLI/HTTP/`settings.json`, `ssh.hosts` (`Include`, wildcards, symlinks, option-injection rejection) and `ssh.check`, and GUI toolbar/key/menu toggling + dotfile text preview.

## Verification

| Check | Result | Gate |
|---|---|---|
| Rust unit tests (`cargo test`) | `584 passed; 0 failed` (`1` ignored; + `2 passed` in `synth_agent`) | `0` failures |
| Changed-area & conformance pytest (`test_hidden_and_ssh.py`, `test_workspace_files.py`, `test_protocol_conformance.py`, `test_control_parity.py`, `test_size_ratchet.py`, `test_no_hidden_characters.py`, `test_selection.py`) | `44 passed` in `12.60 s` | `0` failures |
| Module size ratchets (`core.rs` / `ui.rs` / `files_ui.rs` / `files_render.rs` / `ssh.rs`) | `988` / `1,115` / `551` / `250` / `667` lines | `<= 988` / `<= 1,120` / `<= 800` |

## Gate status

`M06-P01` is complete on `phase/M06-P01-hidden-files-and-ssh-discovery`; execution proceeds directly to `M06-P02` (`phase/M06-P02-ssh-remote-accounts-and-login`) under the owner's G1/G2 approval.
