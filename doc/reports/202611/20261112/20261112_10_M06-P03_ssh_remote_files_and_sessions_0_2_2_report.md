# M06-P03 Report — SSH remote directory browsing, remote `claude`/`codex` session execution & resume, and `0.2.2` patch release

- Status: done (awaiting owner G3 inspection and G4 merge approval)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)](../../../milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md)
- Plan: [M06-P03 plan](../../../plans/202611/20261112/20261112_10_M06-P03_ssh_remote_files_and_sessions_0_2_2_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M06-P03-ssh-remote-files-and-sessions-0-2-2`
- Inspection bundle: `202611/20261112/00013_M06-P03_ssh_remote_files_and_sessions_0_2_2/`

## Summary

`M06-P03` completes Milestone `M06` (`lico 0.2.2` patch release) by extending the
on-demand directory viewer (`Files`), `New session`, CLI, IPC, and loopback HTTP
to browse `~/.ssh/config` remote hosts via a combo-box `Host: local v` /
`Host: <alias> v` dropdown (`H` shortcut, zero network call on open) and
non-blocking background SSH requests (`ssh_unreachable` on offline/removed hosts
without freezing the window), preview `<= 64 KiB` UTF-8 text files over
`BatchMode=yes` SSH, and execute, approve, monitor (`7d` quota telemetry),
mode-switch, and `session.resume` persistent `claude` and `codex` sessions
directly on the remote host inside isolated `0700` remote account homes
(`$HOME/.local/state/lico/accounts/<id>`).

## Delivered changes

1. **S02 — Remote directory listing, UTF-8 preview, combo-box `Host` dropdown, and non-blocking `Files` UI (`src/host/hostd/src/{ssh.rs,ssh_remote.rs,fsview.rs,ipc.rs,http.rs,cli.rs}`, `src/lico_protocol/v1.py`, `gui/{files_ui.rs,files_host.rs,files_render.rs,toolbar.rs,menu.rs,panel.rs,ui.rs,controls.rs,memrow.rs,attn.rs}`)**:
   - Added bounded (`4 s` hard wall-clock, `ConnectTimeout<=3`, `ControlMaster=no`, `ControlPath=none`, process-group reaped, channel-bounded pipe drain) `ssh_remote::folder_list`, `ssh_remote::folder_read`, and `ssh_remote::folder_git` over `ssh_base_command`, returning `"ssh_unreachable"` (`HTTP 503`) on OpenSSH transport/auth exit `255` or timeout, expanding `"~"` and `"~/..."` against remote `$HOME`, enforcing remote `~/.local/state/lico` `folder_protected` refusal, skipping symlinks/special files (`O_RDONLY | O_NOFOLLOW | O_NONBLOCK` + `fstat`), sorting directories first (`<= 10,000` entries), and decoding `<= 64 KiB` UTF-8 previews.
   - Wired optional `ssh_host` and `"ssh_unreachable"` across `folder.list`, `folder.read`, `folder.git`, `GET /v1/files`, `GET /v1/files/read`, `lico files [DIR] [--hidden|-a] [--ssh-host HOST]`, `lico files --read FILE [--ssh-host HOST]`, and `src/lico_protocol/v1.py`.
   - Added the `Host: local v` / `Host: <alias> v` (`id: "files_host"`) combo-box dropdown in `Files` (`H` shortcut, keyboard/mouse/wheel selection, `Esc` or click-outside dismiss) and moved all SSH-capable GUI RPCs (`folder.list`, `folder.read`, `session.create`, `account.login.status`) onto background threads via `EventLoopProxy<Ev>` (`Ev::Async`) so slow, offline, or removed SSH hosts never block the `winit` event loop (`Esc` cancels in-flight connections; offline/removed hosts render a responsive `Files [ssh:<host>]: unreachable` recovery view while keeping `Host` immediately selectable). Kept local `last_cwd` unpolluted by remote paths and wired `+ New here` (`N`) to `form_open_at_host`.
2. **S03 — Persistent SSH remote `claude` and `codex` sessions (`ssh_remote.rs`, `accounts.rs`, `core.rs`, `folders.rs`, `claude.rs`, `codex.rs`, `gui/row.rs`, `gui/newsession.rs`, `tests/test_ssh_sessions_and_files.py`)**:
   - Added `Accounts::target_ssh_host`, `ssh_remote::prepare_session` (validating remote folders over SSH and refusing `worktree_branch` on SSH targets while preserving 100% of local `folders::prepare` error ordering), `ssh_remote::ensure_remote_workspace` (`0700` `$HOME/.local/state/lico/workspaces/<id>` when `cwd` is omitted), `remote_claude_command`, and `remote_codex_command`.
   - Updated `claude::spawn` and `codex::spawn_provider` to launch and `session.resume` over SSH inside remote `0700` `$HOME/.local/state/lico/accounts/<id>/{home,<provider>}` with remote user binary paths (`~/.local/bin`, `~/.npm-global/bin`, `~/.cargo/bin`, `~/bin`) prepended before `HOME` isolation, exposing `"ssh_host"` on `status` session entries and `dir: ssh:<host>:<cwd>` in `lico gui` headers.
   - Added `tests/test_ssh_sessions_and_files.py` covering socket/CLI/HTTP remote file parity, `ssh_unreachable` (`HTTP 503`), symlink & `folder_protected` refusals, remote `claude` streaming/approval/mode-switch/`session.resume`, remote `codex` `7d` quota telemetry & `session.resume`, and GUI `Host` combo-box dropdown, non-blocking hanging/offline host recovery, remote file preview, and `+ New here` remote session launch.
3. **S04 — Patch version bump (`0.2.1` -> `0.2.2`), full release verification, and manual inspection bundle**:
   - Bumped `lico` package version from `0.2.1` to `0.2.2` across `Cargo.toml`, `Cargo.lock`, `tests/test_control_parity.py`, `tests/test_single_executable.py`, `doc/manual_end_to_end.md`, `PLAN.md`, `README.md`, and `roadmap.md`.

## Measured verification (`lico 0.2.2`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `585 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `475` tests in `89` files, `0` failed, **`97.8 s`**, peak **`1,645.4 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.52 MiB` (`24.19..24.52 MiB`), median PSS `17.11 MiB`, anon `3.94 MiB`, host `7.35 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.53 MiB` (`35.27..35.53 MiB`), median PSS `22.60 MiB`, anon `3.75 MiB`, host `7.51 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.26 MiB` (`24.54 MiB` -> `24.75 MiB`; host `8.68` -> `20.33 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.2` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`core.rs` / `ui.rs` / `ssh.rs` / `ssh_remote.rs` / `files_ui.rs` / `files_host.rs` / `files_render.rs` / `newsession.rs` / `panel.rs`) | `984` / `1,115` / `675` / `659` / `678` / `431` / `384` / `661` / `786` lines | `<= 988` / `<= 1,120` / `<= 800` | Met |

## Gate status

All three phases of Milestone `M06` (`M06-P01` through `M06-P03`, `lico 0.2.2`) are implemented, measured, and verified on `phase/M06-P03-ssh-remote-files-and-sessions-0-2-2`. Execution stops here for owner **G3** inspection acceptance and **G4** merge/publication approval (`main` remains at `0.2.1` until G4).
