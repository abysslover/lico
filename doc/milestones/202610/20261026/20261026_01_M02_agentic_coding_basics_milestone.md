# M02 — Agentic-coding basics

- Status: in_progress
- Approved: 2026-10-26 (G1, with the phase plans of G2 included, as written in the [planning set](../../../product/202610/20261026/00001_agentic_basics_plan/planning.md))
- Project: Lico
- Updated: 2026-11-11
- Implementation: M02-P01 to M02-P07 are built, reported and merged to local `main` on the owner's G4 (all `in_review`, G3 inspection pending). POC-0 (D1 to D4, no product code) is finished
- Next gate: G3 inspection for M02-P01 + M02-P02, for M02-P03 + M02-P04, for M02-P05 + M02-P06, and for M02-P07 + the whole M02 milestone; G4 local merges to `main` were authorized on 2026-10-27 (up to P02) and 2026-11-11 (P03 to P07); nothing is published
- Milestone branch: `milestone/M02-agentic-coding-basics`, created at the closing commit when the milestone is achieved (not yet; the phase branches are kept)

## Goal

Give Lico the basic functions that people expect from tools for agentic coding,
before memory is optimised: sessions that run in a real work folder, sending
text between sessions, Lico tools that an agent can call, screen capture and
clipboard, computer use on a private virtual display, and the memory of every
session shown in the window. Memory is optimised afterwards (M02-P07) against
the original limit that stays in force: an idle window under 50 MiB.

## Done when

1. A session starts in an existing folder chosen at creation, can be resumed
   there, and can be closed, renamed and deleted from the window. Deleting never
   touches a folder the user chose. Input of up to 64 KiB with several lines and
   cursor movement reaches both providers intact, and what was sent appears in
   the conversation (the first 12 lines or 2 KiB, then one line saying how much
   is not shown). (M02-P02)
2. The private memory of each session (the agent and all its descendant
   processes, RSS on hover; programs a tool detached from the session are not
   counted) is shown at the right of its row and is measured only while a
   window is attached. (M02-P02)
3. Text, selected lines or a whole conversation can be sent from one session to
   another from the window. Content over 4 KiB is saved as a file and delivered
   as a path; nothing is cut. A Claude Code session can list, send to, read and
   create sessions through Lico tools, with an approval in the window for the
   sensitive ones. (M02-P03)
4. Screen capture and clipboard read and write (text and image) work on X11,
   and an image can be pasted into the input box. (M02-P04)
5. An agent can operate a private virtual display (click, type, keys, scroll,
   capture). The real screen is used only with a per-session, time-limited
   permission. (M02-P05)
6. Git worktree sessions, attention marks and notifications, and conversation
   search work. (M02-P06)
7. With every feature on, the idle window stays under 50 MiB at both display
   scales and grows by at most 10 MiB in 10 minutes while hidden. The whole
   milestone is independently reviewed. (M02-P07)
8. The whole test suite, from scratch, takes at most 180 s, and a normal run
   executes only the tests that a change can affect. (from M02-P01-S05 on, every
   phase)

These are proposed acceptance conditions, not completed capabilities. The
[requirements](../../../product/202610/20261026/00001_agentic_basics_plan/requirements.md)
carry the full list with a check for each item. Numbers are design targets until
a report marks them as measured.

## Why now

Until now one area (fonts and scroll speed) was refined very precisely while the
basics of an agentic tool were missing: no work folder, nothing of Lico offered
to the agent inside a session, no close, rename or send between sessions, no
per-session memory in the window, and an input box that accepts 512 bytes on one
line. Reducing memory before the features are defined risks a sudden jump when
features arrive later. The order is therefore: define the features, build them
with one measured memory line per phase, then reduce memory once with everything
switched on. The [planning document](../../../product/202610/20261026/00001_agentic_basics_plan/planning.md)
gives the reasons, the decisions of the owner and the risks.

