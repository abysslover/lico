# M22-P04a Plan — The Menus As Trees (No Function Removed) And New Session With Three Modes: Claude, Codex, Local LLM

- Status: done (`e69adb0`, 2026-11-18)
- Version: 1.1.0
- Branch: `phase/M22-P04a-tree-menus-three-modes` (stacked on `phase/M22-P04-toolbar-path-row`, `5966698`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18, after P04): no function needs to leave the menu. Starting a session offers **Claude, Codex, Local LLM** as the three modes, and choosing Local LLM shows the candidates naturally. Functions whose time of use the person cannot know are not removed now: they are classified and kept in the menu bar as a **tree**.

## What this phase changes (the window only; the host is not touched)

1. **The menus are trees.** The Session menu had 35 entries and the View menu 19, all in one list. Each entry that has no fixed time of use is classified into a **group**; a group is a heading that opens and closes in place (a click, Enter, Right to open, Left to close). The entries that a person uses all the time stay on the dropdown itself. **No entry is removed; no key changes** (every `Ctrl+...` key works with the menu closed, as before).

   | Menu | On the dropdown | Groups (each a heading) |
   |---|---|---|
   | File | New session, Accounts, Quit | none |
   | Session | New Home session, New browser tab, New local browser tab, Start screen | `Go to` (Next, Previous, Session 1-9), `This session` (Resume, Rebind account, Permission mode, Rename, Close, Delete), `Share` (Send to session, Hand over conversation), `Tools` (Find Local LLM servers, Virtual display, Stop real-screen control), `Approvals` (Allow, Deny), `Edit` (Copy, Cut, Paste) |
   | View | Working directory files, Left sidebar, Capture screen to input | `Find` (in conversation, all sessions), `Scroll` (Older, Newer, Newest), `Text size` (larger, smaller, reset), `Show` (Hidden files, Group sessions, Internal events), `Settings` (Agent clipboard write, Desktop notifications, the two native-login entries) |
   | Help | Keys, About Lico | none |

   A dropdown opens with every group closed; a heading shows `>` (closed) or `v` (open) and its entries are indented under it. The arrows move over the rows that can act (a heading always can), Enter on a heading opens or closes it, on an entry runs it; a greyed entry still says why, as before. The state (`menu.items`) names each row's `kind` (`group` or `entry`), `depth`, `open` and `group`, and `menu.groups` names every group with the labels it holds.
2. **New session shows the three modes at once.** The Provider row reads `[Claude]  Codex  Local LLM` (the chosen one in brackets; the wire still says `claude`, `codex`, `local`). Left, Right, Space or a click moves to the next mode.
3. **Choosing Local LLM shows its candidates.** Moving the Provider row to Local LLM makes the one look for model servers **on this machine** at once (`llm.discover`, read-only, as `Find Local LLM servers...` did), and the servers found are listed under the rows with the chosen one marked; a click on one chooses it. A result that is there is kept (moving away and back looks no more), the Server row (Enter, Space, a click) still looks again, and an ssh host stays the dialog's job (`Find Local LLM servers...`, which stays in the Session menu under `Tools`): it needs a host name that only the person can give.

## What stays as it is

The keys, the toolbar, the sidebar, the tab row, the host, the wire. The decision of M16-P07 that **opening** the form never looks for servers (a form that opens on Local, because the last session was local, offers the server used last and looks at nothing): the look is made when the person **chooses** Local LLM, which is what the owner asked for.

## Questions for the owner

None open. Defaults the lead took (the owner may change any): the names and members of the groups above; a dropdown always opens with its groups closed (a remembered open state would make the dropdown taller than the person expects); the heading marks are `>` and `v` (ASCII, no glyph to check).

## Steps (tests first; each step is green before the next)

1. [x] **Rust tests** (`menu_tests.rs` new, `newsession_tests.rs`): a dropdown opens with its groups closed and the entries that stay on it, no entry of any menu is lost when every group is open, a heading opens and closes in place, an entry in an open group runs and a greyed one says why, the selection starts on the first row that can act; the Provider row shows the three modes, choosing Local LLM looks at once and a result that is there is not asked for again, the candidates are listed and a click chooses one. (`local()` of the form tests is the form before the first look, as the Server-row tests need it.)
2. [x] **The tree** (`menu.rs`: `MenuRow`, `RowKind`, `group_of`, `menu_rows`, `menu_group_set`, `menu_row_run`, the keys; `toolbar.rs`: the rows with their marks and indent; `telemetry.rs`: the state; `ui.rs`: the click runs a row).
3. [x] **The three modes** (`newsession.rs`: the Provider row, the look when Local LLM is chosen; `newsession_local.rs`: `modes_line`, `candidate_lines`, `candidate_click`).
4. [x] **Window tests**: `tests/test_menu_tree.py` (new: the Session and View dropdowns open with closed groups, a click on a heading opens it, the keys Right, Left, Down, Enter; every label of the old menus is reachable; the New session form shows the three modes and, with a fake server on a fake process table, choosing Local LLM lists it with no Enter); the helpers of the older window tests (`tests/gui_helpers.py::menu_reveal`) and the tests that read a Session or View entry by its label follow the tree.
5. [x] `suite run --changed --receipts`, `cargo xtask check|verify|graph`; a manual-inspection page (the Session menu closed and with a group open, View, the three modes, the candidates); [x] commit `e69adb0` with the guard on (the owner approved the prefix `phase/M22-P04a-` in the chat; the pre-commit suite found every receipt current: 2 tests ran).

