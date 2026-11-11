# M01-P08 Plan: Acceptance pack, test-suite bottleneck optimization, and 0.1.0 release version

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-11
- Milestone: [M01 — Measured Linux foundation](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Phase: `M01-P08` (`phase/M01-P08-acceptance-pack`)
- Approval: G2 approved by the owner on 2026-11-11, together with instructions to eliminate slow test-suite bottlenecks and raise the `major.minor.patch` minor version by 1 (`0.0.0` -> `0.1.0`) once the acceptance checks succeed.

## Why

`M01-P01` through `M01-P07a` and `M02-P01` through `M02-P07` are accepted at G3 and published on `main`. Closing `M01` requires `M01-P08`:
1. Proving that a clean clone of the public repository (`https://github.com/abysslover/lico.git`) builds the reviewed single-executable Linux package reproducibly without private repositories, embedded build paths, or unexpected dynamic library dependencies.
2. Providing a single audited evidence index mapping all five `M01` `"Done when"` conditions to measured reports and live owner verification.
3. Updating the owner end-to-end guide (`doc/manual_end_to_end.md`) to match the full `M01` + `M02` workbench.
4. Eliminating redundant fixed sleeps and worker-scheduling bottlenecks in the test suite (`501.1 s` cumulative wall time across 77 test files, `167.7 s` cold on 3 workers) so daily and cold verification stay fast while preserving every assertion and memory bound (`<= 2048 MiB`).
5. Bumping the package version from `0.0.0` to `0.1.0` upon clean-clone and suite verification, then re-verifying reproducible clean-clone builds and cold tests on `0.1.0`.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this approved phase plan on `phase/M01-P08-acceptance-pack` | Guard checks pass |
| S02 | Profile and remove test-suite bottlenecks: pre-compile `cargo test --bin lico --no-run` before worker launch so `rustc` peak RSS never overlaps GUI workers, tune default parallel workers (`5`) within the `2048 MiB` cap, tighten `ProductWindow` non-IME frame-settle polling, and replace blind fixed `time.sleep` delays in `test_frame_cost.py`, `test_host_replacement.py`, `test_activity.py`, `test_first_view.py`, `test_paste_open.py`, `test_window_keys.py`, `test_permission_mode.py`, and `suite acceptance` with deterministic state/frame synchronization without weakening any assertion | Full `--cold` suite passes with zero dropped tests, lower wall time (`<< 180 s`), and peak RSS `<= 2048 MiB` |
| S03 | Clone the public repository twice into distinct directories, run `src/host/hostd/build-release.sh`, verify byte-for-byte `sha256` reproducibility, `ldd` glibc-only imports, `lico licenses`, and zero private path strings; upon success, bump `lico` version `0.0.0` -> `0.1.0` (`Cargo.toml`, `Cargo.lock`, `codex.rs` handshake `CARGO_PKG_VERSION`, `test_single_executable.py`) and re-verify `--cold` + `suite acceptance` | `suite acceptance` and `suite run --cold` pass at `0.1.0` |
| S04 | Update `doc/manual_end_to_end.md`, generate the `M01-P08` manual inspection bundle, write the `M01-P08` phase report with the complete `M01` `"Done when"` evidence index, and update status documents (`M01` milestone, `PLAN.md`, `README.md`, `roadmap.md`, `doc/index_3.md`) | Guard and reference checks pass |

## Personal data and security

- Public-clone verification builds in an isolated scratch directory and asserts that no build path, home directory, or `/private/` string appears anywhere in the compiled ELF binary.
- All secrets remain OS-keyring-only (`S1`–`S12` unchanged; `lico cred` fail-closed check included in `test_single_executable.py`).
- No desktop notifications are emitted to the host session during automated runs (`LICO_NOTIFY_BIN=""` and `DBUS_SESSION_BUS_ADDRESS` guard).

## Manual inspection

- Bundle path: `private/manual_inspection/202611/20261111/00006_M01-P08_acceptance_pack/`
- Contents: `README.md`, `receipt.json`, `index.html`, public-clone two-build logs and `sha256` digests, `ldd` output, `0.1.0` window captures (`start`, `about`, `new_session`, `accounts`), and cold/acceptance suite ledgers.
- Looks right when: `Help > About Lico` and `lico --version` show `0.1.0`, the two clean builds produce identical `sha256` digests with zero host paths, and every `M01` `"Done when"` item links to measured evidence.
