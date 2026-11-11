# M01-P08 report — acceptance pack, test-suite optimization, and `lico 0.1.0`

- Version: 1.0.0
- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Date: 2026-11-11
- Plan: [M01-P08](../../../plans/202611/20261111/20261111_04_M01-P08_acceptance_pack_plan.md) (version 1.0.0, G2 approved by the owner on 2026-11-11)
- Milestone: [M01 — Measured Linux foundation](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Branch: `phase/M01-P08-acceptance-pack` (commits `caad421`, `d8a8ab3`, `0bed618` off `main` `2495f2d`)
- Tested tree: `0bed618` (`lico 0.1.0 (commit 0bed618)`) and clean public clone of `https://github.com/abysslover/lico.git` (`2495f2d`)
- Inspection bundle: `private/manual_inspection/202611/20261111/00006_M01-P08_acceptance_pack/`

## What was done

1. **S02 (`d8a8ab3`) — Test-suite bottleneck optimization (`167.7–179.3 s` -> `92.9–93.2 s` cold, `-44.6 %`, zero dropped assertions):**
   - Pre-built `cargo test --bin lico --no-run` inside `suite.py` before launching parallel pytest workers so `rustc`/`ld` peak RSS never overlaps live Xvfb/GUI test workers, allowing the default worker count to move safely from `3` to `4` (`peak_rss_mib = 1,588.7 MiB` in S02 and `1,856.2 MiB` in S03, well under the `2,048 MiB` guard).
   - Fixed two real product-window responsiveness/input bugs uncovered by faster event pacing:
     - `App::resumed()` (`src/host/hostd/src/gui/mod.rs`) now requests `status` and `account.list` immediately when the window opens instead of waiting `1,000 ms` for the first background tick.
     - `ui_window_event()` (`src/host/hostd/src/gui/ui.rs`, shrunk from `1,168` to `1,167` lines) now clears `self.ui.clicked` (`DOUBLE_CLICK` state) on keyboard events and non-row clicks so `Click row -> Esc -> Click row` within `400 ms` never falsely opens the `Rename session` panel.
   - Replaced blind `time.sleep(...)` waits in `ProductWindow` (`src/lico_measure/captures.py`) and 8 slow test modules (`test_frame_cost.py` `33.4 s -> 7.4 s`, `test_host_replacement.py` `55.8 s -> 27.0 s`, `test_first_view.py` `33.8 s -> 15.6 s`, `test_activity.py` `31.3 s -> 19.2 s`, `test_paste_open.py` `17.5 s -> 8.2 s`, `test_window_keys.py` `12.6 s -> 10.0 s`, `test_permission_mode.py` `13.7 s -> 3.6 s`, `test_single_executable.py`) with deterministic state and frame-counter polling, keeping every test and assertion intact.
2. **S03 (`0bed618`) — Clean public-clone reproducible build verification, `0.1.0` minor version bump, and proc-RSS unit-probe isolation:**
   - Cloned `https://github.com/abysslover/lico.git` (`2495f2d31da26722e7c29aed052063f800a4a8ea`) twice into distinct directories and built both with `src/host/hostd/build-release.sh`. Both builds produced byte-for-byte identical single-executable ELF binaries (`7,852,296` bytes, `sha256` `6d79dd3bc8f2453804b0747dbc025fb8d4ff33996adda1e7c7579e5f4eec4e9e`), importing only glibc/loader libraries (`ld-linux-x86-64.so.2`, `libc.so.6`, `libgcc_s.so.1`, `libm.so.6`, `linux-vdso.so.1`), printing full `lico licenses` (`> 100 KiB`), and containing zero build-path or private-directory strings.
   - Bumped `lico` version from `0.0.0` to **`0.1.0`** (`Cargo.toml`, `Cargo.lock`, `codex.rs` `clientInfo.version` via `env!("CARGO_PKG_VERSION")`, and `tests/test_single_executable.py`).
   - Isolated `memory::tests::trim_heap_returns_dropped_large_buffer_pages_to_the_kernel` in its own `--exact` child process so parallel `png`/`scroll_oracle` unit tests cannot perturb process-wide `private_kb`, and switched `helper::tests::property_driver` `snap()` from per-CPU-batched `VmRSS` (`192 KiB` kernel counter steps + shared-library cache pages) to exact `/proc/self/smaps_rollup` `Anonymous:` (`+32 KiB` total across 50 captures, 20 timeouts, and 300 quick jobs).
   - Re-verified `suite run --cold` (`449/449` passed in `92.9 s`, `0` skipped, peak `1,856.2 MiB`) and `suite acceptance` at `0.1.0` (`1/1` passed, exit `0`).
3. **S04 — Updated owner guide (`doc/manual_end_to_end.md` `1.0.0`), inspection bundle `00006_M01-P08_acceptance_pack/`, and complete `M01` evidence index below.**

## Complete `M01` "Done when" evidence index

| # | `M01` "Done when" condition | Measured reports & automated checks | Live / clean-clone verification | Status |
|---|---|---|---|---|
| 1 | Architecture experiment reports process-tree RSS/PSS, hidden-view growth, compositor-visible activation, and restart latency before selecting a renderer/host; missed budgets return explicit decisions | [M01-P01 report](../../202610/20261023/20261023_01_M01-P01_architecture_measurement_report.md) (4 candidates measured; `winit + softbuffer + cosmic-text` selected at `~21–24 MiB` idle RSS vs `< 50 MiB` budget) and [M01-P07a toolkit comparison report](../../202610/20261025/20261025_03_M01-P07a_ui_toolkit_comparison_report.md) (`egui`, `iced`, `Slint` vs hand-drawn client; Option A selected by owner) | Owner G3 accepted on 2026-11-11 | Met |
| 2 | Synthetic host work and bounded output continue while GUI is hidden, destroyed, or restarted; snapshot/cursor recovery proves continuity without replaying complete transcript or retargeting ambiguous input | [M01-P02 report](../../202610/20261023/20261023_02_M01-P02_persistent_host_report.md), [M01-P05 report](../../202610/20261024/20261024_03_M01-P05_workflow_recovery_and_single_executable_report.md), [M01-P07 report](../../202610/20261025/20261025_02_M01-P07_window_first_view_and_keys_report.md), [M02-P07 report](20261111_04_M02-P07_memory_pass_report.md) (`+0.56 MiB` 10-min hidden drift); `test_protocol_conformance.py`, `test_view_lifecycle.py`, `test_recovery.py`, `test_first_view.py` | Live session `s3f20f6` survived host build replacement (`f145a441 -> 18c77884`) and resumed cleanly (`[provider reconnected]`) on 2026-11-11 | Met |
| 3 | At least one installed official provider path is tested with exact version, documented auth limits, and real input/approval/reconnect evidence; mock and installed-provider results remain separate | Simulator/contract suites (`test_codex_adapter.py`, `test_claude_adapter.py`, `test_korean_ime.py`, `test_permission_mode.py`) recorded separately in [M01-P03](../../202610/20261024/20261024_01_M01-P03_first_real_agent_report.md) and [M01-P04](../../202610/20261024/20261024_02_M01-P04_accounts_and_second_provider_report.md); installed-provider headless/live checks in [M02-P03](../../202610/20261027/20261027_01_M02-P03_sending_between_sessions_report.md) and [M02-P04](20261111_01_M02-P04_screen_and_copy_paste_report.md) | Owner live Claude Code session (`s3f20f6`, `claude / claude_initial`, `claude-sonnet-5-5`, `mode: auto`) exercised in `lico gui` on 2026-11-11: verified multi-turn Korean input, permission-mode behavior, host build replacement & `[provider reconnected]` continuity across multi-day gap, `mcp__lico__*` tools, handoff inbox, and `mem sessions 120M host <1M window 7M` (owner G3 accepted) | Met |
| 4 | Lico-owned secrets use only OS keyring; locked/missing store fails closed; same-provider account A/B routing, refresh, and logout are independent with explicit per-session bindings and no silent fallback | [M01-P04 report](../../202610/20261024/20261024_02_M01-P04_accounts_and_second_provider_report.md), [M01-P06 report](../../202610/20261025/20261025_01_M01-P06_in_window_login_and_session_creation_report.md); `test_credential_store.py`, `test_accounts.py`, `test_account_lifecycle.py`, `test_provider_isolation.py`, `test_login.py`, `test_login_window.py`, `test_mask_code_line.py` | Fail-closed keyring check (`DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` -> `credential_store_unavailable`) verified on standalone release binary in `test_single_executable.py` | Met |
| 5 | Clean public clone produces the reviewed Linux package without private repositories; independent review, automated checks, inspectable bundle, and owner's final use support the release decision | `src/host/hostd/build-release.sh`, `test_single_executable.py`, `suite acceptance` (`s03_acceptance_010.log`), `--cold` ledger (`449/449` in `92.9 s`), `public_guard.py --self-test` (`0` issues), [updated owner guide](../../manual_end_to_end.md) | Two fresh clones of `https://github.com/abysslover/lico.git` built identical `7,852,296`-byte binaries (`sha256` `6d79dd3bc8f2453804b0747dbc025fb8d4ff33996adda1e7c7579e5f4eec4e9e`, glibc-only `ldd`, clean path scan), bumped to `lico 0.1.0` (`0bed618`) and re-verified via `suite acceptance` + bundle `00006_M01-P08_acceptance_pack/` | Met (pending owner G3/G4) |

## Test-suite optimization summary (`M01-P08` S02–S03)

| Metric | Before (`M02-P07` `cbe6816`) | After (`M01-P08` `d8a8ab3` / `0bed618`) | Budget / Guard |
|---|---|---|---|
| Whole `--cold` suite wall time | `167.7 s` (`179.3 s` in `M02-P05`) | **`92.9 s` (`93.2 s` in S02)** | `<= 180.0 s` |
| Parallel workers | `3` | **`4`** (after pre-building `cargo test --no-run`) | `pri-heavy` bounded |
| Peak RSS across all workers | `1,663.1 MiB` | **`1,588.7 MiB` (S02) / `1,856.2 MiB` (S03)** | `<= 2,048.0 MiB` |
| Python pytest items / Rust unit tests | `449` / `564` | **`449` / `564` (`0` dropped or weakened)** | Zero skipped/dropped |

## Captures and inspection bundle

Saved in `private/manual_inspection/202611/20261111/00006_M01-P08_acceptance_pack/` (`index.html`):
1. `p08_1_start_010.png`: First view of `Lico 0.1.0 (commit 0bed618)`.
2. `p08_2_about_010.png`: `Help > About Lico` showing `Lico 0.1.0` and the single-executable license notice.
3. `p08_3_accounts_and_sessions.png`: Bound session `release-check` under `claude / AdminWork` in `Lico 0.1.0`.
4. `public_clone_receipt.json`, `s02_cold_ledger.json`, `s03_cold_010_ledger.json`, and `s03_acceptance_010.log`.
