# Requirements: what is built and how it is checked

Status: v1.3 (v1.2 plus the P03 plan review: reply rule, handoff folder per session, one approval per call, SF-08), v1.0 approved by the owner at G1 on 2026-10-26. All requirements below are *planned* (P01 items excepted). Each one has a **check** and the **phase** it belongs to; the status changes in these tables as phases proceed.

Related documents: [planning](planning.md) · [screens](screens.md) · [test plan](test_plan.md) · [changes](changes.md).

## How to read

**Check codes**

| Code | Meaning | Rough cost |
|---|---|---|
| U | Unit test: one function or module | milliseconds |
| P | Protocol test: the host socket, no display | seconds |
| W | Window test: draw to a surface and compare pixels or a dump | seconds |
| E | End-to-end test: a real X server and several processes (only a few are kept) | seconds to tens of seconds |
| M | Measurement: a number written in the report | |
| L | The lead checks it by hand in the real window | |

**Principle:** one "main path test" per feature is the default. A test that compares every combination does not belong in these tables. The status words are those of the [planning document](planning.md).

**P01** (menu bar, fonts, scroll, activity indicator, interrupt and answer ordering) follows the P01 plan; only new decisions are listed here. The test cleanup (S05) is the TST group.

## WK: work folder (P02)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| WK-01 | A session runs its agent in the **real folder** chosen at creation (cwd). Without one it uses the host-made scratch folder as today and the row shows `scratch`. Claude Code and Codex both. | P | P02 | planned |
| WK-02 | The folder must be an existing directory. Otherwise the window shows the reason and no session is created. | U, W | P02 | planned |
| WK-03 | The new-session screen offers the 10 most recent folders. The list is kept only in the host and sent nowhere. | P | P02 | planned |
| WK-04 | Resume uses the same folder. If the folder is gone, resume is refused and the reason is shown. | P, W | P02 | planned |
| WK-05 | A session with permission prompts on (`default`) may read and write only its work folder and its own Lico handoff folder (`--add-dir`; one per session, SC-05). | P | P02, P03 | planned |
| WK-06 | **Deleting a session removes only the scratch folder the host made. A folder the user chose is never deleted or modified.** (Today the delete code removes `workspace_root/<session>` as a whole, which becomes dangerous the moment a folder can be chosen.) | P | P02 | planned |

