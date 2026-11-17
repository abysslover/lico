# M22-P04 Plan — The Top And The Bottom Of The Window: A Toolbar Of Icon Buttons, No Buttons In The Tab Row, The Path Row Above The Hint Row And Never Empty

- Status: done (`8f9cfb7`, 2026-11-18)
- Version: 1.3.0
- Branch: `phase/M22-P04-toolbar-path-row` (stacked on `phase/M22-P03-two-tabs-home`, `7c5f1b3`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18): the full directory path is still not seen, it belongs at the bottom just above the `Ctrl+N` line; the menu bar simpler; under the menu bar a toolbar that controls almost everything, only what is really needed is a button and the rest is in the menus: New session (the plain icon), New Home session (a house), Open browser (the browser button), Open terminal (a terminal icon, with M23).

## What this phase changes (the window only)

1. **The toolbar is three icon buttons** (the fourth, the terminal, comes with M23 so that no button is shown that does nothing): `new_session` (a plus), `new_home` (a house), `new_browser` (a browser window). They are drawn by the window itself (rectangles and rows of pixels, no font glyph: a glyph needs a font check), square, with the word shown beside the buttons while the pointer is on one (and in the window state, `label`, and in the menu entry). What the nine text buttons did stays reachable from the menus, which already hold every one of them (`Accounts` in File; `Close`, `Rename`, `Mode`, `Send to session`, `Copy` in Session; `Files`, `Find`, `Group` in View). With the Files panel open the panel's own buttons stay after the three icons (`Host`, `+ New here`, `Refresh`, `Hidden`, `< Prev`, `Next >`, and `Copy` when a selection exists): they act on what the panel shows, so they are not the window's.
2. **The tab row holds tabs only.** `+`, `+B`, `+BL` and `+G` are gone from it (the icons above do their work). `+BL` (a browser tab that keeps no history) is the menu entry `New local browser tab`; `+B` is also `New browser tab` in the menu. The empty row says `No open tabs`.
3. **The window never says Global.** `New Global session` is `New Home session` (key `Ctrl+Shift+N`, unchanged); the start screen line, the New session form's helper line, the flash text, the telemetry bar's `Global (general)` and the New chat gate's message say Home; a tab caption of a Home session has the prefix `H` (was `G`). The wire keeps `global`; the state keys keep their names.
4. **The bottom is: the telemetry strip, the PATH row, then the HINT row last.** (Today: telemetry, hint, path.) The two rows swap; nothing else of the footer moves, so no list, panel or modal changes its bottom edge.
5. **The path row is never empty.** The pointer first, then the selection (the Files panel, the sidebar's folder), then the active session's folder, else `lico://home` (the state says `source: "home"`): in Home, in a Home session and with no session at all the row says where the person is.
6. **The menus keep every entry** (the owner asked for a simpler menu bar; the lead takes out only what the new toolbar made redundant, which is nothing in the menus, and asks the owner which rarely used entries to remove, see the questions below). New: `New browser tab`, `New local browser tab`; renamed: `New Home session`.
7. **The left column runs from the top of the window** (the owner, after the commit of the first six, 2026-11-18: the main functions on the left, Directories and Chats, reach the very top, and the menu must not invade that area). The sidebar's two tabs are the first thing at the top; the menu bar, the toolbar (and its dropdowns and the Files host popup) and the real-screen banner are drawn to the right of the column only. The tab row of the sessions stays where it was, to the right.

## What stays as it is

The key `Ctrl+N`, `Ctrl+Shift+N`, `Ctrl+A` and every other key. The Files panel and its buttons. The session tabs, the room tabs and the browser tabs. The telemetry strip's contents. The browser tab's own bar and the help text are P05's.

## Questions for the owner (answered in the chat, 2026-11-18)

- Which menu entries go? **None.** The owner: the functions themselves need not leave the menu. Starting a session offers Claude, Codex and Local LLM as the three modes and choosing Local LLM shows the candidates (so `Find Local LLM servers...` stops being a menu chore); functions whose time of use the person cannot know are classified and kept in the menu bar as a tree (submenus). That is phase **P04a** of the milestone; this phase removes nothing from the menus.
- The terminal button: M23.

## Steps (tests first; each step is green before the next; the window tests run under a private Xvfb with synthetic homes)

1. [x] **Red tests.** `tests/test_toolbar_icons_and_bottom_rows.py` (new, in `tests/testmap.toml` with `gui`): the toolbar buttons are `new_session`, `new_home`, `new_browser` and nothing else (no panel open), square, each with an `icon` name; a click opens the New session form, the New session form for Home, and the browser tab's prompt; the tab row has no `new_*` rect; the Session menu says `New Home session`, `New browser tab` and `New local browser tab`, and no menu label, the hint line, the empty tab-row text or the telemetry `cwd_text` of a Home session says Global; a Home session's tab caption starts with `H`; the path row in Home says `lico://home` (`source` `home`) and never `none`; the hint row is the last row and the path row is above it (the telemetry strip above that). Changed first: `test_path_status_bar.py`, `test_m14_end_to_end.py` (the order of the rows), `test_browser_tab.py` and `probe_browser_window.py` (the buttons of the tab row become the toolbar and the menu), `test_session_tabs_and_telemetry.py` (the `+G` button and the word), `test_toolbar_and_nav.py`, `test_session_comms_and_ux_fixes.py`, `test_peer_reply_copy_and_dir_nav.py`, `test_quota_display.py`, `test_workspace_files.py` and the other tests that click a text button that left the toolbar (they use the menu or the key); the pixel tests of the bottom rows (`test_navigation.py`, `test_frame_cost.py`).
2. [x] **The path row** (`pathbar.rs`: `Source::Home`, `pick` with the fallback; `ui.rs`: the two rows swap; the telemetry word).
3. [x] **The icons and the toolbar** (`icons.rs` new: plus, house, browser drawn into a rect; `toolbar.rs`: the entries, the drawing, `toolbar_json` with `icon`; `menu.rs`: the two commands and the new label).
4. [x] **The tab row** (`tabs.rs`: the right-anchored buttons out, the empty text; the caption prefix) and the words (`telemetry.rs`, `newsession.rs`, `newchat.rs`).
5. [x] `suite run --changed --receipts`, `cargo xtask check|verify|graph`; a manual-inspection page (the top and the bottom of the window); [x] commit `8f9cfb7` with the guard on (the owner approved the prefix `phase/M22-P04-` in the chat; the first attempt was stopped by the guard, which reads whole staged files: a unit-test fixture in `pathbar.rs` whose folder name began with the guard's word for a user's home directory, now `/work/u/...`; the pre-commit suite then ran 4 tests in 2 files, all pass).

## Decisions made while doing it (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The path row says the sidebar's folder when the sidebar is on a folder, and `lico://home` only when nothing else says a place (the sidebar on Home, no pointer, no Files panel) | the row was always "the pointer, then the selection, then the session"; a Home session does not hide the folder the person is looking at |
| D2 | The icons are drawn by the window from rectangles (`icons.rs`), 19 px squares at scale 1; the word of the button under the pointer is shown beside the buttons | no font glyph to check; an icon alone says nothing to a new person, the word on hover does |
| D3 | With the Files panel open the panel's own buttons stay after the three icons; `Group`, `Accounts`, `Files`, `Close`, `Rename`, `Mode`, `Find`, `Send`, `Copy` left the toolbar (`Copy` comes back while lines are selected) | the menus hold every one of them; the Files buttons act on what the panel shows |
| D4 | `tests/probe_*.py` is declared inert in `tests/testmap.toml` | a probe that is run by hand with the real engine was an undeclared file: any change to it selected the whole suite |
| D5 | `icons.rs` is a submodule of `ui.rs` (`#[path]`, as `toolbar.rs`, `tabs.rs`, `telemetry.rs` are) and `ui.rs`'s allowance in the size ratchet is lowered to its new length (1117) | `gui/mod.rs` was at the 800-line limit |

## Results (2026-11-18)

- Unit tests: `cargo test --bin lico gui::` 258 pass (four new in `icons.rs`: every icon inks pixels only inside its button, the three are three pictures, the plus is a centred cross and the house is centred with a door of the button's colour, a too-small rectangle gets none; `pathbar.rs`: the fallback is Home and an empty text is no text; `ui_tests.rs`: the toolbar is the three icons, `Send to session` is the Session menu's entry with its reason). Mutation checks: the plus without its vertical bar, the house without its door and `pick` falling back to an empty text each fail their test; restored (files identical).
- Window tests: `tests/test_toolbar_icons_and_bottom_rows.py` (2) pass; the changed-files suite selected 252 tests in 73 files, 250 passed on the first run; the two that failed were fixed (the converted Send-to test asserted the entry enabled before anything was selected, it is greyed with its reason; `gui/mod.rs` was one line over the size limit) and the rerun passes.
- Gates: `cargo xtask check` and `verify` pass; `graph` 204 modules, 71 nodes (one cycle, as before).
- Manual inspection: `private/manual_inspection/202611/20261118/00002_M22-P04_toolbar_icons_path_row_hint_last/` (the toolbar, the hover word, the Home form, a Home session with the path row `lico://home`, the Session menu).
- What the person may notice: the toolbar is three small icons; the tab row has no buttons; a Home tab starts with `H` (was `G`); the hint row is the last line and the full folder is the line above it.

## The left column from the top (item 7)

- Steps: [x] red test (`tests/test_toolbar_icons_and_bottom_rows.py`, third test: the sidebar's tabs start at the top, the menu titles, the toolbar buttons, the window button and every dropdown start right of the column, a click on the sidebar's top tab reaches the sidebar); [x] `ui.rs` (the column's top by `left_column_tops` in `sidebar.rs`), `sidebar_render.rs` (the header is its own height, no longer the tab row's), `toolbar.rs` (`draw_top_bars` takes the column's width; the bars, the buttons and the dropdowns start there), `realbar.rs` (the banner starts there); [x] two older tests followed: `test_frame_cost.py` clicks the first menu title at the column's width + 20 (it clicked x 50), `corner_is_background` of the window helper says what the corner holds now (the sidebar's tabs or the background, never a solid marker square); [x] `ui.rs` is one line shorter than before (1116, the allowance is lowered).
- Results: unit test `the_left_column_starts_at_the_top...` (mutation: the column starting at `top` fails it; restored); the changed-files suite: 243 tests in 79 files, 242 pass; the one that failed (`test_first_view.py`, a click on a list row, 23 s under four workers at load) passes alone in 8.9 s and passed in the run before it, a timing flake that this change does not touch.
- Not done: the realbar banner's own window test does not exist; the banner is drawn from the column's edge by the same code as the bars.

## What this phase does not do

The browser tab (P05), the terminal (M23), the host (not touched), the release (P06). It removes no menu entry and no key.
