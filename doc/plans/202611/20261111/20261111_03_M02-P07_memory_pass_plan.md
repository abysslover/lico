# M02-P07 - Memory pass and full-milestone review with every feature switched on (2026-11-11)

> Status: in_progress
> Milestone: [M02](../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md)
> Version: 1.0.0
> Branch: `phase/M02-P07-memory-pass`, cut at `main` `c98b11e`; nothing is pushed.
> Approval: G2 for this phase is part of the M02 G1 of 2026-10-26 (milestone, gate row G2: one plan note per phase, written as the phase's first step). G3 falls at the end of M02-P07 for M02-P07 and the whole M02 milestone. G4 local merge to `main` was authorized by the owner on 2026-11-11.

**Version 1.0.0.** Requirements are MB-01 to MB-06, SF-01 to SF-10, TST-01 to TST-11, and cross-feature review of WK, SX, MI, IN, SC, AC, XF, CU, WT, NT and SR (planning set 1.6.6). Numbers marked *measured* come from earlier M02 reports (`c98b11e`); everything else is a design target.

## Goal

With every capability of M02 switched on (`M02-P01` through `M02-P06`), verify and tighten Lico's standing memory footprint, reclaim transient heap arenas after peak operations, fix cross-feature edge cases found in a full-milestone adversarial review, and record complete milestone evidence:
1. **Heap reclamation and hidden-view release (`MB-01`, `MB-02`, `MB-06`, `TST-07`):** return freed glibc heap arenas (`malloc_trim(0)` on `linux` + `gnu`) after multi-MiB image chip drops/sends, panel/search closes, hidden-view transitions, large HTTP/MCP base64 payloads, on-demand segment searches, and file handoffs; clear stale `Composed` layout and `hit_lines` buffers in `App::hide()` so hidden windows hold zero presentation state; ratchet `tests/size_allowlist.toml` down to the shrunk line counts (`core.rs` `1,087 -> 988`, `gui/ui.rs` `1,185 -> 1,172`).
2. **Full-milestone adversarial review fixes (`R1` to `R4`, `SR-01`, `CU-01`, `CU-05`, `WT-01`, `SF-06`):**
   - **R1 (`gui/scroll.rs`, `gui/ui.rs`):** give conversation search its own `Overlay::search` flag and `Cause::Search` full-repaint cause (`Cause::ALL` `17 -> 18`) instead of piggybacking on `Overlay::selection`, preventing scroll-shift pixel corruption when `self.ui.search.open` highlights transcript rows and keeping frame diagnostics exact.
   - **R2 (`computer/real.rs`, `vdisplay/mod.rs`):** clamp `LICO_REAL_SCREEN_S` to `1..=DEFAULT_SECS` (`900 s`) and `LICO_DISPLAY_IDLE_S` to `1..=DEFAULT_IDLE_SECS` (`600 s`) so test environment overrides can shorten timeouts without ever extending real-screen grants past 15 minutes or idle virtual displays past 10 minutes; sanitize `status["real_screen"][i]["name"]` against control and dropped invisible characters.
   - **R3 (`transfer.rs`, `gui/row.rs`, `calls.rs`, `computer/real.rs`, `search.rs`, `history/mod.rs`):** when a git worktree session has no explicit `name`, derive its human-readable label from `worktree_repo` before `cwd` (`<workspace_root>/worktrees/<session_id>`) in both `lico gui` (`Row::name`) and `lico-host` (`transfer::session_label`), matching `claude.rs`, `codex.rs`, `pending.rs` and `create.rs`.
   - **R4 (`M02-P04` Known Limit #3):** resolve the ~5 MiB retained PNG arena noted in the M02-P04 report when dropping or sending pasted screenshots in `lico gui`.
3. **Full 10-minute hidden-growth measurement, clean-clone acceptance build, and milestone evidence matrix (`MB-01`, `MB-02`, `MB-04`, `MB-06`, `TST-01`, `TST-08`, `TST-10`):** measure `idle_memory` at scales 1 and 2 (`< 50.0 MiB`), `hidden_growth` (`10` vs `100` sessions), a real 10-minute (`600 s`) hidden-window drift run after exercising every M02 feature (`<= +10.0 MiB`), `suite run --cold` (`<= 180.0 s`), and `suite acceptance` (`build-release.sh` reproducible clean-clone check).

**Order of the steps:**
- **S01 (`MB-01`, `MB-02`, `MB-06`, `TST-07`):** add `memory::trim_heap()`, release `composed` and `hit_lines` in `App::hide()`, wire `trim_heap()` into GUI (`hide`, `panel_close`, `search_close`, `Pictures`) and host (`http::respond`, `tools::serve_one`, `pasted::with_cleanup`, `search::search_op`, `transfer::deliver`, `history::read`) transient paths after dropping buffers, and tighten `tests/size_allowlist.toml`.
- **S02 (`R1`, `R2`, `R3`):** implement `Overlay::search` + `Cause::Search`, timeout clamps + `real_screen` label sanitization, and `transfer::session_label` + `Row::name` worktree-repo fallback, with Rust unit tests in each touched module.
- **S03 (`MB-01`, `MB-02`, `MB-04`, `TST-01` to `TST-10`):** add `tests/test_m02_milestone_review.py` + `tests/testmap.toml`, run `umask 022`/`077` `cargo test`, warm `suite run --cold` (`<= 180.0 s`), `suite acceptance`, `idle_memory` (scales 1 and 2), `hidden_growth`, and the 10-minute (`600 s`) all-features + hidden-window drift measurement, build the standalone `lico` release binary, assemble bundle `00005_M02-P07_memory_pass`, and write `20261111_04_M02-P07_memory_pass_report.md`.

## What exists already (read on 2026-11-11)

- Idle window RSS at `c98b11e` (`M02-P06`): `23.98 to 24.03 MiB` at scale 1 and `35.07 to 35.20 MiB` at scale 2 (*measured*); 10-session host RSS: `7.24 MiB` (`+0 KiB` search delta, *measured*); warm `--cold` suite: `448` tests in `172.3 s` (*measured*).
- Known post-peak retention from `M02-P04` report ("Known limits" item 3): dropping a ~2 to 3 MiB PNG chip in `lico gui` leaves freed glibc arena pages mapped (~5 MiB warm reuse) until `malloc_trim(0)` returns them to the OS.
- `App::hide()` (`gui/mod.rs:217`) drops `self.view` and detaches from the host, while `self.ui.composed` (`Option<Composed>`) and `self.ui.hit_lines` (`Vec<(usize, usize, usize)>`) stayed allocated until the next visible frame.
- `tests/size_allowlist.toml` still lists `core.rs = 1087` (now `988` lines) and `gui/ui.rs = 1185` (now `1,172` lines).

## Deviations and refinements

- **D1 (targeted `malloc_trim(0)` only after large or lifecycle drops):** `trim_heap()` is called only on coarse lifecycle transitions (`hide`, `panel_close`, `search_close`, picture chip drop/send, search completion, attachment cleanup, file handoff, and HTTP/MCP responses `>= 64 KiB` after the response `String` is dropped), never on per-frame redraws or per-record streaming, preserving frame latency and CPU (`PERF-03`, `PERF-05`).

## Verification plan

1. **Unit tests (`cargo test --bin lico` under `umask 022` and `umask 077`):**
   - `memory::tests`: `trim_heap()` reclaims multi-MiB freed `Vec<u8>` allocations (`RssAnon` drops back near baseline).
   - `gui::scroll::tests`: `Overlay { search: true, .. }` triggers `Err(Cause::Search)` and `Cause::ALL` covers all 18 causes uniquely.
   - `computer::real::tests` and `vdisplay::tests`: `LICO_REAL_SCREEN_S` clamps to `1..=900` and `LICO_DISPLAY_IDLE_S` clamps to `1..=600`; `real_screen` status sanitizes control/hidden chars and uses `worktree_repo` for unnamed worktree sessions.
   - `transfer::tests` and `gui::row`: `session_label` and `Row::name` prefer `name`, then `worktree_repo` basename, then `cwd` basename, then `id7`.
2. **End-to-end regression (`tests/test_m02_milestone_review.py`):**
   - Unnamed git worktree session displays `<repo> (wt)` in `lico gui` and `<repo>` in `session.search` / `transfer` headers; attaching a large PNG chip and pressing `Escape` returns window `RssAnon` immediately; `Ctrl+F` search records `frame.causes.search >= 1` with zero pixel corruption; hiding the view (`Escape`) drops `view` and trims window heap.
3. **Suite, acceptance, and 10-minute memory gates:**
   - Warm `suite run --cold` `<= 180.0 s`, peak `< 2,048 MiB`, `exit 0`; `suite acceptance` `exit 0`.
   - `idle_memory` at scales 1 and 2 `< 50.0 MiB` (`MB-01`); `hidden_growth` (`10` vs `100` sessions) and 10-minute (`600 s`) hidden-window drift with background output `<= +10.0 MiB` (`MB-02`).
