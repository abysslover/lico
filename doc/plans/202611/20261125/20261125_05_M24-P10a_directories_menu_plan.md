# M24-P10a Plan — A Right-Click Menu In Directories: The Menu, Open Terminal, New Folder

- Status: done (2026-11-25), code `d564040`; not pushed (published with the release 0.8.2)
- Version: 1.0.0
- Branch: `phase/M24-P10a-directories-menu-terminal-and-new-folder` from `phase/M24-P09-toolbar-icons-twice-the-size` (`ecf980d`; P07 to P09 are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-25): "왼쪽 Directories에서 마우스 오른쪽 버튼 눌러서 디렉토리 생성하기 기능 추가", then "디렉토리 만들기는 컨텍스트 메뉴 초안으로 하나 만들어 보는 것으로 만들기, 이름 변경, 지우기, 압축풀기, 압축하기, 터미널 열기(여기로 옮기는 게 나을 듯)의 기능을 담으면 될 것 같고 제일 많이 할 것은 터미널 열기라서 터미널 열기가 제일 첫 아이템".
- The owner's standing yes (2026-11-25, "agy까지 쭈욱 지정된 버전 번호로 진행"): the guard prefix `phase/M24-P10a-` is added without a question; see `AGENTS.md`, "2026-11-25 standing authorization".
- No version bump (the release 0.8.2 carries it).

## Reading of the words (the lead's; the owner may correct)

A right click in the **Directories** list of the left column (a folder or file row, or the empty space of the list) opens a small menu at the pointer. Its items, in the owner's order: **Open terminal** (first), **New folder**, **Rename**, **Delete**, **Extract**, **Compress**. This phase builds the menu and its first two items; P10b adds the other four. "여기로 옮기는 게 나을 듯" is read as: the menu's Open terminal is the quick way, in the folder that was clicked; the wizard's Terminal picture stays (it is one of the owner's five pictures).

## What this phase does (decisions D1 to D7)

| # | Decision | Detail |
|---|---|---|
| D1 | The menu opens on the **right button** over the list area of the Directories tab (from the folder line down to the page footer: the folder and file rows, the `[..]` row, the empty space), not over the header, the tabs, the filter or the buttons | the list is where the person points at folders |
| D2 | **The target** is the row under the pointer: a folder row is that folder; a file or link row and the empty space mean the folder the list shows | "here" is always a folder |
| D3 | **Open terminal** (first item) opens a terminal tab in the target folder (the same `terminal_open` that the menu and the Terminal picture use, with the folder given) | the most used one, so the first |
| D4 | **New folder** asks a name on the input line (the panel of the rename, so the Korean input works as it does there), makes the folder inside the target folder, refreshes the list and says so; a name may not be empty, hold a `/` or a NUL or a line break, be `.` or `..`, or be longer than 255 bytes; a name that is taken is refused with its reason and nothing is overwritten; what was typed stays to be corrected | the owner's first words: create a directory |
| D5 | **Greyed with a reason** (the items stay in their place, the reason is the hover and the flash): on an ssh host `this machine only`; with no folder listed yet or an unreachable list `no folder here`; New folder while another screen (a panel) is open `close the open screen first` | no surprise on another machine; no lost form |
| D6 | **Keys and mouse**: Up and Down move over the enabled items, Enter runs, Escape closes, a click on an item runs it and a click anywhere else closes the menu (and is not passed on), another mouse button closes it; the menu is clamped inside the left column and the window | like the bookmarks popup |
| D7 | The menu is in the window's dump (`sidebar.dirmenu`: open, the target folder and row, the items with label, enabled, reason, rect; the selected one) so the tests read it | the way every popup is tested |

## The pieces

