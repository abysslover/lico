# M09-P02 Report — Cross-Provider `mcp__lico__create` (`Claude` <-> `Codex`, Optional `prompt`, New GUI Tab)

- Status: done
- Date: 2026-11-13
- Branch: `phase/M09-P02-cross-provider-session-create`
- Milestone: [M09 — Clean Call Control Markers, Collapsible Tool Cards, and Cross-Provider Session Creation (`lico 0.2.5`)](../../../milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md)
- Plan: [M09-P02 plan](../../../plans/202611/20261113/20261113_07_M09-P02_cross_provider_session_create_plan.md)

## Summary

`M09-P02` extends `mcp__lico__create` so any session (`Claude` or `Codex`) can ask to open a new `Claude` or `Codex` conversation tab on the owner's behalf, regardless of the caller session's own provider kind:

1. **Modularization under the `800`-line ratchet**:
   - Extracted `create::tests` into `src/host/hostd/src/create/tests.rs` (`391` lines), reducing `src/host/hostd/src/create.rs` from `796` lines to `366` lines.
2. **Cross-provider `kind`, optional `prompt`, and `scope` (`src/host/hostd/src/create.rs`, `src/host/hostd/src/mcp.rs`)**:
   - `kind`: optional `"claude"` or `"codex"` (case-insensitive; defaults to the caller's own provider). Any other value or non-string type fails closed with `bad_request`.
   - `prompt`: optional (`<= 100 KiB`). Omitting `prompt` or passing `""` opens a ready idle session tab (`opens an empty ready tab` on the approval card, no initial prompt delivered, `incoming_hops = hops + 1` preserved); non-empty strings are sanitized and validated (`!shows_nothing`) and delivered as a chain step.
   - `scope`: optional `"folder"` (default: caller's `worktree_repo.or(cwd)`) or `"global"` (`cwd: None`, opening a `G·` Global tab). Arbitrary `cwd`, `mode`, `account`, `model`, `effort`, and `from` arguments from the agent remain ignored (`mode` is always `"default"`).
   - **Account and model resolution**: same-provider creation inherits the caller's `account`, `model`, and `effort`; cross-provider creation (`Claude` -> `Codex` or `Codex` -> `Claude`) clears `model`/`effort` to the target provider's defaults and resolves the target provider's default or matching account (preferring matching SSH host locality).
   - Updated `mcp.rs` `TOOLS` schema (keeping `tools/list` within `LIST_BUDGET <= 5,120` B) and `initialize` `INSTRUCTIONS` item `(4)`.
3. **Tests (`src/host/hostd/src/create/tests.rs`, `tests/test_tool_create.py`)**:
   - Added unit and dual-simulator (`Claude` + `Codex`) integration tests verifying `Claude -> Codex` empty-tab creation, `Codex -> Claude` Global-scope prompt creation, same-kind Codex inheritance, and spoofing/cap/chain-depth invariants; also tightened `program_of` in `tests/test_tool_create.py` to wait for `CWD ` flush after `ARGV `.

## Measured Verification

| Check | Result | Budget / Target | Verdict |
|---|---|---|---|
| `cargo test --manifest-path src/host/hostd/Cargo.toml -- --test-threads=4` | `608 passed; 0 failed` (`27.08 s`) | `0` failures | Met |
| Scoped Python integration & guard tests (`test_tool_create.py`, `test_tool_connection.py`, `test_card_window.py`, `test_size_ratchet.py`, `test_selection.py`, `test_no_hidden_characters.py`) | `70 passed; 0 failed` | `0` failures | Met |
| Line-count ratchets (`create.rs`, `create/tests.rs`, `mcp.rs`, `test_tool_create.py`) | `366`, `391`, `430`, `432` lines | `<= 800` lines | Met |
