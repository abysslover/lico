# Lico agentic-coding basics: what is built, and how far

Status: v1.0, approved by the owner at G1 on 2026-10-26 (scope, order and the phase plans, G2). P01 is finishing: the test cleanup (S05) is under way and P02 starts when it is reported. Numbers in this document are **design targets** unless they are marked *measured*.

Related documents: [requirements](requirements.md) · [screens](screens.md) · [test plan](test_plan.md) · [changes](changes.md) · [milestone M02](../../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md).

## At a glance

| | |
|---|---|
| Goal | Give Lico the basics of an agentic-coding tool: sessions in a real folder, sending between sessions, Lico tools the agent can call, screen capture and copy/paste, computer use, and the memory of each session shown in the window |
| Order | **Define the features (this document), build them (P02 to P06), then optimise memory (P07).** The original philosophy stays as a hard limit: the idle window stays under 50 MiB |
| In parallel | **POC-0**: a throwaway proof of concept, no product code, that runs the four features roughly and measures memory (see "POC-0" below). Option A, approved 2026-10-26 |
| Now | P01 wrap-up: menu bar, fonts, scroll speed and the activity indicator are done; the **test cleanup (S05)** is in progress. P02 starts when it is reported |
| Owner's next step | G1 is approved (2026-10-26). The next thing that reaches the owner is the **inspection (G3) and merge (G4) at the end of P02**, together with P01 |

## Why the order changes

Until now one area (font and scroll speed) was refined very precisely while the basics of an agentic tool were missing. Reducing memory before the features are defined means that, when features arrive later, memory can jump with no room to react, or the result lacks features.

So: fix the feature list first (this document), write a memory budget for each feature, report one measured line per phase while building, and reduce everything at once in P07 with all features switched on.

**Current state, checked in the code**

- Already there: Ctrl+N new session, switching and resuming sessions, allow/deny of approvals, copy and paste, several accounts, automatic host replacement, a menu bar, an activity indicator (a clock while working).
- Five basics are missing:
  1. **No work folder.** A session only runs in an empty scratch folder made by the host, so a real repository cannot be opened. This is the biggest hole.
  2. **The agent in a session is given nothing of Lico.** No display (DISPLAY), no Lico connection, no tools, so it can call neither Lico nor a screen.
  3. The window has no **close, rename or send to another session** (only the CLI has `send`).
  4. **No per-session memory in the window.** The host already measures the memory of one agent process per session (`lico resources`) but leaves out its descendants, and the window does not show it.
  5. **The input box lacks basics.** The host accepts only 512 bytes (about 170 Korean characters) on one line per input (newline and tab are refused), and the window's input line has no cursor movement (left/right, Home). Long prompts and multi-line pastes cannot be sent. This was found by reading the code; what the window shows when a draft exceeds the limit has not been looked at yet. The solution is IN-01 to IN-07 in the requirements, put into P02 with the owner's approval (2026-10-26).

## What is built

Milestone **M02 "Agentic-coding basics"**. It replaces P02 to P06 of the earlier roadmap's M02 (where the old items went: see [changes](changes.md)).

| Phase | What the user gets | Memory budget (design target) | Status |
|---|---|---|---|
| **POC-0** facts first | The four features run roughly with no product code, and memory is measured (option A, 4 h target) | not applicable | approved, in progress |
| **P01** menu, controls, speed | Menu bar, font and scroll speed, activity indicator, test cleanup (S05) | idle window 23.6 / 34.6 MiB at scale 1 / 2 (*measured*) | in progress |
| **P02** sessions with a folder | New-session screen (folder, name, account, model, effort, permission), close, rename, delete, **memory shown at the right of each session**, **input-box basics** (long text, several lines, cursor movement) | window +1 MiB at most | planned |
| **P03** between sessions, tool channel | Send text, selected lines or a whole conversation from session A to B (long content goes by file path); Lico tools the agent can call (list, send, read, create) | window 0, no new process per session | planned |
| **P04** screen, copy/paste | Capture screen, window or region; clipboard (text, image) read and write; image paste; middle-button paste | temporary +6 MiB at most, returned afterwards | planned |
| **P05** computer use | Click, type, keys, scroll; a private virtual display for the agent; the real screen only when allowed | only for the session that asks, ended with the session. One probe: virtual display 61 + helper 12 MiB (*measured*) | planned |
| **P06** everyday comfort | git worktree sessions, attention indicator and notifications, conversation search | per item | planned |
| **P07** memory pass | Re-measure the 50 MiB limit and the +10 MiB hidden growth with every feature on, reduce, review the whole milestone | limit 50 MiB kept | planned |

Details and checks are in the [requirements](requirements.md); the look of each screen is in [screens](screens.md).

## POC-0: turn assumptions into facts first (option A approved, in progress)

**What.** Before building the product, put throwaway tools on the existing host, run the four features roughly, and measure. The host already takes extra arguments for the Claude process (`--claude-arg`), so **no product code changes.** Target 4 hours, cap 8 hours. The result is one page with captures, numbers and a verdict (works / conditional / does not work). It replaces the values in "Working assumptions" with measurements.

