# What the daily run costs for the big Rust files, and the options (2026-10-26)

> Status: for the owner's decision at the M02-P02 gate. Measured on the M02-P01 branch; nothing here is built.
> Why now: the test plan says that over 60 s a crate-split plan is written and the owner decides. One changed line in `core.rs` costs 111 s of selected tests, in `gui/ui.rs` 96 s.

## 1. The three big files

| file | lines | modules that use it | test files a change selects (of 39) | daily run, measured |
|---|---|---|---|---|
| `core.rs` | 1169 | 11 (accounts, broker, claude, codex, gui, host, http, ipc, login, registry, session) | 32 + `cargo test` of 12 modules | 111.4 s (31 files; the 32nd, 5 tests, adds 1.7 s) |
| `gui/ui.rs` | 1535 | none (module `gui`) | 20 + `cargo test gui::` | 95.8 s (19 files) |
| `gui/mod.rs` | 933 | none (module `gui`) | 20, the same | the same: the map works per top-level module, so each of the 9 files of `gui/` (5151 lines) selects what `ui.rs` selects |

A leaf that exists today: `cli.rs` (276 lines, used by nothing): a changed line selects 4 test files + `cargo test cli::` and ran in **4.7 s** of tests after an 11.0 s build, 9 tests (rc 0). Other cheap modules: `credcmd`, `dbus`, `keyring` select 4 files each (not timed); `bin/synth_agent.rs` selects 13.

## 2. Where the time is (cold run, 145 tests, 112.6 s on 3 workers, 321 s of test time)

- 19 files that open a window: 278 s (87 %). 12 host files without a window: 38 s. 8 files that touch neither: 6 s.
- Any change to host code selects 31 to 32 files because every window test also starts a real host. The window tests are the cost; the layout of the Rust source is not.
- The selection follows `crate::` uses transitively: a module that `core` calls is selected as `core` is. So `ipc`, `session`, `registry` and the P02 `folders.rs` all cost the same 32 files as `core.rs`. "New logic into a leaf module" (R5 of the P02 plan) therefore keeps files under 800 lines but does not shorten the run, unless the module is one nothing uses (`gui/*`, `cli`).
- `gui` takes exactly one thing from the rest of the crate, `core::build_id()` (5 calls in 2 files). Simulating that edge away on the real map: `core.rs` still selects 32 files (the cargo filters go from 12 to 11), because the window tests run the host anyway.

## 3. Options

**A. No split now.** R5 of the P02 plan stands: `core.rs` does not grow, new host logic goes into its own module, and a phase that touches a big file cuts one piece out. Cost: none; a host change stays about 110 s, a window change about 96 s, a `cli` change about 16 s with the build (4.7 s + 11.0 s, measured). Gain: size and readability, not run time.

**B. One slice before P03 that cuts the stable part out of `core.rs`.** Quiet by `git log -L` (commits touching the region, of 24 for the file): `resources` 1, `hello`/`status`/`build_id` 2-3, `bounded_lines` 3, view attach/detach 3, `events` 4: 183 lines. Busy: session start, create, purge and provider start 7-9, the account glue 7 (181 lines), `replay` 7 (157 lines). Cutting the quiet pieces, the account glue and `replay` leaves `core.rs` near 650 lines. Measured effect on the daily run: **0 test files deselected**, because the pieces stay inside the host and the host is what 31 files start. It buys `core.rs` under 800 lines and fewer conflicts between phases; one slice and one review.

**C. A real crate split** (host, window, command line). Beyond B it buys a separate incremental build and `cargo test -p` per crate. The whole build is 11-12 s and `cargo test` 5 s, so at most 12 s of 111. The tests start the one executable, so the selection stays at 31 to 32 files. The cost is a workspace of three crates that still link into one executable (not estimated).

## 4. My pick, and the one thing that would shorten the run

Pick **A**. B and C do not touch the 87 %.

What would: a lane for the window tests. A change in host code that is not a protocol change runs the 12 host files (38 s of test time, about 14 s on 3 workers) in the daily run, and the 19 window files run `--cold` before a commit that touches the host, at the end of a phase and before a merge, as the acceptance group does. That moves the guarantee from "every edit" to "every commit", so it is the owner's decision, not mine. A smaller move needs no decision: the longest window files (`test_host_replacement` 50 s, `test_frame_cost` 39 s, `test_activity` 37 s) set the wall time of a 3-worker run, and splitting each into two files can bring a cold run down by at most the imbalance, to 107 s at best (321 s of work divided by 3); I have not tried it.
