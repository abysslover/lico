# M15-P03 Report — The First Component, `png`

- Status: implemented; waits for the owner's acceptance (G3) and the P04 release run
- Version: 1.0.0
- Branch: `phase/M15-P03-first-component-png` (stacked on `phase/M15-P02-workspace-and-rust-tools` at `5a49682`)
- Plan: [M15-P03](../../plans/202611/20261114/20261114_13_M15-P03_first_component_plan.md); contract: `components/png/SPEC.md` (the interface specification the owner approved on 2026-11-14)

## 1. What was done (commits in order)

| Commit | What |
|---|---|
| `79f77ad` | docs: the approved specification and the steps |
| `884b116` | tests: the same input gives the same bytes (I7), the header bytes of a one-pixel file; green on the unchanged code, then shown red under two mutations |
| `0287fba` | refactor: png's proofs use a scratch folder of their own, not `handoff::Scratch` (the last edge out of png) |
| `0e60c3b` | refactor: the check sums are private (Q1); `the_public_names_are_exactly_these` written first and red |
| `cd17575` | fix: removes an empty `src/main.rs` that `0e60c3b` carried by mistake (my own `touch`; no effect on the build) |
| `0a643d2` | the move: `components/png/` as crate `lico-png`; the program depends on it; consumers repointed; no test changed in meaning |
| `506ad1a` | manifest check, `SPEC.md`, first `RECEIPT.json` |
| `cf63f51` | test selection knows components (tests first) |
| `c1a95f8` | the limits test the mutation check asked for; receipt recorded again |

## 2. The component

`components/png/`: `Cargo.toml` (manifest in `[package.metadata.lico]`: purpose, spec, `depends = []`, external `python3`/`identify`/`convert`, budget 256 MiB and 30 s), `SPEC.md`, `src/lib.rs` (the encoder, 513 lines, unchanged but for the module lines, `Crc32`/`Adler32` private, `adler32` test-only), `src/testing.rs` (feature `testing`: the readers, the inputs, a local `Scratch`), `src/tests.rs` (13 white-box tests), `tests/contract.rs` (12 contract tests, each named in SPEC.md), `RECEIPT.json`. The program depends on `lico-png` (production) and, in `[dev-dependencies]` only, on it with `testing`; the release build holds none of the test support (checked in P04 by the binary).

Counts: the 21 tests of before became 25 (the 4 new ones: I7, the header bytes, the public names, the limits); the program's Rust tests went from 670 to 649 (21 moved). **Pattern: a plain extraction** (png had no upward edge to invert, so no port), plus the `testing` feature as the adapter for the consumers' tests. The graph tool now reads 167 modules, 62 nodes, 401 production edges; the cycle is still 48 modules (png was never in it); layer 0 holds 8 modules (it held 9).

## 3. Measured

| | Result |
|---|---|
| `cargo xtask test lico-png` | 25 tests, tests 4.17 s, tree peak 94.8 MiB (82.3 MiB with 24 tests; the tree includes `python3` and ImageMagick), compile 1.1 s; budget 256 MiB / 30 s |
| `cargo xtask check`, `verify` | pass |
| A change inside png, BEFORE (`5a49682`, the same selection rules) | program: 632 Rust tests, 29.2 s (3 runs: 29.12, 29.31, 29.35), largest process 358 MiB; Python files 105 of 117 |
| A change inside png, AFTER | program: 44 Rust tests (`capture::`, `clipboard::`, the direct consumers only), 11.8 s (11.65, 11.76, 12.10), largest process 101 MiB; plus the component: 25 tests, 4.2 s; Python files 84 of 117. Together 69 tests, about 16 s |
| A change in `capture`, before / after | 632 tests, 29.6 s / 611 tests, 26.9 s (3 runs each). Almost no change: `capture` sits in the 48-module cycle, so its dependents are still 51 modules |

The gain exists for a change inside png (29 s to 16 s, 632 to 69 tests), not for a change in a consumer, where it is the 21 tests that left (about 2.7 s). Phase 0 had estimated 11.0 s for the png tests; that was the sum of one process per test, which overstates the run the suite actually does (all 21 in one process: about 4 s). Times are of the debug test build, `--test-threads=4`, one machine; the memory is the largest single process (`/usr/bin/time`), not the tree.

## 4. The mutation check (step 7)

The same breakage in `lib.rs`, the Rust tests of the component and the four Python files that touch PNGs (`test_capture_cli`, `test_computer_use`, `test_xjob`, `test_agent_tools`: 46 tests, baseline all pass):

| Mutation | Rust fails | Python fails |
|---|---|---|
| header says colour type 6 | 7 tests | 7 tests (capture_cli 2, agent_tools 3, computer_use 1, xjob 1) |
| every chunk CRC wrong by one bit | 7 tests | the same 7 |
| the row above never remembered (Up, Average, Paeth against black) | 5 tests | none |
| the matcher looks at no earlier position (valid, much larger file) | 4 tests | none |
| `IDAT_MAX` doubled | none (first run) | none |

Every Python test that fails also has a Rust contract test that fails for the same breakage, and Rust sees two breakages Python never sees. **Retired: none.** Reason: each of those Python tests is a test of the consumer (the capture command, the hidden file name, the offsets, the agent's picture); the PNG check is one assertion inside them, so retiring a whole test would lose the consumer part. The fifth line is a hole the check found in my own tests: nothing pinned the constant `IDAT_MAX` (the tests compare against it). `the_limits_have_the_values_of_the_specification` was written, shown to fail under the mutation, and added (`c1a95f8`).

## 5. Test selection and the runner

`testmap.select` (tests first): a path inside `components/<dir>/` selects `cargo xtask test <package>` and the cargo tests plus Python areas of the program modules that name the library, directly (their own tests only, not what uses them); `RECEIPT.json` selects `cargo xtask verify`; a build input of the binary (workspace manifest, lock, everything) selects every component. `suite.py`: `component_job` and `verify_job`, the source hash of the binary now covers `components/**` except receipts (test first). A full run has all components and the verify job.

## 6. Honest notes

- The specification said `cargo xtask test png`; the component's name is its package, `lico-png`.
- The test support (`lico_png::testing`) holds more names than the approved list (`inflate`, `unzlib`, `slow_crc`, `slow_adler`, `paeth_spec`, `run`, `imagemagick_size`, `encode`, `colour`, `Rng`): `SPEC.md` says it is not contract. This is a difference from the table the owner saw.
- `capture/tests_x.rs` takes its sample text from the first 62 lines of the encoder; it now reads `components/png/src/lib.rs`, whose first lines differ by the new module doc (the test passes).
- Two of my steps went wrong and were corrected: the first nondeterminism mutation did not change the output on the first row (an equivalent mutant; a second one failed the test), and the first mutation run stopped at the first failing test binary (`--no-fail-fast` added; all results above are from the second run). The empty `main.rs` is listed in section 1.
- Not done here (P04): the release binary and the cold full runs, idle and hidden memory, the two-clone acceptance of this tree, the version bump, the install.
