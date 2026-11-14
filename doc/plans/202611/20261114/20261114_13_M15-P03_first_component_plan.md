# M15-P03 Plan — The First Component (Sketch)

- Status: sketch (completed after the owner has picked the component from the P01 report)
- Version: 1.0.0
- Approved: not yet (the interface specification is shown to the owner first; that approval is G2 for this phase)
- Branch: `phase/M15-P03-*` (stacked on the P02 tip)
- Milestone: [M15](../../../milestones/202611/20261114/20261114_04_M15_component_foundation_milestone.md)

## Decisions of the owner after Phase 0 (2026-11-14)

The first component is `png` (`dbus` is the second component, after M15). The interface specification of `png` is shown to the owner on the screen first (each line SEEN IN CODE or PROPOSED); that approval is G2 of this phase and the phase stops there until it is given. System tests are a short named list in one place with their own measured peak and time (512 MiB and 60 s per test); a test that is not on the list gets the component default (256 MiB, 30 s); no test is moved onto the list to make it pass.

## Objective (sketch)

The component the owner picks. The plan has the interface specification FIRST (purpose, public API, invariants and errors, dependencies and direction, versioning rule, contract tests, pattern used; each line marked SEEN IN CODE or PROPOSED). Then: characterization tests that pin the present; small green commits with the smallest fitting pattern (port, facade, adapter, strategy); the component detached as its own crate with `SPEC.md`, its tests and `RECEIPT.json`; the code that used it depends on the named version and keeps only a thin test of its own use. A Python test that its Rust contract tests replace is retired only after a mutation check (the same breakage fails the Rust test where it failed the Python test). Measured and shown to the owner: the cost of the component's own tests (count, time, MiB) and how many tests and MiB a typical change to a consumer now needs against before.

## Steps

- [ ] To be written after the pick.

## Acceptance Criteria

To be written with the plan.
