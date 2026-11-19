# M23 — A Terminal In Lico: Run An Agent In It, And It Comes Into Lico

- Status: in_progress (2026-11-18); the owner named the first step in the chat the same day: **choose Terminal in New session** (P01 to P03, P03a and P03b below are done; the owner looked at P03 with `lico gui` on 2026-11-19 and P03a is what the owner asked for then: the icon opens a shell at once, a shell that ends closes its tab, and the suite's memory brought down); the agent that comes into Lico from a terminal is a later phase, after the terminal works
- Version: 2.7.0
- Project: Lico
- Updated: 2026-11-19

## The owner's words (the source, in the chat, 2026-11-18)

"The next experiment is putting in a terminal. If an agent run in the terminal comes into Lico, things may become simpler still."

And, after M22-P04c: "New Session에서 Terminal을 선택할 수 있도록 하는게 다음 단계" (the next step is to make Terminal a choice in New session). The same message asks for a better first address of the browser tab and its look: that is M22-P05 and it follows.

## What it means (the lead's reading)

A terminal tab in the window runs a shell. The person chooses it in the New session wizard (a fourth icon beside Claude, Codex and Local LLM), picks the folder, and a tab opens with a shell in that folder. When a person starts an agent there (`claude`, `codex`), the agent may later become a Lico session (a later phase); the first step is only that the terminal is there and works well enough to run those agents' own screens.

## The research, answered (the lead's defaults; the owner may change any)

| # | Question | Answer | Why |
|---|---|---|---|
| Q1 | How does Lico learn that an agent started in a terminal? | **Not decided; the P04 plan note (2026-11-19) proposes the provider's own record files and the terminal's foreground process, and waits for the owner's choice of level.** Candidates stay as written: a Lico variable in the shell's environment that points at the host, a wrapper command (`lico run claude`), the provider's own record files (M11 reads them already) | the owner's first step is the terminal itself; the host owning the shell (D2) keeps every candidate open |
| Q2 | Terminal emulation: crate, size, memory, frame cost | **Written anew as a std-only component `lico-vt`** (the parser, the grid, the scrollback, the keys), and the pty through `extern "C"` as the rest of the host does (no `libc` crate, no new dependency). Memory: the scrollback is bounded by cells (about 10 MB at most), a screen of 120 x 40 is about 80 KB; the window draws only the rows it shows | the project pins exact versions and keeps few; the emulator is plain, testable code with no display; the component rules ask for a spec first and a receipt |
| Q3 | Security: a terminal is a shell for the person | The shell runs as the person with their environment, **not** in the host's fence (the browser tab's engine is fenced, a shell is not). The tab says so in its hint row and the wizard says it on its last step; the credentials of Lico are never put in the shell's environment | the provider's own permission model is what an agent in it obeys; Lico must not suggest a fence that is not there |
| Q4 | Does it replace the session tab? | **No: it sits beside it**, as the browser tab does (D1); the owner decides later whether one kind of tab is wanted | smaller and reversible; "simpler" can come after both exist |

## Decisions

| # | Decision | Why |
|---|---|---|
| D1 | A terminal is a tab of its own kind in the tab row, beside the session tabs and the browser tabs; it is not a session and not in the Chats list | a shell is not a chat; nothing of the session machinery (turns, approvals, resume) applies |
| D2 | The pty, the child and the emulator live in the host (`terminal.*` operations); the window draws what the host says and sends the keys | the same split as the browser; the shell survives the window closing and a second client sees it; every later candidate for Q1 needs the host |
| D3 | The entry is New session: its first step has a fourth icon, Terminal, and the menu tree has the same entry; Terminal needs no account (like Local LLM, never grey). A separate toolbar button is not added now | the owner's words |
| D4 | The Terminal icon in New session opens a shell **at once**, in the current folder (the session in front's, else the one Directories is on, else home): no wizard step and no form (changed in P03 to one step, and in P03a plan D13 to none: the owner looked at `lico gui` and wanted the menu's behaviour from the icon; a terminal has no name, no account and no branch, so a step would ask for something nothing keeps) | the owner's words 2026-11-19 |

## Phases

| Phase | What | Plan | Status |
|---|---|---|---|
| M23-P01 | the component `lico-vt` 0.1.0: the emulator (parser, grid, scrollback, alternate screen, replies) and the keys (keys, paste, mouse) as plain functions with no I/O and no `unsafe`; its spec first | [Plan](../../../plans/202611/20261118/20261118_10_M23-P01_vt_engine_plan.md) | done (`c536948`) |
| M23-P02 | the host: the pty and the shell (its own session and group, size, hang-up, a parent-death signal, the signals at their default action), the table of at most 8 terminals, the operations `terminal.open/list/close/input/resize/watch/text` (socket only: `POST /v1` refuses them), the `lico terminal` command | [Plan](../../../plans/202611/20261118/20261118_11_M23-P02_host_terminal_ops_plan.md) | done (`36ef1b1`) |
| M23-P03 | the window: the terminal tab (draws the grid, the keys, the size follows the window, the wheel, copy and paste), the fourth icon in New session, the menu entry, the hint that says what a terminal is | [Plan](../../../plans/202611/20261119/20261119_01_M23-P03_window_terminal_plan.md) | done (`3cf3b6b`) |
| M23-P03a | after the owner's look: the Terminal icon opens a shell at once in the current folder (no wizard step), a shell that ends closes its own tab, and the suite's memory inspected and brought down (the build script's missing paths that made cargo rebuild the program every time, the sessions a test file kept alive, the unit tests' threads) | [Plan](../../../plans/202611/20261119/20261119_02_M23-P03a_terminal_start_and_suite_memory_plan.md) | done (`46da95f`) |
| M23-P03b | the suite's time and build memory after the owner's word that the limit must not be tight: the guard's 30 s quiet time shortened for the test that waited it (`LICO_GUARD_TYPING_QUIET_MS`), three duplicate tests removed, the debug information of the unit tests' binary cut to the lines (the build's peak 1727 to about 1160 MiB), the budget table with no exception left | [Plan](../../../plans/202611/20261119/20261119_03_M23-P03b_test_diet_plan.md) | done |
| M23-P03c | the owner's report after P06: a new terminal opened at the home, not in the working directory; the start folder is now the session in front's, else the folder Directories shows (which `last_cwd` did not follow), else the last one kept | [Plan](../../../plans/202611/20261119/20261119_08_M23-P03c_terminal_start_folder_plan.md) | done (`ecc8772`) |
| M23-P04 | an agent started in a terminal comes into Lico (Q1): research done (`agy` is not connected today; three levels of "connected"), the plan proposes A (the tab knows what runs in it) and B (the agent's conversations in Chats, one click back into a terminal) first; the owner chooses the level | [Plan](../../../plans/202611/20261119/20261119_10_M23-P04_agent_in_terminal_plan.md) | plan written, waits for the owner's choice |

The order with M22: the owner named the terminal first and asked for the browser address bar after it, so M23-P01 to P03 come before M22-P05 (the browser tab) and M22-P06 (the guide and the release `0.8.0`, which then ships both).

## What this milestone does not do yet

The agent's entry into Lico (P04), a split view of two terminals, a tab that moves between windows, a terminal on an ssh host (the host is local), a settings page for the shell and the font, and the replacement of the session tab.
