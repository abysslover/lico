# M22 — A Simpler Window: Two Left Tabs, Every Past Session In Chats, A Browser That Feels Like One

- Status: proposed (2026-11-18); the owner's words are the source, the decisions below are the lead's and wait for the owner's yes or change in the chat
- Version: 1.0.0
- Project: Lico
- Updated: 2026-11-18
- Base: `main` at `103d17e` (M20 on `main`; M21, our own build of Moli, is still an idea and keeps its number)
- Version of the program: `0.8.0` at the end (a minor step: the window loses a concept)

## Goal

"Simple is best." Every feature added so far made the window harder to read. This milestone takes things away until a person can find a past session, its folder and a page in two clicks, and does not add a feature.

## The owner's words (the source, in the chat, 2026-11-18)

1. The click ring is not needed: with Moli the text cursor is visible.
2. Remove the concept called Global. Think of it as ONE virtual working directory (shown with a virtual path), for quick throwaway work ("Anon").
3. The Chats tab shows every past Lico session, newest first. A click goes at once to that session's working directory.
4. The left side has two tabs only: Directories and Chats. Calls are not shown to the person; they are seen only between the agents that talk to each other.
5. The full directory path is still not seen. It belongs at the very bottom, just above the `Ctrl+N` line.
6. The menu bar in one line.
7. At the start the window is in Global and shows the earlier sessions and browsers (the owner's observation; it stays).
8. In the browser tab the URL is shown very long. It belongs at the TOP, cut when too long, and the person can go to another address from it.
9. The first "open a browser" should look like a browser, not an explanation text.
10. These are small experiments. **The next experiment is a terminal** (M23): run an agent in a terminal and have it come into Lico.

## What the code does today (read 2026-11-18)

- **Left side:** three tabs (`sidebar_tabs.rs`: Directories, Chats, Calls). Directories has a `Global (general)` row above the path row; the active scope is `global_scope`, kept in the history and bookmarks (`sidebar_nav.rs`, a `global` field on each entry). Chats lists the chats of ONE scope, the folder shown or Global (`sidebar_chats.rs`, `session.chats` with `cwd` or `global`); a click resumes the chat but does not move the Directories tab to the chat's folder.
- **Global on the wire:** `session.chats` takes `cwd` or `global` (the host's workspace root), `session.create` takes the scope `global`, the CLI has `--global`, the REST list has `global=`, the tool `create` takes `global`; the tab bar has `+G`, the menu has `New Global session` (`Ctrl+Shift+N`), the New chat form refuses Global without a folder.
- **Calls:** a sidebar tab of call rooms (`sidebar_calls.rs`), a room is a tab beside the sessions' tabs. The operations (`call.*`, REST `/v1/calls`, the agents' tools) are the agents' own and do not change.
- **The path:** the path bar (M14) IS the last row of the window (`ui.rs`: path bar last, the hint line above it). It is EMPTY in Global (no selection path, no session path), which is where the owner starts, and it sits under the hint line, not over it.
- **The top:** two rows (`File Session View Help`, then a toolbar of nine buttons that changes with the panel), and a third row of session tabs with `+ +B +BL +G`.
- **Browser tab:** the address is the input line at the BOTTOM (the whole URL), the header says where typing goes, a new tab shows the help text (about 14 lines, `F1`), the click ring is `browser_cues.rs` and `lico-browser-view` (`click_mark`, BROW-09).

## Decisions (the lead's, to be confirmed)

- **The virtual directory is `Anon`, its path is `lico://anon`.** The word Global leaves the window (rows, buttons, menus, header, hints, help). It stays on the wire where a CLI, REST or tool contract names it (`global`, `--global`), because removing it breaks callers and the parity tests for no gain to the person; the window never shows it. A chat of the host's workspace root is an Anon chat.
- **Chats lists every past session of this host, newest first** (one list: the folder, Anon, ssh hosts are one more column of the card, not another scope). The host answers with a new scope of `session.chats` (`all`), so the CLI and REST see the same list. A click opens the chat and moves Directories to its folder (or to Anon).
- **Calls leave the sidebar, not Lico.** A room that two sessions share is shown as a small `⇄` mark on the session's card and tab; a click on it opens the room as today. The agents' tools and the REST and CLI calls do not change.
- **The bottom is: path row, then the key-hint row** (`Ctrl+N` ...). The path row always has text: the folder, `lico://anon` in Anon, the row under the pointer when there is one.
- **The top is ONE row:** the four menu titles and the five actions people use (`+ New`, `Files`, `Close`, `Mode`, `Find`) on the same line; the rest are in the menus only. The session tabs row keeps `+` and `+B` (`+BL` and `+G` go into the `+` menu entries).
- **The browser tab has an address bar at the top** (`‹ ›  ⟳  address`): the address is cut in the MIDDLE when long (the host name and the end stay), the whole address shows when the bar has the keys, Enter goes. A new tab is an EMPTY page with the bar focused and a one-line placeholder; the explanation is on `F1` only. The click ring and its words go (BROW-09 is removed in `lico-browser-view`, the component's version and its specification change). A search box that guesses a search engine is NOT added: that is a choice of a service, the owner's call.

## Phases (each: a plan, a branch of its own, tests first, green-only commits)

| Phase | Subject | Plan and state | Status |
|---|---|---|---|
| M22-P01 | host: `session.chats` scope `all` (every past session, newest first, one list), CLI and REST, bounded cost | [Plan](../../../plans/202611/20261118/20261118_03_M22-P01_chats_of_all_sessions_plan.md) | planned |
| M22-P02 | window: two left tabs, Anon as one virtual directory, Chats of all sessions with the jump, Calls out of the sidebar | in the P01 plan's follow-up list; written when P01 is done | planned |
| M22-P03 | window: path row above the hint row and never empty, one-line top bar, the tab bar's buttons | written when P02 is done | planned |
| M22-P04 | browser tab: address bar at the top, an empty first page, the click ring removed | written when P03 is done | planned |
| M22-P05 | specification and guide, README, CHANGELOG, the suite, `cargo xtask system`, release `0.8.0` | written when P04 is done | planned |

## What this milestone does not do

- It does not add a feature. Where a thing can be taken out, it is.
- It does not rename or remove an operation on the wire, in the CLI, in REST or in the agents' tools.
- It does not delete a past session or a call room; only what the window shows changes.
- It does not push, open a pull request or merge; each needs the owner's explicit yes after a dry run.
- It does not start the terminal ([M23](20261118_02_M23_terminal_agents_milestone.md)).

## Owner's call (the lead proceeds with the defaults above unless told otherwise)

1. The name `Anon` and the path `lico://anon` (or another word).
2. The five buttons kept on the top line.
3. Whether the Chats tab also lists sessions the provider's own default home holds (the M11 native home), which Lico did not open. Default: only what Lico opened, plus the native home when the owner has turned it on, as today.
