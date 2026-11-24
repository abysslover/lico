# M24-P09: The toolbar icons twice the size

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P09-toolbar-icons-twice-the-size` from `phase/M24-P08-chats-only-what-was-said` (`266f4a2`); code `d3413ce`; not pushed (P07, P08 and P09 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's word in the chat on 2026-11-25, answered "아이콘 크기 (가)" (quoted in the [plan](../../../plans/202611/20261125/20261125_04_M24-P09_toolbar_icons_twice_the_size_plan.md))
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (MEASURED in the window test, at the default text size and scale 1; READ from the code)

| | Before (`0.8.1`) | Now |
|---|---|---|
| An icon button of the toolbar (Accounts, New session, New Home session, Open browser) | a square of `19` px (one text line and a pixel) | a square of `38` px: twice the width and twice the height; at another text size or scale it is twice what it was there |
| The toolbar row | the icon and `5` px around it | the icon and the same `5` px, so the row is `19` px taller and everything under it moves down by that |
| A text button of the Files screen (`+ New here`, `Refresh`, `Hidden`, `< Prev`, `Next >`) and `Copy` | as high as the row | as high as before (half an icon), centred in the taller row |
| The word beside the icon under the pointer | centred on the row | centred on the taller row |
| A row too narrow (a large text size or a narrow window with the Files screen open) | the words fit | the icons shrink as squares, never under the old side, before a button is pushed out |

## 2. How it is built (READ)

- `toolbar.rs`: one number, `icon_side` (`2 x (line x scale + scale)`), used by the row's height (`bar_heights`: the icon side plus `5 x scale`), by the drawing and by the hit boxes; `text_button_h` keeps the old height for the words; `button_rect` gives an icon the whole row and a text button its own height, centred. The picture inside an icon stays at `0.56` of its square, so the picture is twice as large too.
- `icon_fit`: when the row cannot hold the doubled icons and the Files screen's words even at the tightest spacing, the icons shrink (rounded down, as squares) to what fits, never under the old side; the usual spacing is tightened first, as before. Found by the commit hook: at text size `20` the page buttons of the Files screen were pushed out of the row (`tests/test_directory_pagination.py`).
- No other file of the window changed: everything under the toolbar takes its top from `bar_heights`.

## 3. Checks (MEASURED)

- **Tests first, run red**: the unit test of `bar_heights` failed on the old code (`18` against `18`: not doubled); the tightened window test failed on the old code (an icon of `19` px, the doubled side `38`); so did the new lines of the Files screen test. All pass with the change.
- **Unit tests** (3 in `ui_tests.rs`): the side is twice the old side at four sizes and scales and the row grows by the same amount, nothing is shown without the menu, a scale under one counts as one; a text button keeps its height and is centred, an icon fills the row, a text button is never taller than the row.
- **Window tests**: `tests/test_toolbar_icons_and_bottom_rows.py` (each icon is a square of twice the old side, measured from the menu bar's own height and the text size, below the menu bar) and `tests/test_workspace_files.py` (the Files screen's text buttons are half an icon high and centred in the row).
- **What the commit hook found** (four attempts were refused before it passed; nothing was bypassed, the last line of this section tells how it passed): the first ran over its `300 s` wall at a machine load of `70` (before that, four real failures: the scroll-step oracle at scale 2 had a window one row too short once the toolbar grew, so its window is `40` px taller; `tests/test_toolbar_and_nav.py` bounded the toolbar's bottom at `60` px, now `100`; and two window tests that passed on a rerun), the second refused for `tests/test_directory_pagination.py` (the page buttons pushed out at text size `20`), which led to `icon_fit`.
- **Mutation checks**, two sets of one edit at a time on `toolbar.rs` (the file restored and its checksum compared after every mutant). First set, `15` mutants of the icon side, the row's heights, the text button's rectangle and the call site: all `15` killed (`13` by the unit tests, `2` of the call site by the window tests). Second set, after `icon_fit` was added, `19` mutants of `button_rect` and `icon_fit` and their call site (the spacing numbers, the strict comparison, the rounding, the clamps, the scale, the no-icon case, the least and full sizes): `13` killed by the unit tests, `1` more (an icon taller than the row) killed once a case was added, `2` of the call site killed by the window tests (`tests/test_directory_pagination.py` and the toolbar tests), `2` survive and are equivalent (`<` for `<=` where the two answers meet at the full size, and an upper clamp that no input can reach) and `1` survives for a reason that is a limit of the tests: the call site hands `icon_fit` the old side as its least, and no window test is narrow enough to push an icon below it (the unit test of `icon_fit` pins the clamp itself).
- **The commit hook**: four attempts were refused before it passed, and nothing was bypassed or raised: the first at a machine load of 70 over its 300 s wall (and four real failures, listed above, that were then fixed); the second for `tests/test_directory_pagination.py` (the page buttons pushed out at text size 20, which led to `icon_fit`); the third because the suite build alone took 212 s under load (the build was then warmed by hand, which the hook allows); the fourth at a load of 55, where about 140 s of the 300 s went before the first test ran and two window tests failed as flukes (both pass by hand). The same command was then run by hand under the same 300 s limit, twice (the second run: 249 tests in 77 files, 171.7 s, under the 180 s limit, peak 946 MiB), and the commit passed from the receipts ("nothing to run: 82 file(s) skipped by receipt").

## 4. What is not decided (the owner's)

- Nothing new: the owner asked for the icons; the text buttons were left as they are (D2). If the words of the Files screen should grow too, it is one number (`text_button_h`).
- Publication is under the owner's advance yes of 2026-11-25 (with the release `0.8.2`).
