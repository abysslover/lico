# M24-P11g Plan — The Top Icons Level With The Two Rows At The Left

- Status: draft (2026-11-26)
- Version: 1.0.0
- Branch: `phase/M24-P11g-top-rows-level` from `phase/M24-P11h-time-on-narrow-windows`
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-26): "그리고 가능하면 채팅창 상단 아이콘의 크기를 정확히 왼쪽 2줄과 맞춰서 줄이 딱 수평으로 나눠지도록 배치"
- Standing authorization of 2026-11-25: the prefix, the commits and the merge are approved in advance; no version bump (the release 0.8.2 carries it).

## What this phase gives the person

At the top of the window the left column and the right side are two stacks of rows that were built by two pieces of code with their own numbers, so their lines fall at different heights (measured in a window of 1200 x 800 at scale 1):

| | Left column (Directories) | Right side |
|---|---|---|
| first row | tabs `Directories | Chats`, y 0 to 24 | the menu bar, y 0 to 22 |
| second block | the nav row (`< > 1/1 ... Marks`), y 28 to 48, and the host row (`Host: local v .dot R +`), y 52 to 72 | the row of icons, y 22 to 65 (the icons y 24 to 62) |

After this phase **the lines are one line across the window**: the first row is the same height on both sides (menu bar = tabs row), and the second block is the same height on both sides: **the row of icons is exactly as high as the two rows under the tabs on the left** (in the Directories tab the nav row and the host row; in the Chats tab the host row and the filter row). The line under the tabs and the menu bar, and the line under the icons and the second left row, are each at one height from the left edge to the right edge, at every scale of the window.

The icons keep their size (the owner asked for the icons to grow in `M24-P09` and the tile size is the rule of `M24`): the two rows on the left are made to fit the icon, not the icon the rows.

## The reading of the words (to be confirmed by the owner)

"왼쪽 2줄" is read as **the two rows beside the icons**: the nav row and the host row in the Directories tab (they are what stands beside the row of icons in the window today, and the pair that has two rows' height). The first row (the tabs) is matched with the menu bar, which stands beside it. If the owner meant the tabs row and the nav row, the icons would have to be half their size, which is the opposite of what the owner asked for in `M24-P09`; the report says so and shows the window.

## Why it is built this way (what was found)

- The right side is `toolbar::bar_heights(menu, scale, line_h)`: the menu bar is `line_h * s + 4s`, the icon row is `icon_side + 5s` (2s above the icons and 3s below, drawn in `draw_top_bars`). The left side is drawn by `sidebar_render.rs` from the tabs' bottom `tab_y1 = top + lh + 6s`: the nav row `lh + 2s` high at `tab_y1 + 4s`, the host row 4s below it, the filter row 3s below that. Nothing ties the two.
- One function says the geometry for both: `toolbar::top_rows(s, line_h)` gives the height of the first row (`lh + 6s`), the height of a row of the second block (the text button's height, half the icon's side), the gap (2s) and the height of the second block (`2 rows + 3 gaps = icon side + 6s`). The icons sit with 3s above and 3s below (they were 2s and 3s). Both sides take their numbers from it.
- The Chats tab has two rows under its tabs as well (host row and filter row); it takes the same block, so that the line under the second block is the same in both tabs. The count of cards that fit (`calc_chats_ps`) and the rows below the block (the filter row of the Directories tab, the Home row) follow the new height.
- The line itself: the first row's bottom edge and the second block's bottom edge are drawn as the one-pixel lines the bars already have (`2B3646` under the tabs, `2B3440` under the icons), at the same y on both sides, so that they read as one line.

## Decisions

| # | Decision | Detail |
|---|---|---|
| D1 | **One function** | `toolbar::top_rows(s, line_h) -> TopRows { first, row, gap, second }`; `bar_heights` and the sidebar take their heights from it |
| D2 | **The numbers** | first = `lh + 6s` (the tabs row, as it was; the menu bar grows from `lh + 4s`); row = the text button's height (`lh + s`); gap = 2s; second = `2 * row + 3 * gap` = the icon's side + 6s; the icons sit centred in it (3s above and below) |
| D3 | **The left side** | Directories: nav row at `tab_y1 + gap`, host row `row + gap` below it, the rest starts at `tab_y1 + second + gap`; Chats: host row and filter row the same way |
| D4 | **The icons** | their side does not change; the bar is 3 pixels taller at scale 1 (`menu 24 + icons 44 = 68`, was `22 + 43 = 65`) |
| D5 | **The test** | a unit test reads the rectangles of the real window (the menu titles, the toolbar buttons, the sidebar's tabs, nav, host and filter rows, in both tabs, at the scales 1, 1.25, 1.5 and 2) and says: the first row's bottom is the same on both sides, the second block's bottom is the same on both sides, the icons are inside the block with the same room above and below, the rows are inside the block; a window test compares the same numbers in a real window |
| D6 | **Not changed** | the sizes of the icons and of the words, the menu, the panels, the browser view, the tab bar of the sessions (it stays below the bar) |

## Steps

1. [ ] **Tests first, run red**: `toolbar.rs` (`top_rows`, `bar_heights`), the geometry test above on the real drawing (it fails today: 22 against 24 and 65 against 72), the Chats page size.
2. [ ] **The change** in `toolbar.rs`, `sidebar_render.rs`, `sidebar_chats.rs` (and the lines joined in `ui.rs` if any).
3. [ ] **Mutation checks**, one edit at a time on `top_rows` and the lines changed in the sidebar.
4. [ ] **Other tests**: the existing window tests that read positions of the top rows, the oracle at the scales, the suite.
5. [ ] **Words**: the guide.
6. [ ] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report with the screenshot of both tabs.

## Not decided here (the owner's, written in the report)

- Whether "2줄" meant the two rows beside the icons (this plan) or the tabs row and the nav row (the icons would shrink).
- The menu bar is 2 pixels taller (`lh + 6s`); the left rows 1 pixel shorter (the text button's height, not `lh + 2s`) and their gaps 2s: the window is a little tighter at the top left.
