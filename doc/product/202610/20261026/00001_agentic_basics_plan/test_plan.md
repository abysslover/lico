# Test plan: keeping the whole suite under 3 minutes and light

Status: v1.0, approved by the owner at G1 on 2026-10-26. The last step of P01, S05 (test cleanup), carries out this plan. The first figures come from its ledger (135 tests, 596 s cold, *measured*); earlier counts were 135 tests in 554 s, 607 s at report 015 and 130 tests in 545 s at report 023. The "cause candidates" below are hypotheses until the ledger confirms them.

Related documents: [planning](planning.md) · [requirements](requirements.md) · [screens](screens.md) · [changes](changes.md).

## The owner's requirements (2026-10-26)

- The whole suite must **not exceed 3 minutes**.
- **Tests that did not change are not run again and again.** They are chosen by looking at the dependencies between modules.
- Earlier, when a module grew bloated, the tests took enormous time. Keep modules small and weigh efficiency when making a test.
- The **time and the memory** that tests use are both managed.

## Budget

| Item | Budget | Now |
|---|---|---|
| Whole suite (from scratch, build already done) | **at most 180 s** | 596 s, 135 tests (*measured*) |
| Peak of the memory sum of all processes over the whole run | at most 2 GiB (proposal) | 3,571 MiB at the peak, in the clean-clone test (*measured*) |
| One test | at most 15 s (a reason in the table if more) | not yet measured per test |
| Usual run (tests affected by the change, plus receipts) | target within 30 s | not applicable |
| Rebuild and test after changing one line | set after measuring with the ledger (over 60 s means writing a crate-split plan); one changed Rust line cost about 6 s to build | about 6 s build (*measured*) |
| Source file length | at most 800 lines | three files over (see below) |

A phase over 180 s is not merged. If it is over, that phase must clean up the tests and bring it back.

## Where it stands (checked in the code and the ledger)

- 37 Python test files (about 4,600 lines), 135 pytest tests (596 s cold). 27 Rust unit tests (`cargo test --bin lico`, 4.5 s).
- Layers by time in the ledger: the end-to-end layer (L3) takes about 75 % of the time. Fixed sleeps in test helpers dominate.
- The largest test file is `test_host_replacement.py`, 482 lines and 16 tests, with 30 s waits in places.
- There was no `conftest.py`; tests did not share a host or an X server (each started its own).
- One test in `test_single_executable.py` **clones the repository and does a release build twice** (limit 1,500 s). It was the largest single cost and proves a property of the build recipe, not of every source edit. It now sits behind its own acceptance command.
- About 10 thousand lines in one crate: changing one line rebuilds the whole binary. The big modules are `gui/ui.rs` 1,535 lines, `core.rs` 1,169 lines and `gui/mod.rs` 933 lines.
- The first paragraph of `tests/README.md` ("no product tests are implemented") is out of date.

## Layers of tests (proposal; adjusted after the ledger)

| Layer | What | Usual run | Budget share (proposal) |
|---|---|---|---|
| L0 unit | One function or module (Rust `cargo test --bin lico`, pure Python functions). Milliseconds | always (the selected ones) | 50 s for L0 and L1 together |
| L1 protocol | Tests that talk to the host socket, no display. One host is shared per module | always (the selected ones) | |
| L2 window | Start the window once and make several checks (picture or dump comparison) | the selected ones | 50 s |
| L3 end-to-end | A real X server and several processes (host replacement, sign-in and so on). **Few**, and marked | in the full run | 80 s |
| A acceptance | Proofs of the build recipe (two clean-clone builds), pre-publication checks | when recipe files change, at the end of a phase, before a publication | outside the budget, own command |

## Choosing tests by module dependency

- A checked-in **map** from source path to tests. Example: `gui/scroll.rs` maps to `cargo test --bin lico scroll::` plus the Python test that draws scrolling. Python tests are derived from their imports as far as possible; Rust from module paths.
- **`select --changed`** reads `git diff` (working tree, staged, committed) and prints the tests to run. A changed test file selects itself.
- **Default deny:** a changed file that is not in the map selects **everything**. This prevents a wrong map from dropping tests.
- **A test of the selection itself:** touching file X yields exactly the expected set, and a file not in the map yields everything.

| When | What runs |
|---|---|
| While editing code | the selected tests plus receipts |
| Before a commit (hook) | the selected tests (plus receipts) |
| End of a phase, before a merge | **everything from scratch** (`--cold`, receipts ignored), within 180 s |
| When recipe files change, at the end of a phase, before a publication | the acceptance (A) tests |
| The lead's acceptance | tools such as exhaustive comparison and mutation tests (outside the suite) |

