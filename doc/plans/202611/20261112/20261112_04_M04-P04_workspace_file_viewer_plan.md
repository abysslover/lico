# M04-P04 Plan — On-demand working-directory file viewer (`F5` refresh, `+ New here`, `.html` external browser open) and `0.2.0` release pass

- Status: in_progress
- Approved: 2026-11-12 (G1 and G2 approved by the owner for continuous execution through `lico 0.2.0`)
- Project: Lico
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Branch: `phase/M04-P04-workspace-file-viewer`

## Objective

Complete `M04` and ship `lico 0.2.0` by adding a lightweight, strictly on-demand
working-directory file viewer across the host, CLI, local IPC, loopback HTTP,
and `lico gui` (`Files` toolbar button, `View > Working directory files`,
`Ctrl+E`):

1. **Access-triggered directory and file inspection (`zero` background watchers)**:
   `folder.list` and `folder.read` inspect a chosen working directory only when
   explicitly opened or navigated, and refresh in `lico gui` only when the
   person presses `F5` (or clicks toolbar `Refresh`), keeping idle memory and
   descriptor counts flat (`no inotify`, `no periodic polling`).
2. **Start a session directly in the viewed folder (`+ New here`)**: while
   browsing any folder in the file viewer, clicking `+ New here` on the toolbar,
   clicking `[+ New session in this directory]`, or pressing `N` opens the
   new-session form pre-populated with that exact canonical folder and its git
   repository status already checked.
3. **External `.html` / URL launch and bounded text preview**: activating an
   `.html` or `.htm` entry (by click or `Enter`) opens it in the desktop browser
   (`xdg-open`, respecting `LICO_NO_AUTO_OPEN` and `LICO_OPEN_CMD` for
   deterministic headless testing), while pressing `V` on an `.html` file or
   activating any other regular file shows a bounded `64 KiB` UTF-8 text preview
   in-window (refusing symlinks, special files, and protected host state
   directories).
4. **`lico 0.2.0` release pass**: bump `Cargo.toml` to `0.2.0`, update public
   documentation (`PLAN.md`, `README.md`, roadmap, manual guide, milestone, and
   indices), pass Rust unit tests under `umask 022` and `umask 077`, pass the
   full cold pytest suite, pass `idle_memory` (`< 50.0 MiB` at scales `1` and
   `2`) and `hidden_growth` (`<= +10.0 MiB`), pass two-clone `suite acceptance`,
   install `~/bin/lico` `0.2.0`, and capture manual inspection bundle `00004`
   before stopping at `M04` G3/G4.

## Slices

### S01 — Phase plan and index registration
- Record this plan in `doc/plans/202611/20261112/` and link it in the `M04`
  milestone table and `doc/index_4.md`.

### S02 — Host, protocol v1, IPC, loopback HTTP, and CLI file operations (`fsview.rs`)
- Add `src/host/hostd/src/fsview.rs` (`<= 350` lines) implementing:
  - `folder_list(shared, cwd)`: validates `cwd` with `folders::check_folder`
    (canonical absolute directory, never inside host state or scratch roots),
    computes `parent` (`Some(path)` only when `check_folder` allows it), reads
    `std::fs::read_dir` once per call, filters hidden/control characters and
    device/socket/FIFO entries, sorts directories before files (case-insensitive
    name order), caps at `500` entries (`truncated: bool`), and tags `.html` /
    `.htm` regular files with `is_html: true`.
  - `folder_read(shared, path)`: validates absolute path shape, checks
    `symlink_metadata` (`file_missing` when absent, `file_not_regular` for
    symlinks, directories, FIFOs, sockets, or devices), checks the parent folder
    with `folders::check_folder`, opens with `O_NOFOLLOW`, reads up to `64 KiB`
    (`65,536` bytes + `1` probe byte), reports `binary: true` (with empty
    `text`) if a `NUL` byte or invalid UTF-8 is encountered, and normalizes
    `CRLF`/`CR` to `LF` while stripping control/hidden characters (preserving
    `\n` and `\t`).
- Expose `"folder.list"` and `"folder.read"` in `core::OPS` (without increasing
  `core.rs` above `988` lines), `src/lico_protocol/v1.py` (`ERRORS`,
  `INVENTORY`, `REQUESTS`, `REPLIES`), `ipc.rs` (`valid_request` +
  `dispatch_request` for socket and `POST /v1`), `admin::HTTP_ROUTES` +
  `http.rs` (`GET /v1/files?cwd=...` and `GET /v1/files/read?path=...`), and
  `cli.rs` (`lico files [DIR]` and `lico files --read FILE`).