M02 builds on the M01 phase branches. M01 stays open for its own acceptance
(M01-P08 and the owner's use with real accounts); the earlier roadmap's M02
items that are not part of this milestone are listed as M03 candidates in the
[changes](../../../product/202610/20261026/00001_agentic_basics_plan/changes.md).

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M02-P01 | Menu bar and shared controls, font and scroll speed, activity indicator, and the test cleanup (S05) | [Plan](../../../plans/202610/20261025/20261025_02_M02-P01_menu_bar_and_shared_controls_plan.md) | [Report](../../../reports/202610/20261025/20261025_03_M02-P01_menu_bar_and_shared_controls_report.md) | Local bundle, verdict pending (with M02-P02) | None | in_review |
| M02-P02 | Sessions in a real folder: new-session screen, close, rename, delete, memory per session, input-box basics, input echo | [Plan](../../../plans/202610/20261026/20261026_01_M02-P02_folder_sessions_plan.md) | [Report](../../../reports/202610/20261026/20261026_01_M02-P02_folder_sessions_report.md) | Local bundle, verdict pending | None | in_review |
| M02-P03 | Sending between sessions (long content by file path) and the Lico tool channel for agents | [Plan](../../../plans/202610/20261026/20261026_03_M02-P03_sending_between_sessions_plan.md) | [Report](../../../reports/202610/20261027/20261027_01_M02-P03_sending_between_sessions_report.md) | Local bundle, verdict pending (with M02-P04) | None | in_review |
| M02-P04 | Screen capture, clipboard read and write, image paste | [Plan](../../../plans/202610/20261029/20261029_01_M02-P04_screen_and_copy_paste_plan.md) | [Report](../../../reports/202611/20261111/20261111_01_M02-P04_screen_and_copy_paste_report.md) | Local bundle, verdict pending (with M02-P03) | None | in_review |
| M02-P05 | Computer use on a private virtual display; the real screen only by per-session permission | [Plan](../../../plans/202611/20261111/20261111_01_M02-P05_computer_use_plan.md) | [Report](../../../reports/202611/20261111/20261111_02_M02-P05_computer_use_report.md) | Local bundle, verdict pending (with M02-P06) | None | in_review |
| M02-P06 | Everyday comfort: git worktree sessions, attention marks and notifications, conversation search | [Plan](../../../plans/202611/20261111/20261111_02_M02-P06_everyday_comfort_plan.md) | [Report](../../../reports/202611/20261111/20261111_03_M02-P06_everyday_comfort_report.md) | Local bundle, verdict pending (with M02-P05) | None | in_review |
| M02-P07 | Memory pass with every feature on, and review of the whole milestone | [Plan](../../../plans/202611/20261111/20261111_03_M02-P07_memory_pass_plan.md) | [Report](../../../reports/202611/20261111/20261111_04_M02-P07_memory_pass_report.md) | Local bundle, verdict pending (M02-P07 and M02) | None | in_review |

Phases run in this order. The plan of each phase is written as its first step,
reviewed independently, and records any deviation from the planning set. POC-0 is
not a phase: it adds no product code, is not counted as done, and its measured
values replace the working assumptions of the planning set with the date of the
change.

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-10-26 | Scope, order, phases M02-P01 to M02-P07 and the approval rhythm, as written in the planning set |
| G2 phase plan approval | Approved 2026-10-26 together with G1 for M02-P02 to M02-P07 (M02-P01 was approved on 2026-10-25) | One plan note per phase, written as the phase's first step; included in the milestone approval and not asked separately |
| G3 inspection acceptance | Pending | The owner looks at the real window and the report at the end of M02-P02 (with M02-P01), M02-P04, M02-P06 and M02-P07 |
| G4 merge approval | Approved 2026-10-27 and 2026-11-11 for local `main` moves up to M02-P07, otherwise pending | Local `main` fast-forward moves up to M02-P02 (2026-10-27) and M02-P03 to M02-P07 (2026-11-11), each with G3 still pending; nothing is pushed without separate publication approval |

Publication (push) of any branch needs the owner's explicit yes for the exact
list of what is published. Approving this milestone is not a push approval, and
approval of one gate does not carry to the next.

## Blocked on

- G3 inspection verdicts from the owner for M02-P01 + M02-P02, M02-P03 + M02-P04, M02-P05 + M02-P06, and M02-P07 + the whole M02 milestone (all seven phase reports and local inspection bundles are ready).
- Tests with a real provider sign-in need an account of the owner; they are run once at the end of a phase and the installed provider version is recorded.

## Out of scope

Markdown rendering, Wayland capture and control, a tool channel for Codex
sessions, other providers, remote and multi-user use, plugins, session forking,
and Windows (a later milestone with its own credential, windowing and measured
acceptance). Items of the earlier roadmap's M02 that are not part of this
milestone (workspace grouping, conversation view quality beyond what M02-P01
did, REST parity, accessibility, release hygiene) are M03 candidates.

## When done

Not filled yet: M02-P01 to M02-P07 are built, reported and merged to local `main`, and wait for the owner's G3 inspection verdict. When the owner accepts G3 for the milestone, the branch `milestone/M02-agentic-coding-basics` is created at its closing commit and named here.
