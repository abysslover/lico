# M09 — Clean Call Control Markers, Collapsible Tool Cards, and Cross-Provider Session Creation (`lico 0.2.5`)

- Status: done (awaiting G3/G4)
- Approved: 2026-11-13 (G1 and G2 approved by the owner on 2026-11-13)
- Project: Lico
- Updated: 2026-11-13
- Milestone branch: `milestone/M09-collapsible-tools-and-cross-provider-create`
- Implementation: `phase/M09-P01-clean-markers-and-collapsible-tools` .. `phase/M09-P03-verification-and-0-2-5` (`lico 0.2.5`)
- Next gate: G3 / G4 after `M09-P03`

## Goal

Polish Lico's live multi-session UX (`0.2.4` -> `0.2.5`) across three owner-requested capabilities while preserving Lico's single-executable architecture and `< 50.0 MiB` idle GUI RSS budget:
(1) strip internal `[call done]` (and line-clipped `[call done` suffixes) from all user-visible session messages, Call Room entries, and conversation transcripts unless `View > Internal events: shown` is active;
(2) fold tool, command, and patch invocations (`[tool] ...`, `$ ...`, `[patch] ...` and their output/error spans) by default in session chat views into a compact 1-line clickable summary pill (`▸ ...`) that expands (`▾ ...`) and collapses on mouse click, rendered with a distinct indigo-violet tool card palette visually separated from normal conversation prose, human prompts, and teal peer banners (`M09-P01`);
(3) extend `mcp__lico__create` so any session (`Claude` or `Codex`) can ask to open a new `Claude` or `Codex` conversation tab (`kind: "claude" | "codex"`, optional `prompt`, optional `name`, optional `scope: "folder" | "global"`), resolving the target provider's configured account automatically regardless of the caller's own provider kind and opening a visible new tab in `lico gui` upon approval (`M09-P02`); and
(4) verify all three capabilities end to end with automated GUI/MCP tests, Xvfb manual-inspection screenshots, full cold suite + memory budget runs, two-clone `suite acceptance`, and standalone `lico 0.2.5` installation to `~/bin/lico` (`M09-P03`).

## Done when

1. `[call done]` (and clipped `[call done` tails) never appear in delivered `[from session ...]` / `[peer reply ...]` messages, Call Room transcripts, or GUI session views (`!show_events`), while still settling Call Room burst state (`room.active = false`). (`M09-P01`)
2. Session chat views fold `[tool]`, `$`, and `[patch]` spans by default into clickable `▸ ...` summary pills (preserving Call Room `[⇄ call ...]` jump cards and Call Room view headers unfolded), toggle expand/collapse (`▾ ...` / `▸ ...`) on stationary mouse click, and render both collapsed pills and expanded header/body rows in a distinct indigo-violet card palette separated from normal conversation prose. (`M09-P01`)
3. `mcp__lico__create` supports `kind` (`"claude"` or `"codex"`, defaulting to caller's provider), optional `prompt` (allowing empty ready tabs as well as seeded first-prompt tabs, with peer-reply routing active immediately on empty created tabs and base-name `@Name` / `to` resolution across `<Name> - made by <Creator>`), optional `name` (`<= 36` plain characters), and optional `scope` (`"folder"` default vs `"global"`), resolving cross-provider accounts safely while keeping person approval, `mode = "default"`, workspace confinement, chain-depth, and `PER_CALLER = 4` / `IN_ALL = 8` caps intact, and opening a new tab in `lico gui` upon `Allow`. (`M09-P02`, `M09-P03`)
4. End-to-end tests (`tests/test_collapsible_tools_and_create_tab.py`), cold full suite (`<= 180.0 s`, `<= 2,048.0 MiB`), scale 1 & scale 2 idle RSS (`<= 50.0 MiB`), 10->100 hidden growth (`<= +10.0 MiB`), Xvfb inspection bundle, and `suite acceptance` pass on `0.2.5`, with `~/bin/lico` installed and awaiting owner G3/G4 approval. (`M09-P03`)

## Why now

Live inspection of `0.2.4` with named `Claude-L` and `Codex-L` sessions confirmed smooth `@SessionName` Call Room round-trips, and surfaced three concrete next improvements:
- An incoming `mcp__lico__send` reply carrying `[call done]` at the end of a long Korean line showed a clipped `[call done` control suffix in chat.
- Always-expanded `[tool]` / `$` output blocks clutter normal conversation flow until the user actually wants to inspect tool details, and need stronger visual separation from normal prose when opened.
- Users naturally ask an open `Claude` or `Codex` session to spin up a new `Claude` or `Codex` conversation tab on their behalf, whereas `mcp__lico__create` previously created `Claude` sessions only and required a non-empty prompt.

## Out of scope

- Bypassing human approval on `mcp__lico__create` or allowing agents to pick arbitrary host filesystem paths (`cwd`) outside the caller's folder or `global` scope.
- Altering raw `.seg` append-only log records for tool executions.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M09-P01 | Strip `[call done]` control markers across send/reply/view paths and add default-collapsed click-to-expand tool/command cards with distinct palette | [Plan](../../plans/202611/20261113/20261113_06_M09-P01_clean_markers_and_collapsible_tools_plan.md) | [Report](../../reports/202611/20261113/20261113_06_M09-P01_clean_markers_and_collapsible_tools_report.md) | `202611/20261113/00004_M09-P03_collapsible_tools_and_create/` | — | done |
| M09-P02 | Cross-provider `mcp__lico__create` (`Claude` <-> `Codex`, optional `prompt`, optional `scope`, new GUI tab) | [Plan](../../plans/202611/20261113/20261113_07_M09-P02_cross_provider_session_create_plan.md) | [Report](../../reports/202611/20261113/20261113_07_M09-P02_cross_provider_session_create_report.md) | `202611/20261113/00004_M09-P03_collapsible_tools_and_create/` | — | done |
| M09-P03 | End-to-end verification, Xvfb inspection bundle, memory budgets, and `lico 0.2.5` release | [Plan](../../plans/202611/20261113/20261113_08_M09-P03_verification_and_0_2_5_plan.md) | [Report](../../reports/202611/20261113/20261113_08_M09-P03_verification_and_0_2_5_report.md) | `202611/20261113/00004_M09-P03_collapsible_tools_and_create/` | — | done (awaiting G3/G4) |
