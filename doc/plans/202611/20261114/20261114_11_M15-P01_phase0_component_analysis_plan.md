# M15-P01 Plan — Phase 0: Measure and Decide (Read-Only)

- Status: implemented (see report); stopped at the report as planned
- Version: 1.1.0
- Approved: 2026-11-14 (G2 for M15-P01 approved by the owner on 2026-11-14)
- Branch: `phase/M15-P01-phase0-component-analysis` (from `main` at `38a319c`)
- Milestone: [M15](../../../milestones/202611/20261114/20261114_04_M15_component_foundation_milestone.md)

## Objective

Write the Phase 0 report (`doc/reports/202611/20261114/20261114_11_M15-P01_phase0_component_analysis_report.md`, English, a one-screen owner page at its top, every figure MEASURED, COMPUTED or ASSUMED, the method and commands of each part written down). Scratch analysis scripts live in `private/phase0/` (not committed). Nothing is moved, deleted or built beyond that. The import graph is read from the source text (no rustc tool is installed and nothing is downloaded): `use` trees and `crate::`, `super::`, `self::` paths resolved to modules, `#[cfg(test)]` blocks and test-only files counted apart, glob or macro-made paths counted as an uncertainty; cycles by strongly connected components, layers, fan-in and fan-out; the smallest edge set that breaks the hubs' cycles computed exactly for at most 20 nodes and by a stated heuristic otherwise. After the report: a `done` file and a stop; the owner picks the first component; P02 starts only on the lead's next pointer.

## Steps

- [x] S01: Documentation-only first commit (milestone note and four plan notes).
- [x] S02: Inventory (Rust modules and `gui` submodules, Python groups, `exp/`, build files).
- [x] S03: The import graph, cycles, layers, hub dependencies, the minimal cycle-breaking edges, external crates per module.
- [x] S04: Test ownership and cost: every Rust test alone (time, tree peak), every Python test file mapped and costed from a cold ledger.
- [x] S05: Python groups, `lico_measure` function by function, `exp/` proposal.
- [x] S06: At least five candidates and one recommendation; the build order of the next 6 to 10 components; the `gui` split; the hub ports.
- [x] S07: The design of the manifest, `SPEC.md`, `RECEIPT.json`, the hash, the isolated run, the system-test list.
- [x] S08: The report, the index and status updates, `done` file `113`, stop.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The module count equals the `mod` lines of `unified.rs` plus the `gui` submodules on disk; the graph's edge count agrees with a plain `crate::` grep within a stated tolerance | spot checks in the report |
| 2 | The Rust test count equals `cargo test -- --list`; the Python test count equals the cold suite's | spot checks in the report |
| 3 | Every part has its method and commands; every figure carries its mark | the report |
| 4 | No code, test, tool or receipt is added; `git status` shows only the report, the notes and the index | `git diff --stat` |
