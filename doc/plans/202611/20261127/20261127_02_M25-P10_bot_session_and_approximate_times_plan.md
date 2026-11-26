# M25-P10 Plan: The Bot Session In The Toolbar, And A Time For The Lines Of Old Sessions

- Status: done (2026-11-27)
- Version: 1.0.0
- Branch: `phase/M25-P10-bot-session-and-times` from `phase/M25-P09-agy-auto-default-and-id-handoff` (`cfcd0a5`, `lico 0.9.0` plus P09; nothing since `main` `648eb50` is pushed)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P10 (the `0.9.1` patch release)
- The owner's words (2026-11-26, after trying `0.9.0`): (B) "Home session이라는 이름은 사람들에게 직접적으로 와닿지 않으므로 lico 공간(전체 세션 등에 접근이 가능한)에 생기는 Bot으로 보고 New Home Session 대신에 New Bot Session이 조금 더 이해가 잘될 것 같음. 그래서 지금은 계정 세션 홈세션 Browser Theme 순서로 되어 있는데, 홈세션과 세션 순서를 바꾸고, 아이콘은 그 의미를 나타내도록 뭔가 로봇 비슷한 느낌의 아이콘으로 수정" and (C) "여전히 대화 오른쪽에 각 채팅이나 명령 등이 생성된 년월일시분초가 보이지 않음."
- Standing authorization of 2026-11-25 and the owner's answer of 2026-11-26 ("0.9.1로 끝까지 진행"): the prefix, the commits, the merge and the push are approved in advance.

## What this phase changes

### B. The Bot session

1. **The name.** *New Home session* becomes **New Bot session** in the toolbar's tip, the Session menu, the wizard's title (`New Bot session - step 1 of 3 - Choose`) and its Account step ("Bot session: it gets a scratch workspace of its own, not a folder."), the folder row's hint, the start screen's key line (`Ctrl+Shift+N  new Bot session`) and the flash that a session without a folder gives. The shortcut and the action are the same (`Ctrl+Shift+N`, `Cmd::NewGlobalSession`). The place where such a session lives is still **Home** (`lico://home`, the *Home* pill of the left): the owner called it "the Lico space" and the session a Bot of that space, so only the session's name changes, not the place's.
2. **The order.** The toolbar is **Accounts, New Bot session, New session, Open browser, Theme** (it was Accounts, New session, New Home session, Open browser, Theme). The button id is `new_bot` (it was `new_home`), the picture's name `bot` (it was `house`); the Session menu keeps *New Bot session* as its first entry.
3. **The picture.** `Icon::Bot` replaces `Icon::House`: a robot's head drawn from rectangles like the others (one colour, no font): an antenna with a ball on it, a box for the head with an ear on each side, two square eyes and a mouth cut out of it in the button's colour; one piece of ink, filling the same size as the other pictures.

### C. A time for the lines that have none (the owner's "여전히", the second time)

What `M24-P11b` and `M24-P11h` draw is the time the host **recorded** when a line was written (`M24-P11a`, the sidecar file beside the log). A session that was written before that build has **no recorded time for its old lines**, and a host asked for them answers an empty list: the window then draws nothing at the right, which is what the owner sees on an old session. (A session started with this build shows the exact time of every line: the window test of `M24-P11h` and the one added here run that.)

