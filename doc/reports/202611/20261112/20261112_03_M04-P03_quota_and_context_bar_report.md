# M04-P03 report — provider weekly quota telemetry (`7d` remaining) and working-directory context display

- Version: 1.0.0
- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Date: 2026-11-12
- Plan: [M04-P03](../../../plans/202611/20261112/20261112_03_M04-P03_quota_and_context_bar_plan.md) (version 1.0.0, G2 approved by the owner on 2026-11-12)
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Branch: `phase/M04-P03-quota-and-context-bar` (branched from `phase/M04-P02-clean-conversation-view` `c2da53f`)
- Inspection bundle: `private/manual_inspection/202611/20261112/00003_M04-P03_quota_and_context_bar/`

## What was done

1. **S02 — Provider weekly (`7d`) and short-window (`5h`) rate-limit capture across `claude` and `codex`:**
   - Created `src/host/hostd/src/quota.rs` (`267` lines) tracking per-account rate-limit snapshots (`weekly_remaining_pct`, `weekly_resets_at`, `short_remaining_pct`, `short_resets_at`, `updated_at`), merging sparse rolling updates without clearing previously observed windows, forgetting telemetry when an account is removed (`accounts.rs`), and returning `Value::Null` whenever no telemetry has been reported (never fabricating `0%` or `100%`).
   - Wired `claude.rs` (`rate_limit_event`, supporting both flat `rateLimitType: "seven_day*" | "five_hour*"` and nested window objects), `codex.rs` (`account/rateLimits/read` on thread ready and turn completion + `account/rateLimits/updated` notifications), `core.rs` (`account.list` `usage`, keeping `core.rs` at `988` lines), `folders.rs` (`status` `account_usage`), `src/lico_protocol/v1.py`, and `"ratelimit:..."` modes in both provider simulators.
2. **S03 — GUI weekly allowance & working-directory context display:**
   - Updated `src/host/hostd/src/gui/row.rs` (`header_line`, `format_quota_badge`, `format_quota_detail`, `format_quota_list_tag`), `attn.rs`, `panel.rs`, and `ui.rs` (`1,104` lines `<= 1,120` allowlist) so `lico gui` displays `dir: <workspace>` and `7d XX% left` (`7d LOW XX% left` below `20%`, `7d CRITICAL XX% left` below `5%`) in the session header, `7d XX%` / `7d XX%!` in left-sidebar account headings, and `[7d XX% left · 5h YY% left]` in `Accounts` and `Account` actions views.
3. **S04 — Automated verification and manual inspection bundle:**
   - Added Rust unit tests in `quota.rs` and `row.rs`, added end-to-end GUI + protocol test `tests/test_quota_display.py`, and captured 2 real Xvfb screenshots + `index.html` in `private/manual_inspection/202611/20261112/00003_M04-P03_quota_and_context_bar/`.

## Verification results

| Check / Metric | Result | Budget / Guard |
|---|---|---|
| `cargo test --bin lico` | `576` passed (`1` ignored) | Zero failures |
| `pytest tests/test_size_ratchet.py tests/test_no_hidden_characters.py tests/test_selection.py tests/test_account_lifecycle.py tests/test_claude_adapter.py tests/test_codex_adapter.py tests/test_protocol_conformance.py tests/test_first_view.py tests/test_workspace_navigator.py tests/test_quota_display.py` | `69/69` passed (`36.06 s`) | Zero failures |
| `src/host/hostd/src/gui/ui.rs` / `core.rs` / `quota.rs` line counts | `1,104` / `988` / `267` lines | `<= 1,120` / `<= 988` / `<= 800` |
