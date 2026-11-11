# M04-P04 Report — On-demand working-directory file viewer (`F5` refresh, `+ New here`, `.html` external browser open) and `0.2.0` release pass

- Version: 1.0.0
- Status: in_review (awaiting owner G3 acceptance and G4 merge approval at M04 milestone boundary)
- Date: 2026-11-12
- Plan: [M04-P04](../../../plans/202611/20261112/20261112_04_M04-P04_workspace_file_viewer_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Branch: `phase/M04-P04-workspace-file-viewer` (branched from `phase/M04-P03-quota-and-context-bar` `3b720ad`)
- Inspection bundle: `private/manual_inspection/202611/20261112/00005_M04-P04_workspace_file_viewer/`

## Summary

`M04-P04` completes Milestone `M04` (`lico 0.2.0`) by adding a strictly on-demand working-directory file viewer across the host, CLI, local Unix socket, loopback HTTP listener, and `lico gui`, bumping `lico` from `0.1.0` to `0.2.0`, re-verifying the full cold test suite and memory/hidden-growth gates, verifying clean two-clone binary reproducibility (`suite acceptance`), and installing `~/bin/lico` `0.2.0`.

## Delivered changes

1. **S02 — Host, protocol v1, IPC, loopback HTTP, and CLI file operations (`fsview.rs`, `core.rs`, `ipc.rs`, `admin.rs`, `http.rs`, `cli.rs`, `src/lico_protocol/v1.py`):**
   - Created `src/host/hostd/src/fsview.rs` (`337` lines) implementing `folder_list` and `folder_read` with zero background file watchers (`no inotify`, `no periodic polling`).
   - `folder_list` validates canonical folders via `folders::check_folder` (refusing host state and scratch roots), computes `parent`, reads `std::fs::read_dir` once per call, filters hidden/control names and special files, sorts directories first then case-insensitive names, caps at `500` entries, and marks `.html`/`.htm` regular files with `is_html: true`.
   - `folder_read` validates parent folders via `folders::check_folder`, refuses symlinks and non-regular files (`symlink_metadata` + `O_NOFOLLOW | O_NONBLOCK` + `fstat`), reads up to `64 KiB` (`65,536` bytes + `1` probe byte), detects binary files (`NUL` or invalid UTF-8, handling multi-byte UTF-8 boundaries cleanly at truncation), normalizes `CRLF`/`CR` to `LF`, and strips hidden/control characters.
   - Exposed `"folder.list"` and `"folder.read"` across `core::OPS` (`48` operations; `core.rs` kept at `988` lines), Unix socket IPC, `POST /v1`, `GET /v1/files?cwd=...`, `GET /v1/files/read?path=...` (`24` HTTP routes), and `lico files [DIR]` / `lico files --read FILE`.
2. **S03 — In-window Working Directory File Viewer, explicit `F5` refresh, `+ New here`, and `.html` opener (`gui/files_ui.rs`, `panel.rs`, `newsession.rs`, `menu.rs`, `toolbar.rs`):**
   - Created `src/host/hostd/src/gui/files_ui.rs` (`563` lines) implementing `Step::Files`, reachable via toolbar `Files`, `View > Working directory files`, or `Ctrl+E` (`ui.rs` untouched at `1,104` lines).
   - Directory listings load strictly on open, folder navigation, or explicit `F5` / toolbar `Refresh` (`refresh_count` and `load_count` stay invariant across idle ticks).
   - Clicking `[+ New session in this directory]`, clicking contextual toolbar `+ New here`, or pressing `N` calls `form_open_at(cwd)` so `New session` opens pre-populated in that exact canonical folder with git status checked.
   - Activating an `.html`/`.htm` entry launches the external browser (`LICO_OPEN_CMD` override for headless tests, `LICO_NO_AUTO_OPEN` suppression, or detached/reaped `xdg-open`), pressing `V` previews raw HTML source in-window, and activating regular text files opens a bounded `64 KiB` preview (`Esc` / `Backspace` / `[<- Back to directory]` returns to the folder view).
3. **S04 & S05 — Automated tests, `0.2.0` bump, cold suite, memory pass, and inspection bundle:**
   - Added Rust unit tests in `fsview.rs` (`579` total unit tests passing under both `umask 022` and `umask 077`) and end-to-end protocol + GUI test `tests/test_workspace_files.py`; refreshed `src/host/adapters/codex/capabilities.json` and `tests/test_adapter_contract.py` to the installed `codex-cli 0.167.1` schema.
   - Bumped `src/host/hostd/Cargo.toml` to `0.2.0`, updated public documentation, verified cold suite + scale 1/2 `idle_memory` + `hidden_growth` + `suite acceptance`, installed `~/bin/lico` `0.2.0`, and captured 3 real Xvfb screenshots + `index.html` in `private/manual_inspection/202611/20261112/00005_M04-P04_workspace_file_viewer/`.

## Measured verification (`lico 0.2.0`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `579 passed; 0 failed` (`1` ignored, both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `463` tests in `84` files, `0` failed, **`96.7 s`**, peak **`1,580.1 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.42 MiB` (`24.18..24.42 MiB`), median PSS `16.95 MiB`, anon `3.94 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.50 MiB` (`35.29..35.50 MiB`), median PSS `22.49 MiB`, anon `3.94 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.38 MiB` (`24.43 MiB` -> `24.64 MiB`; host `8.54` -> `20.13 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.0` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`core.rs` / `ui.rs` / `fsview.rs` / `files_ui.rs` / `panel.rs`) | `988` / `1,104` / `337` / `563` / `624` lines | `<= 988` / `<= 1,120` / `<= 800` | Met |

## Gate status

All four phases of Milestone `M04` (`M04-P01` through `M04-P04`, `lico 0.2.0`) are complete on a linear fast-forward branch chain (`main` -> `phase/M04-P01-session-nav-and-toolbar` -> `phase/M04-P02-clean-conversation-view` -> `phase/M04-P03-quota-and-context-bar` -> `phase/M04-P04-workspace-file-viewer`) and await owner G3 inspection acceptance and G4 merge/publication approval.
