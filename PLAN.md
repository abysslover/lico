# Lico status

Updated: 2026-10-27

| Item | Status | Evidence boundary |
|---|---|---|
| Product direction | Selected | Lico; MIT; Linux first, Windows later |
| [Roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) | Updated 2026-10-27 | Shows where each milestone stands; grants no approval |
| [M01 milestone](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md) | In progress | G1 approved 2026-10-23; P01 to P07 and P07a have plans and reports (all `in_review`, inspection pending; the experiment and the report of P07a stay on its own branch); P08 (acceptance pack) is not started |
| [M02 milestone](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md) | In progress | G1 approved 2026-10-26 with the phase plans of G2 included; M02-P01 and M02-P02 are reported (`in_review`, inspection pending); M02-P03 is in progress |
| [M02 planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) | Approved 2026-10-26 | Scope, order, requirements, screens and test plan; numbers are design targets unless a report marks them measured |
| Renderer and runtime | Selected from M01 measurements | Recorded in the M01 reports; the owner's inspection (G3) is pending |
| Implementation | On `main` up to M02-P02; later phases on phase branches | Moved to `main` on the owner's merge approval (G4) alone, twice (see the gate table), with G3 pending for all of it; later phases reach `main` only after G3 and G4; nothing is published without the owner's explicit yes |
| Resource budgets | Targets, with first measured values in the reports | A number counts as measured only where a report says so |
| Release and product acceptance | Pending | No release |

| Gate | State | Unlocks |
|---|---|---|
| G1 milestone approval | Approved 2026-10-23 for M01; approved 2026-10-26 for M02 | Approved milestone scope |
| G2 phase-plan approval | Approved 2026-10-23 for M01-P01 to M01-P03; 2026-10-24 for M01-P04 to M01-P06 as drafted; 2026-10-25 for M01-P07, M01-P07a and M02-P01 by the owner's decisions in chat; 2026-10-26 for M02-P02 to M02-P07 together with the M02 G1 (a plan note per phase is written as its first step) | Implementation and experiments for those phases only |
| G3 inspection acceptance | Pending | An observed acceptance verdict; in M02 it is asked every two phases |
| G4 merge approval | Approved for two moves of `main`, otherwise pending | The owner approved, each time without G3: on 2026-10-25 the move of `main` to pull request 1 and M01-P01 to M01-P05 (with the plan of P06), on 2026-10-27 the move to the finished work up to M02-P02. Both are fast-forwards; every other merge needs its own approval |

Project setup approval does not approve a milestone, phase implementation or
merge. A phase needs its own plan approval unless a milestone approval covers
its plan in writing (the M02 G1 does). No automatic-merge authorization is
inherited. Do not convert this table to completed merely because a repository,
planning PR or development session exists.
