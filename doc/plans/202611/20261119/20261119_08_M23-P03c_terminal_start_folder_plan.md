# M23-P03c Plan — The Terminal Starts In The Folder Directories Shows

- Status: done (2026-11-19), commit `ecc8772` (the prefix `phase/M23-P03c-` approved by the owner at commit time)
- Version: 1.0.0
- Branch: `phase/M23-P03c-terminal-start-folder` from `main` (`708bdd9`); the prefix `phase/M23-P03c-` needs the owner's yes at commit time
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), a fix after P03a
- The owner's words (2026-11-19, after M22-P06 was published): "터미널을 만들 때 현재 디렉토리가 안열리고 항상 홈으로 열리게 되어 있는것 같은데 작업 디렉토리가 열려야 함." (a terminal opens at home, not in the working directory, which is what must open)
- This phase changes no operation, no setting and no file format: it is the rule that picks the folder a new terminal starts in.

## What was found (2026-11-19)

- The rule (`terminal_start_dir`, M23-P03a) says in its comment "the folder of the session in front, else the one Directories is on", and the code reads `ui.saved_cwd`. That is not the folder Directories shows: `saved_cwd` is the window's *kept* folder (`last_cwd` of the host's settings), which only a session start, a chat resume and the Files screen write. Moving about in the Directories tab changes `ui.sidebar.cwd` and nothing else.
- So a person who only opens the window and walks in Directories has an empty or an old `saved_cwd`; with no session in front the rule falls to `None` and the host starts the shell at home. A probe of the old build confirmed it: the sidebar showed the folder the window was started in and the terminal tab's `cwd` was the home.
- The P03a window test could not see it: it wrote `last_cwd` through `settings.set` and so made the kept folder the very thing it then expected.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The order is: the folder of the session in front (this machine's), then **the folder Directories shows now**, then the last folder kept, else the home | "the working directory" is what the person sees; the session in front stays first because a shell for that conversation's project is what the menu entry always meant |
| D2 | The shown folder is not used when Directories is on an ssh host, in the Global scope, or when its last listing failed (`unreachable`); a candidate that is not a folder of this machine is skipped, not the end of the search (the next one is tried before the home) | a folder of another machine is not one the host can start in; a vanished folder should not cost the home when another is at hand |
| D3 | No new setting, no new operation, no version change; the fix is released with the next version | a one-rule fix |

## Steps

1. [x] **Tests first**: two Rust tests (`terminal/view_tests.rs`: the order; the skip rules) and one window test (`tests/test_terminal_tab.py`: no folder kept, and a folder moved to with another kept); red on the old build (`None`, and the terminal at the home while the sidebar showed another folder).
2. [x] **The rule** in `terminal/view.rs`.
3. [x] **Gates**: the changed-files suite (243 tests in 77 files, 243 passed, 127.3 s, peak 995.9 MiB), `cargo xtask check`, 9 mutation checks on the rule.
4. [x] **The owner's build** (`lico gui` shows the fix).

## Not decided here

- Whether the terminal tab's own working folder should follow later `cd` commands in Directories (it does not: the shell is the person's).

## Results (2026-11-19)

- **Red first**: the unit test of the shown folder failed on the old rule (`None` where the folder was expected); the window test failed on the old build (the sidebar showed the folder the window had been started in and the terminal tab's `cwd` was the home).
- **The rule** now takes, in order, the session in front's folder, the folder Directories shows, the last folder kept; the first that is a folder of this machine.
- **Two older window tests** assumed a fresh window opens a terminal at the home (the menu test, and the case of a kept folder that is gone). They are the proof of the owner's report, so they changed: the first keeps a folder and expects it; the second expects the terminal where Directories ended up (it falls back to a folder that exists) and still expects no refusal.
- **Mutation checks**: 9 changes of the rule (Directories' folder never used, an ssh host's, the Global scope's and a failed listing's folder used, the order of the three swapped in the two ways that matter, only the first candidate tried, a folder that does not exist used, a session on an ssh host used): all 9 caught, the clean run passes (59 tests of the terminal view).
- **Gates**: the changed-files suite 243 of 243 in 127.3 s (peak 995.9 MiB of 1536), `cargo xtask check` passes.
- **Seen on the way** (not changed here): when the kept folder is gone, Directories falls back to a folder that exists (by the code of its listing, the root `/`; the window test only checks that it is a folder of this machine), and so a terminal then starts there, not at the home. It is what Directories shows; the owner may want the home there instead.
