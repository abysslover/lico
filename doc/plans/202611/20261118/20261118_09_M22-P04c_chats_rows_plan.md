# M22-P04c Plan — A Chats Card That Shows The Provider, The Title, The Time In A Colour, And The State As A Picture

- Status: done (`2223fe5`, 2026-11-18)
- Version: 1.1.0
- Branch: `phase/M22-P04c-chats-rows` (on `main`, `c404a49`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18, after P04b was published and `lico gui` opened):
  1. In Chats the **memory indicator** (green, orange, red at the right) is missing.
  2. The word **`elsewhere`** is not needed.
  3. `closed` and the like: an **icon, not text**.
  4. **What do `C` and `X` mean?** (They are the provider; the owner did not know.)
  5. At bottom the **title and the path are not seen at all**; what is wanted is the **title and the time of the last access, in a colour**.
  6. The time need not be followed finely beyond a certain age.

## What this phase changes (the window only; the host, the wire and the CLI are not touched)

The Chats tab keeps its two-line card (as many cards fit as before). Each card is:

```
[icon]  The title of the chat, as much of it as fits           3h
[    ]  api/src                                              (o)
```

| Part | What it shows |
|---|---|
| Left icon, over both lines | the provider as a picture, the ones of the New session wizard: Claude (the spark), Codex (`</>`), Local (the chip). No letters |
| Line 1, left | the title, the whole width up to the time column (no mark takes its room any more); a title that does not fit goes on to line 2 (D7) |
| Line 1, right | the time of the last access, short, right-aligned in a column of one width: `now`, `5m`, `3h`, `4d`, `3w`, `5mo`, `1y`; nothing finer than a minute, nothing finer than a week beyond a week, and **its colour says how old it is** (see D2) |
| Line 2, left | the rest of a long title (D7); for a title that fits one line, where the chat is: `lico://home` for Home, else the end of its folder as long as it fits |
| Line 2, right | the state as a picture: **a filled disc** while the chat is open in a tab, **green** while its memory is fine, **orange** from the warning level (or when it is stalled), **red** from the limit (the same levels and colours as the memory figure of the tab list); **a ring** (gold) when its tab is closed; **a small square** (grey) when it ended; nothing for a chat that is not open |

