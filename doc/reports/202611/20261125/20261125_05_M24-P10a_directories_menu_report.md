# M24-P10a: A right-click menu in Directories (Open terminal, New folder)

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P10a-directories-menu-terminal-and-new-folder` from `phase/M24-P09-toolbar-icons-twice-the-size` (`ecf980d`); code `d564040`; not pushed (P07 to P10 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words in the chat on 2026-11-25 (quoted in the [plan](../../../plans/202611/20261125/20261125_05_M24-P10a_directories_menu_plan.md))
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (MEASURED in the window test; READ from the code)

| | Before (`0.8.1`) | Now |
|---|---|---|
| The right button over the Directories list | nothing | a small menu at the pointer: **Open terminal** (first), **New folder** |
| Open terminal | the Terminal picture or the menu opens a shell in the folder the list shows | also a line of the menu: a shell in the folder that was pointed at (a folder row means that folder; a file row, a link and the empty space mean the folder the list shows) |
| New folder | no way in the window | asks a name on the input line (the line a session is renamed on, so the Korean input works the same); Enter makes the folder inside the folder pointed at, the list shows it; a name that is taken, empty, with a `/` or a line break, `.` or `..`, or over 255 bytes is refused with its reason and stays on the line; Escape makes nothing |
| An ssh host, no folder listed, another screen open | | every line greyed `this machine only` / `no folder here`; New folder greyed `close the open screen first`; a click on a greyed line says why |
| Keys and mouse | | Up and Down over the usable lines, Enter runs, Escape closes, the pointer over a line selects it, a click away closes the menu and goes no further, another mouse button closes it |

## 2. How it is built (READ)

- `dirops.rs` (std only, 6 unit tests): `name_problem` (the rules of a name, the words of each refusal) and `make_dir` (the parent must be a folder; `create_dir`, never `create_dir_all`; a taken name, a link, a dangling link, a file, a missing parent and a denied parent are each said and nothing is touched).
- `dirmenu.rs` (7 unit tests of the pure part, then the window part): `target_for`, `items` (the order, the greying and its reasons in the order machine, folder, open screen), `first_enabled`, `step`, `place` are pure; `DirMenu` is the open menu; `dirmenu_button`, `dirmenu_click`, `dirmenu_hover`, `dirmenu_key`, `draw_dirmenu`, `dirmenu_json` are the window's part; Open terminal calls the `terminal_open` of the menu and the pictures with the folder given.
- The question of New folder is the panel of the rename (`Step::Rename`) with `Act::NewFolder` and the folder in `Ask::cwd`: the same input line, the same Escape, the same Korean input; `rename_submit` hands the name to `new_folder_submit`.
- Edits in files at their limit add no line: the right-button arm in `ui.rs` (the call put first on the line), the draw call on the line of the bookmarks popup, the first line of `sidebar_popup_click`, the line of `sidebar_nav_key` in `sidebar_key`, the hover arm in `memrow.rs`; `sidebar.rs` is at `794` of `800`, `mod.rs` at `800`.
- `list_rect` (the rows' area, from the folder line to the page footer) is new in the sidebar's geometry and dump, with `dirmenu` (open, folder, row, selected, rect, the lines with label, enabled, reason, rect).

## 3. Checks (MEASURED)

- **Tests first**: the `dirops` and `dirmenu` unit tests were run red on stubs (`5` of `6` and `7` of `7` failed) and then green; the window test `tests/test_directories_menu.py` (6 tests at first, 7 with the right-edge test that the mutation checks asked for) was written before the window code and first run after it (`4` of `6` passed at once; the two that failed were the test's own mistakes: a select-all key that the input line reads as "go to the start", and a dump key in the wrong place).
- **Window tests** (`tests/test_directories_menu.py`, a real host with a `/bin/sh` shell and a real window on a private display): the menu on a folder row, a file row and the empty space, and at the right edge of the list (moved left to stay inside the column and the window, its lines still taking clicks); the lines in order; Open terminal puts the shell in the pointed folder; the keys; hover; the click away not entering the folder under it; the header, the filter and the middle button open nothing; New folder: a taken name is refused and stays on the line, a good name makes the folder and the list shows it, on a folder row the folder is made inside it, Escape makes nothing; the greyed New folder over an open screen does nothing and keeps what was typed.
- **Mutation checks**: `43` mutants, one edit at a time (the file restored and its checksum compared after every one): `29` of the unit-tested part (`dirops.rs`: the trim, the dots, the slash and NUL, control characters, invisible names, the limit and how it counts, the parent check, the taken name, the denied parent, `create_dir_all`; `dirmenu.rs`: the child path, the target rule, the dropped row, the order and the greying of the lines, the first usable line, Up and Down, the end stops, the three bounds of `place`) and `14` of the window part (where the menu opens, which button, the click away, a greyed line, the terminal's folder, the refresh, the lost name, the targets, hover, Escape, Enter, the folder row as target, the first selected line, the column's bound). All `43` were killed in the end, and two of them showed gaps in the tests that were then closed: the denied-parent mutant first survived because the unit test only looked for the word `permission`, which the system's own fallback text (`permission denied`) also holds, so it now asserts the exact words; and the mutant that took the column's bound off at the call site survived because no window test opened the menu at the right edge, so `test_a_right_click_at_the_right_edge_keeps_the_whole_menu_inside_the_left_column_and_the_window` was added (the window tests are now `7`). One window mutant did not compile and was replaced by one that does.
- **The commit hook**: nothing was bypassed or raised. The first commit attempt was refused by the public guard (a unit-test fixture held a path that starts like a home folder; the fixture was reworded, the guard untouched). Before it, the suite run by hand under the hook's `300 s` limit was cut at the wall at a machine load of `48` (the second run, at load `20`: `301` tests in `84` files passed in `137.1 s`, peak `959 MiB`, under the `180 s` and `1,536 MiB` limits); after the reword `4` tests needed a rerun (`21.3 s`) and the commit passed from the receipts ("nothing to run: 84 file(s) skipped by receipt").

## 4. What is not decided (the owner's)

- The owner's words "여기로 옮기는 게 나을 듯" are read as: the menu has Open terminal in the pointed folder and the wizard's Terminal picture stays. If the picture should go, it is the wizard's one tile.
- Rename and Delete are `M24-P10b`; Compress and Extract are `M24-P10c`.
- Publication is under the owner's advance yes of 2026-11-25 (with the release `0.8.2`).
