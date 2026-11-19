# M22 — A Simpler Window: Two Left Tabs, Every Past Session In Chats, Four Buttons, A Browser That Feels Like One

- Status: implemented (2026-11-19), release `0.8.0` built (`c587de6`), not pushed, inspection by the owner pending; before that in_progress (2026-11-18); the owner answered the open questions in the chat the same day (section "The owner's answers"); P01 and P02 are done, P03 is done (`7361c18`), P04 is done (`8f9cfb7`), P04a (the menu as a tree, the three modes) is done (`e69adb0`), P04b (the account icon, the new-session wizard of three icons) is done (`192e304`), P04c (the Chats cards) is done (`2223fe5`), P05a (the browser tab's address bar at the top, the empty first page, the click ring removed) is done (`8b30b19`), P05b (words typed in the bar are searched with the setting `browser_search`, Google by default) is done (`68ca2c3`), P05c (the pages the browser remembers, listed in Chats, reopened by a click, `Reopen closed tab`) is done (`69b16c6`), and P06 (the guide, the README, the CHANGELOG, the report and the release `0.8.0`) is done (`c587de6`)
- Version: 3.5.0
- Project: Lico
- Updated: 2026-11-19
- Base: `main` at `103d17e` (M20 on `main`; M21, our own build of Moli, is still an idea and keeps its number)
- Version of the program: `0.8.0` at the end (a minor step: the window loses a concept)

## Goal

"Simple is best." Every feature added so far made the window harder to read. This milestone takes things away until a person can find a past session, its folder and a page in two clicks, and does not add a feature the person did not ask for. **Nothing may make the window wait**: what a list shows is ready before it is asked for.

## The owner's words (the source, in the chat, 2026-11-18)

1. The click ring is not needed: with Moli the text cursor is visible.
2. Remove the concept called Global. Think of it as ONE virtual working directory, shown with a virtual path.
3. The Chats tab shows every past Lico session, newest first. A click goes at once to that session's working directory.
4. The left side has two tabs only: Directories and Chats. Calls are not shown to the person; they are seen only between the agents that talk to each other.
5. The full directory path is still not seen. It belongs at the very bottom, just above the `Ctrl+N` line.
6. The menu bar simpler.
7. At the start the window is in Global and shows the earlier sessions and browsers (the owner's observation; it stays).
8. In the browser tab the URL is shown very long. It belongs at the TOP, cut when too long, and the person can go to another address from it.
9. The first "open a browser" should look like a browser, not an explanation text.
10. These are small experiments. **The next experiment is a terminal** (M23): run an agent in a terminal and have it come into Lico.

## The owner's answers (the chat, 2026-11-18, after the plan)

- **The virtual directory is `Home`, its path is `lico://home`** (not `Anon`).
- **The top is the menu bar and, under it, a toolbar that controls almost everything.** Only what is really needed is a button; the rest is in the menus. The buttons: **New session** (the plain icon), **New Home session** (a house icon), **Open browser** (the browser button), **Open terminal** (a terminal icon). What they open appears in the chat area.
- **The browser's address bar takes a search too, for now**: words that are not an address are searched.
- **A browser page opened before can be opened again.**
- **Chats also lists what was opened outside Lico, with Claude or Codex themselves; this is on by default** (the setting that includes the provider's own home is ON unless the person turns it off).
- **The menu is not cut, it is a tree** (the chat, after P04, asked which entries to remove): no function is removed. Starting a session offers the three modes **Claude, Codex, Local LLM** by default, and choosing Local LLM shows the candidates (the servers found) there, so `Find Local LLM servers...` is no longer a menu chore; functions the person may not know when they will use are **classified and kept in the menu bar as a tree** (submenus), not deleted. This is phase P04a.
- **The account icon and the wizard** (the chat, 2026-11-18, after P04a was installed and opened): a build must reach `lico gui` by itself (the lead made `~/bin/lico` a link to the build); pressing New session must not draw text but just **three icons, Claude, Codex, Local LLM**; **an account icon first in the toolbar, then the plus**; Local LLM needs no account, so only the **Claude and Codex icons are grey** without an account of theirs; **pressing an icon opens a wizard that goes on step by step**. The browser tab is still text (it is P05, next).
- **Performance** (a message sent while the lead worked): listing everything may need performance work and a background index. Experiment, think it through, and leave no delay.

## What the code does today (read 2026-11-18)

- **Left side:** three tabs (`sidebar_tabs.rs`: Directories, Chats, Calls). Directories has a `Global (general)` row above the path row; the active scope is `global_scope`, kept in the history and bookmarks (`sidebar_nav.rs`). Chats lists the chats of ONE scope; a click resumes the chat but does not move Directories to its folder.
- **Global on the wire:** `session.chats` `cwd` or `global`, `session.create` scope `global`, CLI `--global`, REST `global=`, the tool `create` `global`; the tab bar has `+G`, the menu `New Global session`.
- **Calls:** a sidebar tab (`sidebar_calls.rs`); a room is a tab beside the sessions. The operations (`call.*`, REST `/v1/calls`, the agents' tools) are the agents' own.
- **The path:** the path bar (M14) IS the last row (path last, the hint line above it), and EMPTY in Global, where the owner starts.
- **The top:** two rows (`File Session View Help`, then a toolbar of nine buttons that changes with the panel), and a third row of session tabs with `+ +B +BL +G`.
- **Browser tab:** the address is the input line at the BOTTOM (the whole URL); the header says where typing goes; a new tab shows about 14 lines of help; the click ring is `browser_cues.rs` and `lico-browser-view` (BROW-09).
- **Chats cost (measured 2026-11-18 on this machine's own homes, timing only):** the first call 147 ms, a repeat 19 ms, the second page 79 ms (the turn counts read whole transcripts). A real defect showed: a home with many other files (Codex keeps 2,415 files for 50 chats) fills the listing, so only 153 of the chats were found and `truncated` was true. Only `.jsonl` may count.

## Decisions (the lead's, to be confirmed)

- **The window never says Global.** The wire keeps `global` (a CLI, REST or tool contract names it); a chat of the host's workspace root is a Home chat, its path `lico://home`.
- **Chats is one list of everything opened before, newest first:** sessions (Lico's own and the provider's own home), later also browser pages (a small mark tells the kind). A click on a session opens it and moves Directories to its folder (or to Home); a click on a page opens it in a browser tab.
- **A chat index in the host, not a scan per call.** A snapshot of every record of every home (path, size, modification time, folder, title, turn count) is built when the host starts, kept fresh by comparing sizes and times (no record is opened again unless it changed), and read by every scope of `session.chats`. A call reads memory. The lead measures first (cold disk, warm, one changed record, 5,000 records), and the budget is: a page in under 5 ms warm, the first build under 1 s in the background, no call waits for a build. Phase P02.
- **Calls leave the sidebar, not Lico.** A room two sessions share shows as a small `⇄` mark on the tab of each of the two sessions (decided in P03: a session with no transcript has no card, a tab always exists while the session is open); a click opens the room. The agents' tools and the REST and CLI calls do not change.
- **The bottom is: path row, then the key-hint row.** The path row always has text: the folder, `lico://home` in Home, the row under the pointer when there is one.
- **The top is the menu bar and a toolbar of icon buttons**: the account (a person), New session (a plus), New Home session (a house), Open browser (a browser window); the terminal's appears with M23, so that no button is shown that does nothing. The session tabs row loses `+ +B +BL +G`. Every other action stays in the menus.
- **New session is a wizard of three steps** (P04b): the three icons (Claude and Codex grey without an account of theirs, Local LLM never), then the account and the folder (Local LLM: the server), then the options and the start; Esc goes a step back. The one-page form stays as File > New session options... and for the folder actions of the sidebar.
- **The browser tab has an address bar at the top** (`‹ ›  ⟳  address`): cut in the MIDDLE when long (the host name and the end stay), the whole address when the bar has the keys, Enter goes. Text that is not an address is a search with the engine of the setting `browser_search` (a URL with `%s`); the default engine is chosen by one measured probe in P05 (three requests, to find one the engine can use). A new tab is an EMPTY page with the bar focused; the explanation is on `F1`. The click ring goes (BROW-09 is removed in `lico-browser-view`).
- **Browser pages are remembered** (address, title, time; at most 100, in the host's state directory, mode 0600, never for a tab opened `--local`, cleared by a menu entry), listed in Chats and reopened by a click; "Reopen closed tab" is in the menu.
- **The provider's own home is included by default.** The setting `native_claude` / `native_codex` has `ask` today (the New chat dialog asks); the default becomes `on`. A person who chose `off` keeps `off`.

## Phases (each: a plan, a branch of its own, tests first, green-only commits)

| Phase | Subject | Plan and state | Status |
|---|---|---|---|
| M22-P01 | host: `session.chats` scope `all` (every past session, newest first, one list), CLI and REST | [Plan](../../../plans/202611/20261118/20261118_03_M22-P01_chats_of_all_sessions_plan.md) | done (`0900c95`) |
| M22-P02 | host: the chat index (background, incremental, every scope reads it), `.jsonl` only, the provider's own home on by default, the performance experiments | [Plan](../../../plans/202611/20261118/20261118_04_M22-P02_chat_index_plan.md) | done (`4cb06c9`) |
| M22-P03 | window: two left tabs, Home as one virtual directory, Chats of all sessions with the jump, Calls out of the sidebar | [Plan](../../../plans/202611/20261118/20261118_05_M22-P03_window_two_tabs_home_chats_plan.md) | done (`7361c18`) |
| M22-P04 | window: the toolbar of icon buttons (three now, the terminal's comes with M23), the path row above the hint row and never empty, the tab row without buttons, Global gone from every word | [Plan](../../../plans/202611/20261118/20261118_06_M22-P04_window_toolbar_icons_bottom_rows_plan.md) | done (`8f9cfb7`) |
| M22-P04a | window: the menu bar as a tree (classified submenus, no function removed) and the New session screen with the three modes Claude, Codex, Local LLM (Local LLM shows the servers found) | [Plan](../../../plans/202611/20261118/20261118_07_M22-P04a_tree_menus_three_modes_plan.md) | done (`e69adb0`) |
| M22-P04b | window: the account icon first in the toolbar, New session as a wizard that starts with three icons (Claude, Codex, Local LLM; Claude and Codex grey without an account, Local LLM never), the build reaches `lico gui` by a link | [Plan](../../../plans/202611/20261118/20261118_08_M22-P04b_account_icon_new_session_wizard_plan.md) | done (`192e304`) |
| M22-P04c | window: a Chats card with the provider as a picture, the title (two lines when it needs them), the time of the last access in a colour that follows its age, and the state as a picture (disc green, orange or red by memory, ring for a closed tab, square for an ended one); no letters, no words of state; the hint row says the whole card | [Plan](../../../plans/202611/20261118/20261118_09_M22-P04c_chats_rows_plan.md) | done (`2223fe5`) |
| M22-P05a | browser tab: the address bar at the top (back, forward, reload and the address, cut in the middle), an empty first page with the bar focused, the click ring removed; `lico-browser-view` 0.8.0 | [plan](../../plans/202611/20261119/20261119_04_M22-P05a_address_bar_plan.md) | done (`8b30b19`) |
| M22-P05b | browser tab: text that is not an address is searched with the engine of the setting `browser_search`; the default engine (Google) by one measured probe; `lico-browser-view` 0.9.0 | [plan](../../plans/202611/20261119/20261119_05_M22-P05b_search_plan.md) | done (`68ca2c3`) |
| M22-P05c | browser tab: remembered pages (address, title, time; at most 100; mode 0600; never for a local tab), listed in Chats, reopened by a click, `Reopen closed tab` in the menu, cleared by a menu entry | [plan](../../plans/202611/20261119/20261119_06_M22-P05c_remembered_pages_plan.md) | done (`69b16c6`) |
| M22-P06 | specification and guide, README, CHANGELOG, the suite, `cargo xtask system`, release `0.8.0` | [plan](../../plans/202611/20261119/20261119_07_M22-P06_release_plan.md), [report](../../reports/202611/20261119/20261119_01_M22-P06_simple_window_browser_bar_and_0_8_0_report.md) | done (`c587de6`) |

## What this milestone does not do

- It does not add a feature beyond what the owner asked for in the chat.
- It does not rename or remove an operation on the wire, in the CLI, in REST or in the agents' tools.
- It does not delete a past session or a call room; only what the window shows changes.
- It does not push, open a pull request or merge; each needs the owner's explicit yes after a dry run.
- It does not start the terminal ([M23](20261118_02_M23_terminal_agents_milestone.md)); the terminal's button comes with it.
