# M15 — The Component Foundation: Phase 0 Analysis, Workspace and Rust Tools, the First Component, Release (`lico 0.3.3`)

- Status: done (accepted at G3, merged and published on `main` on 2026-11-17; the retired prototypes are left for the next milestone)
- Version: 1.1.1
- Approved: 2026-11-14 (the owner's plan approval in the chat on 2026-11-14 for the milestone and for M15-P01; P02 to P04 are sketches whose plans are shown at their own G2)
- Project: Lico
- Updated: 2026-11-14
- Base: `main` at `38a319c` (M14 accepted, merged and pushed; the component rules 1.0.1 are on `main`); later phases are stacked on the previous phase tip
- Milestone branch: `milestone/M15-component-foundation` (created at the closing commit)
- Implementation: `phase/M15-P01-phase0-component-analysis` .. `phase/M15-P04-*` (`lico 0.3.3`, proposed)
- Next gate: closed (accepted at G3, merged and published on `main` at G4 on 2026-11-17)

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
| M15-P01 | Phase 0: measure and decide (read-only) | [Plan](../../../plans/202611/20261114/20261114_11_M15-P01_phase0_component_analysis_plan.md) | [Report](../../../reports/202611/20261114/20261114_11_M15-P01_phase0_component_analysis_report.md) | accepted & merged |
| M15-P02 | The workspace and the Rust tools | [Plan](../../../plans/202611/20261114/20261114_12_M15-P02_workspace_and_rust_tools_plan.md) | [Report](../../../reports/202611/20261114/20261114_12_M15-P02_workspace_and_rust_tools_report.md) | accepted & merged |
| M15-P03 | The first component | [Plan](../../../plans/202611/20261114/20261114_13_M15-P03_first_component_plan.md) | [Report](../../../reports/202611/20261114/20261114_13_M15-P03_first_component_png_report.md) | accepted & merged |
| M15-P04 | Verification and release | [Plan](../../../plans/202611/20261114/20261114_14_M15-P04_verification_and_release_plan.md) | [Report](../../../reports/202611/20261114/20261114_14_M15-P04_verification_and_0_3_3_report.md) | accepted & merged |

## Gate record

- G3, G4 and the push for this milestone were given by the owner in the chat on 2026-11-17: the owner said that, if the question was whether the merge and the rest may be done, they may, and that the work should go on from there to the next milestone and the Lico development that had stood still. The chain moved to `main` by fast-forward to the M15-P04 tip `53dcd1a`, and this closing commit then recorded the acceptance. The owner named no individual tip.
- What the owner had observed before giving it: nothing is recorded as inspected. The interface specification of `lico-png` was approved by the owner on the implementing session's screen on 2026-11-14 before P03 began (the P03 report). The measurements in the phase reports were not independently reproduced at the gate.
- Left for the next milestone: the retired prototypes (`src/candidates`, `src/host/rust`, `src/host/c`, `lico_measure.legacy` and the rows that use them). The owner answered A (delete them, every product row stays) on 2026-11-14 and again on 2026-11-17; the answer did not reach the implementing session during P02 to P04 (a line typed while a turn runs is dropped by its front-end), so the deletion was not made; it becomes the first phase of the next milestone. No product behaviour is involved.
- Notes carried over: the release binary is 9,781,232 bytes, +168,352 (+1.75 %) over `0.3.2`, measured to come from splitting `png` into a crate (the mechanism is not proven; a build-profile decision may be wanted if later components add the same kind of growth); the Python `suite acceptance` stays next to `cargo xtask accept` and is proposed for retirement after one more milestone in which both agree; the maximized first start (M14) stays open and is scheduled after the Local LLM session.
