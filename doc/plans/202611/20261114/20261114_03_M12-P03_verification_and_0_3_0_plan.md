# M12-P03 Plan — End-to-End Verification, Measurements, and `lico 0.3.0`

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M12-P03-verification-and-0-3-0`
- Milestone: [M12](../../../milestones/202611/20261114/20261114_01_M12_local_llm_endpoint_discovery_milestone.md)

## Objective

End-to-end tests on fake servers and the fake ssh rig (a private socket and a private Xvfb; no real host, no real service), screenshots for inspection (the dialog before a scan, after a local scan, after an ssh scan, the failure lines), cold full suite (each run reported; `<= 180.0 s`, `<= 2,048.0 MiB`), Rust units under both umasks, idle RSS (scale 1 and 2), hidden growth, two-clone `suite acceptance`, the cost of a local scan with fixture ports (median and worst of 30), version `0.3.0`, CHANGELOG, PLAN, roadmap, README, manual, index, install to `~/bin/lico` with the `0.2.7` binary kept as a backup. The proposed spec requirements LLM-01 to LLM-03 are written into the report; the spec is not edited.

## Steps

- [ ] S01: Bump to `0.3.0`, the end-to-end test and its `tests/testmap.toml` entry.
- [ ] S02: Release build, screenshots, cold runs, idle and hidden-growth measurements, acceptance.
- [ ] S03: Docs, report, install, stop at G3 and G4.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The whole flow verified on a private Xvfb with fake servers and fake ssh | end-to-end test and screenshots |
| 2 | Budgets met as measured | `lico_measure.suite run --cold`, `lico_measure run` |
| 3 | Two clean clones identical; `~/bin/lico` reports `0.3.0` | `suite acceptance`, `lico --version` |
