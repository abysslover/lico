# M15-P02 Plan — The Workspace and the Rust Tools (Sketch)

- Status: sketch (completed at its own G2, after the P01 report and the lead's next pointer)
- Version: 1.0.0
- Approved: not yet (G2 for this phase is asked when the plan is completed)
- Branch: `phase/M15-P02-*` (stacked on the P01 tip)
- Milestone: [M15](../../../milestones/202611/20261114/20261114_04_M15_component_foundation_milestone.md)

## Objective (sketch)

One Cargo workspace with the existing crate as a member and a tools crate (the rules name it `lico-xtask`) holding the manifest check, the graph check (no cycle, only downward edges, only listed dependencies), the hash and receipt tool, the per-component test runner with time and peak memory against the budget (`256 MiB` and `30 s` unless the manifest says otherwise), and the release acceptance (the two-clone byte-for-byte build ported from `suite acceptance`; if the port is too big, the Python one stays until P04 with a dated reason). No code moves; the binary keeps its behaviour. Red first for the tools themselves (a graph with a cycle is refused, a changed file invalidates a receipt, an over-budget run is reported). The change in release binary size and build time is stated; `Cargo.lock` stays committed.

## Steps

- [ ] S01 to S0n: to be written from the P01 report.

## Acceptance Criteria

To be written with the plan; the sketch's red-first cases above are the minimum.
