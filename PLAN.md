# Lico status

Updated: 2026-11-12

| Item | Status | Evidence boundary |
|---|---|---|
| Product direction | Selected | Lico; MIT; Linux first, Windows later |
| [Roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) | Updated 2026-11-12 | Shows where each milestone stands; grants no approval |
| [M01 milestone](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md) | Done (2026-11-12) | G1 approved 2026-10-23; all phases M01-P01 to M01-P08 (including P07a) are accepted by the owner at G3 and merged and published on `main` at G4 (`milestone/M01-measured-linux-foundation`) |
| [M02 milestone](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md) | Done (2026-11-11) | G1 and G2 approved 2026-10-26; all seven phases M02-P01 to M02-P07 are accepted by the owner at G3 and merged and published on `main` at G4 (`milestone/M02-agentic-coding-basics`) |
| [M02 planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) | Completed 2026-11-11 | All 63 M02 requirements measured across M02-P01 to M02-P07 |
| [M03 milestone](doc/milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md) | Done (2026-11-12) | G1 and G2 Option A approved 2026-11-11; all four phases M03-P01 to M03-P04 are accepted by the owner at G3 and merged and published on `main` at G4 (`milestone/M03-admin-gui-and-control-parity`, `lico 0.1.0`) |
| [M04 milestone](doc/milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md) | Done (2026-11-12) | G1 and G2 approved 2026-11-12; all four phases M04-P01 to M04-P04 (`lico 0.2.0`) are accepted by the owner at G3 and merged and published on `main` at G4 (`milestone/M04-workspace-workbench-and-conversation-ux`) |
| [M05 milestone](doc/milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md) | Done (2026-11-12) | G1 and G2 approved 2026-11-12; all three phases M05-P01 to M05-P03 (`lico 0.2.1`) are accepted by the owner at G3 and merged and published on `main` at G4 (`milestone/M05-workspace-continuity-and-file-navigation`) |
| [M06 milestone](doc/milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md) | Ready for G3/G4 (2026-11-12) | G1 and G2 approved 2026-11-12; `M06-P01` to `M06-P03` (`lico 0.2.2`) implemented and verified on `phase/M06-P03-ssh-remote-files-and-sessions-0-2-2`; `main` stays at `0.2.1` until G4 |
| Renderer and runtime | Selected and accepted | Measured in M01 reports and accepted by the owner at G3 on 2026-11-11 |
| Implementation | On `main` through M05-P03 (`lico 0.2.1`); `M06-P03` (`lico 0.2.2`) ready on phase branch | Merged to `main` and published to `origin` through M05-P03 on 2026-11-12; `M06` awaits G3/G4 |
| Branches | Phase branches kept; `milestone/M01-measured-linux-foundation`, `milestone/M02-agentic-coding-basics`, `milestone/M03-admin-gui-and-control-parity`, `milestone/M04-workspace-workbench-and-conversation-ux`, and `milestone/M05-workspace-continuity-and-file-navigation` created | A branch `milestone/Mxx-<name>` is created at each milestone's closing commit and listed in its milestone document (rule of 2026-10-27) |
| Resource budgets | Measured through M06-P03 (`lico 0.2.2`) | Idle window RSS `24.25..24.33` / `35.23..35.40 MiB` (`< 50 MiB`), 10->100 hidden growth `+0.31 MiB` (`<= +10 MiB`), cold 475-test suite `100.4 s` (`<= 180 s`, peak `1,672.9 MiB`) |
| Release and product acceptance | Accepted on `main` (`lico 0.2.1`, 2026-11-12) | Two clean clones build byte-for-byte identical `lico 0.2.1` binaries (`suite acceptance`); standalone binary installed to `~/bin/lico`; accepted by the owner at G3 and merged/published at G4 |

| Gate | State | Unlocks |
|---|---|---|
| G1 milestone approval | Approved 2026-10-23 for M01; approved 2026-10-26 for M02; approved 2026-11-11 for M03; approved 2026-11-12 for M04, M05, and M06 | Approved milestone scope |
| G2 phase-plan approval | Approved for M01-P01 to M01-P08, M02-P01 to M02-P07, M03-P01 to M03-P04, M04-P01 to M04-P04, M05-P01 to M05-P03, and M06-P01 to M06-P03 | Implementation and experiments for approved phases only |
| G3 inspection acceptance | Approved 2026-11-11 and 2026-11-12 through M05-P03 (full M01, M02, M03, M04, and M05 milestones) | Observed owner acceptance on live session use and inspection bundles |
| G4 merge approval | Approved 2026-10-25, 2026-10-27, 2026-11-11, and 2026-11-12 through M05-P03 (merged and published) | `main`, `phase/*` through `M05-P03`, and `milestone/M01-*`..`milestone/M05-*` published on the owner's explicit approval |

Project setup approval does not approve a milestone, phase implementation or
merge. A phase needs its own plan approval unless a milestone approval covers
its plan in writing (the M02 G1 does). No automatic-merge authorization is
inherited. Do not convert this table to completed merely because a repository,
planning PR or development session exists.
