# M24-P10c Plan — The Directories Menu: Extract And Compress

- Status: done (2026-11-25), code `5de5c7d`; not pushed (published with the release 0.8.2)
- Version: 1.0.0
- Branch: `phase/M24-P10c-directories-menu-compress-and-extract` from `phase/M24-P10b-directories-menu-rename-and-delete` (`b82fe30`; P07 to P10b are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-25): "…이름 변경, 지우기, 압축풀기, 압축하기, 터미널 열기 … 의 기능을 담으면 될 것 같고" (see the [P10a plan](20261125_05_M24-P10a_directories_menu_plan.md)). The owner's standing yes (2026-11-25): the guard prefix `phase/M24-P10c-` is added without a question (done together with P10b's).
- No version bump (the release 0.8.2 carries it).

## What this phase does (decisions D1 to D7)

The menu gets its last two lines, in the owner's order: **Extract**, **Compress** (so the menu is Open terminal, New folder, Rename, Delete, Extract, Compress).

| # | Decision | Detail |
|---|---|---|
| D1 | **Extract** works on a **file row** whose name ends in `.zip`, `.tar`, `.tar.gz`, `.tgz`, `.tar.bz2`, `.tbz2`, `.tar.xz` or `.txz` (capitals too); on anything else it is greyed `not an archive` | one click for the thing most people have |
| D2 | Extract makes a **new folder beside the archive**, named like it without its ending (`photos.zip` makes `photos`); when that name is taken it takes the first free `photos 2`, `photos 3`; **nothing is replaced and nothing is written outside the new folder** | no surprise in the folder the person is looking at |
| D3 | **Safe**: the members are listed first and the archive is refused, with the reason and nothing written, when one has an absolute name, a `..` part, is a link (symbolic or hard) or a device, when there are more than `100,000` members, when a name is not text the window can read (a line break in a name), or when the sizes add up to more than `8 GiB`; a failure in the middle removes the folder that was made | an archive is somebody else's file |
| D4 | **Compress** works on a **row** (a folder, a file or a link) and makes `name.zip` beside it (`zip` stores a link as a link and never follows it); a taken name takes the first free `name 2.zip`; nothing is replaced; on the empty space it is greyed `point at a file or folder`; when `zip` or `unzip`/`tar` is not installed the line is greyed `zip is not installed` and so on | the friendliest format for a person who sends the file on |
| D5 | Both run **off the window's thread** (the window stays usable); a flash says `Compressing name...` and then `Compressed: name.zip` or `Not compressed: why`; the list refreshes at the end; **one archive at a time** (a second line is greyed `an archive is being made`); a run longer than **30 minutes** is stopped and what it made is removed | a big folder must not freeze the window |
| D6 | The tools are run as the system's own commands with a list of arguments (**no shell**), a cleared environment (a short `PATH`, `LANG`), `--` before every name, in the folder of the row; their error text is read up to 4 KiB and said in one line | nothing in a file name can become a command |
| D7 | The order of the reasons on a line: machine, folder (as the other lines), then `an archive is being made`, then for Extract `point at a file or folder` / `not an archive`, for Compress `point at a file or folder` and the missing tool; these two lines do not ask a question, so an open screen does not grey them | same rules as P10a and P10b |

## The pieces

1. `archive.rs` (leaf module, std only): `kind_of(name)` (the ending), `stem_of(name)`, `free_name(parent, wanted, ending)`, the pure checks of a listing (`member_problem` over the names and the type letters, `sizes_total`), `tool_missing(kind)`, and the two runs `compress(parent, name)` and `extract(archive)` that call the tools; unit tests over real archives made by the tools in a temporary tree (a zip and a `.tar.gz` round trip, a taken name, an archive with `..`, with an absolute name, with a link, a refusal that leaves nothing behind) and over listings as plain text.
2. `dirmenu.rs`: two more `Kind`s and the lines' reasons; the job (`AsyncJob::Archive`) started on a worker thread with the result sent back to the window as an event; the flash and the refresh at the end; `Ctx` gets `busy`, `archive` (the row is an archive) and the tools' presence.
3. Window tests (`tests/test_directories_menu.py`): Compress a folder and a file and read the archive back with `zipfile`; Extract a `.zip` and a `.tar.gz` and compare the files; the greyed reasons (`not an archive`, the empty space); an archive made with a `..` member and with a link is refused and nothing appears in the folder; a taken name takes `2`; a second archive while one runs is greyed.

