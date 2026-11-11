# M05-P01 Report — Persist and restore previous working directory and workspace view across `lico` restarts

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)](../../../milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md)
- Plan: [M05-P01 plan](../../../plans/202611/20261112/20261112_05_M05-P01_restore_working_directory_plan.md)
- Branch: `phase/M05-P01-restore-working-directory`

## Summary

`M05-P01` persists the administrator's current working directory and workspace
view across `lico gui` and `lico host` restarts while preserving owner-only
`0600` state safety and full CLI / IPC / loopback HTTP parity:

1. **Validated host workspace settings (`settings.rs`, `folders.rs`, `ipc.rs`, `http.rs`, `cli.rs`, `v1.py`)**:
   - Extended `settings.json` (`Patch`, `apply_settings`, `status_json`) with
     optional `last_cwd`, `last_view` (`"start" | "files" | "session"`), and
     `last_session`.
   - Added `folders::remember_cwd`, validating `last_cwd` through
     `folders::check_folder` (canonical absolute directory, refusing host
     state, credential, log, handoff, and scratch workspaces) and updating
     `recent_folders.json` (`folders[0]` and `last.cwd`) atomically (`0600`,
     `O_NOFOLLOW`).
   - Omitted deleted, unreadable, or protected directories cleanly on readback
     without failing startup or `status` queries, and kept default `status.settings`
     unchanged when `last_view == "start"`.
   - Wired `--last-cwd`, `--last-view`, and `--last-session` in `lico settings`,
     `settings.set` over Unix socket IPC, and `POST /v1/settings` over loopback
     HTTP, with matching schema validation in `src/lico_protocol/v1.py`.
2. **Automatic GUI persistence and startup restore (`gui/{ui.rs,attn.rs,files_ui.rs,newsession.rs,mod.rs}`)**:
   - Persists workspace state when browsing directories in `Files`
     (`files_load_dir`), switching or starting sessions (`switch_to`,
     `form_start`), returning to the start screen (`go_start`, `Esc`), and
     closing `lico gui`.
   - Restores once on startup (`maybe_restore_workspace()`, skipped while a
     background host binary upgrade is in flight): re-opens `Files` at
     `last_cwd` when `last_view == "files"`, or re-attaches `last_session`
     when `last_view == "session"` (falling back to `Files` at `last_cwd` if
     that session was deleted).
   - Surfaces `Working directory: <last_cwd>` on the start screen and pre-fills
     `+ New` / `Files` with `last_cwd`.
   - Tightened turn-settlement synchronization in `tests/test_everyday_comfort.py`
     so reverse-chronological active session sorting settles deterministically
     before `Alt+N` selection.

## Verification

- `cargo test --manifest-path src/host/hostd/Cargo.toml clipboard::` — `27 passed` (including `workspace_fields_round_trip_clear_cleanly_and_drop_missing_or_state_dirs_on_readback`).
- `PYTHONPATH=src:. private/venv/bin/pytest -q tests/test_size_ratchet.py tests/test_no_hidden_characters.py tests/test_protocol_conformance.py tests/test_workspace_restore.py tests/test_everyday_comfort.py` — all passed (`434` changed-area tests verified).
- Manual inspection bundle captured and inspected locally (`202611/20261112/00006_M05-P01_restore_working_directory/`).
