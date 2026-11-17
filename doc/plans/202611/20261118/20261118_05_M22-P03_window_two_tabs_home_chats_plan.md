# M22-P03 Plan — The Left Side: Two Tabs, One Virtual Directory `Home`, Every Past Session In Chats

- Status: in work (2026-11-18)
- Version: 1.0.0
- Branch: `phase/M22-P03-two-tabs-home` (stacked on `phase/M22-P02-chat-index`, `636d629`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18): remove the concept called Global (one virtual directory, `lico://home`); Chats shows every past Lico session newest first and a click goes at once to that session's working directory; the left side has two tabs only; Calls are not shown to the person, only seen between the agents that talk to each other.

## What this phase changes (the window only; the host did its part in P01 and P02)

1. **Two tabs.** `Directories | Chats`. The Calls tab, its list, its quick-connect peer buttons and its per-room `×` go. The rooms themselves stay: the host, `call.*`, REST, the agents' tools, the room tab beside the sessions and the jump card in the transcript are not touched.
2. **`Home`, not `Global`.** The pill `Global (general)` is `Home  lico://home` (active: `★ Home  lico://home  [active]`). No text of the window says Global after this phase except what P04 still owns (the menu entry `New Global session`, the tab bar's `+G`, the path bar's old wording) and the wire, which keeps `global` (a CLI, REST or tool contract names it). The state keeps its internal name `global_scope`; a window state JSON key that says `global_*` stays (a test hook, not a word the person reads), and a new key `home_label` says what the pill says.
3. **Chats is one list of every past session of this machine** (`session.chats` with `all`), newest first, whatever folder the Directories tab is on. The scope rows of the old tab (the Global pill and the viewed-folder row) are gone from it: the list does not depend on them, so they cannot mislead. The filter box stays (`query`). A selected ssh host keeps its own list as today (`all` is this machine's; the host refuses the field with an ssh host), shown with a one-line `ssh:<host>:<folder>` row, so no remote capability is lost.
4. **A card says where the session is.** The second line is `<ago> · <folder> · <who> · #<id>`: the folder is the last two components of the path, `lico://home` for a Home session. The title line and the pills are as before.
5. **A click goes to the folder, at once.** A card of an open session switches to it; a closed one is resumed; in both cases Directories moves to the session's folder (`lico://home` for Home: `global_scope` on) without waiting for the resume, because the card already knows the folder. The existing follow of the active session (`sidebar_sync_to_active_session`) stays as the second path.
6. **The mark `⇄` on a card.** A session that is in an active room (a `summaries` item with the session as `session_a` or `session_b`) shows `⇄` at the right of its second line; a click on the mark opens the room tab (`open_call_tab`). It replaces the Calls tab as the way to reach a room whose tab was closed. The mark on the session's tab in the tab bar comes with P04, which owns that row.
7. **No wait.** The Chats list is fetched when the sidebar starts (not on the first click of the tab), so the tab opens with its list; a click on the tab refreshes it in the background and the old list stays on screen until the new one arrives (it does today: `Loading chats...` shows only for an empty list). The host answers in 5 ms (P02); the window never blocks (`spawn_host_call`).
8. **The window's question about the provider's own home is gone.** `native_prompt.rs`, `Prompt`, `native_entries`, `native_asked` and the state key `native_prompt` are removed (P02 made the setting `on` by default; `ask` is never emitted). The New chat modal keeps its own list.

## What stays as it is (and why)

- **The Directories body under `Home`.** It keeps showing the last viewed folder and its files, as today: a file tree must stay reachable from Home, and a click on a folder leaves Home (as the Global pill did). `Home` is a place the person can be in, not a listing.
- **Bookmarks and history** keep their `global` flag (`sidebar_nav.rs`) and persist it under that key (a saved file of an earlier build still loads); their label says `Home` instead of `Global`.
- **The path bar** (empty in Home today) is P04's: its wording and the row above the hint row.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | A selected ssh host keeps the per-host list in Chats | `all` is this machine's; dropping the remote list would remove a capability nobody asked to remove |
| D2 | The Directories body under Home keeps the last folder | the file tree must stay reachable; the owner's words ask for a Home chat list and a virtual path, not an empty tree |
| D3 | The person no longer starts a room from the sidebar | the owner: calls are for the agents; a room is started by an agent's tool or by `@name message` in the input, both unchanged |
| D4 | The `⇄` mark is on the Chats card now, on the tab in P04 | the card is the only place that replaces the removed list |
| D5 | The glyphs are the ones the font has (`★ ○ ⇄`); the house icon is the toolbar's, P04 | a new glyph needs a font check; the toolbar draws its icons itself |

## Steps (tests first; each step is green before the next; the window tests run under a private Xvfb with synthetic homes: no real home is read)

1. **Red tests.** `tests/test_two_tabs_home_chats.py` (new): the tab labels are `Directories` and `Chats` and `calls_tab_rect` is null; the pill says `Home  lico://home`; Chats asks with `all` and lists sessions of two folders and a Home session in one list, newest first, each card's `sub` naming its folder (`lico://home` for Home); a click on a card of another folder moves `cwd` of the Directories tab to it and a click on a Home card turns `global_scope` on; a card of a session that is in an active room has `room` set and a click on the mark opens the room tab; the window state has no `native_prompt`. Changed first: the tests of the Calls tab (`test_call_rooms_and_mentions.py`: the room list and the peer buttons become the card mark), `test_left_directory_sidebar.py`, `test_chats_pagination_and_resume.py`, `test_chats_end_to_end.py`, `test_chat_list_polish.py`, `test_tool_create.py`, `test_session_tabs_and_telemetry.py` where they name the Calls tab, the Global pill or the scope rows.
2. **Chats = `all`, the card's folder line, the click's jump** (`sidebar_chats.rs`, `sidebar.rs`).
3. **Two tabs and the Home pill** (`sidebar_render.rs`, `sidebar_tabs.rs`: the palette keeps two hues; `sidebar_nav.rs` labels).
4. **The Calls tab out, the `⇄` mark in** (`sidebar_calls.rs` keeps the data and the click; its drawing, paging and peer buttons go).
5. **The native prompt out** (`native_prompt.rs` and its call sites).
6. **A first fetch at start**, and a measurement: the time from the click on the Chats tab to the list on screen, with the list prefetched (the number goes in the report).
7. `suite run --changed --receipts`, `cargo xtask check|verify|graph`; a manual-inspection page of the left side (the screenshot of the two tabs, the Home pill, the cards with their folder line and the `⇄` mark); commit with the guard on (the prefix `phase/M22-P03-` needs the owner's yes).

## What this phase does not do

The toolbar, the menus, the path row, the tab row (`+ +B +BL +G` and the `⇄` on a tab) are P04. The browser tab is P05. The host is not touched.
