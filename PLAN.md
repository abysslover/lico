# Lico status

Updated: 2026-11-11

| Item | Status | Evidence boundary |
|---|---|---|
| Product direction | Selected | Lico; MIT; Linux first, Windows later |
| [Roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) | Updated 2026-11-11 | Shows where each milestone stands; grants no approval |
| [M01 milestone](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md) | In review (`M01-P08`) | G1 approved 2026-10-23; P01 to P07a are accepted at G3 and merged and published on `main` (`done`, 2026-11-11); P08 (`lico 0.1.0` acceptance pack, G2 approved 2026-11-11) is built and awaits G3/G4 |
| [M02 milestone](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md) | Done (2026-11-11) | G1 and G2 approved 2026-10-26; all seven phases M02-P01 to M02-P07 are accepted by the owner at G3 and merged and published on `main` at G4 (`milestone/M02-agentic-coding-basics`) |
| [M02 planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) | Completed 2026-11-11 | All 63 M02 requirements measured across M02-P01 to M02-P07 |
| Renderer and runtime | Selected and accepted | Measured in M01 reports and accepted by the owner at G3 on 2026-11-11 |
| Implementation | On `main` through M02-P07; `M01-P08` (`lico 0.1.0`) in review on `phase/M01-P08-acceptance-pack` | Merged to `main` and published to `origin` through M02-P07 on 2026-11-11; `M01-P08` built on its phase branch |
| Branches | Phase branches kept; `milestone/M02-agentic-coding-basics` created at M02 close | A branch `milestone/Mxx-<name>` is created at each milestone's closing commit and listed in its milestone document (rule of 2026-10-27) |
| Resource budgets | Measured through M02-P07 and M01-P08 | Idle window RSS `24.01..24.18` / `35.14..35.23 MiB` (`< 50 MiB`), 10-min hidden drift `+0.56 MiB` (`<= +10 MiB`), cold 449-test suite `92.9 s` (`<= 180 s`) |
| Release and product acceptance | In review (`M01-P08`, `lico 0.1.0`) | Two clean clones build byte-for-byte identical `lico 0.1.0` binaries (`suite acceptance`); awaits owner G3/G4 |

| Gate | State | Unlocks |
|---|---|---|
| G1 milestone approval | Approved 2026-10-23 for M01; approved 2026-10-26 for M02; approved 2026-11-11 for M03 | Approved milestone scope |
| G2 phase-plan approval | Approved for M01-P01 to M01-P08, M02-P01 to M02-P07, and M03-P01 to M03-P04 | Implementation and experiments for approved phases only |
| G3 inspection acceptance | Approved 2026-11-11 for M01-P01 to M01-P07a and M02-P01 to M02-P07 (full M02 milestone) | Observed owner acceptance on live session use and inspection bundles |
| G4 merge approval | Approved 2026-10-25, 2026-10-27 and 2026-11-11 through M02-P07 (merged and published) | `main`, `phase/*` through `M02-P07`, and `milestone/M02-agentic-coding-basics` published on the owner's explicit approval |

Project setup approval does not approve a milestone, phase implementation or
merge. A phase needs its own plan approval unless a milestone approval covers
its plan in writing (the M02 G1 does). No automatic-merge authorization is
inherited. Do not convert this table to completed merely because a repository,
planning PR or development session exists.
