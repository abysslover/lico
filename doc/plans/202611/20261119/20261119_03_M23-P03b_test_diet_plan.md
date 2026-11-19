# M23-P03b Plan — The Suite's Time And Build Memory: What Waited, What Was Duplicate, What The Build Held

- Status: done (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M23-P03b-test-diet` from `main` (`a063ea8`); the prefix `phase/M23-P03b-` approved by the owner at commit time (the chat, 2026-11-19)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md)
- The owner's words (the chat, 2026-11-19, after looking at P03a with `lico gui`): "exit는 잘됨 commit merge push 등을 하면 될 것 같음. 시간은 한도를 두고 빠듯하지 않도록 테스트 자체도 최적화 하는 목표는 계속 유지함. 테스트 중 의미없거나 중복되는 것도 제거, 그 다음으로 M22-P05 진행" (exit works; commit, merge and push; the goal stays that the suite has a time limit and is not tight against it, and that the tests themselves are optimised; remove the tests that mean nothing or repeat another; then M22-P05)

## What this phase changes

1. **Waits that were real time are shortened** (D20, D21): the longest test of the suite waited 30 s for the guard's quiet time after typing; it now waits 1 s, set by the test through a variable the host reads.
2. **Duplicate tests are removed** (D23): three tests whose whole content another test holds.
3. **The build's memory** (D22): a rebuild after a change peaked at 1.5 to 1.7 GiB (the bound is 1.5); the debug information of the unit tests' binary was the cause and is cut to the lines.
4. **The budget table has no exceptions left** (D24).

## What was found (measured 2026-11-19)

- The cold run is **703 s of work in 149 jobs on 4 workers**; the 609 pytest tests are 600 s of it before this phase and 557 s after (the tests' own `wall_s`), the rest is the Rust jobs (the program's 953 unit tests 34 s, the components' tests 3 to 7 s each, `cargo xtask`). A test starts a window in 0.3 to 0.8 s (Xvfb 0.09 to 0.3 s): the windows' start is not where the time goes.
- **The time is in the number of tests, not in a few slow ones**: the 45 slowest tests are 242 s of 557, and the other 564 tests average 0.56 s. No removal of a handful of tests moves the wall time by tens of seconds; the only large levers are fewer tests or more workers.
- **One test was 32 s of its own**: `test_with_the_owners_switch_on_an_idle_red_session_is_closed_and_resumed_by_the_guard_and_nothing_else_is_touched` waited the guard's real quiet time after typing (30 s, `TYPING_QUIET_MS`).
- **The wall time depends on the other users of this machine**: runs of the same tree took 169.4 s (load average 8), 173.5 s (load 15), 187.4 s (load 15 to 24), and the earlier baseline 177.0 s (load about 5). A limit of 180 s on a wall clock is therefore tight by the weather, not by the tests (see "Not decided here").
- **The build's peak**: a touched source costs, measured alone (the tree's resident sets every 100 ms): the release build of the program 1146 MiB (rustc), the unit tests' build 1503 MiB (rust-lld 1072 MiB linking the test binary with its debug information, rustc 371 MiB). The 1176 MiB recorded in P03a was a run where the tests' build had nothing to link. With the lines only: 966 MiB (rust-lld 520), and 5.5 s instead of 13.9 s.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D20 | The host reads `LICO_GUARD_TYPING_QUIET_MS` (the guard's quiet time after the person's last typing; default and upper bound 30 000, at least 100, anything else is the default), once, and uses it where it used the constant. The window test of the auto-close sets 1000 | the one test waited 30 s for nothing but the clock; the accepted pattern for "for tests" values in the host (`LICO_STALL_S`, `LICO_LIVE_WRITE_SECS`); the bounds keep a stray value from turning the guard off or into a hair trigger; one unit test holds the parsing |
| D21 | With the short quiet time the guard may close the session before a poll of the test has seen the red figure (the host looks every 3 s, the guard acts at the next look at the status), so the auto-close test no longer waits for the figure: it waits for the resume, from the generation the session was made with. The other tests' wait for the figure is 30 s (it was 10 s, and one run in a loaded machine did not reach it) | a flake found by the first run after D20, repeated six times green after the change |
| D22 | `[profile.dev] debug = "line-tables-only"` in the root `Cargo.toml` | the unit tests' binary is linked by rust-lld, which held 1.07 GiB for the full debug information; the lines are kept so that a failing test still names the file and the line; the program (release, `debug = false`) is unchanged; the owner's `cargo build --release` is unchanged. The first build after the change rebuilds every dependency of the tests once (1.5 GiB for that one run) |
| D23 | Removed: `tests/test_m14_end_to_end.py` (its one test is a composition of `test_maximized_start` and `test_path_status_bar`, whose helpers it imports: the maximized start and the icon in both directions are asserted in the first, the bar on the last row, the pointed row and the two idle seconds that draw nothing in the second); `test_gui_exit_preserves_output_cursor` in `tests/test_view_lifecycle.py` (the window's crash and the reattach with no gap and no duplicate are asserted by `test_gui_kill_and_reattach_keeps_agent_and_sequence`: records while the GUI was absent, contiguous log, restarted gaps 0, dups 0, cursor up to date); `test_a_working_session_that_shows_nothing_for_the_stall_time_turns_the_line_under_the_list_orange` (75 s of waiting to take a picture of a line the activity tests assert: `test_activity.py` holds the stall report and the window's silent-session line; the host's stall figure is asserted through `memory_limits` in the red-flow test). The dead helper `holding()` went with it | the owner's words: tests that mean nothing or repeat another go. Only those whose whole content another test holds were removed; the many tests that share a scenario but assert different things stay |
| D24 | `tests/budget.toml` has no `[[allow]]` entry left: the three memory-guard entries (90 s each) are gone, the longest test of the cold run is 11.5 s, under the default 15 s | an exception that is not needed is a limit that is not held; the table keeps its format in a comment |

## Steps

1. [x] **Analysis**: the ledgers of three cold runs compared per test and per job; the windows' start measured per test with a temporary log (removed again: the start is 0.3 to 0.8 s, not worth keeping the code); the build measured alone, touched and not.
2. [x] **The guard's variable** with its unit test (`the_quiet_time_after_typing_is_thirty_seconds_unless_the_variable_names_a_shorter_one_of_at_least_a_tenth_of_a_second`: unset, `1500`, `100`, `99`, `0`, `30000`, `30001`, `soon`).
3. [x] **The guard's window tests** (the stall time and the quiet time through the owner's `memory.json` and the variable; the picture wait only when pictures are taken), the three removals, two flakes (`test_tool_history.py`: a wait for what the simulated provider reads asynchronously; `test_first_view.py`: the click on a hidden session's row is made again, from the layout as it is then, when the busy machine did not let the window take it).
4. [x] **The profile** and the measurement of the build before and after.
5. [x] **Mutation checks** on the new code: the lower bound 100 to 99, the upper bound one over, the default 0 (each caught by the unit test), both uses of the quiet time back to the constant (caught by the window test, which waits 20 s for the resume and the host takes 30 s of quiet; the wait was 90 s before, which would have let it pass). Bound `100..30000` made exclusive on the upper side is the same value either way (30 000 is the default), so it is not a mutant.
6. [x] **Gates**: the memory-guard file six times in a row, the cold run, the changed-files run with receipts, `cargo test --bin lico`.

## Results (2026-11-19)

| What | Before (P03a) | After |
|---|---|---|
| tests in the cold run | 612 | 609 |
| the tests' own time, summed | 600 s | 562 s (557 s in a quieter run) |
| the memory-guard file | 5 tests, 61.8 s | 4 tests, 22.0 s |
| the longest test | 31.8 s | 11.5 s (no test needs an exception) |
| the cold run's wall time | 177.0 s | **170.5 s**, **170.2 s** and **167.9 s** (the commit hook's own run), all green (load average 7 to 12: other users' jobs); other runs of the same tree 169.4 s (load 8), 173.5 s (load 15, one failure, fixed after it), 187.4 s (load 15 to 24: the guard tripped), 191.6 s and 196.9 s (the same load, a budget and a flaky wait tripped) |
| the tests' peak | 1161.4 MiB | 1222.7 MiB in the green run (1091.9 and 1167.2 in the others: what overlaps changes by a hundred MiB); 1357.9 MiB once, the first run after D22, when four components compiled their tests for the new profile |
| the build's peak after a touched source (the suite's own measure) | 1726.9 MiB (the run that tripped the bound) | 1159.4 MiB |
| the unit tests' build, alone, after a touched source | 1503 MiB, 13.9 s | 966 MiB, 5.5 s |
| the memory-guard file, six runs in a row | (not run in a row) | 6 of 6 green; the first run after D20 had failed once, in the loaded machine, before D21 |

- **The limit is still not roomy.** 170.5 s of 180 s is 5% of margin, and it was reached on a machine with other users' jobs; the same tree on a quiet machine is expected near 160 s, but that was not measured (the machine did not go quiet). The work is 7% less than before, so the margin is a little wider than P03a's and not wide: the lead does not call the goal met. What would widen it is in "Not decided here".
- **Found on the way**: (1) the build's 1176 MiB of P03a was not the peak of a rebuild (see "What was found"); (2) the harness's `settle` already waits on the window's frame counter, so the keys and clicks are not where the time goes; (3) a temporary log of every window's start, up and down, showed 0.3 to 0.8 s per window (Xvfb 0.09 to 0.3 s): it was removed again, nothing worth the code; (4) a test that waits for a figure the host can already have acted on is a flake with a short guard time, so it waits for the effect instead (D21).
- **Gates**: `cargo test --bin lico` and the whole cold suite pass; the commit hook finds every receipt current.

## Not decided here (the owner's)

- **The time limit is a wall clock on a shared machine.** After the removals the work is 7% less, and the quiet-machine figure is expected to be about 165 s of the 180 s; a run beside other users' jobs is 170 to 190 s, and the guard then trips (exit 3) on a tree that did not change. Three ways, none applied: (1) judge the limit only when the machine was quiet and say "not judged: load N" when it was not (small, and a quiet rerun settles it); (2) judge the CPU seconds of the jobs divided by the workers (independent of the other users, a new measure); (3) a fifth worker (about 20% less wall time for about 230 MiB more: the tests' peak would be about 1.4 GiB of the 1.5 GiB bound, which the lead does not recommend because the owner asked for the memory to come down). The lead recommends (1).
- The structural part of the owner's remark of P03a stays as written there: components with receipts are the way the cost stays low (`lico-vt` is 20 MiB and 1.2 s), and the terminal's and the window's pure code are the candidates; that is a refactor of its own after M22.
