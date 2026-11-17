# M22-P03 Plan — The Left Side: Two Tabs, One Virtual Directory `Home`, Every Past Session In Chats

- Status: done (`7361c18`, 2026-11-18; the owner approved the guard prefix `phase/M22-P03-` in the chat and the commit ran with every hook on)
- Version: 1.2.0
- Branch: `phase/M22-P03-two-tabs-home` (stacked on `phase/M22-P02-chat-index`, `636d629`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18): remove the concept called Global (one virtual directory, `lico://home`); Chats shows every past Lico session newest first and a click goes at once to that session's working directory; the left side has two tabs only; Calls are not shown to the person, only seen between the agents that talk to each other.

## What this phase changes (the window only; the host did its part in P01 and P02)

1. **Two tabs.** `Directories | Chats`. The Calls tab, its list, its quick-connect peer buttons and its per-room `×` go. The rooms themselves stay: the host, `call.*`, REST, the agents' tools, the room tab beside the sessions and the jump card in the transcript are not touched.
2. **`Home`, not `Global`.** The pill `Global (general)` is `Home  lico://home` (active: `★ Home  lico://home  [active]`). No text of the window says Global after this phase except what P04 still owns (the menu entry `New Global session`, the tab bar's `+G`, the path bar's old wording) and the wire, which keeps `global` (a CLI, REST or tool contract names it). The state keeps its internal name `global_scope`; a window state JSON key that says `global_*` stays (a test hook, not a word the person reads), and a new key `home_label` says what the pill says.
3. **Chats is one list of every past session of this machine** (`session.chats` with `all`), newest first, whatever folder the Directories tab is on. The scope rows of the old tab (the Global pill and the viewed-folder row) are gone from it: the list does not depend on them, so they cannot mislead. The filter box stays (`query`). A selected ssh host keeps its own list as today (`all` is this machine's; the host refuses the field with an ssh host), shown with a one-line `ssh:<host>:<folder>` row, so no remote capability is lost.
4. **A card says where the session is.** The second line is `<ago> · <folder> · <who> · #<id>`: the folder is the last two components of the path, `lico://home` for a Home session. The title line and the pills are as before.
5. **A click goes to the folder, at once.** A card of an open session switches to it; a closed one is resumed; in both cases Directories moves to the session's folder (`lico://home` for Home: `global_scope` on) without waiting for the resume, because the card already knows the folder. The existing follow of the active session (`sidebar_sync_to_active_session`) stays as the second path.
6. **The mark `⇄` on the tab of a session in a room.** A session that is in a room (a `summaries` item with the session as `session_a` or `session_b`; the running one wins, else the newest) shows `⇄` after its caption on its tab; a click on the mark opens the room tab (`open_call_tab`). It replaces the Calls tab as the way to reach a room whose tab was closed. (The first draft put the mark on the Chats card; the tab is better: a room is seen between the two sessions that talk, and a session with no transcript on disk has no card.) The window state says `room` (the call id) and `mark_rect` on a tab item.
7. **No wait.** The Chats list is fetched when the sidebar starts (not on the first click of the tab), so the tab opens with its list; a click on the tab refreshes it in the background and the old list stays on screen until the new one arrives (it does today: `Loading chats...` shows only for an empty list). The host answers in 5 ms (P02); the window never blocks (`spawn_host_call`).
8. **The window's question about the provider's own home is gone.** The modal, its three buttons and keys (`Prompt`, `PromptGeom`, `native_prompt`, `native_asked`, `NativeYes|No|Later`, `wanted`) and the state key `native_prompt` are removed (P02 made the setting `on` by default; `ask` is never emitted). `native_prompt.rs` is trimmed, not removed: `NativeEntry`, `row_label`, `native_seen` (now only keeps the entries), `native_menu_label` and `toggle_native` stay, because the Settings menu and the account line read them. The New chat modal keeps its own list.

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
| D4 | The `⇄` mark is on the session's tab (not on the Chats card) | a room is seen between the two sessions that talk; a session with no transcript has no card, a tab always exists while the session is open |
| D5 | The glyphs are the ones the font has (`★ ○ ⇄`); the house icon is the toolbar's, P04 | a new glyph needs a font check; the toolbar draws its icons itself |

## Steps (tests first; each step is green before the next; the window tests run under a private Xvfb with synthetic homes: no real home is read)

1. [x] **Red tests.** `tests/test_two_tabs_home_chats.py` (new, in `tests/testmap.toml` with `gui`, `host`): the tab labels are `Directories` and `Chats`, no Calls tab, the pill says `Home  lico://home`, Chats asks with `all` and lists two folders and a Home session in one list newest first with each card naming its folder, the click moves `cwd` or turns Home on, the list is there before the tab is opened, `native_prompt` is absent from the state. Changed first: `test_call_rooms_and_mentions.py` (the room list and the peer buttons become the mark on the tab), `test_chats_end_to_end.py`, `test_chats_pagination_and_resume.py`, `test_chat_list_polish.py`, `test_native_end_to_end.py`, `test_left_directory_sidebar.py` (a comment). Red for the intended reasons before any source edit (`red1.log`).
2. [x] **Chats = `all`, the card's folder line, the click's jump** (`sidebar_chats.rs`, `sidebar.rs`).
3. [x] **Two tabs and the Home pill** (`sidebar_render.rs`, `sidebar_tabs.rs`: two hues; `sidebar_nav.rs`: the bookmark says `Home`).
4. [x] **The Calls tab out, the `⇄` mark in** (`sidebar_calls.rs` keeps the data and `room_of`; its drawing, paging, peer buttons and the `SidebarCall*` actions are gone, and so are `start_call_between` and the `CallStart` job; `tabs.rs` draws the mark and registers its click after the tab's, so it wins).
5. [x] **The native prompt out** (`native_prompt.rs` trimmed as in item 8, the `Native*` actions and their four call sites in `ui.rs`).
6. [x] **A first fetch at start** (`maybe_init_sidebar` -> `chats_prefetch`).
7. [x] Gates and the manual-inspection bundle; [x] commit `7361c18` with the guard on (the owner approved the prefix `phase/M22-P03-` in the chat; the pre-commit suite ran 13 tests in 7 files, all pass).

## Results (2026-11-18)

- Unit tests: `cargo test --bin lico gui::` 254 pass (two new in `sidebar_calls.rs`: the room of a session, the running room wins then the newer; one new in `sidebar_chats.rs`: the folder of a card; the `wanted` test went with the question). Mutation check: `room_of` with `min_by_key` and `folder_of` with three components instead of two each fail their test; restored.
- Window tests: the six changed files and the new one pass (9 tests); the changed-files suite found three more that read `sidebar.native_prompt` (`test_native_first_run.py` twice, `test_running_elsewhere_window.py` once) and they now assert the key is absent; the rerun passes.
- Gates: `cargo xtask check` and `verify` pass; `graph` 203 modules, 71 nodes (one cycle, as before).
- No wait: with 40 sessions seeded in 6 folders the list is in the window at start (`chats_total` 40 with one load) and the first poll after a click on the Chats tab already sees the cards (1 ms, the poll itself; eight clicks, all 1 ms). The host answers in 5 ms (P02), the window never blocks.
- Manual inspection: `private/manual_inspection/202611/20261118/00001_M22-P03_two_tabs_home_chats/` (the Home pill, the cards with their folder, the click's jump, the mark on the tabs of a room).
- What the person loses: starting a room from the sidebar (the peer buttons). `@name message` in the input and an agent's tool still start one; the room tab, `Hang up` and the jump card are unchanged.
- What the person may notice: a resumed chat moves to the top of the Chats list ("just now"), as it always did; the path row at the bottom shows the folder of the card under the mouse (the old hover, P04 owns that row).

## What this phase does not do

The toolbar, the menus, the path row, the tab row (`+ +B +BL +G` and the `⇄` on a tab) are P04. The browser tab is P05. The host is not touched.
