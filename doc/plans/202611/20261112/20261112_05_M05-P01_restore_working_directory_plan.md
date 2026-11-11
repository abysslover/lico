# M05-P01 Plan — Persist and restore previous working directory and workspace view across `lico` restarts

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)](../../../milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md)
- Branch: `phase/M05-P01-restore-working-directory`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

In `lico 0.2.0`, `folders::remember` recorded a folder only when `session.create`
ran, and `lico gui` started with an empty selection and no memory of the last
browsed directory or active workspace view. Closing `lico` while inspecting a
project directory in `Files` (or working inside a session) and reopening `lico`
returned to `No session is open.` instead of restoring the previous working
directory.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Record the `M05` milestone and commit this `M05-P01` plan | Pre-commit guards pass |
| S02 | Extend owner-only `0600` `settings.json` (`settings.rs`), `folders::remember_cwd` (`folders.rs`), `ipc.rs`, `http.rs`, `cli.rs`, and `src/lico_protocol/v1.py` with optional validated `last_cwd` (`folders::check_folder`), `last_view` (`"start"` \| `"files"` \| `"session"`), and `last_session`, keeping deleted/protected directories out on readback and updating `recent` + `last.cwd` in `session.options` when `last_cwd` is recorded | Rust unit tests and `test_protocol_conformance.py` pass |
| S03 | Wire `lico gui` (`ui.rs`, `files_ui.rs`, `newsession.rs`, `mod.rs`) so browsing a directory in `Files`, switching to a session, returning to start (`Esc`), or closing the window persists the current working directory and view state, and launching `lico gui` restores `Files` at `last_cwd` or re-attaches `last_session` (while fresh state directories with no prior UI selection continue to open cleanly on the start screen, and `start_lines` / `New session` / `Files` surface `last_cwd` whenever saved) | `tests/test_workspace_restore.py`, `tests/test_window_keys.py`, `tests/test_agent_tools.py`, and `tests/test_workspace_files.py` pass |
| S04 | Capture manual inspection bundle `00006_M05-P01_restore_working_directory/`, write the `M05-P01` report, update status tables, and commit | Changed suite and pre-commit guards pass |

## Personal data and security

`last_cwd` is validated through `folders::check_folder` (canonical absolute
directory, refusing host state, credential, log, and scratch directories) and
persisted strictly inside owner-only `0600` files in Lico's state directory. If
the directory no longer exists or becomes unreadable before a later startup, it
is omitted cleanly without error.
