# M13-P01 Plan — Split the Big GUI Tests; Reproduce the Flaky Ones

- Status: implemented (see report)
- Version: 1.1.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M13-P01-test-split-and-flaky` (from `main` at `cc34ab4`)
- Milestone: [M13](../../../milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md)

## Objective

No product behaviour change. Measure per-file and per-test time and peak with the ledger (`suite run --cold --ledger`, `ledger_report`, three cold runs), split the heaviest GUI and host test files by test so that the longest-first order no longer starts the three heaviest files beside the Rust unit-test job, and prove by a by-name count that no test is lost. Reproduce the two flaky tests (`test_first_view` near its 15 s limit, `test_picture_paste` waiting for a note) under controlled load, name the cause, and fix it when it is on the test side. If the cold tree peak does not reach `<= 1,800 MiB` with time not worse than `120 s` by more than a few percent, report the measured best with at least three alternatives and a recommendation, and change neither the runner nor the bound.

## Steps

- [x] S01: Milestone and plan notes (documentation only first commit).
- [x] S02: The "before" ledger, the per-file and per-job peaks (the measurement showed that a split cannot lower the peak).
- [x] S03: On the owner's instruction, instead of a split: the memory of the Rust unit tests reduced with the same thread count (`build_id` hashed in pieces, a test-only gate for font systems and private X servers); no test removed.
- [x] S04: Both flaky tests reproduced under load, the state at failure recorded, the cause named, fixed on the test side.
- [x] S05: Three cold runs; each peak and time reported; report `106`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Per-file time and peak measured, commands in the report | ledger and `ledger_report` output |
| 2 | Same or more tests, none deleted without saying so | the Python count `533` unchanged, Rust `656 + 2` |
| 3 | Cold tree peak `<= 1,800 MiB`, time within a few percent of `120 s`, or measured best with three alternatives | three cold runs |
| 4 | Cause of both flaky tests named; fixed when on the test side | reproduction logs under `private/runs` |
