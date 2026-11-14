# M15 — The Component Foundation: Phase 0 Analysis, Workspace and Rust Tools, the First Component, Release (`lico 0.3.3`)

- Status: P01 done and stopped at its report; waiting for the owner's choice of the first component (G1 approved on 2026-11-14)
- Version: 1.1.0
- Approved: 2026-11-14 (the owner's plan approval in the chat on 2026-11-14 for the milestone and for M15-P01; P02 to P04 are sketches whose plans are shown at their own G2)
- Project: Lico
- Updated: 2026-11-14
- Base: `main` at `38a319c` (M14 accepted, merged and pushed; the component rules 1.0.1 are on `main`); later phases are stacked on the previous phase tip
- Milestone branch: `milestone/M15-*` is created by the lead at the closing commit, not by the implementing session
- Implementation: `phase/M15-P01-phase0-component-analysis` .. `phase/M15-P04-*` (`lico 0.3.3`, proposed)
- Next gate: the owner picks the first component from the P01 report; P02 starts only on the lead's next pointer

## Goal

Lay the foundation of the owner's component rules (`doc/product/202611/20261114/00001_component_rules/component_rules.md`, Version 1.0.1): the code becomes components with a version, explicit downward-only dependencies, a contract and a spec each; a component's own tests are not re-run while its inputs are unchanged (a `RECEIPT.json` next to the component, checkable by a hash); a phase's verification is the sum of what changed and the whole suite runs only at release; the tools and the system tests are Rust; Python is almost eliminated; one executable stays. The milestone changes no behaviour a person can see.

1. P01: measure and decide (read-only, no code moves): inventory, a resolved import graph, test ownership and cost, the Python groups, at least five candidates for the first component with one recommendation, the build order, and the design of the machinery. The owner picks the first component from the report.
2. P02: the Cargo workspace and the Rust tools (manifest check, graph check, hash and receipt tool, per-component test runner, release acceptance).
3. P03: the first component, the owner's pick.
4. P04: release acceptance once and whole, version, install.

## Rules kept

A move and a behaviour change never share a commit; every test that exists today stays green at every commit until a named better one replaces it (a mutation check first); the release binary stays one executable and the two-clone byte-for-byte `suite acceptance` keeps passing; idle RSS `< 50 MiB`; no spec edit without the owner's yes; no push, PR, merge or publication; no guard edit; tests never touch the owner's real services, hosts, sockets, `~/.claude` or `~/.codex`; no billable turn. A cycle that cannot be cut without a behaviour change stops the phase and goes to the owner with three options and a recommendation.

## Done when

1. P01: the Phase 0 report (English, a one-screen owner page at its top, every figure MEASURED, COMPUTED or ASSUMED, the method of each part written so that it can be repeated) covers the seven parts of the plan; nothing is moved, deleted or built beyond scratch analysis in `private/`; a `done` file reports it; the work stops there.
2. P02 to P04: as the sketches in their plan notes, completed at their own plan stage; the final acceptance is three cold runs of everything within `180 s` and `2,048 MiB`, all receipts matching their hashes, idle and hidden-growth bounds, the two-clone acceptance, `lico 0.3.3` installed with the `0.3.2` binary kept as a backup. Numbers only as measured.

## Out of scope

- Any code move in P01; any behaviour change; a spec edit (proposed text goes in a report).
- Using a found Local LLM server as a session (it follows this milestone, as new components under the rules), a lico host on an ssh host, computer use on Linux, Windows, macOS.
- `lico_chat.py` (a private tool outside the repository, a later milestone) and the commit guard in `.git/hooks` (the owner's governance tool).
- The owner's open M14 follow-up (the window still not maximized on the real desktop).
- Push, pull request, merge or publication (owner G4).

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M15-P01 | Phase 0: measure and decide (read-only) | [Plan](../../../plans/202611/20261114/20261114_11_M15-P01_phase0_component_analysis_plan.md) | [Report](../../../reports/202611/20261114/20261114_11_M15-P01_phase0_component_analysis_report.md) | implemented, waiting for the owner's pick |
| M15-P02 | The workspace and the Rust tools | [Plan](../../../plans/202611/20261114/20261114_12_M15-P02_workspace_and_rust_tools_plan.md) | [Report](../../../reports/202611/20261114/20261114_12_M15-P02_workspace_and_rust_tools_report.md) | implemented (the prototypes wait for the owner) |
| M15-P03 | The first component | [Plan](../../../plans/202611/20261114/20261114_13_M15-P03_first_component_plan.md) | pending | sketch |
| M15-P04 | Verification and release | [Plan](../../../plans/202611/20261114/20261114_14_M15-P04_verification_and_release_plan.md) | pending | sketch |
