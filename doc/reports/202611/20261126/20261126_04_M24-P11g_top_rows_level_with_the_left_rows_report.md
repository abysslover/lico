# M24-P11g: the top icons level with the two rows at the left

- Version: 1.0.0
- Date: 2026-11-26
- Branch: `phase/M24-P11g-top-rows-level` from `phase/M24-P11h-time-on-narrow-windows`; code `3272548`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-26 (quoted in the [plan](../../../plans/202611/20261126/20261126_04_M24-P11g_top_rows_level_with_the_left_rows_plan.md)): "그리고 가능하면 채팅창 상단 아이콘의 크기를 정확히 왼쪽 2줄과 맞춰서 줄이 딱 수평으로 나눠지도록 배치"
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test)

At the top of the window the left column and the right side were two stacks of rows built by two pieces of code with their own numbers, so their lines fell at different heights. Now they are one line across the window:

| At scale 1 | Left column | Right side |
|---|---|---|
| first row | the tabs, 24 high | the menu bar, 24 high (was 22) |
| second block | two rows of 19 with a gap of 2 above, between and below: 44 high | the row of icons, 44 high (was 43), the icons (38) 3 above and 3 below |

- The line under the tabs and the menu bar, and the line under the icons and the second left row, are drawn from the left edge to the right edge at one height, at the scales 1, 1.25, 1.5 and 2 (the window test reads the pixels at both ends of each line).
- In the Directories tab the second block is the nav row and the host row; in the Chats tab the host row and the filter row. The rows below the block (the filter row of Directories, the first card of Chats) start 2 units under its line; the Chats page holds the cards that fit under it.
- The icons keep their size: the rows were made to fit the icons, not the icons the rows.

## 2. How it is built (READ)

- `toolbar::top_rows(s, line_h)` gives the first row (`lh + 6s`), the height of a row of the second block (a text button's height, `lh + s`), the gap (`2s`) and the second block (`2 rows + 3 gaps`, which is the icon's side and `6s`). `bar_heights` and the sidebar take their numbers from it, so the two sides cannot drift again.
- `draw_left_block` paints the band of the second block in the tone of the icon row and its one-pixel line; the menu bar gets the one-pixel line under it that the tabs already had.
- `ui.rs` (pinned at 1114 lines by the size ratchet) changes by one word on one line; the module graph is unchanged.

## 3. Checks (MEASURED)

- **Tests first**: run red against the old geometry, then green.
- **The tests now**: unit: 5 new in `toprows_tests` (the numbers of `top_rows` and `bar_heights` at five scales; the Chats page size at its edges at scales 1 and 2; the real rectangles, the two lines and the band's tone at the left edge at the scales 1, 1.25, 1.5 and 2 in both tabs; the filter row of the Directories tab and the first Chats card 2 units under the block); one old test (the toolbar icons' height) changed with its reasons; the window test `tests/test_top_rows_window.py` (the lines in both tabs, level, read from the pixels at both ends) and two window tests with the old row height changed. The whole binary: 1,350 unit tests pass (12 ignored)
- **Mutation checks**: **35 mutants** of the new lines (`toolbar.rs`: the height of the first row, of a row and of the gap, the second block's total, `bar_heights`, the band and the two lines of `draw_left_block`, the line under the menu bar, the room above and below an icon; `sidebar_render.rs` and `sidebar_chats.rs`: the tabs' bottom, the gap above, the height of each row, the gap between them, the start of the rows and of the list under the block, the call that draws the block, the page size), each one edit on a restored tree whose files were checked by their sha256 after each; the unit tests of `toprows_tests` and of the toolbar icons run against each. The first run: **34 killed, 1 survived**: the band of the second block starting a gap lower (the strip under the tabs' line kept the old ground, and no test read a pixel of it). A test now reads the band's tone at the left edge on the first row of the band, in the gap between the two rows and on its last row, at the four scales; the mutant is killed. Three mutants (the line under the menu bar dropped, an icon's room below changed, no gap between the two left rows) were also run through the window test `tests/test_top_rows_window.py`, which was written after the code: all three failed it (killed). Result: 35 of 35 killed, none written down as equivalent.
- **The commit hook**: three runs. The first ran the suite and **failed one window test** (`tests/test_toolbar_icons_and_bottom_rows.py` told the scale from the old row height, `font + 8`; it is `font + 10` now: the test and `tests/test_directory_pagination.py`, which made the same assumption, were changed and say why) and was also over the build's peak memory, **1936.7 MiB over the bound of 1536 MiB**, as the runs of P09 to P11d and P11h were. The second run (247 tests in 78 files, 157.5 s, peak 967.2 MiB of the workers) **failed one test that has nothing to do with this change**: `tests/test_session_memory.py` expects the list's total line to begin `mem sessions` and read `free 19% sessions 160M ...`, because the line shows the free memory of the machine when it is under 20 percent, and another session was running `cargo test --workspace` on the same machine at that moment; the test passed in the run of P11h and passed again in the third run (1 test, 10.7 s, the other 87 files from receipts), which accepted the commit. The bounds (180 s, 1,536 MiB, 300 s) stay, never raised or bypassed. The test is host-load dependent: the owner may want it to fix the free memory it expects (a later small phase, not done here)
- Two window tests carried the old geometry in their arithmetic (the scale was told from `font + 8`; it is `font + 10` now): both are changed and say why. The unit test of the old menu height says why its numbers moved.

## 4. What is not decided (the owner's)

- **The reading of "왼쪽 2줄"**: read as the two rows beside the icons (the nav row and the host row). If the owner meant the tabs row and the nav row, the icons would have to be half their size, which contradicts the growth the owner asked for in P09; say so and the rows are redone.
- **With marks shown** (`ui.markers`) the left column starts 16 pixels lower by design, so the lines are not level in that mode.
- The menu bar is 2 pixels taller, the left rows 1 pixel shorter (with 2 gaps), the toolbar 3 pixels taller, the Chats list starts 5 pixels higher: the window is a little tighter at the top left.
