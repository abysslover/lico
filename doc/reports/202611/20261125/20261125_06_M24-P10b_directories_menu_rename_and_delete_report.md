# M24-P10b: Rename and Delete in the Directories menu

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P10b-directories-menu-rename-and-delete` from `phase/M24-P10a-directories-menu-terminal-and-new-folder` (`35b95d5`); code `CODE_SHA`; not pushed (P07 to P10 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words in the chat on 2026-11-25 (quoted in the [P10a plan](../../../plans/202611/20261125/20261125_05_M24-P10a_directories_menu_plan.md) and in the [plan](../../../plans/202611/20261125/20261125_06_M24-P10b_directories_menu_rename_and_delete_plan.md))
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (MEASURED in the window test; READ from the code)

| | Before (P10a) | Now |
|---|---|---|
| The menu on a row | Open terminal, New folder | also **Rename** and **Delete**, in the owner's order |
| Rename | no way in the window | asks the new name on the input line with the old name on it (the line a session is renamed on, so the Korean input works); Enter renames the row in place and the list shows it; an empty, bad, unchanged or **taken** name is refused with its reason and stays on the line; **nothing is ever replaced** (a file, a folder, a link or a dangling link holding the name all refuse); Escape renames nothing |
| Delete | no way in the window | a question first, with the name, what it is (`a folder with 12 items`, counted up to 5,000 and then `more than`; `a file of 1.5 KiB`; `a link to ...`) and `This cannot be undone` (there is no Trash); **Y** deletes, **N** or Escape keeps it; a folder goes with all that is in it; **a link is removed as a link, never followed**, also inside a folder: what it points to is not touched |
| The empty space | | Rename and Delete greyed `point at a file or folder` |
| The root, the home folder and the folders above it | | greyed `protected`; a link that points at one of them is only a link and may go |
| Another screen open, an ssh host, no folder listed | | greyed `close the open screen first`, `this machine only`, `no folder here` (the order of the reasons: machine, folder, open screen, then the row, then the protection) |

## 2. How it is built (READ)

- `dirops.rs` (std only, 13 unit tests, 7 of them new): `rename(parent, old, new)` (the name's rules, the same name, a thing that is gone, a name that anything holds, then the move), `remove(path)` (`remove_dir_all` for a folder, `remove_file` for a file and a link, `symlink_metadata` so a link is never followed), `describe` and `describe_limited` (the words of the question; the count walks the tree without entering a link and stops at `5,000` items or `250 ms`), `human_bytes`, `protected(entry, home)` (the entry's own last name is not resolved, the folder it stands in and the home folder are).
- `dirmenu.rs`: two more `Kind`s; `Ctx` knows whether the pointer is on a row and whether it is protected; `Target` carries the folder the list shows (`parent`) and `entry()`, the path of the row itself; `entry_ask` opens the question (the rename panel with the old name on the line, or the confirmation); `rename_entry_submit`, `delete_entry_do`, and the two views.
- `confirm.rs`: `Act::RenameEntry` and `Act::DeleteEntry` ride the rename panel and the confirmation of the session actions (`rename_submit`, `confirm_key`, `confirm_click`, `ask_do`, `ask_view`), so Enter, Y, N, Escape and the click on the `Y` line work as they do there; `Ask` got one field, `detail`, worked out once when the question is asked (not at every frame).
- **Said, not hidden:** the rename checks that the new name is free and then moves (two steps; the standard library has no move that refuses to replace), so a program that made the same name in between could be replaced. Nothing in this window makes files in the background.

## 3. Checks (MEASURED)

- **Tests first**: the seven new `dirops` tests were run red on stubs (all `7` failed: `rename`, `remove`, `describe` and `protected` answered `stub`) and then green; the new menu tests did not compile on the old code (no `Kind::Rename`, no `on_row`) and are green now. The window tests (`tests/test_directories_menu.py`, now `11` tests) were written together with the window code and passed at their first run: the mutants below are what shows that they bite.
- **Window tests** (a real host with a `/bin/sh` shell and a real window on a private display, the test's own home folder): Rename and Delete greyed on the empty space with their reason and a click on one asking nothing; Rename with the old name on the line, a taken name refused with what was typed kept, the same name said, a good Korean name renaming the file, Escape renaming nothing, a folder renamed with its content; Delete of a file with N, with Escape and with Y, a link to a folder removed as a link with the folder it pointed to untouched, a folder with a link inside removed with the outside untouched; the home folder's row greyed `protected` with Open terminal and New folder still usable.
- MUTATION_LINE
- HOOK_LINE

## 4. What is not decided (the owner's)

- Whether a deleted row should go to a Trash folder instead of for good: this machine has no Trash to count on, so the question says `This cannot be undone`; a trash would be its own phase.
- Compress and Extract are `M24-P10c`.
- Publication is under the owner's advance yes of 2026-11-25 (with the release `0.8.2`).
