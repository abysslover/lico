# M13-P01 Plan — Split the Big GUI Tests; Reproduce the Flaky Ones

- Status: planned
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M13-P01-test-split-and-flaky` (from `main` at `cc34ab4`)
- Milestone: [M13](../../../milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md)

## Objective

No product behaviour change. Measure per-file and per-test time and peak with the ledger (`suite run --cold --ledger`, `ledger_report`, three cold runs), split the heaviest GUI and host test files by test so that the longest-first order no longer starts the three heaviest files beside the Rust unit-test job, and prove by a by-name count that no test is lost. Reproduce the two flaky tests (`test_first_view` near its 15 s limit, `test_picture_paste` waiting for a note) under controlled load, name the cause, and fix it when it is on the test side. If the cold tree peak does not reach `<= 1,800 MiB` with time not worse than `120 s` by more than a few percent, report the measured best with at least three alternatives and a recommendation, and change neither the runner nor the bound.

## Steps

- [ ] S01: Milestone and plan notes (documentation only first commit).
- [ ] S02: The by-name test-count guard, run against the unsplit tree; the "before" ledger and three cold runs.
- [ ] S03: Split the files the ledger names; shared fixtures in one helper module per family; `tests/testmap.toml` carried over file by file; the guard green.
- [ ] S04: Reproduce the two flaky tests under load, record the state at failure, name the cause, fix on the test side.
- [ ] S05: Three cold runs after the split; report each peak and time; report `106`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Per-file time and peak measured, commands in the report | ledger and `ledger_report` output |
| 2 | Same or more tests, none deleted without saying so | by-name test-count guard |
| 3 | Cold tree peak `<= 1,800 MiB`, time within a few percent of `120 s`, or measured best with three alternatives | three cold runs |
| 4 | Cause of both flaky tests named; fixed when on the test side | reproduction logs under `private/runs` |
