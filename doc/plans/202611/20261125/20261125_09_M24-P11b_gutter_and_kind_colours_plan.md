# M24-P11b Plan — The Transcript View: The Gutter With The Time, And A Colour For Who Spoke

- Status: in_progress (2026-11-25)
- Version: 1.0.0
- Branch: `phase/M24-P11b-gutter-and-kind-colours` from `phase/M24-P11a-time-of-each-line` (`0b0b32f`; P07 to P11a are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-24, the content of 0.8.2), the part of this phase: "각 대화 블록의 너비를 풀로 쓰지 않고 오른쪽에 어느 정도 일정 간격을 남기고 거기에 당시 년월일시분초를 남기는 것이 자원을 많이 안 쓰면 표시를 해주면 좋겠고, 그게 내가 보낸 것인지, peer 끼리 보낸 메시지인지, 응답인지, 뭔가 command 실행인지에 따라서 배경에 보이는 색상이 달라지는 형태로 보이고"
- Standing authorization of 2026-11-25: the prefix, the commits and the merge are approved in advance; no version bump (the release 0.8.2 carries it).

## What this phase gives the person

On a window that is wide enough, every block of the conversation stops short of the right edge, and in the gap that is left, in dim small-looking text, stands **when it was written, to the second** (`2026-11-25 14:03:07`, the local time). The background of a block says who spoke: **me** (blue band), **a peer** (teal card), **the agent's reply** (a faint neutral band, new), **a command or tool** (purple box). Nothing else moves: the text is the same text, a little narrower.

## Why it is built this way (what was found)

- The conversation is one text buffer of lines (`cosmic-text`), drawn by hand into a pixel buffer; a scroll keeps the pixel rows of the last frame and draws only the rows that are new (`scroll.rs`, with an oracle that compares it with a full repaint pixel for pixel). A time drawn into the rows that are kept would stay where it was (good) but one that arrives late (the host answers `stamps` a moment after the lines) would never be drawn.
- So the gutter is **cleared and drawn again in every frame for the lines on screen** (a few dozen short labels, shaped once and cached by `Controls`), over a gutter that is plain background by design (no block reaches it): the same pixels whether the rows were kept or drawn, and a late answer shows at the next frame.
- The text must not run under the gutter, so the buffer gets a narrower width (`text_w`) when there is a gutter: the lines wrap earlier, the input box too. The gutter is kept **whether or not a time is known** so that a time arriving late never makes the text move.
- The window already has block colours for the person's line, a peer's card and a command box (`transcript.rs`); only the agent's reply had none, and the user's band ran to the window edge.

## Decisions

| # | Decision | Detail |
|---|---|---|
| D1 | **Kinds** | `blocks::Kind` = `User` (`ROLE_USER_BG`, or a person's `>>` line), `Peer` (`ROLE_PEER_BG`), `Command` (`ROLE_BOX`: a command, a tool card, an exit note, a tool error), `Response` (the agent's own text), `Note` (everything else: an event that is shown, an error note). `kind_of(role)` is a pure function of the role bits the transcript already computes |
| D2 | **Blocks** | a block starts at the first line of the view, at a line whose kind differs from the line before, at a line that begins a card (`ROLE_BOX_TOP`: a command, a tool, a peer card) and at every `Note`. The lines of a multi-line message of the person (each is its own record `>> ...`) are one block. `starts_block(previous_role, role)` is pure |
| D3 | **Which time** | the time of a block is the time of its first line (`Times::at(seq)`); the first line **on screen** counts as a start whatever it is, so that the reply being read keeps a time when its first line has scrolled off (it shows the time of that line's own run). A line the window has no time for shows nothing |
| D4 | **The gutter** | `GUTTER_COLS = 21` columns (a date and time of 19 characters and two of air), reserved only when the area has at least `101` columns (the text keeps 80 at least), only for the transcript of a session (not a panel, not a page, not the wizard); on a narrower window there is no gutter and no time (the bands and boxes keep the whole width; the reply's new band, D5, shows at every width) |
| D5 | **Colours** | the user's band, the peer card and the command box end at the gutter instead of the window edge; a reply gets `RESPONSE_BG` (`0xFF17_1A1A`, a neutral a little lighter than the window's `0xFF10_1418`, apart in hue from the blue, teal and purple of the others) over the same width as the user's band; a `Note` keeps the plain background |
| D6 | **The text** | `YYYY-MM-DD HH:MM:SS` in the local time of the machine that shows it (`transfer::utc_offset` and `transfer::civil` are there already), in a dim ink (`0xFF7A_8496`) at the first row of the block; `stamp_text(secs, offset)` is pure |
| D7 | **A frame** | after the decorations: every line on screen has its gutter cleared to the background (the rows of a kept scroll step included); after the text, selection and search rectangles: the times are drawn (`Controls::text`, shaped once per text and cached) at the left of the gutter |
| D8 | **A late answer** | the reply of `stamps` marks the window dirty when it was the one awaited, so the next frame draws the times it brought |
| D9 | **The dump** | `times` in the state dump gets `gutter` (pixels, 0 when none) and `drawn` (the texts drawn in the last frame with their rows) so the tests can read them |

## The pieces

1. `gui/blocks.rs` (pure, no window): `Kind`, `kind_of`, `starts_block`, `stamp_text`, `gutter_px`, the constants; `gui/blocks_tests.rs`.
2. `gui/transcript.rs`: `paint_transcript_decorations` gets a `Look { gutter, response }`: the right limit of every band and box, and the reply's band; the colours are constants there.
3. `gui/times.rs`: `Times` gets `gutter` and `rows` (what the last frame drew, for the dump); the pure `stamp_rows` (the lines of the screen that start a block, with their times), `gutter_clear` and `gutter_draw`; `Times::paint` (the rows and the draw of one frame); `App::times_gutter` (the pixels and the column width for this frame); `ui_stamps` marks the window dirty when the answer was the one awaited.
4. `gui/ui.rs` (size pinned at 1114: only joined lines, none added): the narrower `text_w`, the `Look` in the decorations call, the clear after it, `Times::paint` after the text.
5. Tests: unit (`blocks`, the decorations' pixels), the oracle (`scroll_oracle.rs`: a scripted session with times and a gutter draws the same pixels with the scroll step as with a full repaint, at the scales 1, 1.25 and 2, with a late answer), a window test (the dump says the gutter and the texts of a real session).

## Steps

1. [ ] **Tests first, run red.**
2. [ ] **The change.**
3. [ ] **Mutation checks**, one edit at a time on `blocks.rs`, the lines added to `transcript.rs` and `times.rs`.
4. [ ] **Other tests**: the pixel tests of the decorations and the oracle that already exist, the suite.
5. [ ] **Words**: the guide.
6. [ ] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report.

## Not decided here (the owner's, written in the report)

- The colour of the reply's band and the dim ink are the lead's choice; they are constants and one line each to change.
- The size of the time is the size of the text (the window has one text size); a smaller one needs a second text size in the window and is not done here.
- On a window narrower than 101 columns there is no gutter and no time; whether to show a shorter time there is the owner's.
- Whole-block select and fold (P11c) and the file under the pointer (P11d) are the next phases.
