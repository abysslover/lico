# M15-P04 Report — Verification and `lico 0.3.3`

- Status: implemented; waits for the owner's acceptance (G3)
- Version: 1.0.0
- Branch: `phase/M15-P04-verification-and-0-3-3` (stacked on the P03 tip `f566ace`; the measurements ran on the committed clean tree `4eaaf15`, the documents of this phase are the next commits)
- Plan: [M15-P04](../../plans/202611/20261114/20261114_14_M15-P04_verification_and_release_plan.md)

## 1. Measured verification (`lico 0.3.3`)

| Check | Measured result | Budget / gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `649 passed; 0 failed` (`1` ignored, both umasks); the tools `12 passed`; the component `25 passed` | `0` failures | Met |
| Full cold suite (`suite run --cold`), three counted runs, each started with load below `9` | `557` tests in `117` files, all passed: `131.5 s` / peak `1,633.2 MiB`; `130.9 s` / `1,586.5 MiB`; `131.9 s` / `1,603.8 MiB` | `<= 180.0 s`, `<= 2,048.0 MiB` | Met; margin `414.8..461.5 MiB` and `48.1..49.1 s` |
| Idle window RSS, scale 1 (`idle_memory`, 10 accounts, menu on, 3 launches) | `max = 25.65 MiB` (`25.33..25.65`), PSS `17.79..18.09`, host `8.07..8.27 MiB` | `< 50 MiB` | Met |
| Idle window RSS, scale 2 | `max = 36.62 MiB` (`36.58..36.62`), PSS `23.53..23.54`, host `8.14..8.30 MiB` | `< 50 MiB` | Met |
| Hidden background growth (`hidden_growth`, 10 vs 100 sessions, 3 repetitions) | `max = +0.40 MiB` (`+0.24`, `+0.40`, `+0.36`; GUI `25.55` to `25.90` median, host `9.24` to `20.95`) | `<= +10 MiB` | Met |
| Two clean clones build identical binaries, Python (`suite acceptance`) | `1 passed` in `131.57 s` | identical | Met |
| The same, Rust (`cargo xtask accept`) | the same executable in both clones, sha256 `1ed8034e980b3d4c66e5dac51bfea2a35d4729fda4ff9f66c26adae5af65fb65` | identical | Met |
| Tool list size (`tools/list`, both ends of the clipboard-write switch; printed once by the budget test) | `4,849` bytes with writing off, `5,103` with writing on (the same as `0.3.2`) | `<= 5,120` | Met with `17` bytes to spare |
| Receipts | `cargo xtask check`: the components are in order; `cargo xtask verify`: every receipt matches its inputs | match | Met |
| The release test support | the suite-built binary holds none of: the Python checker's text, `slow_adler`, `paeth_spec`, the test sink's canary text, `lico-png-` (`strings`, 0 matches each); `cargo tree -e features -i lico-png` shows the production edge with the default features only, and `testing` only on the dev-dependency edge | none in the binary | Met |

The measurements ran one after the other in the heavy lane, each waiting for the load average to fall under `9` (it stood at `2.5..6`). One further cold run is not counted: the second of the first series failed one test, `test_first_view.py::test_session_list_shows_state_account_and_a_pending_approval_while_the_session_is_hidden` (the second row was still `idle` when its deadline passed), the timing-dependent test M14 reported too; a replacement run passed and is the third counted one. Compared with `0.3.2` (`123.5..125.1 s`, peak `1,455.4..1,472.2 MiB`): about `6..8 s` slower and about `115..180 MiB` higher at the peak. The peak is reached in the first seconds, while `cargo test --bin lico`, `cargo test -p lico-xtask` and now `cargo xtask test lico-png` start together; I did not change that. Idle and hidden growth are at the level of `0.3.2` (`25.13..25.48` and `36.21..36.29` then; `+0.50` then).

## 2. What the milestone changed, in numbers

| | Before (`5f9a991`, the P01 tip) | After |
|---|---|---|
| Repository | `exp/` of 75 files, no workspace, tools in Python only | `exp/` keeps one evidence directory; a workspace of 3 packages; `cargo xtask` with graph, check, receipts, isolated test run, clean-clone check |
| Components | none | `lico-png 1.0.0` (specification, 12 contract tests, 13 white-box tests, receipt `4.17 s`, tree peak `94.8 MiB`, budget `256 MiB / 30 s`) |
| A change inside the PNG writer (P03 report) | 632 Rust tests, `29.2 s`, 105 Python files | 69 tests (44 + 25), about `16 s`, 84 Python files |
| A change in `capture` | 632 tests, `29.6 s` | 611 tests, `26.9 s` (the module is inside the 48-module cycle) |
| Module graph | 170 modules, 63 nodes, cycle of 48 | 167 modules, 62 nodes, cycle of 48 |

## 3. Decisions and honest notes

- The Python `suite acceptance` is kept next to the Rust `cargo xtask accept`. Both passed on the same tree; the Python one is outside the cold run (`578 deselected` in the marker run), so it costs nothing there. Retiring it needs the owner's word: the testmap's acceptance triggers and the README name it. I recommend retiring it once a milestone has passed with both agreeing (they agree on this tree; the Rust one alone ran on the P02 tree).
- The version is `0.3.3` as proposed in mail 135. The small patch for the maximized first start is not in this milestone.
- The tool-list figure was printed by a throwaway `eprintln!` in the budget test (reverted, never committed).
- The binary built by the suite (`9,805,224` bytes) is not the release artifact; the release artifact is built by `build-release.sh` (section 4).
- No specification edit is needed by this milestone; none is proposed.
- Open from before: the answer to mail 114 (the retired prototypes) and the owner's check of the maximized start on the real desktop (M14).

## 4. Install

Filled in after the build (see the mailbox report): the release is built by `src/host/hostd/build-release.sh` from the clean commit named there, the `0.3.2` binary (`9,612,880` bytes, `lico 0.3.2 (commit bc218ea, build dde7b4ca61ca1789)`) is kept at `private/release_0_3_2_backup/lico`, and the new one is copied to `~/bin/lico`.
