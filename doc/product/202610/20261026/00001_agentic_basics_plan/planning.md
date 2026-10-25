# Lico agentic-coding basics: what is built, and how far

Status: v1.4 (v1.3 plus the S01 result: the tool connection works, an approval is never a held call), v1.0 approved by the owner at G1 on 2026-10-26 (scope, order and the phase plans, G2). P01 is finished on its branch (test cleanup S05 included) and P02 is under way: its plan note is written and its host step (S02) is built and tested; the commit waits for the repository's list of approved branches to include P02 (an owner action). Numbers in this document are **design targets** unless they are marked *measured*.

Related documents: [requirements](requirements.md) · [screens](screens.md) · [test plan](test_plan.md) · [changes](changes.md) · [milestone M02](../../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md).

## At a glance

| | |
|---|---|
| Goal | Give Lico the basics of an agentic-coding tool: sessions in a real folder, sending between sessions, Lico tools the agent can call, screen capture and copy/paste, computer use, and the memory of each session shown in the window |
| Order | **Define the features (this document), build them (P02 to P06), then optimise memory (P07).** The original philosophy stays as a hard limit: the idle window stays under 50 MiB |
| Done first | **POC-0** (finished 2026-10-26 03:20): a throwaway proof of concept, no product code, that ran the four features roughly and measured memory (see "POC-0" below). Option A, approved 2026-10-26, started 01:05 |
| Now | P01 is finished on its branch (menu bar, fonts, scroll speed, activity indicator, test cleanup S05: whole suite 596 s to 112 s). **P02 is under way:** plan note done, host step S02 built and tested; its commit waits for the owner's approval |
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
| **P02** sessions with a folder | New-session screen (folder, name, account, model, effort, permission), close, rename, delete, **memory shown at the right of each session**, **input-box basics** (long text, several lines, cursor movement) and the **input echo** in the conversation (IN-08) | window +1 MiB at most | planned |
| **P03** between sessions, tool channel | Send text, selected lines or a whole conversation from session A to B (long content goes by file path); Lico tools the agent can call (list, send, read, create) | window 0, no new process per session | planned |
| **P04** screen, copy/paste | Capture screen, window or region; clipboard (text, image) read and write; image paste; middle-button paste | temporary +6 MiB at most, returned afterwards | planned |
| **P05** computer use | Click, type, keys, scroll; a private virtual display for the agent; the real screen only when allowed | only for the session that asks, ended with the session. One probe: virtual display 61 + helper 12 MiB (*measured*) | planned |
| **P06** everyday comfort | git worktree sessions, attention indicator and notifications, conversation search | per item | planned |
| **P07** memory pass | Re-measure the 50 MiB limit and the +10 MiB hidden growth with every feature on, reduce, review the whole milestone | limit 50 MiB kept | planned |

Details and checks are in the [requirements](requirements.md); the look of each screen is in [screens](screens.md).

## POC-0: turn assumptions into facts first (option A approved, D1 to D4 done)

**What.** Before building the product, put throwaway tools on the existing host, run the four features roughly, and measure. The host already takes extra arguments for the Claude process (`--claude-arg`), so **no product code changes.** Target 4 hours, cap 8 hours. The result is one page with captures, numbers and a verdict (works / conditional / does not work). It replaces the values in "Working assumptions" with measurements.

| Demo | What it checks |
|---|---|
| D1 memory | How far the sum of the agent and its descendants is from the value the host reports today (one process) |
| D2 session-to-session | Short text inline, long text by file path. Does the receiving Claude open the file and answer (permissions skipped and in-window approval modes)? How much of "the whole conversation" is left in the host record (2,048 records)? |
| D3 X tools | In a private virtual display, does Claude work from a capture image with click, type, keys and clipboard? Time and memory of one step |
| D4 combined | Session A copies, hands over to session B, B answers; a memory table at the end |

