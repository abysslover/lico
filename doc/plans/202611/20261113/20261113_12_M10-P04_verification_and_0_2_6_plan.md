# M10-P04 Plan — End-to-End Verification, Measurements, and `lico 0.2.6`

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M10-P04-verification-and-0-2-6`
- Milestone: [M10](../../../milestones/202611/20261113/20261113_03_M10_chats_tab_rename_and_resume_milestone.md)

## Objective

End-to-end test under a private Xvfb with mocked providers (no billable turn, no real account), screenshot bundle (three tab colors, modal steps), cold full suite (target `<= 180.0 s`, tree peak `<= 2,048.0 MiB`), idle RSS `< 50.0 MiB`, 10->100 hidden growth `<= +10.0 MiB`, two-clone `suite acceptance`, version bump to `0.2.6`, CHANGELOG, PLAN, roadmap and README, install to `~/bin/lico`. Numbers are recorded only as measured; a missed budget stops the run and a decision with at least three alternatives and a recommendation is returned.

## Steps

- [ ] S01: Bump to `0.2.6` and add the end-to-end test with its `tests/testmap.toml` entry.
- [ ] S02: Release build, screenshots, cold suite, idle and hidden-growth measurements.
- [ ] S03: Docs, report, `suite acceptance`, install, stop at G3 and G4.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Whole flow verified on Xvfb | end-to-end test and screenshots |
| 2 | Budgets met as measured | `lico_measure.suite run --cold`, idle and hidden-growth runs |
| 3 | Two clean clones build identical `lico 0.2.6`; `~/bin/lico` reports `0.2.6` | `lico_measure.suite acceptance`, `lico --version` |