| Demo | What it checks |
|---|---|
| D1 memory | How far the sum of the agent and its descendants is from the value the host reports today (one process) |
| D2 session-to-session | Short text inline, long text by file path. Does the receiving Claude open the file and answer (permissions skipped and in-window approval modes)? How much of "the whole conversation" is left in the host record (2,048 records)? |
| D3 X tools | In a private virtual display, does Claude work from a capture image with click, type, keys and clipboard? Time and memory of one step |
| D4 combined | Session A copies, hands over to session B, B answers; a memory table at the end |

**How.** Two helper agents run in parallel: (1) D1 and D2 (memory, transfer), (2) D3 (X tools and the Lico window). D4 follows when both are done. The helper tools passed their own test: a real Claude session (Haiku) on a private host answered in 5 s and ended cleanly with no leftover process (*measured*, one run).

**Already known (one probe, *measured*).** An external tool server (70 lines of Python) attached to `claude -p` let Claude (Haiku) read a random code from a **capture image of a private virtual display**, close a window by clicking, and capture again to confirm it was gone (5 turns, 9.7 s). One session used 281 MiB for the Claude process, 12 MiB for the tool helper and 61 MiB for the virtual display (800x500); a session using X tools costs about +73 MiB. This is one run in `-p` mode; the host's stream-json mode and the hand-drawn Lico window are checked in D3.

**Not done in POC-0.** Controlling the real screen, changing product code, publishing. Only a private socket and a private virtual display are used; the running host is never touched. A POC is not counted as "done".

**Options and decision.** A: D1 to D4 above (half a day). B: the smallest window-visible slice (memory indicator plus a minimal send) first (2 to 3 days). C: go straight through the product path (5 days or more). **The owner chose A (2026-10-26).** B and C are reconsidered only if the POC result calls for it.

## Decided by the owner

| Date | Decision |
|---|---|
| 2026-10-26 | Build the features first; optimise memory afterwards. Plan in conversation, fix the scope, then go full steam |
| 2026-10-26 | Show **memory at the right of each session** |
| 2026-10-26 | Planning documents are easy to read, with details in Markdown. The Korean master is kept outside the repository and the repository holds the English version |
| 2026-10-26 | Computer use: **a private virtual display by default, the real screen when allowed** |
| 2026-10-26 | Session-to-session: text, selected lines and a whole conversation are all offered; **when the content is long it is saved to a file and Lico delivers the path automatically.** Nothing is cut (information is preserved) |
| 2026-10-26 | P06 items: git worktree sessions, notification/activity indicator, conversation search. **No Markdown rendering this time** |
| 2026-10-26 | Approval rhythm: milestone approval once (G1, phase plans G2 included), then inspection and merge (G3/G4) **every two phases** |
| 2026-10-26 | Tests: the **whole suite stays within 3 minutes**, unchanged tests are not run again (chosen by module dependency), and the **time and memory of tests** are managed |
| 2026-10-26 | **POC-0 goes ahead as option A** (no product code, D1 to D4, 4 h target and 8 h cap, 1 to 2 helper agents, in parallel with test cleanup S05) |
| 2026-10-26 | **G1 approved**: this whole planning set (scope, order, P01 to P07, approval rhythm, phase plans G2 included). The owner answered "proceed with the recommended option" |
| 2026-10-26 | **Input-box basics (IN-01 to IN-07) go into P02 as the recommended option A** (64 KiB per input, newline and multi-line, cursor movement, recall with the up key, per-session drafts) |

Rules that keep applying: development workers are Claude-family agents only. The idle window stays under 50 MiB and fast, in one executable. The screen tools use the hand-drawn client. New sessions skip the agent's own permission prompts and use approvals inside the window. The sign-in code is never visible or logged. The host replaces itself when it meets a new build and resumes the sessions that were alive.

## Working assumptions (change them on request)

The owner proceeded with the recommended option on 2026-10-26, so these values stand. A value changed by POC-0 measurement is written here with the date it changed.

| Item | Value | Used in |
|---|---|---|
| "X" | Linux X11. Wayland is out of scope (the X server tested is always recorded) | P04, P05 |
| How the agent calls Lico | A tool connection served directly by the host (MCP, per-session token) plus a CLI doing the same things. If the tool connection does not work, CLI only | P03 |
| Inline size for sending | Up to 4 KiB inline, longer goes by file. The host's input limit (today 512 B of text, 4 KiB per request line) is raised first by IN-01 | P02, P03 |
| Input-box basics (approved) | Up to 64 KiB per input (newline and tab allowed), several lines (Enter sends, Shift+Enter inserts a newline), cursor movement, recall with the up key, per-session drafts. Today: 512 B, one line, no cursor movement | P02 |
| Handoff file retention | 30 days, 256 MiB in total (oldest first when over) | P03 |
| Host record retention | Today 2,048 records. Too small for "the whole conversation": raise in P03 or use the provider's own record alongside | P03 |
| Virtual display | 1280x800, ended after 10 minutes idle | P05 |
| Real-screen permission | Per session, expires after 15 minutes, released when the host restarts | P05 |
| Memory refresh and colour | Every 3 s while a window is attached; colour change at 1 GiB (again at 2 GiB) | P02 |
| Test budget | 180 s for the whole suite, 2 GiB peak memory over the whole run, 15 s per test | all phases |
| Tool channel scope | Claude Code sessions only in this milestone. Codex sessions get folder, memory indicator, close and rename only | P03 to P05 |

