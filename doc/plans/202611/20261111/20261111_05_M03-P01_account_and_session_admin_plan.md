# M03-P01 Plan: In-window account rename, session rebind, and live permission-mode control

- Status: done
- Version: 1.0.0
- Date: 2026-11-11
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Phase: `M03-P01` (`phase/M03-P01-account-and-session-admin`)
- Approval: G1 and G2 Option A approved by the owner on 2026-11-11.

## Why

Three everyday administrative actions still required terminal workarounds or recreating a session:
1. Account labels created during `Ctrl+N` or `account create` could not be renamed (`account.rename` was missing in host, CLI, HTTP, and the `Ctrl+A` account actions panel).
2. `session.rebind` existed on the host and CLI (`lico session rebind ID ACCOUNT`) to move a disconnected session to another account of the same provider, but had no menu item, shortcut, or picker panel inside `lico gui`.
3. Permission mode (`auto` vs `default`) was chosen once at session creation and could not be switched on an existing session when an administrator wanted to tighten or relax approvals mid-workflow.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Record the `M03` milestone and commit this `M03-P01` plan | Guard checks pass |
| S02 | Add `Accounts::rename` (`accounts.rs`) and a focused `admin.rs` module implementing `account.rename` and `session.mode` (`auto` \| `default`, generation-checked; Codex applies per turn immediately, idle running Claude restarts cleanly with `--permission-mode` and `--resume` while busy/pending-approval Claude refuses with `"busy"`); wire `core::OPS`, `ipc.rs`, `http.rs` (`POST /v1/accounts/<id>/rename`, `POST /v1/sessions/<id>/mode`, `POST /v1/sessions/<id>/rebind`), `cli.rs` (`lico account rename ID LABEL`, `lico session mode ID auto\|default`), and `src/lico_protocol/v1.py` | Rust unit tests and `test_protocol_conformance.py` pass |
| S03 | Expose all three controls in `lico gui`: `R  rename this account` in `Step::Actions` (`Step::AccountRename`), `Session > Rebind account...` (`Ctrl+Shift+B`, `Step::Rebind`), and `Session > Permission mode: ...` (`Ctrl+Shift+M`, `Cmd::ToggleMode`) plus updated `Keys` help | Virtual-display X11 integration tests (`tests/test_admin_controls.py`) pass |
| S04 | Generate manual inspection bundle `00007_M03-P01_account_and_session_admin/`, write the `M03-P01` phase report, update `M03` milestone and `doc/index_4.md`, and run `suite run --changed --receipts` | Changed suite and commit guards pass |

## Personal data and security

- `Accounts::rename` writes `accounts.json` atomically with mode `0600`; credentials and private provider homes remain untouched.
- Switching a session's permission mode is explicit, generation-checked, recorded in the conversation log (`[permission mode: auto]` / `[permission mode: default]`), and persisted in `sessions.json`.
- `session.rebind` continues to refuse running sessions (`"busy"`) and cross-provider accounts (`"unknown_account"`), never silently switching accounts or billing modes.

## Manual inspection

- Local inspection bundle: `private/manual_inspection/202611/20261111/00007_M03-P01_account_and_session_admin/index.html` (account rename flow, session rebind panel, and live permission-mode toggle captures).