## Steps

1. [x] **Tests first, run red** (unit and window).
2. [x] **The change.**
3. [x] **Mutation checks**, one edit at a time on `archive.rs` and `dirmenu.rs` (the mutants that could delete or write outside are written so that they cannot).
4. [x] **Other window tests** that read the left column, the panel or the job events still pass.
5. [x] **Words**: the guide.
6. [x] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report.

## Not decided here

- Other formats (7z, rar) and a choice of the archive's name or format on Compress (a question before it runs) are later, if the owner asks.
- A size limit on what an archive may write is the listing's sum only; a lying listing is a limit of the tools (said in the report).
- The transcript view, the release 0.8.2 and level C come after.

## Results (2026-11-25)

- **Tests first**: the archive unit tests that need the tools were run on stubs and failed, then green (`26` in `archive_tests.rs`); the menu's unit tests did not compile on the old code and are green (`10`); the window tests (`15` in `tests/test_directories_menu.py`, `4` new) passed once the code was done, the mutants show that they bite.
- **The commit** `5de5c7d`: `archive.rs` (a leaf module: the endings, the free name, the vetting of a listing, the tools run without a shell, `compress_within`, `extract_within`), `dirmenu.rs` (the two lines, the job, the flash), `jobs.rs`, `panel.rs`, `sidebar.rs`, `mod.rs`, the unit and window tests, the guide `1.26.0`, the plan and the report. D1 to D7 held as written, with one widening (D1): a link to an archive file extracts too, because the list shows it as a file; no limit was raised and no version was bumped.
- **Mutation checks**: `81` mutants, one edit at a time (the file restored and its checksum compared after every one): `68` of the unit-tested part (`archive.rs`: the endings and their case, a bare ending, the length of an ending, the numbering of a free name and a dangling link holding a name; every refusal of the vetting at its edge: the count, a link, a hard link, a device, an empty name, a name at the root, a `..` part (and only a whole part), a control character, the total at its edge and its overflow, an unknown kind; the strict reading of the two listings: the footer, the count of items, the first line, the kind letter, an empty archive, the escapes and the octal digits of a tar name, the numeric owners, the date and time fields, the size of a device; the search of a tool, the language, the run: the environment kept, the folder, the time limit, the cut of the error text, a long answer, a failure taken for success, the words of a failure; how Compress puts the file in place, leaves its work folder, follows links, skips the recursion, replaces a taken name, forgets a gone row or lets a name pass as an option; Extract with a relative path, a folder taken for an archive, a dangling link, the vetting skipped, the new folder left behind after a failure, the owners by name; `dirmenu.rs`: which line is greyed for which reason and in which order) and `13` of the window part (Extract run as Compress, the busy flag never set and never cleared, the list not refreshed, the archive aimed at the wrong folder, a folder named like an archive taken for one, the made name as a whole path, the words of success and of failure swapped, Extract given no folder, Compress given no time, the reason of a failure lost, the start not said). Mutants that could delete or write outside were written so that they cannot (a bad name for Compress runs with a limit of two seconds and an exact error). `4` were not killed at the first run: one did not compile and was rewritten, and three showed gaps in the tests (a listing without its footer, a first line that is not the title, the start notice `Compressing name...` that nothing looked at); tests were added and the three are killed. **`80` of `81` are killed.** The one left, taking out the explicit `--quoting-style=escape` of the tar listing, is **equivalent**: GNU tar's own default for a listing is the same (measured with tar 1.35: a name with a line break is listed as `new\nline` without the flag), the flag only pins it. The mapping of a missing tool to its greyed line cannot be mutated in the window, where the tools are installed.
- **The commit hook**: nothing was bypassed or raised. The suite run by hand under the hook's `300 s` limit passed all `274` tests in `83` files but was refused by the suite's own guard because the test build's peak memory (the build after the source change) was `1905 MiB`, over the bound of `1536 MiB`, as in P10b; the bound stays, the run was repeated with the build cached (`274` tests passed in `137.1 s`, peak `941.7 MiB`, under the `180 s` and `1,536 MiB` limits) and the commit passed from the receipts ("nothing to run: 83 file(s) skipped by receipt").
- **Next**: the transcript view, the release `0.8.2`, then level C.
