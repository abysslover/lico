# M11-P03 Plan — End-to-End Verification, Measurements, and `lico 0.2.7`

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M11-P03-verification-and-0-2-7`
- Milestone: [M11](../../../milestones/202611/20261113/20261113_04_M11_native_provider_home_chats_milestone.md)

## Objective

End-to-end tests on a private Xvfb and a private socket with synthetic homes and mocked providers (no billable turn): the owner's case (a live Claude process in a project folder is listed `running elsewhere`, its Resume is refused, New is allowed, a closed chat of the same folder resumes), the first-run prompt, Native in the account list, the hint. A screenshot bundle for inspection. Cold full suite (`<= 180.0 s`, `<= 2,048.0 MiB`; each run reported), idle RSS (scale 1 and 2), hidden growth, two-clone `suite acceptance`, bump to `0.2.7`, CHANGELOG, PLAN, roadmap, README, manual, index, install to `~/bin/lico` with the `0.2.6` binary kept as a backup. Also: repeat full runs and say whether M11 changes the picture of the four GUI or tool tests that failed once each in M10.

## Steps

- [ ] S01: Bump to `0.2.7`, the end-to-end test and its `tests/testmap.toml` entry.
- [ ] S02: Release build, screenshots, cold runs, idle and hidden-growth measurements, acceptance.
- [ ] S03: Docs, report, install, stop at G3 and G4.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The whole flow verified on a private Xvfb with synthetic homes | end-to-end test and screenshots |
| 2 | Budgets met as measured | `lico_measure.suite run --cold`, `lico_measure run` |
| 3 | Two clean clones identical; `~/bin/lico` reports `0.2.7` | `suite acceptance`, `lico --version` |
