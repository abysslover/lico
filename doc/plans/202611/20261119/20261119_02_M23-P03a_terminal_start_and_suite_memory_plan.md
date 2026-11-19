# M23-P03a Plan — The Terminal Icon Opens A Shell At Once, A Shell That Ends Closes Its Tab, And The Suite's Memory Brought Down

- Status: done (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M23-P03-window-terminal` (on `e72d1f3`; the prefix `phase/M23-P03-` was approved for P03 and this is its follow-up, so no new prefix)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md)
- The owner's words (the chat, 2026-11-19, after looking at P03 with `lico gui`): "지금 이상한게 아이콘 눌렀을 때 터미널이 2개 단계 통해서 생성되는지 그냥 메뉴에서 눌렀을 때처럼 현재 디렉토리기준으로 바로 터미널 생기도록 수정 종료했을 때 종료 후 탭이 자동 없어져야하는데 지금은 남아 있음. 그것을 추가하고, 스위트 최대 메모리 증가에 대해서 원래는 모듈별로 테스트를 완료하고 모듈 버전을 관리하면서 모듈에 있는 것을 동시에 검사하는 등의 동작을 하지 않도록 테스트를 해서 메모리 소비 등을 줄이도록 했는데, 이것은 검사를 통해서 이 마일스톤 지나가기 전에 줄여야 할 듯." (the icon should open a terminal at once in the current folder, as the menu does; a tab must go away when its shell ends; and the suite's peak memory has grown against the original design of testing module by module with versions, so inspect it and bring it down before this milestone passes)

## What this phase changes

1. **The Terminal icon opens a shell at once** (D13, D14): no wizard step, no folder row. The one function that the menu's `New terminal tab` uses opens it, in the current folder.
2. **A shell that ends closes its own tab** (D15): the window tells the host to give the terminal's place back; a shell that ended with a failure or a signal says so for a moment on the header's line; one that ended with `exit` or Ctrl+D just goes.
3. **The suite's memory** (D16 to D19): the peak was inspected; three causes were found and fixed, and a fourth, which is structural, is recommended to the owner (below).

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D13 | The Terminal icon in New session calls `panel_close()` and `terminal_open_here()`; the wizard has no terminal step and no terminal state (`Wiz` is `{ page, home }`). This replaces the milestone's D4 (the wizard for a terminal has one step) and plan D10 of P03 | the owner's words; a terminal has no name, no account, no branch, so a step would ask for something nothing keeps |
| D14 | The current folder is the folder of the session in front when it is on this machine and not empty, else the folder Directories is on (`saved_cwd`); it counts only if it exists on this machine as a folder; else the host opens the shell in the person's home. The menu entry uses the same function | "the current directory, as when pressed in the menu"; the Directories fallback is a small widening of what the menu did (it asked the session only): a window with no session in front now opens where the person is looking |
| D15 | When a watch (or the first list after a start) finds the shell ended, the tab is closed at once and the host is sent `terminal.close`, so the table of eight gets its place back. A flash on the header's line only for an exit other than 0 (`The terminal's shell ended (exit N).`) or none (`The terminal's shell ended.`, a signal); `exit` and Ctrl+D say nothing. A terminal the host lists as ended when the window starts is closed in the host and gets no tab | the owner's words; a person who typed `exit` knows; a crash should not vanish silently |
| D16 | The build script of the program (`src/host/hostd/build.rs`) names only paths that exist: `../../../Cargo.toml` and `../../../Cargo.lock` (they were `../../...`, which is `src/`, where neither exists). A test (`tests/test_build_commit.py`) runs the script in the real package and fails on any watched path that is missing | cargo takes a missing path as changed on every build: the program was compiled and linked again each time, including inside the suite's cargo job in the middle of the window tests, with a link of 1.07 GiB (rust-lld) and rustc 0.36 GiB on top of the workers; the owner's own `cargo build --release` was rebuilding an unchanged tree as well |
| D17 | A test file that keeps one host for all its tests ends the sessions each test made (`lico_measure/sweep.py`, `end_all`: `session.purge` of every session the host has, an autouse fixture after each test). Applied to the six files that make the most: `test_tool_send`, `test_tool_create`, `test_tool_connection`, `test_tool_history`, `test_transfer`, `test_conversation`, and `test_agent_tools` | a session is a program (the Claude simulator is a Python interpreter of about 20 MiB) and the host keeps it until the file ends: a file of 25 tests that made 52 sessions held 46 at once, 1 GiB for two seconds of tests |
| D18 | The unit tests of the program run at most four at a time in the suite's cargo job (`RUST_TEST_THREADS=4`, `TEST_THREADS` in `suite.py`; the default is one a core, 24 here) | measured: 875 MiB and 17 s with 24 threads, 569 MiB and 39 s with 4; the job runs beside three other workers, so the longer job costs no wall time |
| D19 | The suite's summary says who held the peak (`the peak was held by: ...`, `peak_split_mib` in the ledger) and judges the tests' peak apart from the build's (the build is alone, so a build peak is no overlap) | "the inspection" the owner asked for: a peak that is named can be argued with; before this the line named only what was running |

## Steps

1. [x] **Tests first**: the unit tests of the wizard (the icon opens at once, the panel closes, no terminal step), of the start folder (the session's, an ssh session's is not used, an empty one, the Directories fallback, a folder that does not exist), of the end of a shell (the tab is closed and the host told; the flash for a failure, for a signal and none for exit 0; a list that finds an ended shell closes it and adopts nothing); the window tests in `tests/test_terminal_tab.py` against a real host and shell.
2. [x] **The code** (`terminal/view.rs`, `terminal/screen.rs`, `newsession_wizard.rs`, `newsession.rs`).
3. [x] **The inspection**: the cargo jobs' memory measured on their own (`scratchpad` probe: the tree's resident sets every 100 ms), the large test files measured the same way, the cause of each found, then fixed and measured again.
4. [x] **Mutation checks** on the new and the rewritten code.
5. [x] **Gates**: `cargo test --bin lico`, clippy on `gui/terminal`, `cargo xtask check|verify|graph`, the cold suite, the owner's build.

## Results (2026-11-19)

- **The terminal** (commit `46da95f`): the Terminal icon closes the panel and opens a shell in the current folder; the wizard lost its terminal step (`Wiz` is `{ page, home }`, `terminal_problem`, `wizard_terminal_start` and the placeholder line are gone). `terminal_start_dir` serves the icon and the menu. `terminal_ended` closes the tab of a shell that ended, tells the host `terminal.close` and flashes only for a failure or a signal; the list at start closes an ended shell and adopts nothing. The unit tests of `gui::terminal`, the wizard and the menu were rewritten (117 in those modules; the whole `cargo test --bin lico` is 953, all passing); `tests/test_terminal_tab.py` has 8 real-host window tests (the icon opens at once in the folder Directories is on, and at home when that folder is not of this machine; `exit` closes the tab and says nothing; `exit 3` closes it and says `The terminal's shell ended (exit 3).`; an ended shell found at start is closed in the host and gets no tab).
- **Mutation checks**: 54 on the new and the rewritten code, 53 caught. The one that survived (A09: the list at start drops the ended shell's tab but does not tell the host) cannot be seen by a unit test, because the unit tests' window has no socket; the same edit is caught by the window test added for it (A09b). Found on the way: the harness restored a mutated file with its old modification time, so cargo took the mutant's binary as fresh, and the suite's build stamp then recorded the mutant's binary against the restored sources: the cold run failed on a unit test and a window test that were right. The harness now sets the time on restore; the stamp was removed to force the build.
- **The inspection** (the tree's resident sets every 100 ms, per job and per test file; before and after):

  | What | Before | After |
  |---|---|---|
  | the cold suite, the tests' peak | 1651.7 MiB | **1161.4 MiB** |
  | the changed-files run, the peak | 2010.6 MiB (of 2048) | not run again; its parts are below |
  | the program's unit tests (`cargo test --bin lico`) | 875 MiB, 17 s, and a 1.5 GiB compile and link inside the job | 575 MiB, 36 s, nothing compiled |
  | `test_tool_send.py` | 1006 MiB | 104 MiB |
  | `test_transfer.py`, `test_tool_create.py`, `test_tool_history.py`, `test_conversation.py`, `test_tool_connection.py` | 533, 548, 415, 425, 393 MiB | 100, 135, 96, 131, 142 MiB |
  | the build before the tests | 1.2 GiB, alone | 1.18 GiB, alone (judged apart now) |
  | a release build of an unchanged tree | a full compile and link every time | 0.08 s |

  Who held the cold run's peak: `cargo test --bin lico` 575 MiB, `test_memory_guard_window.py` 242 MiB (by design: sessions that hold over 500 MB), `test_host_replacement.py` 163 MiB, `test_terminal_tab.py` 133 MiB. The window tests are 130 to 290 MiB each (Xvfb, the window and the host), which is what four of them cost together.
- **The bound** in `tests/budget.toml` is now 1536 MiB (it was 2048): the tests and the build are each 1.18 GiB or less, and the guard test in `tests/test_selection.py` holds the number. The lead lowered it because the owner asked for the memory to come down before the milestone passes; one line puts it back.
- **Time, the other limit**: the cold run is 177 s of 180 s. The four workers are balanced (174.5 to 177.2 s each, 703 s of work in 149 jobs), so the wall time is the total amount of testing and only less work lowers it. The thread cap of the unit tests costs about 1.4 s of wall time. Runs with a failing test ran 183 and 188 s, so the guard trips at ordinary noise: this is the next thing to look at, and it is the owner's (the limit or the work).
- **Found on the way**: besides the harness, a clean-clone acceptance run is due because `build.rs` is one of its triggers (`python -m lico_measure.suite acceptance`, about 3.5 GiB, after the commit, since it clones the committed tree); `pri-heavy`'s "left 4 processes behind" is the suite's baseline and not a fault of these tests.
- **Not changed**: the clippy errors in `gui/editor/app.rs` and `llm/net.rs` are older than this phase and outside the terminal; `gui/terminal/` has no finding.


## Not decided here (the owner's)

- Whether the suite's bound of memory (`peak_rss_mib` in `tests/budget.toml`) is to be lowered to what the suite now needs, which is the diet the owner has asked for in words; the lead recommends it (see the results).
- The structural part of the owner's remark: the program is one executable of 224 modules, so a change in any of them selects its 953 unit tests and every window test of its area. The components with receipts (`lico-vt`: 20 MiB, 1.2 s) are the way the cost stays low; the terminal's pure code (`screen.rs`, `keys.rs`) and the other pure parts of the window are candidates for the next components. That is a refactor of its own (the `/refactor` method: the interface specification first), and the lead recommends it as a phase of its own after M22.