## SX: creating and managing sessions (P02)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| SX-01 | One screen: folder, name, account, model, effort, permission, Start. The last used values are the defaults. | W, L | P02 | planned |
| SX-02 | Model and effort choices come from what the installed Claude Code accepts (not hard-coded). How they were found out is written in the plan. | P, M | P02 | planned |
| SX-03 | Permission: by default as today (the agent's own prompts are skipped; only sensitive actions defined by Lico ask in the window). Choosing "ask" brings the agent's prompts into the window's approvals. | P, W | P02 | planned |
| SX-04 | Close: while the session is working the window asks "stop and close?". After closing, the conversation stays resumable. | W, P | P02 | planned |
| SX-05 | Delete: after a confirmation in the window the log and the scratch folder are removed (WK-06 holds). | P, W | P02 | planned |
| SX-06 | Rename (proposal: F2, double click on the row): stored in the host and visible in other windows. | P, W | P02 | planned |
| SX-07 | Every new action is reachable by at least two of: menu, shortcut, CLI (menu rule: each bar item shows its shortcut; an item that cannot act shows why). | W | P02 | planned |

## MI: memory per session (P02)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| MI-01 | The **right end** of each session row shows the memory of that session, like `312 MiB`. | W, L | P02 | planned |
| MI-02 | Value = **private memory (RssAnon)** summed over the agent process and **all its descendants** (tools, builds, a virtual display). The tooltip also shows the RSS. PSS is not used (the PSS of one idle session moved by up to 28 % between two runs, RssAnon by about 5 %, *measured*). Programs a tool has detached from the session (setsid and double fork) are not counted and the tooltip says so. S06 tests whether shared memory (RssShmem) belongs in the sum. | U, P | P02 | planned |
| MI-03 | The host samples every few seconds (3 s by default) only while a window is attached. With no window it does not measure. | P | P02 | planned |
| MI-04 | The colour changes above 1 GiB and again above 2 GiB. | U, W | P02 | planned |
| MI-05 | One line at the bottom of the list shows the total: sessions sum, host, window, in the same definition as the rows. The phase report gives RSS and private memory for host and window, both. | W, M | P02 | planned |
| MI-06 | Measure the cost of looking itself (host CPU and window RSS growth with 10 sessions, with and without a window attached). The Python figures of POC-0 (reading 6 processes: private memory 1.0 ms, PSS 13 to 18 ms) are upper bounds only; the Rust cost is stated. It is recorded; the limit is judged under MB. | M | P02 | planned |
| MI-07 | **Proposal; the owner decides at the P02 gate:** closing or purging a session ends everything it started, detached programs included (POC-0: `session purge` left detached programs running, one run, *measured*). Proposed way: one systemd scope per session in its own slice (`systemd-run --user --scope`, `memory.stat` for the figure, `cgroup.kill` to end it). | P, E | own step | proposed |

## IN: input-box basics (P02)

Found by re-reading the code (2026-10-26). The host accepts only **512 bytes on one line** per input (control characters such as newline and tab are refused), and one request line is at most 4 KiB. The window sends the draft without checking any limit, and the input line has no cursor movement (only Backspace and Ctrl+U). Long prompts, pasted logs and multi-line input therefore cannot be sent. This is a basic of agentic coding, so it was put into P02 (owner approval 2026-10-26, recommended option A). The "long goes to a file" rule of P03 also only makes sense once this limit is raised.

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| IN-01 | The host accepts up to **64 KiB** of text per input (the request-line limit and the local HTTP body limit are raised to match) and allows newline and tab. Other control characters are still refused. Window, CLI and local HTTP use the same limit, and multi-line text reaches both providers (Claude Code, Codex) intact. | U, P | P02 | planned |
| IN-02 | The input box has several lines: Enter sends, Shift+Enter inserts a newline. The box grows with the number of lines up to 8 and scrolls inside. | W | P02 | planned |
| IN-03 | Cursor movement: left/right, Home/End, Ctrl+left/right (word), up/down (line). Hangul composition (IME) keeps working as today. | U, W | P02 | planned |
| IN-04 | Deleting: Delete, Ctrl+Backspace (word), Ctrl+K (cursor to end). The existing Backspace and Ctrl+U (clear all) stay. | U | P02 | planned |
| IN-05 | Multi-line paste: text with newlines enters the box as it is. Over the limit it is not pasted and the reason is shown. | W, P | P02 | planned |
| IN-06 | A draft over the limit is **neither dropped silently nor sent**: one line under the box says why and the draft stays. Long text is better saved to a file and sent as a path (the SC-04 rule). | W | P02 | planned |
| IN-07 | With the box empty or on its first line, the up key recalls what this session was sent before. Drafts are kept per session when you switch sessions. | W | P02 | planned |
| IN-08 | What is sent appears in the conversation at once (the input echo, the P02 part of SC-10): the first 12 lines or 2 KiB as lines that start with `>> ` (tool output lines start with `> `), and when more was sent one line with the exact amount left out. The input box has no echo code of its own; it draws the records the host gives. **Added by the lead on 2026-10-26 03:40 (changes v1.2).** | U, W | P02 | planned |

Out of scope: selection, placing the cursor with the mouse, undo (M03 candidates).

## SC: sending between sessions (P03)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| SC-01 | Send the text of the input box to another session (Session > Send to session...). A picker for the receiving session opens. | W, P | P03 | planned |
| SC-02 | Send the selected lines to another session. | W, P | P03 | planned |
| SC-03 | Hand a whole conversation over to another session (Hand over conversation...). | W, P | P03 | planned |
| SC-04 | **Size rule:** up to 4 KiB is delivered as it is, with a header (after IN-01 has raised the host input limit; today the host takes 512 B at most). Anything longer is saved as a file in the receiving session's handoff folder (SC-05) and that session gets a short notice (source, time, size, path, first lines) and one line on how to read it (the Read tool refuses over about 25,000 tokens, so the notice says to read in parts; in POC-0, 5 of 16 runs first hit that limit and all recovered, *measured*). **Nothing is cut.** The sending tool checks the exit code: a refused send exits 2 and the receiving session does nothing. | U, P | P03 | planned |
| SC-05 | **Each session has its own handoff folder** (an inbox), under Lico's state folder outside the repository (folder 0700, files 0600, laid out by receiving session and date). It is made when the session starts and removed with the session (SF-07). It is opened with `--add-dir` for **that session only**, so sessions with permission prompts on can read it (POC-0, with one folder shared by all: with the folder opened, 0 of 4 runs asked for approval; without it, 4 of 4 did, *measured*; a folder per session is the same mechanism and is checked again in P03). A session with permission prompts on can read what was sent to it and nothing sent to another session, including a whole conversation that holds secrets. A session whose program has permissions skipped can read any file of the user, as it can today, so for it the split is least privilege and not a sandbox (SF-08). | P | P03 | planned |
| SC-06 | Retention: files older than 30 days are removed, and when the total over all handoff folders exceeds 256 MiB the oldest go first. The notice says the file may be removed later. | U | P03 | planned |
| SC-07 | The whole conversation is built from the **provider's own record** (read by the adapter), not from the host record: the host record holds no user input and is deleted in pieces of a quarter of its retention (*measured*). The file states the first and last record numbers and, if older records have already been pruned, says so (**a pruned record never looks like the whole**). Tool calls and results are included unfolded; images are listed by size only. Whether to raise the host record retention (2,048 records today) is decided in P03 by measurement. | U, P | P03 | planned |
| SC-08 | The receiving session sees clearly, by the header, that this is "sent by another session" and not text a person wrote. The window also shows `from <name>`. | W | P03 | planned |
| SC-09 | Transfer log: who sent what (kind, size) to whom and when is kept in the host log. The content is not kept. | P | P03 | planned |
| SC-10 | The host records the **inputs** it receives (typed by a person, sent by another session). **P02 (input echo):** on accepting an input the host records up to the first 12 lines and 2 KiB of it as records that start with `>> ` (tool output already uses `> `), each line cleaned like every other record (a tab becomes a space; bidi and line-separator characters are removed), and adds one line with the exact count and size of what was left out ("the agent received all of it"). It never cuts silently. The full text is not kept in the host record (the provider's record holds it and SC-07 builds from there). A repeated request id echoes once; a refused input echoes nothing. **P03:** text sent by another session carries a header (`from <name>`). Today inputs are not in the host record, so the conversation shows an answer with no question (*measured*). | U, P, W | P02, P03 | planned |

## AC: Lico tools the agent can call (P03)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| AC-01 | The host serves a **tool connection** (MCP, loopback HTTP) guarded by a per-session token and attaches it with `--mcp-config` when the session starts. | P, E | P03 | planned |
| AC-02 | Tools: list sessions, send to another session, read another session, create a session. Each has a CLI doing the same thing (people and tests use the same path without a model). | P | P03 | planned |
| AC-03 | Sensitive tools (create a session, send to another session, read another session's conversation, write to the screen) go through an approval in the window, **once per call**: a session whose program also asks before running a tool must not show the person two prompts for one call (P03 step S01 measures how to achieve this). A session can be given "keep allowing for this session". | W, P | P03 | planned |
| AC-04 | Runaway protection: (1) a per-session sends-per-minute limit; (2) a **hop limit on exchanges between agents**: every message an agent sends carries the number of agent hops since a person last wrote into the chain, a person's input restarts the count, and a send beyond 4 hops is refused. A reply is allowed (A sends to B, B answers A: hops 1 and 2); an exchange that keeps going stops at the limit; (3) the sessions created by agents that are alive at once are capped (4 per creating session, 8 in all), and "keep allowing" never lifts the cap. When a limit is hit, the tool returns the reason with the numbers. | U, P | P03 | planned |
| AC-05 | No new process per session (the connection is served inside the host). | M | P03 | planned |
| AC-06 | The token is new for every session and every start, the config file is 0600 and removed when the session ends. It appears in no log and no screen. | U, P | P03 | planned |
| AC-07 | **The first step (P03-S01) is a test:** check that the installed Claude Code really uses this tool connection in `-p stream-json` mode. If not, go with the CLI only (the agent calls `lico ...` from a shell) under the same approval rules. | E | P03 | planned |

## XF: screen capture and copy/paste (P04)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| XF-01 | X11 only (Wayland is out of scope). The X server and version tested are written in the report. | L | P04 | planned |
| XF-02 | Capture: the whole screen, a window or a region, saved as PNG with its path and size returned. The tool for agents also returns a copy shrunk to at most 1568 px on the long side (image tokens grow with width x height / 750 plus about 200, not with file size; 1568 is better for text-heavy windows, *measured*). A region capture also returns its **offset and scale** (in POC-0 a click missed because a region's coordinates were read as full-screen ones). | E, U | P04 | planned |
| XF-03 | Capture is read in bands (a few rows at a time) and shrunk on the way, so no large buffer is built at once. The target is at most 6 MiB extra, temporarily, at 1080p. | M | P04 | planned |
| XF-04 | Clipboard read and write (text, image). The window owns the clipboard. With no window it fails with a reason. | E, P | P04 | planned |
| XF-05 | Image paste: Ctrl+V of an image in the input box attaches it to the next message and it reaches the agent. There is a size limit and a notice. | W, E | P04 | planned |
| XF-06 | Middle-button paste (the selection) is supported. | E | P04 | planned |
| XF-07 | X work is done by a short-lived helper process of the same executable that disappears when the job ends (standing cost 0). | M | P04 | planned |
| XF-08 | **A screenshot never vanishes from the record without a trace.** When one provider line is over 256 KiB (real window captures mostly are), the host today drops the whole line and leaves nothing (*measured*, 6 of 6). After the fix the record says "image N KiB omitted". The line is read as a stream that keeps its first 8 KiB or so and discards the rest, so no large buffer is built. The record holds `[image]` and not the provider's folder path. | U, P | P04 | planned |

## CU: computer use (P05)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| CU-01 | `lico display` starts a per-session **virtual display** (Xvfb, 1280x800 by default) on first call and returns its name (DISPLAY). It is stopped when the session ends or after 10 idle minutes. | E | P05 | planned |
| CU-02 | Actions: click (left, right, middle, double), move, drag, type text, key combinations, scroll, wait, look at the screen. Done through XTEST. | E | P05 | planned |
| CU-03 | After an action a shrunk screen is returned. Coordinates are in the shrunk image; the scale conversion is documented. | U, E | P05 | planned |
| CU-04 | The agent can start programs on the virtual display (`DISPLAY=$(lico display) command` or `lico display run`). | E | P05 | planned |
| CU-05 | The **real screen** is used only when the user allows it per session. While on there is a conspicuous bar in the window, it expires automatically after 15 minutes, there is a key that stops it at once, and each action is logged (the screen contents are not logged). | W, E | P05 | planned |
| CU-06 | Real-screen permission is per session and is released when the host restarts. The default is off. | P | P05 | planned |
| CU-07 | **The first step (P05-S01) is a test:** have the agent do a simple task on the virtual display (open a browser, read the title) and record the success rate. Small tasks in Lico's own window passed in POC-0 (Haiku, six tasks, 8 of 8 each, *measured*). The success rate on a real X program such as a browser is not yet known. | M | P05 | planned |
| CU-08 | The virtual display has no window manager, so keys are ignored until focus is given by hand (*measured*). Whether a real X program (a browser, a dialog) needs a tiny window manager, and its memory (RSS), is **decided by P05 after measuring** (no guess is built in). | M | P05 | planned |

## WT, NT, SR: everyday comfort (P06)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| WT-01 | After choosing a repository folder, "start in a new git worktree" can be chosen: give a new branch name and the worktree is created and the session starts in that folder. | E, P | P06 | planned |
| WT-02 | When a session is closed or deleted the user is asked whether to keep or remove the worktree. With uncommitted changes there is one more confirmation before removal. The branch is kept. | E | P06 | planned |
| NT-01 | When a background session has finished or waits for an approval, its list row shows an attention mark (a dot) and the window title carries a hint. Input echo records (they start with `>> `) are not new output and never raise it. | W | P06 | planned |
| NT-02 | Desktop notifications use `notify-send` when it exists (no new dependency). There is a setting to turn them off. | E | P06 | planned |
| SR-01 | Search inside the current conversation (proposal: Ctrl+F): next and previous with highlighting. | W | P06 | planned |
| SR-02 | Search across all sessions: choosing a result goes to that line of that session. No index stays resident; the records are scanned when needed. | P, W | P06 | planned |

## SF: safety (all phases)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| SF-01 | Sensitive actions (real screen, create a session, send to another session) do not run without an approval in the window (a per-session pre-allow excepted; what this does not cover is stated in SF-08). | P, W | P03 to P05 | planned |
| SF-02 | The sign-in code appears neither on screen nor in any record (existing rule kept). | P | all | planned |
| SF-03 | Lico actions started by an agent are kept in an audit record (what, which session, when). Message bodies and screen contents are not kept. | P | P03 to P05 | planned |
| SF-04 | Public hygiene: handoff folders, capture folders and tokens are not placed inside the repository. Tests and documents contain no workstation paths, names or raw conversations. | U, L | all | planned |
| SF-05 | The tool channel (P03 to P05) targets Claude Code sessions only. Other providers are out of scope. | P | P03 to P05 | planned |
| SF-06 | Real-screen control and clipboard writing are off by default. | P | P04, P05 | planned |
| SF-07 | Lico never deletes or modifies a folder the user chose. It removes only the scratch, handoff and capture folders it made (WK-06). | P | all | planned |
| SF-08 | **Approvals and handoff folders are consent and least privilege, not a sandbox.** A session whose program has permissions skipped runs shell commands as the owner: it can read the owner's files and call the `lico` command line as the owner does. Lico stops unasked use of its own tool channel (SF-01), keeps each session's handoff folder to that session where the program asks first, records what agents do (SF-03), and says this limit plainly in the user documentation. A sandbox per session is outside M02. | L | P03 | planned |

## MB: memory budget (design targets, before measurement)

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| MB-01 | **Idle window RSS under 50 MiB** (both scales). Today 23.6 / 34.6 MiB. A miss stops the work. | M | all | measured |
| MB-02 | Growth of at most 10 MiB in 10 minutes with the window hidden. A miss stops the work (leak). | M | all | planned |
| MB-03 | Per-feature design targets: P02 window +1 MiB at most; P03 window 0 and no new process per session; P04 temporary +6 MiB at most (returned afterwards); P05 only for the session that asks and ended with the session. | M | P02 to P05 | planned |
| MB-04 | One line in every phase report: idle window (both scales), idle host (10 sessions), the number for that feature. | report | all | planned |
| MB-05 | Overshooting a target does not block a phase: it is recorded and reduced in P07. The only stops are MB-01 and MB-02. | rule | all | planned |
| MB-06 | P07: re-measure MB-01 and MB-02 with every feature on, reduce, and review the whole milestone. | M | P07 | planned |

## TST: test discipline (from P01-S05, all phases)

Details and reasons are in the [test plan](test_plan.md).

| ID | Requirement | Check | Phase | Status |
|---|---|---|---|---|
| TST-01 | The whole suite runs from scratch in **at most 180 s** (build already done). Ledger before cleanup (work report 025a): 135 tests, 596 s cold. After S05: 140 tests, 112 s (*measured*). A phase over 180 s is not merged. | M | S05 on | measured |
| TST-02 | The peak of the memory sum of all processes during the whole suite is at most 2 GiB (proposal). Each test's time and peak memory go in a ledger. | M | S05 on | planned |
| TST-03 | A **module-to-test map** and `--changed` selection: normally only the tests affected by the changed files run. A changed file missing from the map runs everything (default deny). | U | S05 | planned |
| TST-04 | **Receipts:** a test whose inputs (test file, mapped sources, binary, fixtures) hash the same and that already passed is skipped. A full (`--cold`) run does not use receipts. | U | S05 | planned |
| TST-05 | One build per run; every test uses that binary and the shared fixtures (host, X server). Heavy (end-to-end) tests are few and marked. | M | S05 | planned |
| TST-06 | One test takes at most 15 s; more needs a reason in the table. The budget check reports an over-budget test by name and fails if the whole suite is over 180 s. | U | S05 | planned |
| TST-07 | Source files stay at or under 800 lines. Files over it (`gui/ui.rs` 1,535, `core.rs` 1,169, `gui/mod.rs` 933) are listed with their current length, **may not grow**, and the phase that touches one splits that part out. | U | S05 on | planned |
| TST-08 | The acceptance test that proves the build recipe (two clean-clone builds) leaves the daily run and runs when recipe files change, at the end of a phase and before a publication. | U | S05 | planned |
| TST-09 | A new test goes through a checklist (is there an overlapping test, can a smaller unit do it, which module does it map to, time and memory, shared fixture or not). Exhaustive comparison stays with the lead's acceptance tools and is not put in the suite. | L | all | planned |
| TST-10 | One line in every report: whole-suite time (from scratch), usual run time, peak memory, number of tests, what grew. | report | all | planned |
| TST-11 | A flaky test is fixed in the same phase or quarantined with a reason. It is not hidden by rerunning. | L | S05 on | planned |
