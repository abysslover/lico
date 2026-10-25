# Changes: what changed in this planning set

Status: v1.2 (v1.0 plus the measurements of POC-0 and S05, the D4 result and the input echo), v1.0 approved by the owner at G1 on 2026-10-26. One line is added when the owner decides something and at the end of every phase. The newest is at the top.

Related documents: [planning](planning.md) · [requirements](requirements.md) · [screens](screens.md) · [test plan](test_plan.md).

## History

| Date | Version | What changed | Why |
|---|---|---|---|
| 2026-10-26 | v1.2 (D4, input echo) | (1) POC-0 D4 (the combined chain) is recorded: 3 of 3 runs complete, memory of one session measured (idle private about 120 MiB), reading a big file through the shell asks for approvals (handled in P03), and the `[session started]` record does not appear before the first message (the window must not use it as "ready"). (2) **SC-10 (recording inputs) moves from P03 to P02 with a fold rule, and IN-08 is added:** on accepting an input the host records the first 12 lines or 2 KiB as lines that start with `> ` and one line with the exact amount left out, never silently. Without it a pasted prompt leaves no trace after Enter, the owner tries the input box at the end of P02, P03 needs the same record anyway, and an echo drawn only by the window would be code thrown away. The sender header (`from <name>`) for text sent by another session stays in P03. (3) The POC-0 section is the result page; no separate report is written, and B and C are not chosen separately | The owner tries the input box at the end of P02 and P03 needs the same record. The lead decided it (it is one function in the host and can be reverted); the owner is told in the next status |
| 2026-10-26 | v1.1 (POC-0 measured) | POC-0 D1 to D3 replaced assumptions with measurements. (1) MI-02: the row value changes from "RSS sum" to the **private memory (RssAnon) sum** with RSS on hover (PSS is not used: it moved up to 28 % between runs); MI-05 and MI-06 record RSS and private memory for host and window; MI-07 added as a proposal (closing a session also ends detached programs). (2) SC-04, SC-05, SC-07: the notice gets a reading hint, the whole conversation is built from the provider's record, SC-10 added (inputs are recorded). (3) XF-02 extended, XF-08 added (a large screenshot never vanishes), CU-07 extended, CU-08 added (a window manager is decided after measuring). (4) Two risk rows are resolved and two are added. TST-01 and the test results are filled from S05. The evidence is in the POC-0 table of the [planning document](planning.md) | D1 to D3 are done and S05 is measured |
| 2026-10-26 | v1.0 + ledger | Test figures were updated to the S05 ledger (work report 025a): 135 tests, 596 s from scratch, peak memory of all processes 3,571 MiB. They stand in the budget and current state of the [test plan](test_plan.md) and in TST-01 | The first measurement of S05 |
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
| Input-box basics (IN-01 to IN-07) | **Approved** (2026-10-26, option A). Included in P02. IN-08 (input echo) was added by the lead afterwards and the owner can revert it (v1.2) |
| POC-0 result | D1 to D4 all done (2026-10-26 03:20). The result page is the POC-0 section of the [planning document](planning.md); no separate report was written. B and C are not chosen separately; the work goes on along the phase plans under way |
| Values in "Working assumptions" | They stand, following the 2026-10-26 answer. A value changed by POC-0 measurement is written in the planning document with the date it changed |

## What will be written

At the end of every phase one line: date, phase, measured values (idle window at both scales, idle host, the number for that feature), test time (whole suite, usual selected run) and the requirement IDs that changed. When a requirement changes, its ID in the [requirements](requirements.md) is never deleted: only its status changes.
