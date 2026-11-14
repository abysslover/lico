# M15-P02 Plan — The Workspace and the Rust Tools (No Code Moves)

- Status: implemented (see report); one item waits for the owner (the retired prototypes, mail 114)
- Version: 1.1.0
- Approved: 2026-11-14 (the owner approved the M15 plan; the lead's mail 135 settles the decisions after Phase 0)
- Branch: `phase/M15-P02-workspace-and-rust-tools` (stacked on the P01 tip `5f9a991`)
- Milestone: [M15](../../../milestones/202611/20261114/20261114_04_M15_component_foundation_milestone.md)

## Objective

1. First commit(s): delete `exp/` except the record-layout evidence (condition: nothing built, tested or released reads it; the citing documents corrected; a link scan with no dead link). The prototypes (`src/candidates`, `src/host/rust`, `src/host/c`) and their harness are read by tests behind the `legacy_candidates` marker: that item stops and goes to the owner with three alternatives (mail 114).
2. A Cargo workspace at the repository root: the `lico` crate as a member, a tools crate `lico-xtask` (`tools/xtask`), the lock file and the profiles at the root, `.cargo/config.toml` with the `xtask` alias. No code moves; the binary keeps its behaviour; the size and the build time are stated.
3. The tools in Rust, std and `serde_json` only: the module graph (it must reproduce the Phase 0 numbers), the component check (manifest fields, no cycle, only listed dependencies), receipts (a SHA-256 over the inputs, naming what changed), the isolated test run with tree RSS from `/proc` against the budget, and the two-clone acceptance. Red first for the tools themselves.
4. The tools' own tests run in the suite as a job of their own; the test selection knows `tools/xtask/**`.

## Steps

- [x] S01: Repair the 65 dead relative links (the condition says no dead link in the tracked tree); delete `exp/` (72 files) and correct the citing notes; the link scan clean.
- [x] S02: The workspace and a stub tools crate; the release binary compared with the one built before.
- [x] S03: The tools' tests first (11 red against stubs), then the tools; the graph tool compared with the Phase 0 numbers edge by edge.
- [x] S04: The runner job and the selection (red first: `test_selection`), the acceptance run for real (one bug found, test first).
- [x] S05: Documentation, report `115`.
- [ ] S06 (waits for the owner): the retired prototypes, alternative A or B of mail 114.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | No dead relative link in the tracked tree after the deletion | link scan before and after |
| 2 | The graph tool reproduces nodes, edges (with their counts), the cycle of 48 and the layers of Phase 0 | comparison of its JSON with the scratch analysis |
| 3 | A cycle is refused, a changed file invalidates a receipt, an over-budget run is reported, a differing build is refused | `cargo test -p lico-xtask` (12 tests) |
| 4 | Two clean clones build the same bytes with the Rust acceptance | `cargo xtask accept` |
| 5 | The whole suite passes | cold run |
