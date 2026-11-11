# M05-P02 Report — Top-anchored panel layout, large-directory listing, and page/wheel navigation

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)](../../../milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md)
- Plan: [M05-P02 plan](../../../plans/202611/20261112/20261112_06_M05-P02_directory_pagination_plan.md)
- Branch: `phase/M05-P02-directory-pagination`

## Summary

`M05-P02` eliminates top-row panel clipping and adds complete page and
mouse-wheel navigation for large directories and UTF-8 file previews:

1. **Top-anchored panel rendering (`src/host/hostd/src/gui/ui.rs`)**:
   - Computed `panel_rows` and `panel_cols` in `compose()` prior to
     `panel_view()` so `files_view()` sizes pages to fit the active viewport on
     the very first frame after startup, resize, or font scaling (`12..=20` px).
   - Changed non-transcript (`!is_tx`, i.e. open panel) layout slicing to anchor
     at `first = 0` and advance `end` forward up to `rows_avail` (and trim `end`
     rather than `first` in the post-shaping overflow guard), keeping live
     transcript bottom-anchoring (`scroll_oracle`) untouched.
2. **Large directories & viewport-sized pagination (`fsview.rs`, `files_ui.rs`, `menu.rs`, `toolbar.rs`)**:
   - Raised `fsview::MAX_ENTRIES` from `500` to `10_000`.
   - Replaced the fixed 14-row sliding window in `files_ui.rs` with discrete
     viewport-sized pages (`calc_page_size`), `clip_cols` row truncation so
     long filenames never wrap and displace rows, and `Page X / Y` headings and
     footers in both directory listings and regular-file text previews.
   - Added contextual `< Prev` (`prev_page`) and `Next >` (`next_page`) toolbar
     controls (with disabled reasons at first/last page), `PageUp`/`PageDown`
     and `[`/`]` keyboard page stepping, and `MouseWheel` scrolling
     (`files_wheel`) across both directory and preview modes.
   - Exposed `page`, `total_pages`, `page_size`, and `visible_range` in
     `files_json()`.

## Verification

- `cargo test --manifest-path src/host/hostd/Cargo.toml -- fsview:: gui::` — `182 passed` (including `folder_list_returns_over_500_entries_without_truncating` and all `scroll_oracle` tests).
- `PYTHONPATH=src:. private/venv/bin/pytest -q tests/test_size_ratchet.py tests/test_selection.py tests/test_no_hidden_characters.py tests/test_directory_pagination.py tests/test_workspace_files.py tests/test_workspace_restore.py tests/test_toolbar_and_nav.py` — `39 passed` (`ui.rs` at `1,115 <= 1,120` lines).
- Manual inspection bundle captured and inspected locally (`202611/20261112/00007_M05-P02_directory_pagination/`).