**How.** Two helper agents ran in parallel: (1) D1 and D2 (memory, transfer), (2) D3 (X tools and the Lico window); the lead ran D4 (the combined chain) alone. **D1 to D4 are done (2026-10-26, started 01:05, finished 03:20).** The lead recomputed every number from the raw logs and corrected two counts in the helpers' reports (runs that first hit the read limit: 6 became 5 of 16; oversized lines dropped: "9" became 6 of 6). D4 ran two sessions and a private virtual display and gave the same result three times (table below). Every run used `claude-haiku-5-5`, so "works" is a floor on an easy task, not a statement about the model the owner will pick. The table gives the number of runs behind each figure and says "one run" where it is one.

| Question | Verdict | Key numbers |
|---|---|---|
| What memory value does the host give today? | One process only: the agent (`claude`) | 211 MiB; with tools and helpers 1.07 to 1.12 times (tiny children only; a heavy child was not measured) (2 runs) |
| What should a session row show? | **Private memory (RssAnon)**, RSS on hover. Not PSS | Three idle sessions: RSS 707 to 712, PSS 409 to 412, private 350 to 363 MiB (3 runs). The PSS of one idle session moved 147 to 188 MiB between two runs, private memory by about 5 %. Cost of reading: 1.0 ms against 13 to 18 ms (Python walks, so upper bounds) |
| Programs a tool detached from the session | Not in the process tree, and they survive deleting the session | One run. Not counted, and the tooltip says so; ending them properly is for the owner to decide at the P02 gate (MI-07) |
| Short text transfer | Today one line of at most 512 B | 170 Korean characters (510 B) pass, 171 are refused. IN-01 (64 KiB) is needed first |
| Long text by file path | **Works** | 16 of 16 right, nothing cut (200 KiB and 1 MiB, three permission setups). 5 of 16 first hit the read limit (25,000 tokens) and recovered alone. Approval prompts: permissions skipped 0 of 8, default mode 4 of 4 runs, default mode with the handoff folder opened 0 of 4 |
| What the receiving session sees | The answer without the question | The host record holds no input. Records are deleted in pieces of a quarter of the retention (at 2,048, after 2,613 records the oldest kept is number 512) |
| Does Claude work from a screen through X tools (host in stream-json mode)? | **Works** | Read the code, click, confirm: 5 of 5 (median 9.5 s). Six small tasks in the hand-drawn Lico window, 8 of 8 each (one scorer was changed after seeing data, and said so) |
| What about a big screenshot? | **The host drops a provider line over 256 KiB whole and leaves no trace** | 6 of 6 dropped. Real window captures are mostly over the limit, so P04 must fix it (XF-08) |
| Memory of one session using X tools | The provider process dominates | Session tree 382 to 487 MiB RSS (Claude 260 to 300, virtual display 60 to 98, helper 29 to 44) (48 runs) |
| Chain: read the screen, session A sends a file, session B reads it and answers (D4) | **Works** | 3 of 3 complete: the window was gone and B's answer was right on the value in the file, the line count and the order code read from the screen. Prompt to A until B's answer: 13.8, 16.3, 13.9 s (Haiku). A's calls: tool lookup once or twice, screen read 49 to 53 ms, click 124 ms, one send |
| Approval prompts for the reading session | Opening the handoff folder with `--add-dir` opens **only the Read tool** | 200 KiB, 2 runs: one Read, 0 approvals. 1 MiB, 1 run: shell commands (wc, grep, od), 2 approvals. Proposal for P03 (not tested): a read tool of our own that reads in parts, so the receiver does not reach for the shell |
| Memory of one session (agent plus two helpers): idle | About 120 MiB private, about 240 MiB RSS | 5 of 6 sessions: private 119.5 to 120.9, RSS 238.7 to 242.2, PSS 151 to 153. One read private 163.5, RSS 285.1 (cause unknown, one sample) |
| The same session at its peak during a run | Private 150.5 to 176.9, RSS 274.4 to 303.4 MiB | 6 sessions. The screen helper grows from 13 to 29 MiB RSS after its first screenshot (it loads an image library; an upper bound for a Rust helper). 3 to 8 processes |
| Two sessions, virtual display and host together | Peak sum: RSS 636.5 to 671.2, private 325.2 to 364.1, PSS 433.4 to 445.9 MiB | 3 runs. Host RSS 5.1 to 5.8 (private 0.4 to 1.0), virtual display 62.8 to 63.5 (99.8 once) |
| The `[session started]` record of a new session | **Appears only after the first message (likely)** | Absent after 60 s in all 6 starts. The host writes it when the provider sends its `init` event (checked in the code); the provider seems to send that only after the first input, not tested separately. The window must not use this record to decide "ready to type" |

