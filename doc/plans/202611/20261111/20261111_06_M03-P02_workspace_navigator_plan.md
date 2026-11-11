# M03-P02 Plan: Persistent workspace-grouped session navigator and adjustable text size

- Status: done
- Version: 1.0.0
- Date: 2026-11-11
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Phase: `M03-P02` (`phase/M03-P02-workspace-navigator`)
- Approval: G1 and G2 Option A approved by the owner on 2026-11-11.

## Why

Administrators running sessions across multiple repositories and providers need two everyday view controls directly inside `lico gui` without sacrificing `< 50.0 MiB` idle RSS:
1. **Workspace-grouped session list (`group_by`: `"account"` | `"workspace"`)**: Grouping by `"account"` (default) shows billing/provider boundaries, whereas grouping by `"workspace"` groups sessions by repository/folder root and rolls git worktree sessions (`worktree.repo`) up under their parent repository regardless of which provider or account runs them.
2. **Persistent adjustable text size (`font_px`: `12..=20`, default `14`)**: High-density multi-session monitoring and comfortable reading require immediate keyboard/menu text scaling (`Ctrl+=`, `Ctrl+-`, `Ctrl+0`) persisted in `settings.json` alongside CLI and HTTP parity.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M03-P02` plan and link it in the `M03` milestone and `doc/index_4.md` | Guard checks pass |
| S02 | Extend `settings.json` (`src/host/hostd/src/clipboard/settings.rs`), `ipc.rs` (`settings.set`), `cli.rs` (`lico settings --group-by account\|workspace --font-size 12..20`), `http.rs` (`GET /v1/settings`, `POST /v1/settings`), and `src/lico_protocol/v1.py` (`status` and `settings.set` schemas) | Rust unit tests and `test_protocol_conformance.py` pass |
| S03 | Wire workspace grouping (with parent-repo rollup for git worktrees and `parent/name` disambiguation) and live `12..=20` px font metrics across `gui/attn.rs`, `gui/controls.rs`, `gui/realbar.rs`, `gui/menu.rs`, `gui/panel.rs`, `gui/mod.rs`, and `gui/ui.rs` (`View` menu + `Ctrl+Shift+G`, `Ctrl+=`, `Ctrl+-`, `Ctrl+0`) while keeping `ui.rs <= 1,168` lines | `tests/test_workspace_navigator.py`, `tests/test_menu_bar.py`, and `idle_memory` (`< 50.0 MiB` at scales 1 and 2) pass |
| S04 | Generate manual inspection bundle `00008_M03-P02_workspace_navigator/`, write the `M03-P02` report, update status tables, and commit | Pre-commit guards and changed suite pass |

## Personal data and security

- `group_by` and `font_px` are non-secret view preferences stored atomically (`0600`, `O_NOFOLLOW`) in `settings.json`; invalid or out-of-range values fall back safely to `"account"` and `14`.
- Sidebar workspace headings show only the short folder basename (or `parent/basename` on collision), never leaking full filesystem paths in narrow headings.
