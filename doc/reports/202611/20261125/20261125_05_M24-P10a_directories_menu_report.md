# M24-P10a: A right-click menu in Directories (Open terminal, New folder)

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P10a-directories-menu-terminal-and-new-folder` from `phase/M24-P09-toolbar-icons-twice-the-size` (`ecf980d`); code `CODE_SHA`; not pushed (P07 to P10 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
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

- **Tests first**: the `dirops` and `dirmenu` unit tests were run red on stubs (`5` of `6` and `7` of `7` failed) and then green; the window test `tests/test_directories_menu.py` (6 tests) was written before the window code and first run after it (`4` of `6` passed at once; the two that failed were the test's own mistakes: a select-all key that the input line reads as "go to the start", and a dump key in the wrong place).
- **Window tests** (`tests/test_directories_menu.py`, a real host with a `/bin/sh` shell and a real window on a private display): the menu on a folder row, a file row and the empty space; the lines in order; Open terminal puts the shell in the pointed folder; the keys; hover; the click away not entering the folder under it; the header, the filter and the middle button open nothing; New folder: a taken name is refused and stays on the line, a good name makes the folder and the list shows it, on a folder row the folder is made inside it, Escape makes nothing; the greyed New folder over an open screen does nothing and keeps what was typed.
- MUTATION_LINE
- HOOK_LINE

## 4. What is not decided (the owner's)

- The owner's words "여기로 옮기는 게 나을 듯" are read as: the menu has Open terminal in the pointed folder and the wizard's Terminal picture stays. If the picture should go, it is the wizard's one tile.
- Rename, Delete, Extract and Compress are `M24-P10b`.
- Publication is under the owner's advance yes of 2026-11-25 (with the release `0.8.2`).
