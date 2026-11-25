# M24-P11b: the time beside each block and a colour for who spoke (the transcript view, second part)

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P11b-gutter-and-kind-colours` from `phase/M24-P11a-time-of-each-line` (`0b0b32f`); code `c929fbf`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-24 (quoted in the [plan](../../../plans/202611/20261125/20261125_09_M24-P11b_gutter_and_kind_colours_plan.md)): "각 대화 블록의 너비를 풀로 쓰지 않고 오른쪽에 어느 정도 일정 간격을 남기고 거기에 당시 년월일시분초를 남기는 것이 ... 그게 내가 보낸 것인지, peer 끼리 보낸 메시지인지, 응답인지, 뭔가 command 실행인지에 따라서 배경에 보이는 색상이 달라지는 형태로 보이고" and "이게 0.8.2의 내용"
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test and in a screenshot of the window)

On a window whose text area holds at least 101 columns (a window of 1200 pixels has about 107), every block of the conversation stops short of the right edge, and in the room that is left stands **when it was written, to the second** (`2026-11-25 13:12:51`, the local time), in a dim grey, at the first row of the block.

| | Before | Now |
|---|---|---|
| A block's width | the whole window | the window less a room of 21 columns (about 179 pixels at the usual size) at the right; the text, the input box and the bands are that much narrower |
| The time | none | at the first row of each block (a message of yours, a message from a peer, the agent's reply, a command or tool with its output; a note of the window is a block each), and at the first line on the screen, so that the reply being read keeps one when its first line has scrolled off |
| A time that comes late | | the host answers `stamps` a moment after the lines; the room is kept whether or not a time is known, so the text never moves, and the time appears at the next redraw |
| Who spoke | your line blue, a peer's card teal, a command purple, the agent's reply no colour | the same, and the agent's own reply has a band of its own (a neutral a little lighter than the window); every band and box ends where the room begins |
| A narrow window, a panel, a page, the start lines | | no room and no time (below 101 columns the bands keep the whole width; the reply's band shows at every width) |

## 2. How it is built (READ)

- `gui/blocks.rs` (new, pure functions of the roles the transcript already computes): `kind_of` (user, peer, command, response, note), `starts_block` (the first line on the screen, a change of kind, the top of a card, every note), `stamp_text`, `gutter_px`.
- `gui/transcript.rs`: `Look { gutter, response }` for the decorations (the right limit of every band and box, and the reply's band); the colours are constants.
- `gui/times.rs`: `stamp_rows` (the lines that carry a time), `gutter_clear`, `gutter_draw`, `Times::paint`, `App::times_gutter`; an answer of `stamps` that was the one awaited marks the window to be drawn again.
- `gui/ui.rs` (pinned at 1114 lines, none added): the text is `gutter` pixels narrower, the decorations get the `Look` and the room is cleared after them, and the times are drawn after the text.
- **Why the room is cleared and drawn again in every frame**: the window draws by hand into a pixel buffer and a scroll step keeps the rows of the last frame; a time drawn into kept rows would stay (good) but one that comes late would never be drawn. The room is plain background by design (no block reaches it), so clearing it for the lines on screen and drawing a few dozen cached labels gives the same pixels whether the rows were kept or drawn (the oracle proves it).
- The module graph is unchanged (the cycle stays at 50). The size ratchet holds (`ui.rs` 1114, `mod.rs` 797).

## 3. Checks (MEASURED)

- **Tests first**: the unit tests of the pure functions, of the decorations' pixels and of the three drawing functions were run on stubs and all failed, then green.
- **The tests now**: unit `5` (`blocks`), `5` (the decorations' pixels), `32` (`gui::times`: the rows, the clear, the draw, the room, the redraw on an answer), the scroll oracle `7` (one new: a scripted session with a gutter and times, a late answer, at the scales 1, 1.25 and 2: the pixels with the scroll step are the pixels of a full repaint, the room is plain before a time is known, the bands end at the room, every time is the time of the first line of its block, none is under the hint line; one more: a page frame has no room and no reply band); the window test of `stamps` now also reads the room and the texts drawn in a real window (the local time to the second, in the order of the lines). The whole binary: 1,253 unit tests pass.
- **Mutation checks**: **65 mutants** of the new code (`blocks.rs`, the lines added to `transcript.rs`, the new functions of `times.rs`, the wiring in `ui.rs`) in the first run: 51 killed, 14 survived. **Three were code that did nothing** and went: the guard for a column of no width or no number in `gutter_px` (such a column gives no columns, or a gutter of no pixels, by itself), and the `page_active()` clause of `times_gutter` (a browser or terminal tab is shown only with no session selected, which the empty-session test already refuses). **Eight were test gaps** and are closed: a band alone is the person's, a hint row below the window, the height of a label at scale 2, the row and the column a time is drawn at (the pixels are the pixels of a label of that text at that place), no rows drawn at all when there is no gutter, the padding when the columns are counted (866 and 867 pixels), the command box that ends before the room (the oracle looks at its edge), and the page frame that has no reply band. The 17 mutants the closed gaps touch were run again: 15 killed. **Three are equivalent** and written down: `{:04}` for the year (every year from 1970 to 9999 has four digits), the lower bound of the draw area in `ui.rs` (`hit_lines` holds only the lines inside the text area, and the bound inside `gutter_draw` is pinned by its unit tests), and the top bound of the clear (no line of the screen begins above the area). Final: 62 mutants, 59 killed, 3 equivalent.
- **Other tests**: four window tests read what the narrower text changed and were fixed in the same commit (`test_conversation_view`, `test_frame_cost`, `test_editor_selection`, `test_activity`; each is named in the plan's results with the reason).
- **The commit hook**: the first run of the hook refused: five window tests were red (they read a pixel at the right edge, count the lines a screenful holds, wait for a wrapped line to grow when the window narrows, or expect a still screen after a turn; all five read what the narrower text and the new redraw change, and are fixed in the same commit), and the build's peak of 1960.8 MiB was over the bound of 1536 MiB (the bound stays); the second run, with the build cached, passed (262 tests in 81 files, 130.7 s, peak 940.3 MiB).

## 4. What is not decided (the owner's)

- The colour of the reply's band (`RESPONSE_BG`) and the dim ink of the time are the lead's choice: one line each in `gui/transcript.rs` and `gui/times.rs`. In a screenshot the reply's band is faint on purpose.
- The time is the size of the text (the window has one text size); a smaller one needs a second text size in the window and is not done.
- On a window narrower than 101 columns there is no time; whether to show a shorter one there (`14:03:07`, without the date) is the owner's.
- A session made before `0.8.2` has no times for its old lines (the log never had them), and they show none.
- The whole-block select and fold (P11c) and the file under the pointer (P11d) come next; the release is `0.8.2`.
