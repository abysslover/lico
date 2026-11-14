# M14-P03 Plan — End-to-End Verification, Measurements, and `lico 0.3.2`

- Status: planned
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M14-P03-verification-and-0-3-2` (stacked on `phase/M14-P02-maximized-first-start`)
- Milestone: [M14](../../../milestones/202611/20261114/20261114_03_M14_path_status_bar_and_maximized_start_milestone.md)

## Objective

End-to-end window test (the bar and the maximized outcome), screenshots, three cold full-suite runs each reported (`<= 180.0 s`, `<= 2,048.0 MiB`, the margin reached), Rust units under both umasks, idle RSS at scale 1 and 2, hidden growth, the two-clone `suite acceptance`, the tool-list bytes stated, a grep showing `pathbar.rs` has no edge into a hub, version `0.3.2`, CHANGELOG, PLAN, roadmap, README, manual, index, install to `~/bin/lico` with the `0.3.1` binary kept at `private/release_0_3_1_backup/lico` (only `--version` on the installed file). No spec edit.

## Steps

- [ ] S01: Bump to `0.3.2`, the end-to-end test and its `tests/testmap.toml` entry.
- [ ] S02: Release build, screenshots, three cold runs, idle and hidden-growth measurements, acceptance, tool-list bytes.
- [ ] S03: Docs, report `112`, install, stop at G3 and G4.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The whole flow verified on a private Xvfb with fake servers or homes | end-to-end test, screenshots |
| 2 | Budgets met as measured | `lico_measure.suite run --cold`, `lico_measure run` |
| 3 | Two clean clones identical; `~/bin/lico` reports `0.3.2` | `suite acceptance`, `lico --version` |