### S03 — In-window Working Directory File Viewer, explicit `F5` refresh, `+ New here`, and `.html` opener (`files_ui.rs`)
- Add `src/host/hostd/src/gui/files_ui.rs` (`<= 480` lines) and `Step::Files` on
  `panel::Step`:
  - Open via toolbar `Files`, `View > Working directory files`, or `Ctrl+E`,
    starting in the selected session's `cwd` (or most recent session folder ->
    process working directory -> `"/"`).
  - Expose contextual toolbar actions `+ New here` (`Cmd::NewInViewedDir`) and
    `Refresh` (`Cmd::RefreshFiles`, `F5`) whenever `Step::Files` is active.
  - Perform `folder.list` strictly on open, directory navigation (`Enter`,
    `Backspace`, or row click), and explicit `F5` / `Refresh` (`ui_tick` never
    touches `fsview`); expose `panel["files"]` metadata (`cwd`, `mode`,
    `refresh_count`, `entry_count`, `cursor`, `last_opened`) in `ui_dump()` for
    deterministic verification.
  - Support mouse clicks and keyboard navigation (`Up`/`Down`, `PageUp`/
    `PageDown`, `Home`/`End`, `Enter`, `Backspace`, `N` for `+ New here`, `F5`
    for refresh, `V` to view `.html` source, `Esc` to step back from file
    preview to directory listing or close the viewer).
  - Open `.html`/`.htm` files (and `http://` / `https://` URLs) externally via
    `LICO_OPEN_CMD` override (when set for headless tests), `LICO_NO_AUTO_OPEN`
    suppression, or detached reaped `xdg-open`.

### S04 — Automated unit and integration tests (`tests/test_workspace_files.py`)
- Add Rust unit tests in `fsview.rs` covering canonicalization, protected host
  state refusal, symlink refusal, binary detection, `64 KiB` truncation on
  multi-byte UTF-8 boundaries, and deterministic directory sorting.
- Add `tests/test_workspace_files.py` (registered in `tests/testmap.toml`)
  covering:
  1. Control-surface parity across CLI (`lico files`, `lico files --read`), Unix
     socket (`folder.list`, `folder.read`), `POST /v1`, and `GET /v1/files` /
     `GET /v1/files/read`, plus security refusals (`folder_protected`, symlink,
     relative path).
  2. GUI `Files` toolbar/shortcut opening, zero background polling when files
     are added on disk, explicit `F5` and toolbar `Refresh` updating the view
     and `refresh_count`.
  3. Directory navigation, in-window text preview + `Esc` back to directory,
     `.html` click spawning `LICO_OPEN_CMD` with exact path (and `V` previewing
     HTML source), and `+ New here` pre-filling `New session` in the browsed
     directory and launching an agent session there.

### S05 — `0.2.0` version bump, documentation sync, full cold suite, memory gates, two-clone build, `~/bin/lico` install, and inspection bundle `00004`
- Bump `src/host/hostd/Cargo.toml` from `0.1.0` to `0.2.0` and update version
  assertions and public documentation.
- Run Rust unit tests (`umask 022` and `umask 077`), full cold pytest suite,
  `idle_memory` (`scale 1` and `scale 2`, `< 50.0 MiB`), `hidden_growth`
  (`<= +10.0 MiB`), and two-clone `suite acceptance`.
- Install `~/bin/lico` `0.2.0`, capture manual inspection bundle
  `00004_M04-P04_workspace_file_viewer/`, visually inspect all rendered PNGs,
  and stop cleanly at `M04` G3/G4.

## Acceptance criteria

1. `folder.list` and `folder.read` pass protocol v1 validation and behave
   identically across Unix socket, `POST /v1`, `GET /v1/files*`, and `lico files`.
2. `lico gui` never polls directories in the background (`refresh_count` stays
   constant across idle ticks even after new files are created) and updates
   immediately on `F5` or clicking `Refresh`.
3. Clicking `.html` opens via the external browser opener (`LICO_OPEN_CMD` /
   `xdg-open`), and clicking `+ New here` starts a session in the viewed folder.
4. `core.rs <= 988` lines, `ui.rs <= 1,120` lines, all new modules `<= 800`
   lines, idle GUI RSS `< 50.0 MiB` at scales `1` and `2`, hidden growth
   `<= +10.0 MiB`, two-clone `suite acceptance` passes for `lico 0.2.0`, and no
   private paths or AI attribution markers appear anywhere in git history or
   tracked files.