## How the work is done

- **Meaning of "done":** it works in the real window, there is one main-path test, and there is one memory measurement. Exhaustive checks (every character combination and the like) are not part of a phase; if needed the lead runs them outside the phase as an acceptance tool.
- **Review:** one independent review per phase (the session that built it does not judge it). Every two phases the owner receives a G3/G4 package.
- **Memory:** at the end of each phase one line is reported (idle window at both scales, idle host, the number for that feature). Overshooting a target does not block a phase: it is recorded and reduced in P07. **There are only two stops:** idle window at or above 50 MiB, and growth that keeps rising over time.
- **Tests:** whole suite within 180 s; normally only the tests affected by a change run, and the whole suite runs before a merge. Details in the [test plan](test_plan.md).
- **Public hygiene:** no workstation paths, names, raw conversations or private instructions in git. Handoff and capture folders and tokens stay outside the repository.

| Gate | Meaning | State |
|---|---|---|
| G1 milestone approval | The whole scope of this document | approved 2026-10-26 |
| G2 phase plan | One plan per phase. Included in the milestone approval, not asked separately | approved with G1 |
| G3 inspection acceptance | The owner looks at the real window and the report | every two phases (end of P02, P04, P06, P07) |
| G4 merge approval | The inspected branch is merged into main | together with G3 |

Publishing (push) to the public repository happens only when the owner confirms the list and says yes. Approving this document is not a push approval.

## Order and dependencies

```
P01 finish --> P02 folder sessions --> P03 transfer, tool channel --> P04 screen, copy/paste --> P05 computer use --> P06 comfort --> P07 memory pass
(S05 tests)    memory indicator              |                            |                       |                    |
                                             +-- tool channel -----------+-- agent tools --------+                    +-- worktree needs P02
G3/G4:                      ^ end of P02                     ^ end of P04                                  ^ end of P06                 ^ end of P07
```

- The tool channel of P03 is the base that the "agent tools" of P04 and P05 sit on. The CLI parts of P04 and P05 can be built and tested without it.
- The worktree session of P06 attaches to the new-session screen of P02. Notifications and search are independent.
- P07 depends on every phase.

## Risks

| Risk | Response |
|---|---|
| It is not known whether the external tool connection also works in the host's stream-json mode (`-p` mode worked in one probe, *measured*) | POC-0 D3 or the first step of P03 tests it; if it fails, CLI only with the same approval rules |
| It is not known whether a model operates the hand-drawn Lico window well (pressing OK in a simple window worked with Haiku, *measured*) | POC-0 D3 or the first step of P05 measures a success rate on simple tasks and records it |
| Memory grows as features are added | A budget per feature, one measured line per phase, a P07 pass, only two stops |
| Tests get slow again | Test budgets, selection, receipts and size ratchet ([test plan](test_plan.md)). A phase over 180 s is not merged |
| Controlling the real screen can do great harm if wrong | Off by default, per-session permission, a conspicuous bar, 15-minute expiry, a key that stops it at once, an action log |
| The host record is short, so "the whole conversation" is cut | An export states its range and never looks like the whole; P03 raises retention or uses the provider's record alongside |
| The host takes 512 B on one line per input, which blocks long prompts and the transfer of P03 | IN-01 (64 KiB, newline allowed) is the first host work of P02 |
| Claude Code updates change its options | Record the tested version, ordinary tests use a fake provider, the real provider runs once at the end of a phase |
| Usage limit | There is no limit constraint, but phases run in order and the weekly guide sets the size of parallel units |

## Out of scope

Markdown rendering, Wayland capture and control, a tool channel for Codex sessions, other providers, remote and multi-user use, plugins, Windows (a later milestone), and extras such as session forking (`--fork-session`). Items pushed out of the earlier roadmap's M02 (workspace grouping, conversation view quality, REST parity, accessibility, release hygiene) are recorded as M03 candidates in [changes](changes.md).

## Reading the status

| Mark | Meaning |
|---|---|
| planned | Written in the documents, not started |
| in progress | A worker is building it |
| seen in window | The lead has watched it work in the real window |
| measured | The number is in the report (if absent, it is a design target) |
| in review | Our work is finished and the owner's G3/G4 is awaited |
| approved | The owner approved it |
| proposed | A proposal before owner confirmation |

"Done" is used only when both *seen in window* and *measured* are present. A cell without a number is empty, not passed.
