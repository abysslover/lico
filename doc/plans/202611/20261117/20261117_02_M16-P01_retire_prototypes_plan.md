# M16-P01 Plan — Retire the Prototypes

- Status: approved by the owner on 2026-11-17 (the owner's answer A to question 114; the plan shown on the screen); implemented (commit `575f757`)
- Version: 1.0.0
- Branch: `phase/M16-P01-retire-prototypes` (from `main` at `6712fe5`)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)

## Objective

Delete the retired prototype clients and hosts and everything that exists only to run them. Every row of the product stays (`a2`, the reference host `oracle`, the product host `hostd`); no product test shrinks.

## What goes (found by reading, 2026-11-17)

`src/candidates/` (README, `a_native`, `b_gtk`, `c_qt`, `d_webkit`), `src/host/rust/`, `src/host/c/` (12 tracked files, about 160 KB); `src/lico_measure/legacy.py` and `hostcmp.py`; the `hostcmp` command and the `suite legacy` command; the `legacy_candidates` marker and the `addopts` that leaves it out; the two `[[sources]]` entries of `tests/testmap.toml` for `src/host/rust/**` and `src/host/c/**`; the rig rows `a`, `b`, `b2`, `b3`, `c`, `c2`, `d` (`candidate_command`, `CANDIDATE_ENV`) and the `HOST_BUILD` rows `rust_std`, `rust_serde`, `c`; the helper calls `legacy.clients` and `legacy.hosts` in five test files; the test about the marker in `test_selection`; three documents that cite the deleted paths get a closing note.

## Steps

- [x] 1. This note, the milestone note, the P02 note, the index and PLAN rows (documentation only).
- [x] 2. Red first: a test that the rig knows only the product rows (the retired names are refused, `HOST_BUILD` holds only `hostd`, there is no `legacy` module, no marker); it fails on the present tree.
- [x] 3. One commit `chore(M16-P01)`: the sources, the modules, the commands, the marker and testmap entries, the rig rows, the helper calls turned into plain product-only lists, the old marker test removed, the citing notes corrected; the new test green.
- [x] 4. Record: what was deleted by path group and size; the results below.

## Acceptance Criteria

The verification is the sum of what changed, not the whole suite: the affected test files green with their counts named and the product rows (`a2`, `oracle`, `hostd`) the same before and after; `cargo test -p lico-xtask`; `cargo xtask check` and `verify` with the `lico-png` receipt unchanged; the product source hash (`suite.source_hash`) equal before and after; link scan with no dead link. No version bump, no new install, no push.

## Result (2026-11-17, as measured)

Deleted in `575f757` (36 files, 48 lines added, 5,177 removed): `src/candidates` 8 files, 112,524 bytes, 3,593 lines; `src/host/rust` 3 files, 26,340 bytes, 750 lines; `src/host/c` 1 file, 21,295 bytes, 600 lines (together 12 files, 160,159 bytes); in `src/lico_measure`: `legacy.py`, `hostcmp.py`, the `hostcmp` and `suite legacy` commands, the rig rows `a`, `b`, `b2`, `b3`, `c`, `c2`, `d`, `CANDIDATE_ENV`, the `HOST_BUILD` rows `rust_std`, `rust_serde`, `c`, and the runs of `c2` in `bundle_p02.py`, `bundle_p03.py` and `__main__.py` (these would have raised for a retired name; the lead's list did not name them; the data tables of the old decision bundle that only read stored results are untouched); the marker and its `addopts`, the two `[[sources]]` entries, the old marker test; seven documents got a closing note.

Verification (the sum of what changed): the red test `test_the_rig_and_the_tree_know_only_the_product_rows_the_retired_prototypes_are_gone` failed before the edits and passes; the six affected test files (`test_client_integration`, `test_korean_ime`, `test_unicode_clients`, `test_view_lifecycle`, `test_protocol_conformance`, `test_selection`): `46 passed` in 18.6 s (peak 688.2 MiB); the collected rows before and after differ only by the removed marker test (47 to 46); the product rows `a2`, `oracle`, `hostd` are all still collected; `cargo test -p lico-xtask` `12 passed`; `cargo xtask check` and `verify` pass (the `lico-png` receipt unchanged); the product source hash is the same before and after (`bb277f60ad99f888...`); link scan 0 dead in 179 files. The mail named seven files; the sixth-and-seventh count differs because two of the files my search found (`test_native_account`, `test_session_comms_and_ux_fixes`) only contain the word "legacy" in a comment.