Limits of D4: the screen was nearly empty (an image of 3.7 KB), so the 256 KiB line limit (XF-08) was not touched; the 1 MiB run answered with grep, so it shows "opened the file and answered right", not "read every byte"; a real X program, a heavy child process and a window manager were not measured.

The full judgements and the places of the raw data are in the lead's judgement documents (D3, D12, D4), kept in a working folder outside git.

**Already known (one probe, *measured*).** An external tool server (70 lines of Python) attached to `claude -p` let Claude (Haiku) read a random code from a **capture image of a private virtual display**, close a window by clicking, and capture again to confirm it was gone (5 turns, 9.7 s). One session used 281 MiB for the Claude process, 12 MiB for the tool helper and 61 MiB for the virtual display (800x500); a session using X tools costs about +73 MiB. This is one run in `-p` mode; the host's stream-json mode and the hand-drawn Lico window were checked in D3 and both work (table above).

**Not done in POC-0.** Controlling the real screen, changing product code, publishing. Only a private socket and a private virtual display are used; the running host is never touched. A POC is not counted as "done".

**Options and decision.** A: D1 to D4 above (half a day). B: the smallest window-visible slice (memory indicator plus a minimal send) first (2 to 3 days). C: go straight through the product path (5 days or more). **The owner chose A (2026-10-26).** The result did not change the order or the scope of the milestone; it changed some definitions and limits (changes v1.1 and v1.2). So B and C are not chosen separately: the work goes on along the phase plans already under way (from P02). The remaining uncertainties (the size of a real window screenshot, a real X program such as a browser, a heavy child process) are measured where product code exists: P04, P05 and S06. If the owner wants another order, it changes at once.

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
| 2026-10-26 | **The input echo (IN-08) is decided by the lead:** the host records an accepted input as lines that start with `>> ` (tool output already uses `> `), the first 12 lines and 2 KiB, each line cleaned like every other record, and one line with the exact amount left out. SC-10 moves from P03 to P02; only the sender header stays in P03 (changes v1.2). The owner can revert it. |
| 2026-10-26 | **P03 plan review (lead):** a reply between agents is allowed and a runaway is stopped by a hop limit (4) instead of a cycle ban; the handoff folder is one per receiving session; the person is asked once per call, not twice; reading another session's conversation needs an approval too; and the limit of all this is written down (SF-08: consent and least privilege, not a sandbox) (changes v1.3). The owner can revert it. |
| 2026-10-26 | **S01 result and the approval wait (lead):** the tool connection works with the installed Claude Code (6 of 6; 72 runs in all). A tool call that needs an approval is never held open: it answers at once and Lico tells the session when the person decides (AC-03). A hold is possible (a streamed reply with a per-server setting survived 10 minutes, *measured*) but it would depend on a setting of one program version and on the limits of every other agent program, it ends in "ask again" when the person is away, and it needs a streamed reply; the message when the person decides reuses the way a message reaches a session. The measured limits stay in the P03 plan note for a later bounded hold (changes v1.4). The owner can revert it. |

Rules that keep applying: development workers are Claude-family agents only. The idle window stays under 50 MiB and fast, in one executable. The screen tools use the hand-drawn client. New sessions skip the agent's own permission prompts and use approvals inside the window. The sign-in code is never visible or logged. The host replaces itself when it meets a new build and resumes the sessions that were alive.

## Working assumptions (change them on request)

The owner proceeded with the recommended option on 2026-10-26, so these values stand. A value changed by POC-0 measurement is written here with the date it changed.

