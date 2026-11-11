# M03-P02 report — persistent workspace-grouped session navigator and adjustable text size

- Version: 1.0.0
- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Date: 2026-11-11
- Plan: [M03-P02](../../../plans/202611/20261111/20261111_06_M03-P02_workspace_navigator_plan.md) (version 1.0.0, G2 approved by the owner on 2026-11-11)
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Branch: `phase/M03-P02-workspace-navigator` (branched from `phase/M03-P01-account-and-session-admin` `7bd72ac`)
- Inspection bundle: `private/manual_inspection/202611/20261111/00008_M03-P02_workspace_navigator/`

## What was done

1. **S02 — Persistent `group_by` and `font_px` settings across host, socket, CLI, and loopback HTTP:**
   - Extended `src/host/hostd/src/clipboard/settings.rs` (`Patch`, `update_settings`, `group_by_value`, `font_px_value`, `status_json`) with atomic `0600` (`O_NOFOLLOW`) persistence for `group_by` (`"account"` default | `"workspace"`) and `font_px` (`12..=20`, default `14`), falling back safely on missing or invalid file values.
   - Extended `ipc.rs` (`settings.set`), `cli.rs` (`lico settings --group-by account|workspace --font-size 12..20`), `http.rs` (`GET /v1/settings` and `POST /v1/settings`), and `src/lico_protocol/v1.py` (`status` and `settings.set` schemas).
2. **S03 — Workspace-grouped sidebar (with parent-repo worktree rollup) and live `12..=20` px font metrics in `lico gui`:**
   - Updated `src/host/hostd/src/gui/attn.rs` (`refresh_list`, `sync_settings`, `toggle_group_by`, `step_font_px`, `apply_font_px`, `font_px`, `line_h`, `char_w`):
     - When `group_by == "workspace"`, sessions are grouped in order of first appearance by `worktree_repo` (when running in a Lico git worktree) else `cwd`, rolling git worktrees up under their parent repository (`workspace / <repo>`) with `(wt)` row badges, separating scratch sessions under `workspace / (scratch)`, and disambiguating basename collisions as `workspace / <parent>/<leaf>`.
     - Changing `font_px` (`12..=20`) clears cached `BufferLine` layouts, updates `Metrics` on the active view buffer and shared `Controls` label cache (`controls.rs`), scales `realbar.rs`, bumps `fonts_gen` so scroll-step invalidates stale frames cleanly, and persists immediately across window restarts.
   - Added `View` menu entries (`Group sessions: account|workspace`, `Text size: larger`, `Text size: smaller`, `Text size: 14 px (reset)`) and keyboard shortcuts (`Ctrl+Shift+G`, `Ctrl+=`/`Ctrl++`, `Ctrl+-`, `Ctrl+0`), shrinking `src/host/hostd/src/gui/ui.rs` to `1,166` lines (`<= 1,168` allowlist).
3. **S04 — Automated verification, idle GUI memory measurement, and manual inspection bundle:**
   - Added Rust unit tests (`set_clipboard_write_writes_atomically...` extended for `Patch`, plus `workspace_grouping_rolls_worktrees_under_parent_repo_disambiguates_collisions_and_marks_scratch`) and `tests/test_workspace_navigator.py` (`IPC/CLI/HTTP` + `Xvfb` worktree rollup + window reopen persistence).
   - Measured `idle_memory` (`--text --realistic-text --ui --menu --accounts 10`) at scale `1` and scale `2` and captured bundle `00008_M03-P02_workspace_navigator/`.

## Verification and memory results

| Check / Metric | Result | Budget / Guard |
|---|---|---|
| `cargo test --bin lico -- clipboard:: gui::ui::attn::` | `28/28` passed | Zero failures |
| `pytest tests/test_workspace_navigator.py tests/test_menu_bar.py tests/test_protocol_conformance.py tests/test_selection.py tests/test_size_ratchet.py tests/test_no_hidden_characters.py` | `44/44` passed | Zero failures |
| `gui_idle_rss_mib` at **scale 1** (`n = 3` launches, 10 accounts, 10k realistic lines) | **`24.10 MiB` max** (`24.00, 23.91, 24.10 MiB`) | `<= 50.0 MiB` (`met`) |
| `gui_idle_rss_mib` at **scale 2** (`n = 3` launches, 10 accounts, 10k realistic lines) | **`35.12 MiB` max** (`35.07, 35.12, 35.08 MiB`) | `<= 50.0 MiB` (`met`) |
| `src/host/hostd/src/gui/ui.rs` / `core.rs` line counts | `1,166` / `988` lines | `<= 1,168` / `<= 988` |
