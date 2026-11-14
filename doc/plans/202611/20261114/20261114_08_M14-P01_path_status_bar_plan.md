# M14-P01 Plan — Bottom Status Bar with the Full Path

- Status: implemented (see report)
- Version: 1.1.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M14-P01-path-status-bar` (from `main` at `8857a4c`)
- Milestone: [M14](../../../milestones/202611/20261114/20261114_03_M14_path_status_bar_and_maximized_start_milestone.md)

## Objective

One row at the very bottom of the window, at its full width, below the hint line. It shows the full path of the row the pointer is on (a directory or file row of the Directories tab, a Files panel row, a chat card = its folder), else of the selected row, else the working directory of the active session; `host:path` for an ssh host. Too long: the END of the path behind a leading `…` (`sidebar_render::tail_path`, wide characters as two columns), a muted `click: copy` at the right edge, and a click copies the whole text through the existing clipboard path. New code in `gui/pathbar.rs` only (`ui.rs` is `1,090` of `1,120` lines); no edge into a hub; it redraws only when its text changes. The places that shorten a path today (`sidebar_render.rs:441`, `sidebar_chats.rs:543`, `newchat_draw.rs:77`, `telemetry.rs:327-359`; the others clip names, not paths) keep their short forms.

## Steps

- [x] S01: Documentation only first commit (milestone note and three plan notes); the exact list of path shorteners with file and line, and the existing clipboard path found.
- [x] S02: Red-first window tests (`tests/test_path_status_bar.py`) run against the unchanged code; the "before" screenshots.
- [x] S03: `gui/pathbar.rs` (pure functions, state, drawing, click), the one-row layout change, the page-size re-checks; existing bottom-edge tests changed deliberately and named.
- [x] S04: Green, the "after" screenshots (default and narrow width), a cold run, report `110`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The bar is the last row at full width; text by pointer, then selection, then session; `host:path` for ssh | window tests |
| 2 | A long path with Korean characters keeps its end with `…`, never exceeds the bar; a click copies the whole text | window test, clipboard read |
| 3 | No overlap of bar, hint line, telemetry strip, input box; page sizes fit above the bar at `1200x800` and `640` | window tests |
| 4 | Two idle seconds draw nothing new | window test |
| 5 | `pathbar.rs` has no `crate::` edge into a hub | grep in the report |