## A click in a menu keeps the prompt selection (found by the tests, fixed in this phase)

`Copy` and `Cut` act on the text selected in the prompt, but a mouse press anywhere while a menu was open (and any press on a menu title or entry) cleared that selection first, so reaching `Copy` by the mouse found "nothing is selected" (only the keyboard path, Alt+S and Enter, worked). With the groups this became visible at once: a click on the heading `Edit` cleared the selection and `Copy selected text` turned into `Copy selected lines`, greyed (the old test `test_editor_selection.py` went red for this reason). A press on a menu title, a heading or an entry now keeps the selection; a press anywhere else still clears it. The test clicks the entry and checks the clipboard.

## Decisions made while doing it (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The tree is **one level** (a heading and its entries), a dropdown opens with every group closed, and the open state is kept in `MenuGeom` (the window state's size limit for `ui.rs`) | a deeper tree makes the person open more to reach less; a remembered open state makes a dropdown taller than expected |
| D2 | The group an entry belongs to is a function of its command (`group_of`), not a field of each entry; `menu_entries` is still the flat list of every entry, so `menu_run(menu, index)` and the unit tests on entries did not change | one table to read and to change; no entry can be forgotten |
| D3 | Choosing Local LLM looks at this machine at once; **opening** the form does not (M16-P07 stays for that) and neither does `+ New chat`'s Options (the server is chosen there) | what the owner asked for is "choose Local LLM, the candidates come"; opening a form must still cost nothing |
| D4 | The candidates are lines under the rows and a click on one chooses it; the Server row still cycles them and still looks again | the candidates are in sight without a key; nothing the Server row did was taken away |
| D5 | An ssh host is not looked at by itself; `Find Local LLM servers...` (Session > Tools) stays for it | a host name only the person can give |

## Results (2026-11-18)

- Unit tests: `cargo test --bin lico gui::` 267 pass; new: five in `menu_tests.rs` (closed on opening, nothing lost with every group open, a heading opens and closes in place, an entry in an open group runs and a greyed one says why, the selection starts on the first row that can act), four in `newsession_tests.rs` (the three modes, the look on choosing Local LLM and not again, the candidates and the click, the address said once on a line); three older ones follow the Provider row's words. Mutation checks (each fails its test, each restored, files identical): a group open by default; a heading that never closes; the group heading repeated for each entry; Local LLM chosen without the look; the look made again for a result that is there; the click on a candidate off by one line.
- Window tests: `tests/test_menu_tree.py` (3: the dropdowns open closed, a heading opens in place, nothing is lost across Session and View; the keys Right, Left, Down and Enter on headings and entries; New session shows three modes and choosing Local LLM looks at once, with a fake server on a fake process table, while opening the form and choosing Codex look at nothing). The older window tests that read a Session or View entry by its label follow the tree through `tests/gui_helpers.py` (`reveal`, `menu_reveal`, `client_reveal`): 17 files.
- The changed-files suite: 244 tests in 71 files, all pass (4 skipped by receipt), peak 1877 MiB. Runs before it: 3 red tests that read an entry in a closed group (fixed in the tests), one red for the selection (fixed in the window, above), and one `test_first_view.py` timing flake under four workers (passes alone, as in P04).
- Gates: `cargo xtask check` and `verify` pass; `graph` one cycle as before.
- Manual inspection: `private/manual_inspection/202611/20261118/00003_M22-P04a_tree_menus_three_modes/` (the Session menu closed and with groups open, View, the three modes, Codex, Local LLM with its two candidates).
- What the person may notice: the Session menu is 10 rows instead of 35 and View 8 instead of 19, each heading opens in place; New session says `[Claude]  Codex  Local LLM`, and choosing Local LLM lists the servers found on this machine without pressing Enter; a click in a menu no longer drops the selected prompt text (`Copy` and `Cut` by the mouse work).

## What this phase does not do

The browser tab (P05), the terminal (M23), the release (P06). It removes no menu entry and no key, and it does not look at an ssh host by itself.
