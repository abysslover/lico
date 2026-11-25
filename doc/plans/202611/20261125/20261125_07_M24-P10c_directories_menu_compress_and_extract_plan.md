# M24-P10c Plan — The Directories Menu: Extract And Compress

- Status: in_progress (2026-11-25)
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

1. [ ] **Tests first, run red** (unit and window).
2. [ ] **The change.**
3. [ ] **Mutation checks**, one edit at a time on `archive.rs` and `dirmenu.rs` (the mutants that could delete or write outside are written so that they cannot).
4. [ ] **Other window tests** that read the left column, the panel or the job events still pass.
5. [ ] **Words**: the guide.
6. [ ] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report.

## Not decided here

- Other formats (7z, rar) and a choice of the archive's name or format on Compress (a question before it runs) are later, if the owner asks.
- A size limit on what an archive may write is the listing's sum only; a lying listing is a limit of the tools (said in the report).
- The transcript view, the release 0.8.2 and level C come after.
