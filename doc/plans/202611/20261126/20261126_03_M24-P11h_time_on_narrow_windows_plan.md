# M24-P11h Plan — The Time Beside Each Block Also On A Narrower Window

- Status: in_progress (2026-11-26)
- Version: 1.0.0
- Branch: `phase/M24-P11h-time-on-narrow-windows` from `phase/M24-P11d-theme-button` (P07 to P11d are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-26): "여전히 오른쪽에 각 대화의 년월일시분초 표시가 없음." (and, 2026-11-24, the request itself: "각 대화 블록의 너비를 풀로 쓰지 않고 오른쪽에 어느 정도 일정 간격을 남기고 거기에 당시 년월일시분초를 남기는 것이 자원을 많이 안 쓰면 표시를 해주면 좋겠고")
- Standing authorization of 2026-11-25: the prefix, the commits and the merge are approved in advance; no version bump (the release 0.8.2 carries it).

## What this phase gives the person

The time of each block of the conversation (`M24-P11b`) is drawn only when the text area holds **101 columns** or more (`blocks::MIN_COLS`), the rule that was chosen so that the text keeps 80 columns. A window that is not wide (half a screen, a tiled window, the list on the left kept open) holds fewer, so it shows **no time at all**: that is what the owner sees. The rule was too strict for the person's way of working: a time that is shown in a shorter form is worth more than none.

After this phase the time is shown on every window where the text keeps **at least 60 columns**, in the longest of three forms that fits:

| Form | Text | Columns of the gutter | Shown when the text area holds |
|---|---|---|---|
| Full | `2026-11-26 14:03:09` | 21 | 81 columns or more |
| Short | `11-26 14:03:09` (the year dropped) | 16 | 76 to 80 |
| Clock | `14:03:09` (the date dropped) | 10 | 70 to 75 |
| none | | 0 | fewer than 70 |

The seconds are never dropped. A window that is wide enough for the long form shows what it showed (101 columns or more: no change, and 81 to 100 now also show the full form).

## Why it is built this way (what was found)

- The gutter is decided at one place: `blocks::gutter_px(text_w, col_px)` (called by `App::times_gutter`), and the text of a time is made at one place: `blocks::stamp_text`. The block's bands, the room kept at the right, the clearing of the gutter after a scroll step and the drawing all take `gutter` (pixels) and `col_px`; none of them knows how long the text is, except `gutter_draw` which draws what it is given. So the change is the choice of the form and the length of the gutter, and nothing else moves.
- A form is told from the gutter that was kept (`Form::of_gutter(gutter, col_px)`: the one whose columns, rounded up to pixels, are exactly the gutter), so `compose` (`ui.rs` is pinned at 1114 lines) keeps passing the same two numbers.
- The text keeps 60 columns at the least, not 80: a chat line wraps sooner, the person sees when a thing was said. The longer forms need more room, and the table above is that rule written as numbers (`60 + the columns of the form`).

## Decisions

| # | Decision | Detail |
|---|---|---|
| D1 | **Three forms** | Full (19 characters), Short (14: `MM-DD HH:MM:SS`), Clock (8: `HH:MM:SS`); two columns of air in each gutter; the seconds are always there |
| D2 | **The rule** | the text keeps 60 columns at least: the longest form whose gutter leaves that many columns; none when not even Clock fits (the area is under 70 columns) |
| D3 | **Where it is decided** | `blocks::form_for(text_w, col_px)`; `gutter_px` is the pixels of that form's gutter (0 for none); `stamp_form(form, secs, offset)` writes the text; `stamp_text` stays the full form |
| D4 | **The dump** | `times.form` (`full`, `short`, `clock` or `none`) next to `times.gutter`; the rows drawn carry the text of the form |
| D5 | **Not changed** | the blocks' bands and colours, the folding, the selection, the time source, the host (`stamps`), the hit boxes; a panel, the start lines and a view with no session keep no gutter |

## Steps

1. [ ] **Tests first, run red**: `blocks_tests.rs` (the forms, the rule at each edge in columns and in pixels at 10 px and 9.6 px, no overflow, a column of no width, `of_gutter`), `times_tests.rs` (the form that is drawn at three widths, the texts of the rows, the dump key), the scroll oracle (a narrower window: a scroll step draws the same pixels as a full repaint with the Short and Clock forms, at the scales 1, 1.25 and 2), a window test (a real window at a width that gives each form).
2. [ ] **The change** in `blocks.rs`, `times.rs` (and the joined lines of `ui.rs`).
3. [ ] **Mutation checks**, one edit at a time on `blocks.rs` and the lines added to `times.rs`.
4. [ ] **Other tests**: the existing window, oracle and unit tests, the suite.
5. [ ] **Words**: the guide (a section on the time beside each block).
6. [ ] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report.

## Not decided here (the owner's, written in the report)

- A window under 70 columns of text still shows no time (the time would cost the text its room); the person widens the window or closes the list on the left.
- The Short and Clock forms lose the year, or the date, of the time. The full time of a block can be read in the longer forms; a hover that says it in the bottom line is a later small phase if the owner wants it.