3. **An approximate time** for a line with no recorded time, from what the file system knows. The host keeps the log of a session in segment files (`<id>.<first sequence>.seg`, `retain / 4` records each); a segment's *modification time* is when its last record was written and its *birth time* (where the file system has one) is about when its first was. The host answers a `stamps` request with two more fields, both **only in addition** to the old ones: `approx`, runs `[sequence, seconds]` of the same shape that cover the lines **before the first recorded run** (a line is placed in its segment by linear interpolation between the previous segment's modification time, or this segment's birth time for the first segment, and this segment's modification time, at most 32 steps per segment); **the segment that holds the first recorded line** was written to after the upgrade, so its modification time says nothing of its old lines: they are placed at its start (the previous segment's modification time, or its own birth), never spread up to a time that moved when the new lines were written, and `exact_from`, the sequence of the first recorded run (`null`: none was recorded). A window of an older build ignores both fields and draws what it drew.
4. **`~` says it is about.** The window draws a time it took from `approx` with a `~` in the column that every gutter keeps empty at its left, so the text of the time does not move and no gutter gets wider: `~2026-11-25 09:41:07`. A line at or after `exact_from` is exact and has no mark. A line whose segment is gone (the log keeps the newest `retain` records) has no time, as now.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The session is the **Bot session**, the place stays **Home** | the owner's words: a Bot that arises in the Lico space; `lico://home` is the name of that space everywhere else (Chats, the Home pill, the path row) |
| D2 | The id of the button is `new_bot` and the picture's name `bot` | the ids are what the tests and the state dump say; keeping `new_home` would keep the old word alive in the state |
| D3 | A robot's head, not a whole robot | the pictures are drawn at 104 pixels for a near-computer-illiterate person: a face reads at once, a body adds nothing |
| D4 | An old line's time is **interpolated** inside its segment and **marked `~`** | a segment's time alone would put 512 lines at one second (hours off); the mark tells the person it is not a record |
| D5 | **No placeholder** for a line with no time at all | a dash in every block is noise; such a line exists only when the log has dropped its segment |
| D6 | The reply fields are additive (`approx`, `exact_from`) | an older window reads the old fields only; the protocol note goes `0.1.17` |
| D7 | **No time is invented** when the file system gives none: a segment with no modification time gives no run | a time is a gift, not a record of truth (`stamps/mod.rs`); a wrong one is worse than none |
| D8 | The old lines of the segment that holds the first recorded line are **flat at its start**, not interpolated up to its (moved) modification time | after a resume the file's last write is the new lines', so a spread would put the end of the old part days late; at its start the error is at most the span of one file |

## Steps

1. [x] **Tests first (B)**: `icons.rs` (the bot is one piece with three holes, narrow antenna over a wide head, fills the size; centred; `ALL`), `ui_tests.rs` (ids, the labels in order), `menu_tests.rs`, `newsession_wizard_tests.rs`, the Python window tests (ids, icon names, titles, the start screen's key line).
2. [x] **The change (B)**: `icons.rs`, `toolbar.rs`, `menu.rs`, `newsession_wizard.rs`, `newsession.rs`, `telemetry.rs`.
3. [x] **Tests first (C)**: host: `stamps/approx.rs` (a log of three segments with set modification times gives the runs: interpolated, monotonic, bounded, none without a time, none at or after `exact_from`, the run that covers `from`), `core` (the reply carries `approx` and `exact_from`); window: `times_tests.rs` (the reply is merged, a line below `exact_from` is approximate, the row's text begins with `~`, a reply without the fields is as before), the dump (`approx` rows), the scroll oracle (the `~` is cleared and drawn with the rest); a real window with a host whose session has no recorded time (the log folder with the sidecar file removed): the `~` rows carry the time of the file.
4. [x] **The change (C)**: `stamps/mod.rs` (`first`), `stamps/approx.rs` (new), `session.rs` (the three fields of `LogView` become `pub(crate)`, no new line: the file is at the ratchet), `core.rs` (`stamps`), `gui/times.rs` (the fields, the mark).
5. [x] **Mutation checks**; the guide (the toolbar, the wizard, the time with its `~`), the protocol note `0.1.17`, the plan's results, the milestone row, the index; the commit (the hook); `~/bin/lico` rebuilt.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| C1 | the toolbar is Accounts, New Bot session, New session, Open browser, Theme; the ids are `accounts`, `new_bot`, `new_session`, `new_browser`, `theme` and the pictures `account`, `bot`, `plus`, `browser`, `moon` | unit test (`ui_tests.rs`), host tests |
| C2 | no menu, title, hint or key line says *Home session*; the wizard titled `New Bot session` has the three steps as before and starts the session with no folder | unit tests, host tests |
| C3 | the bot is one piece of ink with an antenna, two eyes and a mouth cut out, centred, and not the same picture as any other | unit tests (`icons.rs`) |
| C4 | a line with no recorded time has an approximate one, inside its segment, never later than the segment's last write and never earlier than the one before; none when the file system gives none; a line at or after the first recorded run keeps its exact time | unit tests (`stamps/approx.rs`) |
| C5 | the reply of `stamps` carries `approx` and `exact_from` in addition; a window that does not know them draws as before | unit test, protocol test |
| C6 | the window draws `~` before an approximate time and nothing before an exact one; the gutter's width and the text's place do not change | unit tests, scroll oracle, a real window |
| C7 | no middle dot, no local path, no secret in any new text; no file crosses the size ratchet | the guards of the commit |

## Personal data and security

- The approximate time is read from the **modification and birth time of the host's own log files** (`std::fs::metadata` on `<log folder>/<session>.<n>.seg`, the files the host already owns with mode `0600`); no file of the person's is read, no path is sent (the reply carries sequence numbers and seconds only), nothing is written.
- The reply grows by at most `32` runs per segment of the span (a request spans at most `2,000` lines: a few segments), under the answer's existing bound of `2,048` runs.

## Not decided here

A hover that says the full time in the bottom line; a placeholder for a line with no time at all; a time for the lines of a Chats card (those come from the program's own files and have none); whether the `~` should be explained in the window rather than in the guide (it is a single character, the guide says what it means).

## Results

(2026-11-27)

What was made: the list above. Nothing of an earlier phase's behaviour changed except the words and the order of the toolbar (*New Home session* is *New Bot session* and stands before *New session*) and the time column (an old session now has a time with a `~`).

### Tests (against the owner's budget of about 100 unit tests per turn)

- About 17 new unit tests: `icons.rs` (the bot is one piece of ink with an antenna, two eyes, a mouth and an ear on each side, centred; the plus and the bot centred), `ui_tests.rs` (the account icon test now also names the bot), `stamps/approx_tests.rs` (6: the interpolation inside a segment, no time made up, the segment of the first recorded line, the answer read from real files, no recorded time at all, the bound of a reply), `gui/times_tests.rs` (6: the estimate is merged and marked, an older host marks none and a host with nothing recorded marks every line, a recorded run beats an estimate of the same line, the estimate is let go with the session, the tilde drawn before the time with the time not moved, the dump). The tests that said *Home session* or the old order were changed (`menu_tests.rs`, `newsession_wizard_tests.rs`, `newsession_touch_tests.rs`, `chatcard_tests.rs`).
- 2 new host tests: `tests/test_stamps.py` (a log with no recorded times answers an estimate from its own files and says where the recorded ones begin) and `tests/test_stamps_window.py` (a real window over a session whose file of times is gone and whose files were last written five days ago: every row is `~` and about five days ago; after a new turn the new lines show their own second with no mark). The Python window tests of the toolbar and the wizard were changed for the new order, ids and words.
- The unit set of the touched modules (`stamps::`, `gui::times`, `gui::ui::`, `gui::menu::`, `gui::newsession_wizard::`, `gui::chatcard::`, `gui::icons::`, `gui::toolbar`, `telemetry`): green unmutated.

### Mutation checks

- Part B: 14 mutants (the order, the id, the picture, the tip, the menu entry, the wizard title, the start screen's key line, no eyes, one eye, no mouth, no ball, no ears, the stem, the picture's name). Round one left 3: the wizard title and the key line are only said by host tests (rerun with the host tests: killed) and **no ears was a real gap** (the icon test did not look at the sides: an assertion that the head is at least 8 pixels narrower than the widest row was added, killed). 14 killed, none survived.
- Part C: 23 mutants (the six rules of the estimate, the held segment, the caps, `exact_from`, the bound of a reply, `Stamps::first`, the mark, the merge order, the tilde column, the reply of the host, the segment list of the session). Round one left 5, each a missing assertion and not dead code: the cap by the first recorded second (a file written after it), the segment before the first asked for (its last write is where the next begins), the bound of 2,048 runs when the recorded ones fill the reply, `Stamps::first` (a test with two recorded runs), the merge order (a line named in both lists). Each was answered with a test and killed. 23 killed, none survived.
- The scripts and the logs are in the session's scratchpad, not in the repository; the sources are byte-identical to their state before each round.

### Found on the way

- **The first design of the estimate would have been wrong after a resume.** A session that was written before this build and then resumed has a last segment whose last write is the *new* lines': spreading its old lines up to that time would put them days late. The segment that holds the first recorded line is placed **flat at its start** (the write before it, or its own birth), never spread (D8); the error is at most the span of one file. The window test made the case real (a resumed session, a file of times gone).
- A test that resumed a session right after a hard stop met `bad_request`: the host writes its registry (with the program's own session id, which a resume needs) once a second, so a crash right after a turn loses it. The test now waits 2.2 s before it stops the host.
- The by-hand changed-files run of the staged tree: the first run reached the hook's limit of 300 s with no red test (exit 124; six files done in the first 82 s, then the four in flight made no progress while the host's load average was 14 to 16 from other users and the pressure on its disks stood at 20 %; the same four files pass together in 14 s); the second run: **617 passed, 4 skipped, 195.5 s** (over the figure of 180 s, as before; the hook's limit is 300 s), peak 1,051.8 MiB. Nothing was raised or bypassed.
- `cargo xtask check` and `cargo xtask graph` pass (281 modules, the cycle of the hubs still 50, 474 production edges as before; `stamps::approx` does not use `session`); `session.rs` stays at 800 lines, `core.rs` at 917 of 941; clippy flags no line this phase changed.

### What the estimate is, and is not

A line's time is estimated from the modification and birth time of the host's own log files: good to the span of one file (`retain / 4` records), and only where the file system tells. A line in the segment that holds the first recorded line is placed at that segment's start. It is **marked `~`** so that it is never read as a record. A segment whose file is gone has no time of its own.

### Not done here

A look at the real window by the owner (the shape of the bot, the `~` before the time of an old session: the tests draw both, the person's eye is the check); a hover that says the full time; a time for the lines of a Chats card.
