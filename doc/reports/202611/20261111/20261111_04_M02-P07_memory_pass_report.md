# M02-P07 report - memory pass and full-milestone review with every feature switched on

- Version: 0.1.0
- Status: in_review (inspection verdict G3 pending; G3 falls here at the end of M02-P07 and covers M02-P07 and the whole M02 milestone)
- Date: 2026-11-11
- Plan: [M02-P07](../../../plans/202611/20261111/20261111_03_M02-P07_memory_pass_plan.md) (version 1.1.0; Deviations D1 and D2 record every as-built refinement)
- Milestone: [M02](../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md)
- Branch: `phase/M02-P07-memory-pass`, plan and implementation commits at `cbe6816` (before this closing report and status commit) cut at `main` `c98b11e`; nothing is pushed. Inspection bundle `private/manual_inspection/202611/20261111/00005_M02-P07_memory_pass/` (3 window captures, warm `--cold` suite log, `suite acceptance` log, `umask 022`/`077` Rust logs, 6 clean-tree `idle_memory` logs, `hidden_growth` logs, and 10-minute `600.0 s` hidden-window drift JSON).
- Tested tree: `cbe6816` (`lico 0.0.0 (commit cbe6816, build a664cfd4ec579f54)`, standalone release `build 18c778849cdf1dff`). This closing commit touches `doc/`, `PLAN.md` and `README.md` only.
- Gates: G1 and G2 approved 2026-10-26. G3 pending (covers M02-P07 and the whole M02 milestone). G4 local merge to `main` authorized by the owner on 2026-11-11; nothing is published.
- Changes in 0.1.0: first version.

## What was built

With every M02 capability switched on (`M02-P01` through `M02-P06`), Lico's allocator, view lifecycle, and cross-feature edge cases were reviewed, tightened, and measured end to end:

