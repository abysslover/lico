# M04-P03 Plan: Provider weekly quota telemetry (`7d` remaining) and working-directory context display

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Phase: `M04-P03` (`phase/M04-P03-quota-and-context-bar`)
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

Managing multiple `claude` and `codex` accounts in `lico gui` requires knowing how much weekly (`7d`) allowance remains on the bound account alongside the session's active working directory, without guessing or fabricating numbers when a provider has not reported telemetry. Both official provider streams expose structured rate-limit snapshots (`codex` `account/rateLimits/read` and `account/rateLimits/updated`; `claude` `rate_limit_event` `seven_day` / `five_hour`), while `account.list` previously left `usage` hardcoded to `null`.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M04-P03` phase plan and link it in the `M04` milestone and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Add `src/host/hostd/src/quota.rs` storing per-account rate-limit windows (`weekly_remaining_pct`, `weekly_resets_at`, `short_remaining_pct`, `short_resets_at`, `updated_at`), returning `null` when unreported; wire `claude.rs` (`rate_limit_event`), `codex.rs` (`account/rateLimits/read` on thread ready and turn completion + `account/rateLimits/updated` notifications), `core.rs` (`account.list` `usage`), `folders.rs` (`status` `account_usage`), `src/lico_protocol/v1.py`, and `"ratelimit:..."` modes in both provider simulators | Rust unit tests + `test_account_lifecycle.py`, `test_claude_adapter.py`, `test_codex_adapter.py`, and `test_protocol_conformance.py` pass |
| S03 | Surface `7d XX% left` (with low-allowance markers below `20%` / `5%`) alongside working directory (`dir: ...`) in the `lico gui` session context header, `Accounts` list, and `Account` actions view, moving header formatting into `row.rs` so `ui.rs` stays well below `1,120` lines | `tests/test_quota_display.py`, `tests/test_first_view.py`, and `tests/test_size_ratchet.py` pass |
| S04 | Capture manual inspection bundle `00003_M04-P03_quota_and_context_bar/`, write the `M04-P03` report, update status tables, and commit | Changed suite and pre-commit guards pass |

## Personal data and security

Only bounded numeric quota percentages and Unix reset timestamps are stored in memory per local account ID; no raw tokens, headers, or account emails are logged or persisted. Accounts that have not reported rate-limit telemetry continue to report `usage: null`.