Gone from the card: `[open] [closed] [ended] [elsewhere]`, `written 4m ago`, the account's name, the short id, `C·` and `X·`. Nothing is lost: **the hint row at the bottom** says everything of the card under the pointer (title, state in words with the memory figure, folder, account or ssh host, id), and **the path row** keeps giving the whole folder. A click on a chat that another program writes still says why it cannot be resumed (the flash).

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The provider is a picture (the wizard's own), drawn by the window | the owner did not know `C` and `X`; one picture means one thing in the toolbar, the wizard and the list |
| D2 | The age colour has four steps: under an hour bright blue, under a day light blue-grey, under a week muted, older dim (one colour beyond a week) | "a colour for the time" and "no need to follow it beyond a point" (the owner); four steps are told apart at a glance |
| D3 | The state's pictures differ in shape (disc, ring, square), not only in colour | a colour alone is not read by everyone; the memory colour is on the disc only |
| D4 | A chat that is open takes its state and its memory from the tab list (the window already has the figures), and falls back to what the chat list fetched | the colour must follow the tab as it changes (`attn.rs` redraws the list when a level changes) |
| D5 | A chat that another program writes shows no picture and no word; the click says why it is not resumed | "elsewhere is not needed" (the owner); a hover and the click flash still explain |
| D6 | The card's logic lives in a new module (`chatcard.rs`: age, state, hint, layout, the small pictures); `sidebar_chats.rs` keeps the list and the fetch | the file is at 792 of 800 lines; the old word helpers (`sub_line`, `pill_of`, `rel_age`, `ago`, `who_of`) go with their tests |

| D7 | A title that does not fit line 1 goes on to line 2 (cut at a space when one is in the last 12 columns, else where the line ends; ended by an ellipsis when it does not fit two lines); the folder is then not on the card | the owner's complaint was that the title was not seen; most titles are a first prompt and are longer than one line (a Korean title shows 13 characters on one line); the folder is still one hover away (the hint row, and the path row above it) and one click away (the card goes there) |
| D8 | The window draws again when the state or the memory level of any tab changes, through its own signature (`Look.chat_sig`), not through the list's text | `list_text` is the window state the older tests read; the card follows the same tab list the memory figure does |
| D9 | The state picture is 0.6 of a line high (about 11 px) | the first try (0.5) was hard to tell from the figure's dot at a distance; the screenshot showed it |

## Steps (tests first; each step is green before the next)

1. [x] **Rust tests** (`chatcard.rs`): the age words and the colour steps at their borders; the state of a chat from the list and from a tab (open, closed, ended, none; green, orange, red; stalled is orange; a tab that is gone falls back to the list); the folder line (Home, short, cut by the room); the hint sentence; the layout (icon, title, time, folder, state do not overlap and lie inside the card at three scales and for a narrow list); the pictures (disc, ring and square differ, stay inside their rectangle, the ring is hollow).
2. [x] **Window tests**: `tests/test_chat_list_polish.py` and `tests/test_two_tabs_home_chats.py` follow the new card (the old `pill` and `sub` are replaced by the new fields), a new `tests/test_chat_card_state.py` (an open chat's disc turns orange and then red when its tab passes the limits, a closed tab is a ring, the hint row says it, the card has no letters and no word).
3. [x] **`chatcard.rs`** and the wiring (`sidebar_chats.rs`, `memrow.rs` hover of a card, `telemetry.rs` hint, `attn.rs` signature of the open chats' levels, `ui.rs` makes `icons` visible to the window).
4. [x] Mutation checks on the new unit tests; `suite run --changed --receipts`; `cargo xtask check|verify|graph`; a manual-inspection page; `cargo build --release -p lico` (so `lico gui` shows it); commit with the guard on (the owner approved the prefix `phase/M22-P04c-` through the question at commit time).

## Results (2026-11-18)

- Unit tests: `cargo test --bin lico gui::` 312 pass (304 before: 12 new in `chatcard_tests.rs`, the four of the old word helpers `sub_line`, `pill_of`, `rel_age`, `folder_of` went with them); 27 mutation checks each caught and the file restored (the age words and their borders from a minute to a year, the four colour steps and the one beyond a week, the state of a tab against the list, a stalled session as orange, the signature, the hint words and the room the title gives way, the layout (title over the time, folder over the picture, the lines over each other), the title's break at a space and at the exact fit, the ring filled, the square without its inset, the Local picture).
- Window tests: `tests/test_chat_card_state.py` (new, 3 windows: a green disc for an open chat and a ring for a closed tab with the parts of the card not overlapping; orange from the warning level with the hint `open, N MiB, memory high`; red from the limit with `memory over the limit`); `tests/test_chat_list_polish.py` (the time and its class, the picture, a title on two lines, no mark for a chat another program writes, the hint row) and `tests/test_two_tabs_home_chats.py` (the age and the folder line) follow the new card.
- The changed-files suite: 265 tests in 77 files on 4 workers, all passed, 133 s, peak 1983.2 MiB (the bound is 2048 MiB), receipts recorded. A first run broke two older tests that read `list_text` (the signature of the tabs had been put in it: D8) and the size ratchet (`gui/mod.rs` 801 lines; its two chat modules now share a line), and its peak was 2052.9 MiB at the moment of the build; none of that stayed.
- Gates: `cargo xtask check`, `verify` and `graph` pass (the one cycle as before, the biggest has 48 modules).
- Manual inspection: `private/manual_inspection/202611/20261118/00005_M22-P04c_chats_cards/` (the list with an open green, orange and red chat, a closed one, Codex, a Korean title on two lines, ages from now to a year; the pointer on the red, the closed and the Korean card).
- What the person may notice: each Chats card has the provider's picture at its left, the title, and the time at the top right in a colour (the newest light blue, the oldest dim); an open chat has a dot at the bottom right that is green, orange or red by its memory (a ring for a closed tab); the words `[open]`, `[closed]`, `elsewhere`, `written ... ago`, the account name, the id and the letters `C`/`X` are gone from the card and are in the hint row. The session tabs on the top row still start with `C·` and `X·` (`tabs.rs`): not changed here.

## What this phase does not do

The browser tab (P05), the terminal (M23), the release (P06) and the guide (`doc/manual_end_to_end.md` still describes the old card; P06 rewrites it with the rest of M22). The `C·` and `X·` letters of the session tabs are not changed (the owner asked about the Chats list). The list's contents, its paging and its fetch do not change; the host and the wire are not touched.
