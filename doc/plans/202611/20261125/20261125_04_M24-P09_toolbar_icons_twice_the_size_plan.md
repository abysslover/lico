# M24-P09 Plan — The Toolbar Icons Twice The Size

- Status: done (2026-11-25), code `d3413ce`; not pushed (published with the release 0.8.2)
- Version: 1.0.0
- Branch: `phase/M24-P09-toolbar-icons-twice-the-size` from `phase/M24-P08-chats-only-what-was-said` (`266f4a2`; P07 and P08 are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-25): "그리고 상단 게정 세션 만들기, 브라우저 아이콘 크기를 세션 만들기에 보이는 아이콘 크기를 지금 크기에 너비 높이 2배씩 늘리기". Asked which icons were meant, the owner answered: "아이콘 크기 (가)" = the icons of the top toolbar (Accounts, New session, Open browser) at twice the width and twice the height of the size they have now.
- The owner's standing yes (2026-11-25, "agy까지 쭈욱 지정된 버전 번호로 진행"): the guard prefix `phase/M24-P09-` is added without a question; see `AGENTS.md`, "2026-11-25 standing authorization".
- This phase changes only the size of what is drawn. No version bump (the release 0.8.2 carries it).

## What is today (READ from `toolbar.rs`)

The action toolbar is one row under the menu bar. Its height is `line_h x scale + 6 x scale`; the icon buttons are squares of `line_h x scale + scale` (the row's height without the 2 px above and 3 px below), which is `18` px at the default text size and scale 1. An icon is drawn inside its square at `0.56` of the side. The four icons are Accounts, New session, New Home session and Open browser. The text buttons of the Files panel (`+ New here`, `Refresh`, `Hidden`, `< Prev`, `Next >`, `Copy`) fill the same row height.

## What this phase changes

| | Before | After |
|---|---|---|
| Side of an icon button | `line_h x s + s` (17 px at size 13, scale 1) | **twice that**: `2 x (line_h x s + s)` (`36` px at the default size), the same in width and height |
| Height of the toolbar row | `line_h x s + 6 x s` | the icon side plus `5 x s` (the 2 px above and the 3 px below, in units of the scale), so the row grows with the icons |
| The icons drawn | all four | all four, the house too: three of the four are the owner's words, and a row of three big icons and one small one would look broken; the owner's "Accounts, New session, Open browser" is the whole row |
| A text button of the Files panel | the row's height | **unchanged in size** (`line_h x s + s`), centred in the taller row: the owner asked for the icons, not for bigger words |
| The words beside the pointer | beside the last button | the same, centred on the taller row |
| A row too narrow for the doubled icons and the Files screen's six words (a large text size, a narrow window) | the words fit (the icons were small) | **the icons give way first**: each shrinks, as a square, as far as the old side and no further, before any button is pushed out of the row (found by the commit hook: `tests/test_directory_pagination.py` at text size 20 lost its page buttons) |

The side is one number (`icon_side`) used by the row's height, the drawing and the hit boxes, so they cannot disagree. The icon inside its square stays at `0.56` of the side, so the picture is twice as large too.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | All four icon buttons are doubled, not three | see the table: the row must stay one row of equal squares |
| D2 | The text buttons keep their size | not asked; they are the Files panel's and a doubled word would take the room of the page buttons |
| D3 | The size follows the text size and the scale (`line_h`, `s`), twice the old side | the owner said "twice the size it has now"; at a larger text size it is twice what it would have been |
| D4 | No setting | one size, as for every other picture of M22 to M24 (the owner's rule: almost no settings) |
| D5 | In a narrow row the icons shrink down to the old size rather than hide a button | the page buttons of the Files screen must stay reachable; the owner's `2x` holds wherever the row has the room |

## Steps

1. [x] **Tests first, run red**: a unit test of `bar_heights` (the icon side is twice the old side at three scales, the row grows by the same amount, nothing is shown when the menu is off), and the window test `tests/test_toolbar_icons_and_bottom_rows.py` tightened (each icon is a square of twice the old side, measured from the menu bar's own height and the text size, below the menu bar).
2. [x] **The change** in `toolbar.rs`: `icon_side`, `bar_heights`, `draw_top_bars` (the icon's rectangle, the text button's rectangle centred, the hover word).
3. [x] **Mutation checks**: one edit at a time (the factor, the `+ s`, the `5 x s`, the text button's height, the centring), each must turn a test red.
4. [x] **Other window tests** that read the toolbar's rectangles or the height of the first rows (the toolbar, the wizard, the account screen, the menu tests) still pass: run the tests that name the toolbar.
5. [x] **Words**: the guide `doc/manual_end_to_end.md` (the size of the icons), the protocol note is unchanged.
6. [x] `cargo xtask check`, the changed-files suite through the commit hook, `~/bin/lico` rebuilt, the report.

## Not decided here

- The transcript view, the directory menu, the release 0.8.2 and level C are the next phases.

## Results (2026-11-25)

- **Tests first, run red**: the unit test of `bar_heights` failed on the old code with the symptom (`18` against `18`: the icon side was not doubled); the tightened window test failed on the old code (`('accounts', [336.0, 24.0, 355.0, 43.0], 38.0)`: an icon of `19` px, the doubled side `38`) and so did the new lines of the Files screen test (its text buttons are half an icon high). Both pass with the change.
- **The commit** `d3413ce`: `toolbar.rs` (`text_button_h`, `icon_side`, `button_rect`, `bar_heights`, `draw_top_bars`), the unit tests in `ui_tests.rs` (`3` toolbar tests), the window tests `test_toolbar_icons_and_bottom_rows.py` and `test_workspace_files.py`, the guide `1.23.0`, the plan and the report. D1 to D4 held as written; no limit was raised and no version was bumped.
- **Mutation checks**: `34` mutants of `toolbar.rs` in two sets of one edit at a time (the factor, the pixel, the `5 x s`, the menu bar, the icon's and the text button's rectangles, the centring, the scale floor, the no-menu case, the spacing numbers, the strict comparison, the rounding, the clamps, the call site), `31` killed (the unit tests, the window tests for the call site); `3` survive and are written down in the report: `2` equivalent and `1` that no window test is narrow enough to reach.
- **The commit hook**: four attempts were refused before it passed, and nothing was bypassed or raised: the first at a machine load of 70 over its 300 s wall (and four real failures, listed above, that were then fixed); the second for `tests/test_directory_pagination.py` (the page buttons pushed out at text size 20, which led to `icon_fit`); the third because the suite build alone took 212 s under load (the build was then warmed by hand, which the hook allows); the fourth at a load of 55, where about 140 s of the 300 s went before the first test ran and two window tests failed as flukes (both pass by hand). The same command was then run by hand under the same 300 s limit, twice (the second run: 249 tests in 77 files, 171.7 s, under the 180 s limit, peak 946 MiB), and the commit passed from the receipts ("nothing to run: 82 file(s) skipped by receipt").
