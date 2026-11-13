# M09-P02 Plan — Cross-Provider `mcp__lico__create` (`Claude` <-> `Codex`, Optional `prompt`, New GUI Tab)

- Status: done
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M09-P02-cross-provider-session-create`
- Milestone: [M09](../../../milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md)

## Objective

Allow any session (`Claude` or `Codex`) to open a new `Claude` or `Codex` conversation tab on the owner's behalf via `mcp__lico__create`, regardless of the calling session's own provider kind:
1. Split `src/host/hostd/src/create.rs` unit tests into `src/host/hostd/src/create/tests.rs` so `create.rs` stays well below the `800`-line ratchet (`~340` lines).
2. Extend `mcp__lico__create` (`src/host/hostd/src/create.rs` & `src/host/hostd/src/mcp.rs`):
   - `kind`: optional `"claude"` or `"codex"` (case-insensitive; defaults to caller's provider `s.provider`, or `"claude"` when unset). Any other value/type returns `bad_request`.
   - `prompt`: optional string (`<= 100 KiB`). When omitted or `""`, opens an idle ready session tab without sending an initial prompt (and leaves `finished_seq = 0` so peer calls work immediately). When provided, cleans and validates non-empty drawable text as before and delivers it as a chain step.
   - `name`: optional plain string (`<= 36` characters).
   - `scope`: optional `"folder"` (default: caller's `worktree_repo.or(cwd)`) or `"global"` (`cwd: None`, opening a Global `G·` tab). Arbitrary `cwd` paths from the agent remain ignored for workspace confinement.
   - Cross-provider account & model resolution:
     - Same provider (`target_kind == caller_provider`): inherits caller's `account`, `model`, and `effort`.
     - Cross provider (`target_kind != caller_provider`): sets `model = None, effort = None` (provider default) and resolves `account` from `shared.accounts` (`default_for(target_kind)` matching caller's SSH locality first, else first matching account for `target_kind`, else `None`).
   - Preserves human approval on every `create` (`allow` once, `keep` refused), `mode = "default"`, `MAX_HOPS = 4`, and `PER_CALLER = 4` / `IN_ALL = 8` caps.
3. Update `mcp.rs` `TOOLS` schema + `INSTRUCTIONS` (`LIST_BUDGET <= 5,120` B) and update/extend `src/host/hostd/src/create/tests.rs` and `tests/test_tool_create.py` (testing `Claude -> Codex`, `Codex -> Claude`, empty-prompt tab creation, `scope: "global"`, and security invariants).

## Steps

- [x] S01: Extract `src/host/hostd/src/create/tests.rs` and implement cross-provider `kind`, optional `prompt`, and `scope` in `src/host/hostd/src/create.rs` and `src/host/hostd/src/mcp.rs`.
- [x] S02: Update and expand `src/host/hostd/src/create/tests.rs`, `src/host/hostd/src/mcp.rs` tests, and `tests/test_tool_create.py`.
- [x] S03: Run Rust unit tests, `--release` build, `test_tool_create.py`, `test_tool_connection.py`, `test_size_ratchet.py`, and `test_selection.py`, write `M09-P02` report, and commit.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | `Claude` can create `Codex` tabs and `Codex` can create `Claude` tabs (with or without `prompt`, in `"folder"` or `"global"` scope), resolving the target provider's account automatically | `create::tests`, `tests/test_tool_create.py` |
| 2 | Every security invariant holds: person approval required per session, `mode = "default"`, `cwd` / `account` / `model` / `effort` / `from` spoofing ignored, `MAX_HOPS = 4`, `PER_CALLER = 4`, `IN_ALL = 8`, `LIST_BUDGET <= 5,120` B | `create::tests`, `mcp::tests`, `tests/test_tool_create.py` |
| 3 | All touched files satisfy line-count ratchets (`<= 800`) | `tests/test_size_ratchet.py` |
