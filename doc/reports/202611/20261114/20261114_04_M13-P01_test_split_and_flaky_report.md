# M13-P01 Report — Memory Peak of the Cold Suite, and the Two Flaky Tests

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M13-P01-test-split-and-flaky` (from `main` at `cc34ab4`; commits `83cfd62`, `7e4ecdf` after the documentation commit `e885307`)
- Plan: [M13-P01](../../plans/202611/20261114/20261114_04_M13-P01_test_split_and_flaky_plan.md)

## What changed

The plan was to split the heaviest GUI test files. The measurement showed that a split cannot lower the peak, and on the owner's instruction (keep the thread count, reduce the memory of each test, keep every test, report if it does not work) the memory was reduced where it was spent instead. No test file was split and no test was removed.

- **Where the peak came from.** Every cold run peaked in the first seconds of the test phase with `cargo test --bin lico` and the three heaviest GUI/host files together. Alone, those files peak at `228.6` (`test_host_replacement`), `261.9` (`test_card_window`), `190.3` (`test_activity`) and `263.7 MiB` (`test_client_integration`), and `cargo test --bin lico` alone at `1,200.1 MiB` (tree, runner included) on the 24-thread default. Splitting a file leaves the peak of its single tests (about `250 MiB`) unchanged, so the overlap does not shrink: a split could not reach `1,800 MiB`.
- **Inside the Rust job** (`private/measure/rust_per_test.py`: each of the 657 tests run alone): 43 tests used `>= 200 MiB` and 11 used `100..200 MiB` (the other 603 less). The cause of the 43 (`pending`, `create`, `history`, `calls`, `screentools`, all through `transfer::tests::world`) was found with `strace -k` on one of them: one `mmap` of `207,896,576` bytes made by `std::fs::read` inside `core::build_id()`, which reads the program's own file to hash it; the test binary is `207,894,328` bytes. `build_id()` now hashes in 64 KiB pieces (`ident::fnv1a`, same FNV-1a value, checked against three published vectors and a by-hand fold). A test of that group then peaks at `15 MiB` (was `218 MiB`).
- **Two kinds of test that run on every core at once** were then gated, with the same thread count and every test still run: font systems (`gui::fonts`, about `130 MiB` per `FontSystem`, two per pair) one at a time, private X servers (`Xvfb`, `65..95 MiB`) three at a time (`src/host/hostd/src/testgate.rs`, test-only; a thread that holds a pass passes through again). Before: ten `Xvfb` at the peak, `706..715 MiB`; after: three, `193 MiB`.
- **The product file** `core.rs` was touched (`build_id()`, `941 -> 937` lines, so the size ratchet is not used up): the hash value is identical, so no behaviour changes, but this is product code in a phase that said none, and the first call of `build_id()` in a window or host now holds one piece instead of the whole program file (the release binary is `9.5 MiB`). I say it here instead of hiding it in the commit.
- **Not adopted, measured only:** `RUST_TEST_THREADS=8` as an environment variable (no repository change): `cargo test` alone `745.0 MiB`, `15.8 s`; whole cold run `1,568.5 MiB`, `118.0 s`. `2` threads: `606.5 MiB` but `50.9 s`; `4` threads: `608.9 MiB`, `27.4 s`. The owner asked to keep the thread count, so this stays an alternative.

## Evidence (as measured)

| Run | Tests | Time | Tree peak |
|---|---|---|---|
| Before, cold (`m13_p01_before1.log`) | 533 + Rust | `117.9 s` | `1,998.4 MiB` (cargo, host_replacement, card_window, activity at 48 s into the run, the test build included) |
| After `83cfd62`, cold | 533 passed | `116.2 s` | `1,445.6 MiB` |
| Final tip, cold run 1 | 533 passed | `116.2 s` | `1,421.6 MiB` |
| Final tip, cold run 2 | 533 passed | `114.8 s` | `1,448.2 MiB` |
| Final tip, cold run 3 | 533 passed | `115.2 s` | `1,521.4 MiB` |

Rust unit tests in all three final runs: `658 passed, 1 ignored` (`656` before plus the two new ones), `13.2` to `14.9 s`. `cargo test --bin lico` alone: `1,479.7 MiB` (first snapshot, `15.5 s`), `1,370.3 MiB` after the `build_id` change, `887.2 MiB` after the gates (`13.4 s`). Targets: peak `<= 1,800 MiB` met (`1,421.6..1,521.4`), time not worse than `120 s` met (`114.8..116.2 s`). The spread between runs is about `100 MiB`.

Commands (all through `pri-heavy --label lico --`): `PYTHONPATH=src:. private/venv/bin/python -m lico_measure.suite run --cold --ledger private/measure/m13_ledger_*.json` and `python -m lico_measure.ledger_report FILE 25`; single files with `suite run --workers 1 --ledger FILE tests/test_X.py`; the Rust job alone and per test with two private helpers kept in `private/measure/` (`job_alone.py`, `rust_per_test.py`, not in the repository).

## Red first

- `ident::tests` (`the_build_id_hash_is_...`): the helper was first written with the old whole-file read; the hash vectors passed and the memory bound failed (`a program file of 8 MiB held 8388608 bytes at once`, `private/runs/m13_p01_rust_red.log`), then the pieces version went green (`5 passed`).
- `testgate::tests`: first with a gate that never waited: `12 held a pass at once on a gate of 3` (`m13_p01_gate_red.log`), then green.
- The two Python test fixes: the red is the reproduction below.

## The two flaky tests

- **`tests/test_first_view.py`** (`test_older_history_is_read_on_demand_...`): not a hang but a slow test that scales with load: it sent 85 wheel notches with two `xdotool` processes each (about 170 spawns). Alone `9.5 s` (four runs), with 64 busy loops beside it `11.0..11.8 s` (six runs); after one `xdotool` call per batch (`click(..., repeat=N)`) alone `3.1 s` (five runs), loaded `3.95..5.11 s` (eight runs), the same assertions. I could not make it reach the `15.6 s` of the M12 changed-suite run, so that the earlier failure had this cause is likely, not shown.
- **`tests/test_picture_paste.py`** (`test_a_slot_the_host_refuses_...`): reproduced under 40..64 busy loops, `4 of 250` and `3 of 200` runs (none in 52 earlier runs with fewer loops). In the three runs with a copy of the test that printed its state at the failure the window's row already showed generation `2`, one picture had been sent and the box was empty: after `SIGCONT` the window had learned the new generation (its once-a-second tick) before it read the key, so the input was right and was sent and the refusal path under test never ran. This is a race of the test, not a product fault. The test now tells that attempt apart by what happened to the picture and repeats the whole scenario, at most six times; same assertions. After the change: `0 of 250` under the same load, and one of the 250 needed a second attempt.

## Honest notes

- Nothing was split, so the planned by-name test-count guard was not needed: the Python count is `533` before and after, the Rust count is `656 + 2`.
- The peak measurement is the tree sum of resident sets with shared pages counted per process (an upper bound), as before.
- The Rust per-test figures are one test alone in its own process; the sum of such times fell from `183.2 s` to `161.1 s`.
- Not done in this phase: anything for `RSS` of GUI test files (they were not the cause).

## Next

P02: the discovery dialog (cache with Refresh, a faster scan with every bound kept, the folded list, readable rows).