- **S01 `cbe6816`: cross-thread heap reclamation, hidden-view release, and size-allowlist ratchet (`unified.rs`, `memory.rs`, `clipboard/mod.rs`, `gui/mod.rs`, `gui/panel.rs`, `gui/search.rs`, `gui/picture.rs`, `http.rs`, `tools.rs`, `pasted/mod.rs`, `search.rs`, `transfer.rs`, `history/mod.rs`, `tests/size_allowlist.toml`, `MB-01`, `MB-02`, `MB-06`, `TST-07`, D1, D2).**
  - Added `memory::init_allocator()` (called at startup in `unified.rs`) pinning glibc `M_MMAP_THRESHOLD` to `128 KiB` so large transient buffers (`>= 128 KiB`: pasted PNGs, X11 INCR chunks, base64 payloads, large HTTP/MCP replies, file handoffs) are serviced via `mmap(2)` and returned immediately via `munmap(2)` when dropped, even when allocated on a short-lived worker thread and dropped on the UI thread (D2).
  - Shrunk X11 INCR `Collector::into_bytes()` buffers (`shrink_to_fit()`) so a 2.20 MiB pasted PNG chip adds `+2.79 MiB` RSS instead of `+5.06 MiB`, and pressing `Escape` immediately reclaims `2,100 KiB` (`2.05 MiB`) of `RssAnon` and `VmRSS` (closing `M02-P04` Known Limit #3).
  - Added `memory::trim_heap()` (`malloc_trim(0)`) on coarse lifecycle drops (`App::hide`, `panel_close`, `search_close`, picture chip drop/replace/send, `pasted::with_cleanup`, `search::search_op`, `transfer::deliver` / `hand_over_gated`, `history::read`, and HTTP/MCP replies `>= 64 KiB` after dropping the response `String`), and cleared `self.ui.composed` and `self.ui.hit_lines` in `App::hide()`.
  - Ratcheted `tests/size_allowlist.toml` down to `core.rs = 988` (from `1,087`) and `gui/ui.rs = 1,172` (from `1,185`).
- **S02 `cbe6816`: full-milestone adversarial review fixes (`R1` to `R4`).**
  - **R1 (`gui/scroll.rs`, `gui/ui.rs`, `SR-01`):** gave the conversation find bar its own `Overlay::search` flag and `Cause::Search` full-repaint cause (`Cause::ALL` `17 -> 18`), preventing scroll-shift pixel corruption when `self.ui.search.open` highlights transcript rows and keeping `frame.causes` attribution exact (`search` vs `selection`).
  - **R2 (`computer/real.rs`, `vdisplay/mod.rs`, `CU-01`, `CU-05`):** clamped `LICO_REAL_SCREEN_S` to `1..=900` (`15 min`) and `LICO_DISPLAY_IDLE_S` to `1..=600` (`10 min`) so environment overrides can shorten test timers without ever extending real-screen permissions or idle virtual displays; sanitized `status["real_screen"][i]["name"]` against control and dropped invisible characters.
  - **R3 (`transfer.rs`, `gui/row.rs`, `calls.rs`, `computer/real.rs`, `search.rs`, `history/mod.rs`, `WT-01`):** unified human-readable session naming via `transfer::session_label` and `Row::name` so an unnamed git worktree session displays its parent repository name (`worktree_repo` before `cwd`, e.g. `lico-demo (wt)`) across the GUI session list, header, approval cards, `session.search`, `real_screen` banners, `transfer` headers, and `history` cards instead of the internal `<session_id>` worktree directory name.
  - **R4 (`M02-P04` Known Limit #3):** eliminated the ~5 MiB retained secondary-arena RSS after dropping or sending pasted PNG chips in `lico gui`.

## Tests and evidence

- **TST-10, five lines.**
  - **Whole suite from scratch, `--cold`, warm Cargo cache, at `cbe6816`: 449 tests in 76 files on 3 workers, 449 passed, 0 skipped, 167.7 s against the 180 s line (exit 0).** Build skipped (binary matched `cbe6816`); `cargo test --bin lico` ran inside the suite, and `suite acceptance` (two clean-clone reproducible `build-release.sh` builds, `TST-08`) passed in 103.07 s (exit 0). Machine load average: 7.55 at start, 4.41 at end.
  - **Whole Rust unit suite under `umask 022` and `umask 077` at `cbe6816`:** 564 passed, 0 failed, 1 ignored (`property_driver`, run by its parent test) under `umask 022` (14.14 s) and 564 passed, 0 failed, 1 ignored under `umask 077` (13.71 s).
  - **Usual run (`--changed`):** 0.5 s for documentation changes (`test_no_hidden_characters.py`); 1.10 s for `tests/test_m02_milestone_review.py` mapped in `tests/testmap.toml`.
  - **Peak memory** (all workers together, bound 2,048 MiB): 1,663.1 MiB in the warm `--cold` run at `cbe6816`.
  - **Number of tests and what grew** at `cbe6816` (vs `main` `c98b11e`): Python pytest items 448 to 449 (+1 end-to-end milestone review test in `tests/test_m02_milestone_review.py` across 76 files); Rust unit tests 560 to 564 (+4 across `memory`, `transfer`, `computer::real`, and `vdisplay`, plus extended `gui::scroll` and `gui::row` assertions). Diff `c98b11e..cbe6816`: 23 files changed, +387 / -48. Big files (`TST-07`): `core.rs` 988 (allowlist 988), `gui/ui.rs` 1,172 (allowlist 1,172), `gui/mod.rs` 747 (< 800), `transfer.rs` 785 (< 800).

## Measurements

- **MB-04 summary line:** Idle window RSS (`--text --realistic-text --ui --menu --accounts 10`, 3 runs of 3 launches, clean tree `cbe6816`): **scale 1: 24.18, 24.01, 24.06 MiB** (PSS median 16.97, 16.82, 16.95 MiB; `RssAnon` 3.75, 3.94, 3.94 MiB; limit 50.0 MiB, met); **scale 2: 35.14, 35.23, 35.18 MiB** (PSS median 22.48, 22.54, 22.45 MiB; `RssAnon` 3.94, 3.94, 3.94 MiB; limit 50.0 MiB, met). **Idle host RSS:** **7.11 to 7.25 MiB** (`idle_memory`), **8.27 MiB** for 10 sessions × 10,000 records (`hidden_growth`), and **20.17 MiB** for 100 sessions × 10,000 records (`1,000,000` records total). **Hidden growth (`MB-02`, limit <= +10.0 MiB):** 10-vs-100 sessions (`hidden_growth`) max **+0.36 MiB** (`+0.20, +0.36, +0.26 MiB`); real **10-minute (`600.0 s`) hidden-window drift** after exercising all M02 features while 9 background sessions emitted output every 15 s: **GUI RSS 21.09 to 21.66 MiB (`+0.56 MiB`, plateaued flat from minute 7 to minute 10)**, **Host RSS 7.19 to 7.19 MiB (`+0.00 MiB`)**. **PNG chip arena reclamation (`MB-06`):** pasting a 2.20 MiB (`2,305,043`-byte) PNG chip moved `lico gui` RSS `20.55 -> 23.34 MiB` (`+2.79 MiB`) and pressing `Escape` immediately returned `2,100 KiB` (`23.34 -> 21.29 MiB`, `RssAnon 5.63 -> 3.58 MiB`).

### Across-milestone memory and suite trajectory (`M02-P01` to `M02-P07`)

| Phase | Idle window RSS, scale 1 (MiB, limit < 50.0) | Idle window RSS, scale 2 (MiB, limit < 50.0) | Host RSS (10 sessions, MiB) | Phase feature / pass highlight | Warm `--cold` suite (`<= 180.0 s`) |
|---|---|---|---|---|---|
| M02-P01 | 23.33 to 23.60 | 34.44 to 34.60 | ~6.4 | Menu bar, shared controls, scroll step, test selection | 140 tests, 112.0 s |
| M02-P02 | 23.71 | 34.59 | ~6.5 | Real-folder sessions, 64 KiB multi-line input, per-session memory | 193 tests, 132.6 s |
| M02-P03 | 23.49 to 23.55 | 34.63 to 34.74 | ~6.5 | Inline + file handoffs, loopback MCP tool server, in-window approvals | 272 tests, ~148 s |
| M02-P04 | 23.93 to 24.14 | 35.04 to 35.10 | 6.56 | Banded X11 capture (`+4.1..4.85 MiB` helper), clipboard, PNG chip | 441 tests, 178.0 s |
| M02-P05 | 24.22 to 24.25 | 35.37 to 35.38 | 6.38 to 6.56 | Per-session virtual display, XTEST `xjob computer`, 15-min real-screen gate | 446 tests, 179.3 s |
| M02-P06 | 23.98 to 24.03 | 35.07 to 35.20 | 7.24 (`+0 KiB` search) | Git worktrees, attention marks, `notify-send`, on-demand segment search | 448 tests, 172.3 s |
| **M02-P07** | **24.01 to 24.18** | **35.14 to 35.23** | **7.19 (`+0.00 MiB` 10-min drift)** | **`M_MMAP_THRESHOLD` 128 KiB + `malloc_trim(0)` (`2,100 KiB` PNG reclaimed), `+0.56 MiB` 10-min hidden drift** | **449 tests, 167.7 s** |

Across the entire M02 milestone - adding real-folder and git-worktree sessions, 64 KiB multi-line input, inter-session file handoffs, loopback MCP agent tools, X11 screen capture, clipboard read/write, PNG paste, virtual-display and gated real-screen computer use, desktop notifications, and cross-session search - **idle window RSS grew by only `+0.58 to +0.85 MiB` at scale 1 (`24.01 to 24.18 MiB`, 48 % of the 50 MiB limit) and `+0.63 to +0.79 MiB` at scale 2 (`35.14 to 35.23 MiB`, 70 % of the 50 MiB limit)**.

### Real 10-minute (`600.0 s`) hidden-window drift (`MB-02`, `p07_measurements.json`)

| Minute (`elapsed_s`) | `lico gui` RSS (MiB) | `lico gui` `RssAnon` (MiB) | `lico-host` RSS (MiB) | `lico-host` `RssAnon` (MiB) | Background turns emitted while hidden |
|---|---|---|---|---|---|
| 0 (`0.0 s`) | 21.09 (`21,600 KiB`) | 3.38 (`3,464 KiB`) | 7.19 (`7,360 KiB`) | 1.19 (`1,216 KiB`) | 0 |
| 2 (`120.0 s`) | 21.09 (`21,600 KiB`) | 3.38 (`3,464 KiB`) | 7.19 (`7,360 KiB`) | 1.19 (`1,216 KiB`) | 8 |
| 4 (`240.0 s`) | 21.28 (`21,792 KiB`) | 3.57 (`3,656 KiB`) | 7.19 (`7,360 KiB`) | 1.19 (`1,216 KiB`) | 16 |
| 6 (`360.0 s`) | 21.47 (`21,984 KiB`) | 3.76 (`3,848 KiB`) | 7.19 (`7,360 KiB`) | 1.19 (`1,216 KiB`) | 24 |
| 8 (`480.0 s`) | 21.66 (`22,176 KiB`) | 3.95 (`4,040 KiB`) | 7.19 (`7,360 KiB`) | 1.19 (`1,216 KiB`) | 32 |
| 10 (`600.0 s`) | **21.66 (`22,176 KiB`, `+0.56 MiB`)** | **3.95 (`4,040 KiB`)** | **7.19 (`7,360 KiB`, `+0.00 MiB`)** | **1.19 (`1,216 KiB`)** | **39 (`195` records)** |

## M02 "Done when" evidence index

| # | Milestone "Done when" condition | Evidence |
|---|---|---|
| 1 | Sessions in chosen folder, resume, close/rename/delete without touching user folder; 64 KiB multi-line input and folded `>> ` echo | [M02-P02 report](../../202610/20261026/20261026_01_M02-P02_folder_sessions_report.md), `tests/test_folders.py`, `tests/test_input_box.py`, `tests/test_input_echo.py`, `tests/test_session_actions_window.py` |
| 2 | Per-session private memory (`RssAnon + RssShmem` over descendants, RSS on hover) sampled only while a window is attached | [M02-P02 report](../../202610/20261026/20261026_01_M02-P02_folder_sessions_report.md), `src/host/hostd/src/memory.rs`, `tests/test_session_memory.py` |
| 3 | Send text, selected lines, or whole conversation (`<= 4 KiB` inline, `> 4 KiB` file path, nothing cut); Claude MCP tools (`list`, `send`, `read`, `create`) with in-window approvals | [M02-P03 report](../../202610/20261027/20261027_01_M02-P03_sending_between_sessions_report.md), `tests/test_transfer.py`, `tests/test_send_to_session_window.py`, `tests/test_tool_*.py`, `tests/test_card_window.py` |
| 4 | X11 screen capture (`<= 1568 px` agent copy + offset/scale), clipboard read/write (text & PNG), PNG chip paste, middle-click paste, streaming `> 256 KiB` provider-line reader | [M02-P04 report](20261111_01_M02-P04_screen_and_copy_paste_report.md), `tests/test_capture_cli.py`, `tests/test_clipboard_read.py`, `tests/test_clipboard_write.py`, `tests/test_picture_paste.py`, `tests/test_middle_paste.py`, `tests/test_agent_tools.py` |
| 5 | Private per-session virtual display (`Xvfb`), XTEST `computer` actions (click, type, key, scroll, drag, look), and 15-minute gated real-screen control with stop bar and hotkey | [M02-P05 report](20261111_02_M02-P05_computer_use_report.md), `tests/test_computer_use.py`, live Chrome `CU-07` (4/4) and WM-less focus `CU-08` (5/5) |
| 6 | Git worktree sessions with two-step dirty close, attention marks and `notify-send` notifications, and zero-resident-index single/all-session search | [M02-P06 report](20261111_03_M02-P06_everyday_comfort_report.md), `tests/test_everyday_comfort.py` |
| 7 | Idle window `< 50 MiB` at scales 1 & 2 (`24.01..24.18` / `35.14..35.23 MiB`), hidden growth `<= +10 MiB` in 10 min (`+0.56 MiB` in `600.0 s`; `+0.36 MiB` 10-vs-100 sessions), and full-milestone review (`R1`..`R4`) | This report (`M02-P07`), `tests/test_m02_milestone_review.py`, bundle `00005_M02-P07_memory_pass/` |
| 8 | Whole test suite from scratch `<= 180 s` (`167.7 s` for 449 tests + 564 Rust unit tests) and dependency-based `--changed` selection | `tests/test_selection.py`, `tests/test_size_ratchet.py`, `cold_suite_warm.log`, `acceptance.log` |

## Captures

Saved in `private/manual_inspection/202611/20261111/00005_M02-P07_memory_pass/`:
1. `p07_1_all_features_active.png`: 10-session window with unnamed git worktree session `lico-demo (wt)`, background unread dot `•`, and a 2.20 MiB pasted PNG chip active.
2. `p07_2_unnamed_worktree_and_search.png`: `Ctrl+Shift+F` cross-session search jumping directly to `UNNAMED_WT_HIT` inside the unnamed git worktree session `lico-demo` after `Escape` reclaimed `2,100 KiB` of PNG chip memory.
3. `p07_3_reattached_after_10min_hidden.png`: `lico gui` reattached immediately to `worker-1` after spending `600.0 s` (`10 minutes`) hidden on the detached start screen while background sessions produced 39 turns (`+0.56 MiB` GUI RSS drift, `+0.00 MiB` host RSS drift).

## Security

- **Timeout ceiling enforcement, label sanitization, and public hygiene (`CU-01`, `CU-05`, `SF-01` to `SF-10`):** `LICO_REAL_SCREEN_S` is clamped to at most `900 s` (`15 min`) and `LICO_DISPLAY_IDLE_S` to at most `600 s` (`10 min`), preventing environment overrides from turning time-bounded real-screen grants or virtual displays into indefinite ones. `status["real_screen"]` labels strip control and dropped invisible characters and resolve `worktree_repo` consistently with approval cards. `public_guard.py --self-test` and `test_no_hidden_characters.py` passed with 0 findings.

## Try it yourself (for G3 - M02-P07 and full M02 milestone)

The standalone release binary has been installed to `~/bin/lico` (`lico 0.0.0 (commit cbe6816, build 18c778849cdf1dff)`). You can inspect the full M02 workbench either on your live sessions (`lico gui`) or in an isolated sandbox:

```bash
lico --version
T=$(mktemp -d)
lico host --socket "$T/host.sock" --state-dir "$T/state" --sessions 0 --producing 0 &
LICO_SOCKET="$T/host.sock" lico gui
```

1. **Idle & per-session memory (`MB-01`, `MI-01` to `MI-05`):** observe the bottom-left summary (`sessions ... | host ~7 MiB | window ~21-24 MiB`) and hover over any running session's right-aligned MiB figure to inspect private vs RSS memory.
2. **PNG paste & immediate heap release (`XF-05`, `MB-06`):** copy any screenshot to the clipboard, press `Ctrl+V` inside a Claude session (`[picture ... KiB] - Esc removes it`), then press `Esc` and watch `window` memory drop back immediately.
3. **Hidden-view release (`MB-02`):** press `Esc` on an empty input box to detach the active view while background sessions keep working; click any row (or `Alt+1..9`) later to reattach instantaneously.
4. **Full M02 administrator GUI vs agent CLI parity:** try `Ctrl+N` (folder or git worktree session), `Session` menu (`Send to session...`, `Hand over conversation...`, `Capture screen to input`, `Virtual display: start`, `Rename`, `Close`, `Delete`), and `View` menu (`Find in conversation` `Ctrl+F`, `Search all sessions` `Ctrl+Shift+F`, `Agent clipboard write`, `Desktop notifications`). Clean up sandbox if used: `lico --socket "$T/host.sock" host stop --force && rm -rf "$T"`.
