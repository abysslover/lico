# M04-P01 Plan: Direct session switching from panels, derived titles, active reverse-chronological sorting, and clickable toolbar

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Phase: `M04-P01` (`phase/M04-P01-session-nav-and-toolbar`)
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

Four core navigation and discoverability gaps affected everyday use of `lico gui`:
1. **Blocked left-sidebar clicks when a panel is open**: `App::row_click` returned early whenever `self.ui.panel.is_some()`, so opening `Help > About Lico` (or `Keys`, `Accounts`, `NewSession`, etc.) prevented clicking a session row on the left until `Esc` was pressed first.
2. **Missing derived session titles**: Unnamed scratch sessions displayed raw ID prefixes (`s3f20f6`) and multiple unnamed sessions in the same folder shared identical labels even after distinct conversations began.
3. **Static creation-order session list**: Active (`running`) sessions were not prioritized above closed (`disconnected`/`exited`) sessions or ordered newest-activity-first (`last_active_at` descending).
4. **Menu-only / shortcut-only actions**: Primary everyday actions (`+ New`, `Accounts`, `Resume`/`Close`, `Rename`, `Mode`, `Group`, `Find`) lacked a one-click toolbar beneath the menu bar.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Record the `M04` milestone and commit this `M04-P01` plan | Pre-commit guards pass |
| S02 | Fix `row_click`, `nav_key`, and `Session` menu `GoTo`/`Next`/`Prev` so switching sessions while any panel is open safely calls `panel_close()` (restoring parked drafts and cancelling unfinished logins) and enters the chosen session; add derived `auto_title` (from first `>> ` user prompt, persisted/recovered from `.seg` logs) and `last_active_at` timestamp on `Session` + `status` + `v1.py`; sort `refresh_list` active-first (`running` ahead of inactive) and `last_active_at` descending | Rust unit tests and `test_protocol_conformance.py` pass |
| S03 | Extract menu-bar rendering out of `ui.rs` and add `src/host/hostd/src/gui/toolbar.rs` drawing a clickable action toolbar beneath the menu bar (`+ New`, `Accounts`, `Resume`/`Close`, `Rename`, `Mode`, `Group`, `Find`), exposed in `ui_dump()["toolbar"]`, while keeping `ui.rs <= 1,120` and `core.rs <= 988` lines | `tests/test_toolbar_and_nav.py`, `tests/test_menu_bar.py`, `tests/test_everyday_comfort.py`, `tests/test_size_ratchet.py` pass |
| S04 | Capture manual inspection bundle `00001_M04-P01_session_nav_and_toolbar/`, write the `M04-P01` report, update status tables, and commit | Changed suite and pre-commit guards pass |

## Personal data and security

No credentials, internal filesystem paths, or external network calls are introduced. `auto_title` is derived strictly from local session prompt records already stored in owner-only `0600` `.seg` files and sanitized through `folders::refused_char`.
