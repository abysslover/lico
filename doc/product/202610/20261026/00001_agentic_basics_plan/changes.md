# Changes: what changed in this planning set

Status: v1.0, approved by the owner at G1 on 2026-10-26. One line is added when the owner decides something and at the end of every phase. The newest is at the top.

Related documents: [planning](planning.md) · [requirements](requirements.md) · [screens](screens.md) · [test plan](test_plan.md).

## History

| Date | Version | What changed | Why |
|---|---|---|---|
| 2026-10-26 | v1.0 (G1 approved) | The owner answered "proceed with the recommended option". It was read as (1) approval of G1 (the whole planning set, phase plans G2 included) and (2) approval of input-box basics IN-01 to IN-07 as option A, included in P02. IN-01 to IN-07 moved from *proposed* to *planned*; the cover moved from draft v0.1 to v1.0 | The owner's approval |
| 2026-10-26 | v0.1 + input-box proposal | Re-reading the code found the input limit (512 B on one line per input) and the lack of cursor movement; IN-01 to IN-07 were added to the [requirements](requirements.md) as proposals, and SC-04 and the working assumptions were adjusted | A check of the basics of agentic coding |
| 2026-10-26 | v0.1 + POC-0 | POC-0 was approved as **option A**. The decision is recorded in the POC-0 section and the decisions table of the [planning document](planning.md) | The owner chose option A |
| 2026-10-26 | v0.1 + proposal | The POC-0 proposal and the result of one probe were added to the [planning document](planning.md). Test figures were corrected to work report 024 (135 tests, 554 s, 27 Rust tests) | The owner asked for the fastest POC |
| 2026-10-26 | v0.1 | First version: the order changed to features first, a new M02 (P01 to P07) and the requirements, screens and test plan were written | The owner changed the order and asked for test discipline |

## What changed in v0.1

**Order.** Before, one area (fonts and scroll speed) was refined very finely and memory came first. Now the order is **define features, build features, optimise memory**. The original philosophy (the idle window under 50 MiB) stays as the limit. Memory is reduced after the features are fixed so that it can be handled if it jumps when a feature is added later.

**Milestone.** P02 to P06 of the earlier roadmap's M02 "Usable Linux workbench" were replaced by the new P02 to P07 ("Agentic-coding basics"). P01 continues as it was (menu bar, fonts, scroll, activity indicator and the test cleanup S05).

**Where the owner's decisions went**

| Decision (2026-10-26) | Where it landed |
|---|---|
| Features first, memory after | The order and P07 ([planning](planning.md)) |
| Memory shown at the right of each session | MI-01 to MI-06, P02, screen 1 |
| Computer use: a private virtual display, plus the real screen when allowed | CU-01 to CU-07, P05, screen 7 |
| Session-to-session: save long content to a file and deliver the path automatically; never cut | SC-04 to SC-07, P03, screens 4 and 5 |
| P06: git worktree sessions, notification/activity indicator, conversation search (no Markdown rendering) | WT, NT, SR, P06, screen 8 |
| Approval once per milestone plus every two phases | The gates table in the [planning document](planning.md) |
| Whole suite within 3 minutes, unchanged tests not run again, time and memory managed | TST-01 to TST-11, the [test plan](test_plan.md), work order 025 |
| Planning as an easy document; Korean master outside git, English in git | Where the documents live (this set) |

## Where the old M02 items went

| Old item | Where it went |
|---|---|
| M02-P01 menu bar and status bar | Continues as the new P01 (in progress) |
| M02-P02 workspace and session navigator | Sessions with a folder and the state and memory display go to P02; attention marks go to P06. "Grouping by workspace" is an M03 candidate |
| M02-P03 conversation view quality | Selection, copy, paste, fonts and scrolling are mostly done by P01. The rest (resizing and the like) is an M03 candidate |
| M02-P04 control-surface parity (CLI and local REST) | The CLI and the local HTTP receiver already exist. P03 adds the tools the agent uses. The API inventory is an M03 candidate |
| M02-P05 hidden-session metadata, notifications, resource panel, accessibility, Wayland | Notifications go to P06, resource display to P02 (memory per session). Accessibility and Wayland are M03 candidates |
| M02-P06 release hygiene | Review of the whole milestone is P07; public-clone build, package and documentation are M03 candidates |
| M03 Windows | Unchanged, a later milestone |

## Owner confirmation status

| Item | Status |
|---|---|
| G1 milestone approval | **Approved** (2026-10-26, "proceed with the recommended option") |
| Input-box basics (IN-01 to IN-07) | **Approved** (2026-10-26, option A). Included in P02 |
| POC-0 result | Running as option A (started 2026-10-26). When the result page exists, it is decided whether to look at B and C again. See the POC-0 section of the [planning document](planning.md) |
| Values in "Working assumptions" | They stand, following the 2026-10-26 answer. A value changed by POC-0 measurement is written in the planning document with the date it changed |

## What will be written

At the end of every phase one line: date, phase, measured values (idle window at both scales, idle host, the number for that feature), test time (whole suite, usual selected run) and the requirement IDs that changed. When a requirement changes, its ID in the [requirements](requirements.md) is never deleted: only its status changes.
