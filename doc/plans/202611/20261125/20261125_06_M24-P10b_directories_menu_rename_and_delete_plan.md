# M24-P10b Plan — The Directories Menu: Rename And Delete

- Status: done (2026-11-25), code `cc4c684`; not pushed (published with the release 0.8.2)
- Version: 1.0.0
- Branch: `phase/M24-P10b-directories-menu-rename-and-delete` from `phase/M24-P10a-directories-menu-terminal-and-new-folder` (`35b95d5`; P07 to P10a are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-25): "…이름 변경, 지우기, 압축풀기, 압축하기, 터미널 열기 … 의 기능을 담으면 될 것 같고" (see the [P10a plan](20261125_05_M24-P10a_directories_menu_plan.md)). The owner's standing yes (2026-11-25): the guard prefix `phase/M24-P10b-` is added without a question.
- No version bump (the release 0.8.2 carries it).

## What this phase does (decisions D1 to D6)

The menu of P10a gets its third and fourth line, in the owner's order: **Rename**, **Delete**. Both work on a **row** (a folder, a file or a link); on the empty space they are greyed `point at a file or folder`.

| # | Decision | Detail |
|---|---|---|
| D1 | **Rename** asks the new name on the input line with the old name filled in (the line of New folder and of the session rename; Korean input works); Enter renames in place, the list refreshes | the same input as every other name |
| D2 | A new name follows the rules of a folder name (not empty, no `/` or NUL or line break, not `.` or `..`, at most 255 bytes) and **may not be taken** by anything (a file, a folder, a link, a dangling link): the rename never replaces; the same name as now is said `that is its name already`; what was typed stays to be corrected | nothing existing is overwritten |
| D3 | **Delete** asks first, on a screen that says what goes (the name and what it is: `a folder with N items` (counted up to a bound, then `more than N`), `a file of N bytes`, `a link: only the link goes`) and that **Y** deletes and **N** or Escape keeps; there is no Trash on this machine to count on, so the question says `This cannot be undone`; what the question says is worked out once, when it is asked, not at every frame | the owner's own words: 지우기; the question is the guard |
| D4 | Delete **never follows a link**: a link is removed, not what it points to; a folder is removed with what is in it, the links in it removed as links (the standard library's `remove_dir_all` does exactly that) | a link in the tree can point anywhere |
| D5 | **Protected**: the root, the person's home folder and every folder above it (a row that is one of them) cannot be renamed or deleted (greyed `protected`); the `[..]` row is not one of the list's rows and is never a target; a link that points at one of them is only a link and may go | no way to remove everything with one click |
| D6 | Ssh host, no folder listed, another screen open: greyed as in P10a (`this machine only`, `no folder here`, `close the open screen first`); the order of the reasons: machine, folder, open screen, then `point at a file or folder` (the empty space), then `protected` | same rules |

## The pieces

1. `dirops.rs`: `rename(parent, old, new)`, `remove(path)`, `describe(path)` (what the question says), `protected(path, home)`; unit tests over a temporary tree (the link cases, the taken name, the protected paths, a folder with a link to outside is removed without touching the outside).
2. `dirmenu.rs`: two more `Kind`s, `items` takes the row (greyed on the empty space, on a protected row); `Act::RenameEntry` and `Act::DeleteEntry` through the panel of the rename and the confirmation (`rename_submit`, `confirm_yes` by `ask_do`, `confirm_key`, `confirm_click`, `ask_view`); `Ask` gets one field, `detail`, for what the question says about the target; the target of a row carries the folder the list shows, so the path of the row is the same whatever the line.
3. Window tests (`tests/test_directories_menu.py`): rename a folder and a file, a taken name, the old name filled in, Escape; delete with N and with Y, the link case on the disk, the protected row greyed.

## Steps

1. [x] **Tests first, run red** (unit and window).
2. [x] **The change.**
3. [x] **Mutation checks**, one edit at a time on `dirops.rs` and `dirmenu.rs`.
4. [x] **Other window tests** that read the left column, the panel or the confirmation still pass.
5. [x] **Words**: the guide.
6. [x] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report.

## Not decided here

- Extract and Compress are `M24-P10c`.
- The rename checks that the new name is free and then moves (two steps; the standard library has no move that refuses to replace): a name made taken in between by another program could be replaced. Said in the report; nothing in this window creates files in the background.

## Results (2026-11-25)

- **Tests first**: the seven new unit tests of `dirops` were run on stubs and failed (all `7`), then green; the menu's unit tests did not compile on the old code and are green; the window tests (`11` in `tests/test_directories_menu.py`) passed at their first run, the mutants show that they bite.
- **The commit** `cc4c684`: `dirops.rs` (`rename`, `remove`, `describe`, `human_bytes`, `protected`), `dirmenu.rs` (the lines, the target's folder and entry, the questions), `confirm.rs` (`Act::RenameEntry`, `Act::DeleteEntry`, `Ask::detail`), the unit and window tests, the guide `1.25.0`, the plan and the report. D1 to D6 held as written; no limit was raised and no version was bumped.
- **Mutation checks**: `52` mutants, one edit at a time (the file restored and its checksum compared after every one): `35` of the unit-tested part (`dirops.rs`: the rules of the new name, trimming, the same name, a thing that is gone, a dangling link holding a name, a replacing move, a denied move, the way a folder, a file and a link are removed, how a link is described and counted, the count's bound and its words, the size units and their edges, and the six ways `protected` can go wrong; `dirmenu.rs`: which line is greyed for which reason and in which order, the order of the lines, the target's folder and entry) and `17` of the window part (Rename and Delete swapped, the old name missing from the line, Delete asked on the name line, the lost name after a refusal, the missing refresh after a rename and after a delete, the question left open, the rename aimed into the folder row, the delete aimed at nothing, what the question says, the link and folder words, a row taken for the empty space, nothing protected, the home folder unknown, the host's session rename taking the rename, Y doing nothing). Dangerous mutants (a delete aimed at the folder itself) were left out on purpose: the delete mutant aims at a path that does not exist. All `52` were killed in the end; three needed a second look: the denied-move mutant survived until a unit test of a folder that may not be written was added (as the denied-parent test of New folder, it is skipped for a superuser), one mutant did not compile and was replaced by one that does, and the unreachable branch of `protected` that no input could take was taken out of the code.
- **The commit hook**: nothing was bypassed or raised. The suite run by hand under the hook's `300 s` limit passed all `270` tests in `83` files but was refused by the suite's own guard because the build's peak memory (the test build after the source change) was `1894 MiB`, over the bound of `1536 MiB`; the bound stays, the run was repeated with the build cached (`270` tests passed in `133.8 s`, peak `956 MiB`, under the `180 s` and `1,536 MiB` limits) and the commit passed from the receipts ("nothing to run: 83 file(s) skipped by receipt").
- **Next**: `M24-P10c` (Compress and Extract) on its own branch.
