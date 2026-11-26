# M24-P11f Plan — The Status Line Only At The Bottom, And A Border For The Chat

- Status: draft (2026-11-26)
- Version: 1.0.0
- Branch: `phase/M24-P11f-status-line-at-the-bottom` from `phase/M24-P11g-top-rows-level`
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-26): "헷갈리는 점은 대화창 상단에 상태 표시줄이 표현되는데, 이것 때문에 내가 계속 혼란 스러웠던 것 같아. 채팅창 쪽은 뭔가 경계를 분명히 해서 기존 UI 요소와 안 섞이게 해주고" and, asked whether the line should stay anywhere but the bottom: "상태줄은 제일 밑에만 존재하는 게 맞음."
- Standing authorization of 2026-11-25: the prefix, the commits and the merge are approved in advance; no version bump (the release 0.8.2 carries it).

## What this phase gives the person

Today the top of the conversation is a stack of three thin bars of the same dark tone: the row of tabs (`No open tabs`, or the sessions), then **a line of status** (`claude | Work | claude-sonnet | idle 14:03:09 | mode: auto (permissions skipped) | ... | dir: ... | 5h 80%`), then the first line of the conversation. The status line looks like one more tab bar, and it says what the strip at the bottom says again (model, tokens, quota, folder).

After this phase:

1. **The status exists only at the very bottom.** The line is gone from the top of the conversation. What it said that the bottom strip did not (the provider, the account, whether the session is idle or working with its clock, the permission mode, an account issue, the host's notice and the one-line messages such as "A local session has no folder.") stands at the **left of the strip above the path bar**, in front of the model, the tokens, the quota and the folder it already shows. The first line of the conversation is the first row under the border.
2. **A clear border for the chat.** Under the tab bar a rule of two pixels in a lighter tone and a margin of a few pixels of the chat's own ground separate the chrome (menu, icons, tabs) from the conversation, so that nothing of the chat is read as one more bar.
3. A window with no session open (the start page) has no line at the top either: the start lines say "No session is open."; and a notice or a message with no strip to stand in (no session: the strip is not shown) takes the place of the keys on the bottom line while it lasts.

What does **not** change: the title row of a panel (Accounts, Files, New session, ...; it is the panel's title, not a status), the browser tab (its address bar), a terminal tab and the Call Room (each has a line of its own that is its control or its title), the transcript, the prompt and the "… working (37 s)" line that stands between them.

## Why it is built this way (what was found)

- The line is line 0 of the one text buffer of `compose` (`header`, then the search bar if open, then the transcript's lines, the activity line, the prompt). Everything that follows counts from `base = 1 + search_bar`: the hit boxes of the lines, the keys of the kept lines for the scroll step, the layout runs, the check that the estimate of wrapped rows was short. The scroll step (`scroll::decide`) does not need a header: lines in front of the first kept transcript line only limit the kept rows by their glyph extents, and none in front is the plain case. So the change is `base = has_header + search_bar`, the buffer built without the line when `has_header` is false, `recs[0]` (the extent of the header, needed only when there is one) guarded, and one more row for the transcript (`rows_avail`): the row the header held.
- The bottom strip (`draw_telemetry_bar`) is shown only for a conversation of the workbench (`show_telemetry_bar`: a session selected, no panel); its parts are drawn one after the other, the folder last, elided to what is left. The state part is put in front of them (`provider | account | state clock | mode | issue`, then the host's notice and the message), the model and the rest follow as they were. A part too long for the strip is clipped at the window's edge (the folder is the part that gives way, as it did).
- The top line is also the place of the host's notice (`ui.host_notice`) and of the one-line messages (`ui.flash`); they follow the state part, in a warm colour, so that they cannot be missed at the bottom.
- The dump key `header` stays (29 test files read it, the host's tools too): it keeps the text of the status as it always was, now **the text of the status line at the bottom** for a conversation, and the text of the title row for a panel; a new key `status` says what the bottom strip's state part drew.
- The border is drawn where the chat starts: `chat_top` becomes `top + tab bar + border` (6 pixels at scale 1: a rule of 2 and a margin of 4), and the tab bar is drawn to the old edge. Nothing else reads the chat's top except `compose` (the Call Room strip and the modals take it as an argument).
- `ui.rs` is pinned at 1114 lines (the ratchet): the new code (`status_at_bottom`, the state part, the border, the notice on the hint line) lives in a new module `gui/statusline.rs`, and the lines of `ui.rs` that change are changed in place, joined, not added.

## Decisions

| # | Decision | Detail |
|---|---|---|
| D1 | **Where the line is not** | not for a conversation or the start page of the workbench window; it stays for a panel (its title), the browser tab, a terminal tab, the Call Room, and a window without the menu or the workbench (nothing else says the status there) |
| D2 | **The state part** | `provider | account | state with clock | mode | account issue` (the mode in two words: `auto`, `asks`); `Local | state clock` for a local session; then, in a warm colour, the host's notice and the message; it is the first part of the bottom strip, in the light ink of the path bar |
| D3 | **No strip, a notice** | a notice or a message with no strip to stand in is the first words of the bottom line, in place of the keys, for as long as it lasts |
| D4 | **The border** | `chat_edge(s)` = 6 pixels at scale 1 (`6s`): the rule of `2s` in `3A4B63` on the line where the tab bar ends, then a margin of `4s` in the chat's ground; the first line of the conversation starts under it; it is drawn also over a panel and the browser tab (the chrome ends there as well) |
| D5 | **The rows** | the conversation has one row more (the header's); a panel keeps its rows; the hit boxes, the kept lines and the oracle follow `base` |
| D6 | **The dump** | `header` keeps its text; `status` (new) is the state part of the bottom strip as drawn; `layout.chat_top` (new) is where the chat's first row stands |

## Steps

1. [ ] **Tests first, run red**: unit (`statusline.rs`: which views keep the line, the text of the state part for a session of each provider, a local session, a mode, an issue, a notice and a message, the hint line's fallback), the layout (the first transcript line starts at `chat_top`, the border's rows are the colours said, a panel keeps its title row, a search bar is line 0 now), the scroll oracle at the scales 1, 1.25 and 2 in dark and in light with the new layout (with and without a search bar, with a long wrapped line, the times), a window test (a conversation: nothing at the top but the border; the strip's first part says provider, account and state; the same in light).
2. [ ] **The change** in `statusline.rs` (new), `ui.rs` (joined lines), `telemetry.rs` (the state part first), `tabs.rs` if the edge needs it.
3. [ ] **Mutation checks**, one edit at a time on `statusline.rs` and the lines changed in `ui.rs` and `telemetry.rs`.
4. [ ] **Other tests**: the 29 window tests that read `header`, the tests that count rows from the top, the suite; each that fails is read, not loosened: a test that named the top line is changed to name the bottom strip, and says so.
5. [ ] **Words**: the guide (a section on the status at the bottom and the border), the protocol note if a key is added.
6. [ ] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report with a screenshot before and after.

## Not decided here (the owner's, written in the report)

- A terminal tab, the browser tab and the Call Room keep the line at their top (it is their title or their control); if the owner wants them to say their state at the bottom as well, it is a later phase.
- The strip at the bottom is as wide as the right pane; on a narrow window the folder is elided first, then the end of the strip is clipped.
- The permission mode is two words in the strip (`auto`, `asks`); the full sentence stays in the dump's `header`.
