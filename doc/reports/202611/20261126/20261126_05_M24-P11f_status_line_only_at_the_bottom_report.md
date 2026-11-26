# M24-P11f: the status only at the bottom, and a border for the chat

- Version: 1.0.0
- Date: 2026-11-26
- Branch: `phase/M24-P11f-status-line-at-the-bottom` from `phase/M24-P11g-top-rows-level`; code `1ae1b1c`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-26 (quoted in the [plan](../../../plans/202611/20261126/20261126_05_M24-P11f_status_line_only_at_the_bottom_plan.md)): "헷갈리는 점은 대화창 상단에 상태 표시줄이 표현되는데, 이것 때문에 내가 계속 혼란 스러웠던 것 같아. 채팅창 쪽은 뭔가 경계를 분명히 해서 기존 UI 요소와 안 섞이게 해주고" and "상태줄은 제일 밑에만 존재하는 게 맞음."
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test)

| | Before | After |
|---|---|---|
| top of a conversation | the tab bar, then a line of status in the same dark tone (`claude / Work / model / idle / mode / ... / dir / 5h`), then the first line | the tab bar, a rule of 2 pixels in a lighter tone, a margin of 4 pixels, then the first line of the conversation |
| the state (provider, account, idle or working with its clock, mode, account issue) | in the top line | at the left of the strip above the path bar, in front of the model, tokens, quota and folder |
| the host's notice and the one-line messages | in the top line | in the strip, after the state, in a warm colour |
| rows of the conversation | one less (the line held a row) | one more |

- A strip too narrow for all of it takes a shorter form of the state (without the mode, then the state alone; an issue is never left out), so the folder keeps 20 columns at least.
- A **panel** (the start page is one), a **browser tab**, a **terminal tab**, the **Call Room**, and a window without the bars or the workbench keep their line at the top: it is their title or their control.
- The search bar, when open, is the first line of the conversation, with its band.

## 2. How it is built (READ)

- `gui/statusline.rs` holds the numbers (`RULE`, `MARGIN`, `RULE_TONE`, `SEP_COLS`, `CWD_COLS`), the text of the state part and its forms, `pick_form`, and `status_at_bottom`, which says which views have no line at the top.
- `ui.rs` (pinned at 1114 lines by the size ratchet) changes only in joined lines: `chat_top` includes the border, the buffer is built without line 0 when there is no header, `base` counts the header and the search bar, one more row for the transcript, the tab bar is drawn to its own edge and the rule under it.
- `telemetry.rs` draws the state and the notices first and gives the strip's remaining columns to the model, tokens, quota and folder.
- The dump keeps `header` (29 test files and the host's tools read it) and has `telemetry.status_text`, `telemetry.notice_text` and `layout.chat_top`.

## 3. Checks (MEASURED)

- **Tests first**: run red against the old layout, then green.
- **The tests now**: unit: 10 new in `statusline_tests` (the state part and the border's numbers; the forms of the state and the choice of one; which views have no top line; the first line under the border, the rule's rows and tone at three scales, the start page's title and a bare window; the strip's state, the notices and a session that is not listed; the rows with and without the search bar; the two inks as pixels; the strip's fit at exact widths at two scales; a view with a line of its own keeps its rows; a Call Room's strip) and one line added to the terminal tab's test; the window test `tests/test_status_at_bottom_window.py` (the first line at `layout.chat_top`, the rule across the width and lighter than the bar, the state in the strip, the header's text in the dump); no old test needed a change. The whole binary: 1,361 unit tests pass (12 ignored); the changed-files suite 281 tests in 88 files
- **Mutation checks**: **70 mutants** in the first pass (`statusline.rs`: the rule's height, margin and tone, the border's height with and without the bars, the rule's rows, the words of the mode, the text of the state, the issue and the forms, `pick_form`, the columns of the separator and of the folder, each view that keeps its line, the list of forms and of notices, the rule drawn without bars; `ui.rs`: the border in `chat_top`, `has_header` in four forms, the row count in three, the text and the search bar, `base`, the wanted lines, the search band's line, the guard on `recs`, the tab bar's edge, the rule's place and call, the dump; `telemetry.rs`: the strip's state, the room it leaves, the notices, the unlisted session, the dump keys), each one edit on a restored tree whose files were checked by their sha256 after each; the unit tests of `statusline`, the strip, the terminal tab, the top rows, the blocks, the times, the scroll and the search run against each. **51 killed, 17 survived, 2 did not compile.** The survivors showed what the first tests did not pin: the strip's fit was only tested through the constants it used (the separator's 5 columns, the folder's 20, the room, the notices' columns, the margins), the rows of a conversation were only tested as flush with the prompt (not counted), the first line of the search bar could have the status in front of it, the rule's tone was read through its own constant, a window without bars could draw the rule, and a Call Room's strip could say the state. The fit became one pure function (`strip_form`) with the separator as one constant (`SEP`, its width tested against its number), and tests read the exact boundaries of the fit at two scales, the rows (39 with a conversation, 38 with the search bar, 45 with a header line), the tone as a literal, the two inks as pixels, a bare window, a Call Room's strip and a session that is not listed (no folder). **The second pass (26 mutants, the survivors on the new function and the ink constants): 26 killed**; the Call Room mutant and the unlisted-session mutant were killed too, and the dump's `chat_top` shifted by a pixel was killed by the window test `tests/test_status_at_bottom_window.py`. **One survivor is written down, not closed**: dropping the guard on `recs[0]` in `ui.rs` changes nothing that is drawn (when no line of status stands in front, the first line of the transcript has its extent measured again each frame instead of being used as it is; the list is never empty, the prompt is always in it); the guard stays because it saves the measure
- **The commit hook**: one run, accepted. The suite was run by hand first on the staged tree (281 tests in 88 files, all passed, 143.6 s, peak 1005.7 MiB of the workers, the build's peak **1403.5 MiB, under the bound of 1536 MiB**), so the hook found 88 files from receipts and had nothing to run. An earlier run of the changed files, before the state took forms by width, **failed one window test** (`tests/test_session_tabs_and_telemetry.py`: the long state squeezed out the folder, `cwd_rect` was none); the forms and the 20 columns the folder keeps fixed it, the test is unchanged. The bounds (180 s, 1,536 MiB, 300 s) stay, never raised or bypassed

## 4. What is not decided (the owner's)

- A terminal tab, a browser tab and the Call Room keep their line at the top (it is their title or their control); if you want them to say their state at the bottom as well, it is a later phase.
- On a narrow window the state takes a shorter form first (the mode, then the provider and account), then the folder is elided; the full sentence stays in the dump's `header`.
- The permission mode is two words in the strip (`auto`, `asks`).
