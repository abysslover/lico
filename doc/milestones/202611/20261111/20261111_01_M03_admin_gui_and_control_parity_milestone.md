# M03 — Linux administrator GUI completeness and control-surface parity

- Status: done
- Approved: 2026-11-11 (G1 and G2 Option A approved by the owner)
- Project: Lico
- Updated: 2026-11-12
- Implementation: M03-P01 to M03-P04 built, verified, accepted by the owner at G3 (2026-11-12), and merged and published on `main` at G4 (`lico 0.1.0`, 2026-11-12)
- Next gate: None (milestone achieved and closed on 2026-11-12)
- Milestone branch: `milestone/M03-admin-gui-and-control-parity` (created at the closing commit of M03 on `main`; the phase branches are kept)

## Goal

Ensure the Linux administrator window (`lico gui`) exposes every everyday
account, session, workspace-navigation, font-sizing, and prompt-editing control
directly to the human administrator while keeping the command line (`lico`),
local socket (`ipc`), and loopback HTTP listener (`127.0.0.1`) in strict
machine-discoverable parity for agents and scripts — all inside the unchanged
single-executable footprint and `< 50.0 MiB` idle GUI RSS budget.

## Done when

1. An administrator can rename an account (`account.rename`, `Ctrl+A` -> `1..9`
   -> `R`), rebind a disconnected session to another account of the same
   provider inside the window (`Session > Rebind account...`, `Ctrl+Shift+B`),
   and toggle an open session's permission mode (`auto` <-> `default`) from the
   window (`Session > Permission mode: ...`, `Ctrl+Shift+M`), CLI, and loopback
   HTTP without losing the conversation. (`M03-P01`)
2. The session list can be grouped persistently either by account (`"account"`,
   default) or by workspace folder/repository (`"workspace"`, rolling git
   worktree sessions up under their parent repository), and the window text size
   can be adjusted persistently (`12..=20` px, default `14` px) from the `View`
   menu (`Ctrl+Shift+G`, `Ctrl+=`, `Ctrl+-`, `Ctrl+0`), CLI (`lico settings`),
   and HTTP while keeping idle GUI RSS `< 50.0 MiB` at scales 1 and 2. (`M03-P02`)
3. `src/host/hostd/src/gui/editor.rs` is split cleanly below the 800-line size
   ratchet, and the prompt editor supports mouse click/drag caret placement and
   selection, `Shift` + movement keyboard selection, `Ctrl+C` / `Ctrl+X` on an
   active editor selection (preserving `Ctrl+A` for `Accounts`), bounded
   per-session undo/redo (`Ctrl+Z` / `Ctrl+Y`), and stable scroll anchoring on
   window resize/reflow. (`M03-P03`)
4. A canonical `capabilities` operation (`lico capabilities`, `GET /v1/capabilities`,
   and socket `capabilities`) reports the protocol version, full operation
   inventory, loopback HTTP routes, MCP agent tools, and limits; loopback HTTP
   exposes complete non-streaming parity (`POST /v1` plus REST routes and cursor
   gap recovery); `suite run --cold`, `suite acceptance`, `idle_memory` (`< 50.0 MiB`),
   and `hidden_growth` (`<= +10.0 MiB`) all pass at `lico 0.1.0`; and the
   reviewed standalone release binary is installed atomically to the local user
   `bin/lico` path. (`M03-P04`)

## Why now

With `M01` (measured Linux foundation and `0.1.0` clean-clone acceptance pack)
and `M02` (agentic-coding basics) built, a handful of everyday administrator
controls still forced a person back to the terminal (`session rebind`, changing
permission mode after creation, renaming an account label) or lacked desktop
ergonomics (grouping sessions by workspace repository, adjusting text size,
mouse/keyboard selection and undo in the multi-line prompt box, and a
single machine-readable `capabilities` inventory across CLI and loopback HTTP).
Completing this Linux parity pass before starting the separate Windows backend
milestone fulfills Lico's core design principle: one lightweight executable
where the GUI serves the human administrator and the CLI/HTTP/MCP channels
serve agents and scripts.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M03-P01 | In-window account rename, session rebind, and live permission-mode control | [Plan](../../../plans/202611/20261111/20261111_05_M03-P01_account_and_session_admin_plan.md) | [Report](../../../reports/202611/20261111/20261111_06_M03-P01_account_and_session_admin_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |
| M03-P02 | Persistent workspace-grouped session navigator and adjustable text size | [Plan](../../../plans/202611/20261111/20261111_06_M03-P02_workspace_navigator_plan.md) | [Report](../../../reports/202611/20261111/20261111_07_M03-P02_workspace_navigator_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |
| M03-P03 | Prompt editor modularization, mouse/keyboard selection, undo/redo, and resize reflow | [Plan](../../../plans/202611/20261111/20261111_07_M03-P03_editor_and_reflow_plan.md) | [Report](../../../reports/202611/20261111/20261111_08_M03-P03_editor_and_reflow_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |
| M03-P04 | Control-surface capabilities parity, loopback HTTP completion, full memory pass, and binary install | [Plan](../../../plans/202611/20261111/20261111_08_M03-P04_control_parity_and_review_plan.md) | [Report](../../../reports/202611/20261111/20261111_09_M03-P04_control_parity_and_review_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-11-11 | Scope and phases M03-P01 to M03-P04 |
| G2 phase plan approval | Approved 2026-11-11 | Option A approved across M03-P01 to M03-P04 |
| G3 inspection acceptance | Approved 2026-11-12 | Owner accepted M03-P01 to M03-P04 and bundles `00007`..`00010` |
| G4 merge approval | Approved 2026-11-12 | Fast-forward merge into `main`, milestone branch `milestone/M03-admin-gui-and-control-parity`, and publication to `origin` |

## Blocked on

None. M03 achieved and closed on 2026-11-12.

## When done

Closed on 2026-11-12 at branch `milestone/M03-admin-gui-and-control-parity` (`lico 0.1.0`):
- **Delivered (`4/4` M03 phases):** in-window account rename, session rebind, and live `auto`/`default` permission-mode toggle (`M03-P01`); persistent workspace-grouped session navigation rolling git worktrees under parent repos and `12..=20` px font scaling (`M03-P02`); modularized `gui/editor` (`<= 703` lines, `ui.rs` `1,115` lines) with mouse/keyboard selection, `Ctrl+C`/`Ctrl+X`, undo/redo, and resize reflow (`M03-P03`); and `capabilities` + loopback HTTP parity (`POST /v1` and REST routes) verified within `< 50.0 MiB` idle RSS (`24.21` / `35.38 MiB`), `+0.37 MiB` hidden growth, `94.9 s` cold 458-test suite, and two-clone reproducible `0.1.0` build (`M03-P04`).
