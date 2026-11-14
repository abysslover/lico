# M13-P04 Plan — End-to-End Verification, Measurements, and `lico 0.3.1`

- Status: implemented (see report)
- Version: 1.1.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M13-P04-verification-and-0-3-1` (stacked on `phase/M13-P03-chat-list-polish`)
- Milestone: [M13](../../../milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md)

## Objective

End-to-end on fakes (the dialog with cache, refresh, fold and long ssh rows; the chat list), screenshots, three cold full suite runs each reported (`<= 180.0 s`, `<= 2,048.0 MiB`, and the margin reached), Rust units under both umasks, idle RSS (scale 1 and 2), hidden growth, two-clone `suite acceptance`, the tool-list byte budget measured and stated, version `0.3.1`, CHANGELOG, PLAN, roadmap, README, manual, index, install to `~/bin/lico` with the `0.3.0` binary kept at `private/release_0_3_0_backup/lico`. No spec edit. The end-to-end test covers the discovery dialog (the chat list has its own window test from P03).

## Steps

- [x] S01: Bump to `0.3.1`, the end-to-end test and its `tests/testmap.toml` entry.
- [x] S02: Release build, screenshots, three cold runs, idle and hidden-growth measurements, acceptance.
- [x] S03: Docs, report `109`, install, stop at G3 and G4.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The whole flow verified on a private Xvfb with fake servers and fake ssh | end-to-end test and screenshots |
| 2 | Budgets met as measured | `lico_measure.suite run --cold`, `lico_measure run` |
| 3 | Two clean clones identical; `~/bin/lico` reports `0.3.1` | `suite acceptance`, `lico --version` |
