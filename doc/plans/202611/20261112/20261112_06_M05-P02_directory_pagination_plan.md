# M05-P02 Plan — Top-anchored panel layout, large-directory listing, and page/wheel navigation

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)](../../../milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md)
- Branch: `phase/M05-P02-directory-pagination`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

In `lico 0.2.0`:
1. `compose()` (`src/host/hostd/src/gui/ui.rs`) sliced `lines` backwards from the
   bottom (`while first > 0`) for both transcripts and panels. Whenever a panel
   (`Files`, `Accounts`, `About`, `Keys`, `New session`) exceeded `rows_avail`
   (e.g. small window height, `20 px` text scaling, or a wrapped row), the top
   rows (`[+ New session...]`, `[..]`, first entries) were clipped off screen.
2. `fsview::MAX_ENTRIES` capped directory listings at `500` entries, and
   `files_ui.rs` used a fixed 14-row sliding window without explicit page
   indicators (`Page X / Y`), clickable `< Prev` / `Next >` controls, `[` / `]`
   page keys, or `MouseWheel` scrolling.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M05-P02` plan and update milestone/index links | Pre-commit guards pass |
| S02 | Anchor non-transcript panel layout from the top row (`first = 0`, trimming `end_vis` on overflow) in `gui/ui.rs`, record `panel_rows` and `panel_cols` before `panel_view()` so `files_view()` sizes pages dynamically to fit the active viewport without clipping, and raise `fsview::MAX_ENTRIES` from `500` to `10_000` | Rust unit tests and `scroll_oracle` tests pass |
| S03 | Implement viewport-sized discrete pagination (`Page X / Y`, `< Prev` and `Next >` toolbar buttons, `PgUp`/`PgDn`, `[`/`]`, and `MouseWheel` scrolling) across both directory listings and regular-file text previews in `files_ui.rs`, `menu.rs`, and `toolbar.rs`, truncating long display rows to `panel_cols` so line wrapping never hides entries, and exposing `page`, `total_pages`, `page_size`, and `visible_range` in `files_json()` | `tests/test_directory_pagination.py` and `tests/test_workspace_files.py` pass |
| S04 | Capture manual inspection bundle `00007_M05-P02_directory_pagination/`, write the `M05-P02` report, update status tables, and commit | Changed tests and pre-commit guards pass |

## Personal data and security

No new persistence or external network calls are introduced. `folder.list`
continues to validate every path through `folders::check_folder` and read each
directory strictly once per explicit request (`<= 10,000` entries).