## Receipts: not rerunning tests that did not change

- The **input hash** of a test (or a marked group) = the content hash of the test file and of its mapped source files, plus, for a test that uses the binary, the binary's sha256, plus fixtures.
- If a pass with the same hash is recorded, the test is printed as "skipped (receipt)" and not run. **A test that failed or errored is never skipped.**
- Receipts are kept outside git (under `private/`). A `--cold` run does not use receipts.
- Risk: if the map is incomplete a receipt can hide a regression. That is why default deny, a `--cold` run before a merge and the selection's own tests go together.

## Ways to make it fast (the levers of S05, in the order the ledger points to)

1. **Take the acceptance test out of the daily run** (the two-clean-clone build and the like).
2. Replace fixed `sleep` calls with the waits that already exist (`wait_for`) and shorten long timeouts that only matter when something is broken.
3. **Share one host and one X server per module** (module or session fixtures). Only tests that really need a fresh host start one. Tests must not depend on order (shuffle once to confirm).
4. **Build once per run:** skip it when the tree is unchanged, and every test uses that binary. A test does not build.
5. Parallel up to 3 workers: only when every test has its own socket, runtime folder and display number and the memory sum stays within 2 GiB.
6. Merge tests that prove the same path; delete one only when a stronger test covers it. Every deleted test is reported with what covers it instead. Nothing a requirement relies on is weakened to be faster.
7. For bloated modules: write a crate-split **plan** (not code) in S05 only if one changed line costs more than 60 s to rebuild; otherwise the phase that touches the module splits it.

## Memory

- The ledger records, for every test, the **peak RSS of the whole process tree** (host, Xvfb, window, children).
- Limit: 2 GiB summed over the whole run (proposal). Heavy (L3) tests run one at a time.
- A new heavy test is merged only if its own peak memory is in the ledger.

## Guards

| Guard | What it does |
|---|---|
| Budget | A test over its limit (15 s by default) is reported by name. The whole suite fails if it is over 180 s |
| Size ratchet | A `.rs` or `.py` file in `src/` or `tests/` over 800 lines fails. The three files already over are listed with their current length and **may not grow** |
| Selection's own tests | Prove that the map and `--changed` are right |
| Flaky tests | Fixed in the same phase or quarantined with a reason (`test_recovery` is the target). Not hidden by rerunning |

## When a new test is made (checklist; goes into `tests/README.md`)

1. Does a smaller test already cover the same thing?
2. Which module does it map to (is it added to the map)?
3. Which layer is it, and what are its time and memory in the ledger?
4. Can a shared fixture do, or does it really need a fresh host?
5. Is it **one main-path test per feature**, or a test that weighs every combination? The latter does not go into the suite; it stays with the lead's acceptance tools.

## What stays outside the suite

- Exhaustive comparison harnesses (for example the font decision compared with 3.5 million lines of every character in every context), mutation test batches, named experiments.
- Smoke tests with a real provider (once at the end of a phase, recording the installed version).
- Reproducible-build proof (two clean-clone builds): the acceptance (A) group.

## Schedule

| When | What |
|---|---|
| **P01-S05** (now, before P02) | Ledger, then fix the largest costs, then map and selection, then receipts, then guards, then report |
| P02 to P06 | Each phase builds its tests inside this frame. The report carries the TST-10 line. A phase over 180 s is not merged |
| P07 | Re-measure everything and check the test budget |

## Risks

| Risk | Response |
|---|---|
| Receipts hide a regression | Default deny, `--cold` before a merge, the selection's own tests |
| Shared fixtures make tests interfere | Run in shuffled order to confirm; tests that need a fresh host are marked |
| Parallel runs raise memory | A 2 GiB sum guard; heavy tests run in series |
| 180 s is not reached | The ledger and the five costliest items left, the value of each fix reported; a crate split is decided with the owner |

## Results (filled by S05)

| Item | Before cleanup | After cleanup |
|---|---|---|
| Number of tests | 135 (*measured*) | planned |
| Whole suite (from scratch) | 596 s (*measured*) | planned |
| Peak memory of the whole run | 3,571 MiB (*measured*) | planned |
| Usual run: one line in `gui/ui.rs` | not applicable | planned |
| Usual run: one line in `core.rs` | not applicable | planned |
| Usual run: only a test changed | not applicable | planned |
| Tests skipped by receipt in a second run | not applicable | planned |
