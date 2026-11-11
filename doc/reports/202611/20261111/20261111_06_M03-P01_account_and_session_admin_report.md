# M03-P01 report — in-window account rename, session rebind, and live permission-mode control

- Version: 1.0.0
- Status: in_review (awaiting owner G3 inspection and G4 merge)
- Date: 2026-11-11
- Plan: [M03-P01](../../../plans/202611/20261111/20261111_05_M03-P01_account_and_session_admin_plan.md) (version 1.0.0, G2 approved by the owner on 2026-11-11)
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Branch: `phase/M03-P01-account-and-session-admin` (branched from `phase/M01-P08-acceptance-pack` `b73ecc4`)
- Inspection bundle: `private/manual_inspection/202611/20261111/00007_M03-P01_account_and_session_admin/`

## What was done

1. **S02 — Host operations (`account.rename`, `session.mode`) and CLI / socket / HTTP parity:**
   - Added `Accounts::rename` (`src/host/hostd/src/accounts.rs`) with atomic `0600` persistence, preserving account IDs, provider bindings, default account pointers, and private provider homes.
   - Added `src/host/hostd/src/admin.rs` implementing `account_rename` and `session_mode` (`"auto"` | `"default"`, generation-checked, recorded in the conversation log as `[permission mode: auto]` / `[permission mode: default]` and persisted in `sessions.json`):
     - On Codex sessions, `s.permission_mode` updates immediately and subsequent turns emit `"approvalPolicy": "never"` (`auto`) or `"on-request"` (`default`) without dropping the thread.
     - On Claude Code sessions, busy turns or pending approvals refuse cleanly with `"busy"`; idle running sessions restart cleanly via `claude::restart_after_change` with `--permission-mode bypassPermissions` (`auto`) or `default` plus `--resume <thread>` when a conversation thread exists.
   - Wired `core::OPS` (kept `src/host/hostd/src/core.rs` at `988` lines), `ipc.rs`, `src/lico_protocol/v1.py`, `cli.rs` (`lico account rename ID LABEL`, `lico session mode ID auto|default`), and `http.rs` (`POST /v1/accounts/<id>/rename`, `POST /v1/sessions/<id>/mode`, `POST /v1/sessions/<id>/rebind`).
2. **S03 — Administrator GUI panels, `Session` menu items, and shortcuts:**
   - Added `src/host/hostd/src/gui/admin_ui.rs` and wired `src/host/hostd/src/gui/panel.rs` + `src/host/hostd/src/gui/menu.rs`:
     - `Ctrl+A` -> `1..9` -> `R` (`Step::AccountRename`) renames an account label directly inside `lico gui` and refreshes sidebar headings immediately.
     - `Session > Permission mode: auto` / `Permission mode: approval` (`Ctrl+Shift+M`, `Cmd::ToggleMode`) toggles the active session's permission mode in place and greys out with a clear reason (`"no session is open"` / `"session is busy"` / `"answer pending approval first"`) when unavailable.
     - `Session > Rebind account...` (`Ctrl+Shift+B`, `Cmd::Rebind`, `Step::Rebind`) opens an in-window picker of alternate accounts of the same provider when a session is disconnected, with explicit disabled reasons (`"stop session first"`, `"no other account for provider"`).
     - Updated `Help > Keyboard shortcuts` (`F1`) to list `Ctrl+Shift+B` and `Ctrl+Shift+M`, keeping open `Session` menu bounds within `1200 x 800` at both scale `1` and scale `2`.
3. **S04 — Automated verification and manual inspection bundle:**
   - Added Rust unit test `admin::tests::renaming_an_account_preserves_id_provider_default_and_private_permissions` and X11/CLI/HTTP/IPC integration suite `tests/test_admin_controls.py` (`4` end-to-end tests covering account rename, Claude busy refusal + idle respawn with `--resume`, Codex per-turn `approvalPolicy`, and disconnected session rebind across IPC, CLI, HTTP, and Xvfb GUI).
   - Generated `private/manual_inspection/202611/20261111/00007_M03-P01_account_and_session_admin/` (`index.html` + 3 PNG captures).

## Verification results

| Check | Result |
|---|---|
| `cargo test --bin lico -- admin::` | `1/1` passed |
| `pytest tests/test_admin_controls.py tests/test_menu_bar.py tests/test_protocol_conformance.py tests/test_selection.py tests/test_size_ratchet.py tests/test_no_hidden_characters.py` | `47/47` passed (`0` failed) |
| Changed-suite gate (`pre-commit-suite` / `suite run --changed --receipts`) | Verified on commit |
| Manual inspection bundle | `00007_M03-P01_account_and_session_admin/` (`p01_1_account_rename.png`, `p01_2_permission_mode_menu.png`, `p01_3_session_rebind_panel.png`) |