1. `dirops.rs` (leaf module, std only, no window): `name_problem(name)` and `make_dir(parent, name)`, tested over a temporary tree. P10b adds the other operations here.
2. `dirmenu.rs` (the window part): `target_for` and `items` as pure functions, the state, the geometry, the drawing, the right-press, click, hover and key handling, the New folder question and its answer.
3. Small edits in existing files that add no line to the ones at their limit: the right-button arm in `ui.rs` (`dirmenu_button`), the first line of `sidebar_popup_click`, the line of `sidebar_nav_key` in `sidebar_key`, the hover arm in `memrow.rs`, the draw call on the line of the host popup in `ui.rs`, the `Action::DirMenuItem`, the list rectangle in the sidebar geometry and the dump, `Act::NewFolder` in `confirm.rs` (the question's words, the submit).
4. Tests: unit tests of `dirops` and of `target_for`/`items` (red first), the real window test `tests/test_directories_menu.py` on a private display with a real host and a real shell (right click on a folder row, the items in order, Open terminal puts a shell in that folder, New folder makes the folder on the disk and the list shows it, Escape and click-away close, the refusals).

## Steps

1. [x] **Tests first, run red**: the unit tests of `dirops` (names, the made folder, the taken name, a missing parent), of `target_for` and `items` (the order, the greying), and the window test.
2. [x] **The change**: `dirops.rs`, `dirmenu.rs`, the small edits.
3. [x] **Mutation checks**: one edit at a time on `dirops.rs` and `dirmenu.rs` (the name rules, the target rule, the order, the greying, the clamp), each must turn a test red.
4. [x] **Other window tests** that read the left column or the right button (the sidebar, the toolbar, the terminal tab) still pass.
5. [x] **Words**: the guide `doc/manual_end_to_end.md` (the menu and what New folder does), the protocol note is unchanged.
6. [x] `cargo xtask check` and `graph`, the changed-files suite through the commit hook, `~/bin/lico` rebuilt, the report.

## Not decided here

- Rename, Delete, Extract and Compress are P10b; the transcript view, the release 0.8.2 and level C come after.

## Results (2026-11-25)

- **Tests first, run red**: the unit tests of `dirops` and `dirmenu` were run on stubs and failed (`5` of `6` and `7` of `7`), then green; the window test `tests/test_directories_menu.py` (`6` tests at first, `7` with the right-edge test added by the mutation checks) was written before the window code.
- **The commit** `d564040`: `dirops.rs` and `dirmenu.rs` (new, with their unit tests), the small edits in `ui.rs`, `sidebar.rs`, `sidebar_render.rs`, `memrow.rs`, `controls.rs`, `confirm.rs` and `mod.rs` (no line added to a file at its limit), the window test, the guide `1.24.0`, the plan and the report. D1 to D7 held as written; no limit was raised and no version was bumped. The module graph is unchanged (the cycle of hubs has `50` modules).
- **Mutation checks**: `43` mutants, one edit at a time (the file restored and its checksum compared after every one): `29` of the unit-tested part (`dirops.rs`: the trim, the dots, the slash and NUL, control characters, invisible names, the limit and how it counts, the parent check, the taken name, the denied parent, `create_dir_all`; `dirmenu.rs`: the child path, the target rule, the dropped row, the order and the greying of the lines, the first usable line, Up and Down, the end stops, the three bounds of `place`) and `14` of the window part (where the menu opens, which button, the click away, a greyed line, the terminal's folder, the refresh, the lost name, the targets, hover, Escape, Enter, the folder row as target, the first selected line, the column's bound). All `43` were killed in the end, and two of them showed gaps in the tests that were then closed: the denied-parent mutant first survived because the unit test only looked for the word `permission`, which the system's own fallback text (`permission denied`) also holds, so it now asserts the exact words; and the mutant that took the column's bound off at the call site survived because no window test opened the menu at the right edge, so `test_a_right_click_at_the_right_edge_keeps_the_whole_menu_inside_the_left_column_and_the_window` was added (the window tests are now `7`). One window mutant did not compile and was replaced by one that does.
- **The commit hook**: nothing was bypassed or raised. The first commit attempt was refused by the public guard (a unit-test fixture held a path that starts like a home folder; the fixture was reworded, the guard untouched). Before it, the suite run by hand under the hook's `300 s` limit was cut at the wall at a machine load of `48` (the second run, at load `20`: `301` tests in `84` files passed in `137.1 s`, peak `959 MiB`, under the `180 s` and `1,536 MiB` limits); after the reword `4` tests needed a rerun (`21.3 s`) and the commit passed from the receipts ("nothing to run: 84 file(s) skipped by receipt").
- **Next**: `M24-P10b` (Rename and Delete) on its own branch, then `M24-P10c` (Compress and Extract).