| Item | Value | Used in |
|---|---|---|
| "X" | Linux X11. Wayland is out of scope (the X server tested is always recorded) | P04, P05 |
| How the agent calls Lico | A tool connection served directly by the host (MCP, per-session token) plus a CLI doing the same things. The tool connection works with the installed Claude Code (S01, 2026-10-26), so the CLI-only fallback is not needed | P03 |
| Inline size for sending | Up to 4 KiB inline, longer goes by file (2026-10-26 POC-0: the file way worked 16 of 16). The host's input limit (today 512 B of text, 4 KiB per request line) is raised first by IN-01 | P02, P03 |
| Input-box basics (approved) | Up to 64 KiB per input (newline and tab allowed), several lines (Enter sends, Shift+Enter inserts a newline), cursor movement, recall with the up key, per-session drafts, and what is sent is shown in the conversation folded (IN-08, added by the lead 2026-10-26). Today: 512 B, one line, no cursor movement | P02 |
| Handoff file retention | 30 days, 256 MiB in total over all handoff folders (oldest first when over) | P03 |
| Handoff folder (v1.3, 2026-10-26) | One per receiving session (an inbox), removed with the session; a session with permission prompts on reads only its own | P03 |
| Limits between agents (v1.3, 2026-10-26) | 4 hops since a person last wrote into the chain (replies allowed), 20 sends per minute, sessions created by agents alive at once: 4 per creating session and 8 in all | P03 |
| Approval wait (v1.4, 2026-10-26) | The tool answers at once ("the person has been asked"); the outcome reaches the session as a message; pending approvals expire after 12 hours, at most 8 per session (the P03 plan note may refine the numbers) | P03 |
| Host record retention (changed by POC-0, 2026-10-26) | Today 2,048 records. "The whole conversation" is built from the **provider's own record**, not the host record (the host record holds no input and is deleted in pieces). Whether to raise the host retention is decided in P03 by measurement | P03 |
| Virtual display | 1280x800, ended after 10 minutes idle | P05 |
| Real-screen permission | Per session, expires after 15 minutes, released when the host restarts | P05 |
| Memory refresh and colour | Every 3 s while a window is attached; colour change at 1 GiB (again at 2 GiB) | P02 |
| Definition of the memory value (changed by POC-0, 2026-10-26) | **Private memory (RssAnon)** summed over the agent and its descendants. RSS also shown on hover. PSS is not used (it moves up to 28 % between runs). Detached programs are not counted | P02 |
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
| Does the external tool connection work in the host's stream-json mode? | **It does** (POC-0 D3, 5 of 5, *measured*). S01 re-checked it on the installed version (2026-10-26, 6 of 6; the limits of a held call were measured, see the decision of that day) |
| Does a model operate the hand-drawn Lico window well? | Haiku did six small tasks 8 of 8 each (POC-0 D3, *measured*). The success rate on a real X program such as a browser is measured in the first step of P05 and recorded |
| Memory grows as features are added | A budget per feature, one measured line per phase, a P07 pass, only two stops |
| Tests get slow again | Test budgets, selection, receipts and size ratchet ([test plan](test_plan.md)). A phase over 180 s is not merged |
| Controlling the real screen can do great harm if wrong | Off by default, per-session permission, a conspicuous bar, 15-minute expiry, a key that stops it at once, an action log |
| The host record is short and holds no user input, so "the whole conversation" is cut or loses its questions | The whole conversation is built from the provider's record and an export states its range, so it never looks like the whole; P02 records inputs folded (SC-10) and P03 adds the sender header |
| The host silently drops a provider line over 256 KiB (a big screenshot) (*measured*) | First step of P04: a streaming reader that keeps the first 8 KiB or so and an "image N KiB omitted" record (XF-08) |
| A program a tool detached from the session survives deleting the session (one run, *measured*) | Not counted in this milestone and the tooltip says so. Ending them properly with one systemd scope per session is for the owner to decide at the P02 gate (MI-07) |
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
