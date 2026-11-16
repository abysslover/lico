# M15-P02 Report — The Workspace and the Rust Tools

- Status: accepted at G3 and merged to `main` (2026-11-17); the retired prototypes were left for the next milestone
- Version: 1.0.1
- Branch: `phase/M15-P02-workspace-and-rust-tools` (stacked on `phase/M15-P01-phase0-component-analysis` at `5f9a991`; commits `fdf5314`, `e401ac3`, `5b69ea2`, `00fcf69`, `0fa34ae`)
- Plan: [M15-P02](../../../plans/202611/20261114/20261114_12_M15-P02_workspace_and_rust_tools_plan.md)

## 1. What was deleted, and the proof (condition 2 of mail 135)

- **`exp/`** (commit `e401ac3`, a commit that only deletes and corrects notes): 72 files, about 2,750 KiB (26,655 lines) removed; kept: `exp/202610/20261027/00003_record_layout_2_1_311/` and `exp/README.md` (3 files, 21 KiB). By group: `exp/toolkit_compare/data` 34 files 2,283 KiB (the toolkit measurements), `exp/toolkit_compare` 12 files 22 KiB, `exp/toolkit_slint` 5 files 137 KiB, `exp/toolkit_egui` 3 files 75 KiB, `exp/toolkit_iced` 3 files 67 KiB, `exp/toolkit_common` 2 files 11 KiB, the October canaries and measurements 13 files 157 KiB. History keeps them (`fdf5314` and before).
- **The proof that nothing live reads `exp/`** (MEASURED by grep of the build files, `tests/testmap.toml`, the Python tests, the scripts and the release procedure): no build file, test, script or release step opens a path under `exp/`; what names it is documents, one inert rule of the testmap (`exp/**`, still valid for the kept directory) and `.gitignore` (`/exp/scratch/`). The three places outside `doc/` that cite the kept evidence (`src/host/adapters/README.md`, `tests/fixtures/claude_record/README.md`) are untouched; `tests/README.md` no longer names `exp/` as a place for exhaustive checks; seven plan and report notes of M01 and M02 that cite deleted paths got a closing note (the paths are in git history at `fdf5314`).
- **Links**: the baseline link scan of the 180 tracked markdown files found **65 dead relative links** before I touched `exp/` (milestone and report notes since M09 pointed one folder too high, mine too). Since condition (b) says no link in the tracked tree may be dead, they were repaired mechanically in their own docs-only commit (`fdf5314`, a basename lookup, each target found exactly once); the scan after that: 0. After the deletion (173 files): **0 dead links**.
- **The prototypes** (`src/candidates`, `src/host/rust`, `src/host/c`, `legacy.py`, `hostcmp.py`): the same proof FAILED. They are read by the tests behind the marker `legacy_candidates` (six test files and `test_selection`, the testmap's `[[sources]]`, `pyproject.toml`, the `suite legacy` command, `rig.py`); deleting them changes tests, so I stopped that item and asked (mail 114: A delete with the test edits in a separate commit, B keep until the rig is ported, C only the sources; recommendation A). Nothing of them was deleted; the answer is pending.

## 2. The workspace (commit `5b69ea2`)

A root `Cargo.toml` with `[workspace] members = ["src/host/hostd", "tools/xtask"]`, the profiles (read from the root only) moved there unchanged, `Cargo.lock` moved to the root (`git mv`), `.cargo/config.toml` with `cargo xtask`, `build.rs` and `suite.py` (the source hash) and `notices.py` and `tests/testmap.toml` pointed at the new places; no source file of the program changed. One executable stays: `tools/xtask` is a separate package that the release build does not build.

| | before (`5f9a991`, the same commit string) | after | change |
|---|---:|---:|---|
| release binary | 9,612,880 bytes | 9,614,480 bytes | +1,600 bytes (+0.017 %) |
| release build (`build-release.sh`, cold target) | 34.61 s | 35.62 s | +1.0 s (one measurement each) |
| `cmp` of the two files | | differ | the file paths cargo hands to rustc are now relative to the workspace root: 110 panic-location strings read `src/host/hostd/src/...` where they read `src/...`; no code differs (MEASURED with `strings`) |

The two-clone acceptance after the change: the Rust `cargo xtask accept` passed on the committed tree (below). The unit-test count of the program is unchanged: `670 passed, 1 ignored`.

## 3. The tools (`tools/xtask`, package `lico-xtask`, std and `serde_json` only, 1,753 lines of source and 235 of tests)

| command | what it does |
|---|---|
| `cargo xtask graph [--cut] [--json OUT]` | the module graph of a crate read from its source text (the method of Phase 0, ported): modules, edges with use counts, test-only edges apart, strongly connected components, layers, two-way pairs, the best cut found |
| `cargo xtask check` | the components under `components/`: manifest fields (`purpose`, the spec file, version, budget), `depends` must equal the path dependencies, no cycle; non-zero exit and the problem named |
| `cargo xtask hash NAME`, `test NAME [--record]`, `verify` | a SHA-256 over the component's files, the hashes of its dependencies, the locked crates it uses and the toolchain; `test` compiles apart then runs each test binary alone with the tree RSS sampled from `/proc` against the manifest budget and writes `RECEIPT.json` with `--record`; `verify` names what changed (a file, a dependency, the locked crates, the toolchain, a recorded failure) |
| `cargo xtask accept` | two clean clones in differently named places build with `build-release.sh`; the artifacts must be the same bytes and hold nothing of where they were built |

**The graph tool reproduces Phase 0 exactly** (MEASURED, its JSON compared with the scratch analysis of the report): 170 modules, 63 nodes, **402 production edges and 77 test-only edges with identical use counts on every edge**, the one cycle of 48 modules (the same members), 58 two-way pairs, layers 9/49/4/1 with the same members, and the same best cut of 65 edges. One difference of definition, not of result: the tool counts 1,554 use sites between different top-level modules, the report's 2,245 included the uses inside `gui` between its own submodules.

## 4. Red first for the tools

The tests (`tools/xtask/tests/tools.rs`, 12) were written before the tools against stubs: 11 failed (`private/runs/m15_p02_xtask_red.log`), among them a graph with a cycle that must be refused, a changed file that must invalidate a receipt (and the receipt of what depends on it), an over-budget run (memory and time) that must be reported, a clone build that differs that must be refused, and the command line's non-zero exit. Then the tools were written and the 11 went green; two of those first failures were mistakes of my own in the tests, not of the tools (the TOML sample defined `package` as a table and as an array of tables; and the layers of the synthetic graph, where the cycle uses `c` and so sits on layer 1, not 0), corrected by me and said here. Running the acceptance for real found one bug (a relative scratch folder was taken from inside the clone): a test that reproduces it was written first (red), then the fix; the tools' tests are 12 and pass.

The test selection knows the tools: `testmap.select` names `cargo test -p lico-xtask` (a `Selection.xtask` flag) when `tools/xtask/**` or the workspace manifest or lock change, and the runner has a job for it; written test-first in `test_selection` (red `AttributeError`, then green).

## 5. Evidence (as measured)

| Check | Result |
|---|---|
| `cargo test -p lico-xtask` | `12 passed`, about 4 s |
| `cargo xtask check` on the repository | no component yet: "the components are in order" |
| `cargo xtask accept` on the committed tree (`CARGO_BUILD_JOBS=6`, offline) | **two clean clones build the same `lico` (sha256 `bdb38241e31eaed2...`)**, `103.6 s` for both builds (the Python `suite acceptance` took `165.6 s` in M13 and `205.4 s` in M14; not a controlled comparison) |
| Cold full suite after the tools job was added | `551` tests in `117` files passed, `127.6 s`; the run's peak `1,785.2 MiB` was reached in the build phase (the first run after the workspace change rebuilt the release binary and the test binary: `37.4 s + 23.6 s`), not in the test phase; the P04 runs are three warm cold runs |
| Rust units of the program | `670 passed, 1 ignored`; the tools' tests `12 passed` in the same run |
| Link scan | 65 dead before, 0 after the repair, 0 after the deletion |

## 6. Honest notes

- The tools of this phase are the five of mail 133; the Python runner (`suite.py`, `testmap.py`, `receipts.py`, `ledger.py`) is NOT ported yet and stays the runner; the Python `suite acceptance` is kept until P04 (both exist; this is the dated reason: no test of the program was replaced, so the named better one does not exist yet).
- `components/` is empty: `check`, `verify` and the isolated `test` of a real component run for the first time in P03 (`png`); their behaviour is shown on synthetic components in the tests.
- The graph tool is text-based (module level, no method-call or trait-object coupling), as in Phase 0.
- The hash covers the `Cargo.lock` entries of the registry crates a component names (and what they use in turn) but not a component's own transitive path dependencies' crates beyond what the dependency's hash already holds: a dependency's change reaches its dependents through its hash.
- The release-size and build-time figures are single measurements.

## 7. What waits

The answer to mail 114 (the retired prototypes, A or B). P03: the interface specification of `png`, shown to the owner on the screen (the stop of this milestone before P04's last steps).

> Note (M16-P01, 2026-11-17): the retired prototypes this document names (`src/candidates`, `src/host/rust`, `src/host/c`, `legacy.py`, `hostcmp.py`, the `legacy_candidates` marker) were deleted in M16-P01 on the owner's answer A; git history keeps them at `6712fe5`.
