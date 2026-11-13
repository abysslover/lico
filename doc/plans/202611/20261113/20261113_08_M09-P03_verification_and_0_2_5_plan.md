# M09-P03 Plan — End-to-End GUI & MCP Verification, Xvfb Inspection Bundle, Memory Budgets, and `lico 0.2.5` Release

- Status: done
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M09-P03-verification-and-0-2-5`
- Milestone: [M09](../../../milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md)

## Objective

Complete `M09` (`0.2.4` -> `0.2.5`) by verifying all three owner-requested capabilities together in a live `ProductWindow` on Xvfb, measuring full suite and memory budgets, producing the manual inspection bundle, updating public documentation, verifying two-clone reproducibility (`suite acceptance`), and installing `lico 0.2.5`:
1. Bump version `0.2.4` -> `0.2.5` in `src/host/hostd/Cargo.toml`, `src/host/hostd/Cargo.lock`, `tests/test_control_parity.py`, and `tests/test_single_executable.py`.
2. Add `tests/test_collapsible_tools_and_create_tab.py` (registered in `tests/testmap.toml`) testing on Xvfb:
   - `[call done]` and 201-char clipped `[call done` suffixes hidden in live session & Call Room views while settling Call Room burst state;
   - Default-collapsed `▸ $ ...` and `▸ [tool] ...` invocation pills with distinct indigo-violet background colors, expanding to `▾ ...` + body on click and collapsing back on click;
   - Cross-provider `mcp__lico__create` (`Claude` -> `Codex` empty tab and `Codex` -> `Claude` Global tab with initial prompt) surfacing centered approval modals and opening visible new tabs in `lico gui`.
3. Capture Xvfb screenshots + `index.html` inspection bundle under `private/manual_inspection/202611/20261113/00004_M09-P03_collapsible_tools_and_create/`, run cold full suite (`<= 180.0 s`, `<= 2,048.0 MiB`) and idle/hidden-growth memory budgets (`<= 50.0 MiB`, `<= +10.0 MiB`), update `PLAN.md`, `README.md`, `doc/index_4.md`, `roadmap.md`, `doc/manual_end_to_end.md`, and `M09` milestone/report, run `suite acceptance`, and install `~/bin/lico` (`0.2.5`).

## Steps

- [x] S01: Bump `lico` to `0.2.5` and add `tests/test_collapsible_tools_and_create_tab.py` + `tests/testmap.toml` entry.
- [x] S02: Build `--release`, capture Xvfb inspection screenshots (`00004_M09-P03_collapsible_tools_and_create`), and run cold suite + idle/hidden-growth memory measurements.
- [x] S03: Update status/docs/report, commit cleanly on `phase/M09-P03-verification-and-0-2-5`, run `suite acceptance`, install `~/bin/lico` (`0.2.5`), and stop at G3/G4.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Clean `[call done]` stripping, default-collapsed click-to-expand indigo-violet tool cards, and cross-provider `create` tabs verified end to end on Xvfb | `tests/test_collapsible_tools_and_create_tab.py` + inspection bundle `00004` |
| 2 | Cold full suite (`<= 180.0 s`, `<= 2,048.0 MiB`), scale 1 & 2 idle RSS (`<= 50.0 MiB`), and 10->100 hidden growth (`<= +10.0 MiB`) all pass | `lico_measure.suite run --cold`, `idle_memory`, `hidden_growth` |
| 3 | Two clean clones build identical `lico 0.2.5` binaries and `~/bin/lico` reports `0.2.5` | `lico_measure.suite acceptance`, `~/bin/lico --version` |
