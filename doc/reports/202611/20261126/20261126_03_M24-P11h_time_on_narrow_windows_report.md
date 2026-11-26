# M24-P11h: the time beside each block also on a narrower window

- Version: 1.0.0
- Date: 2026-11-26
- Branch: `phase/M24-P11h-time-on-narrow-windows` from `phase/M24-P11d-theme-button`; code `1ed6542`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-26 (quoted in the [plan](../../../plans/202611/20261126/20261126_03_M24-P11h_time_on_narrow_windows_plan.md)): "여전히 오른쪽에 각 대화의 년월일시분초 표시가 없음."
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test)

The time of each block was drawn only when the text area held 101 columns, so a window that is not wide (half a screen, a tiled window, the list on the left kept open) showed none: that is what the owner saw. Now the text keeps **60 columns at least** and the time takes the longest of three forms that leaves them:

| The text area holds | The time | Kept at the right |
|---|---|---|
| 81 columns or more | `2026-11-26 14:03:09` (full) | 21 columns |
| 76 to 80 | `11-26 14:03:09` (the year dropped) | 16 columns |
| 70 to 75 | `14:03:09` (the date dropped) | 10 columns |
| under 70 | none | none |

The seconds are never dropped. A window wide enough for the long form shows what it showed (101 columns or more: no change; 81 to 100 now show the full form too).

## 2. How it is built (READ)

- One place decides the form: `blocks::form_for(text_w, col_px)`; `blocks::gutter_px` is the pixels of that form's gutter; `stamp_form` writes the text; `stamp_text` stays the full form. The bands, the room kept at the right, the clearing after a scroll step and the drawing keep taking the same two numbers (`gutter`, `col_px`), and `Form::of_gutter` tells the form from the gutter that was kept, so `ui.rs` (pinned at 1114 lines) is untouched.
- The state dump's `times` has `form` next to `gutter` and `drawn`.
- The module graph is unchanged; the size ratchet holds.

## 3. Checks (MEASURED)

- **Tests first**: run red on stubs (nine failed), then green.
- **The tests now**: unit: 9 new (`blocks` 4: the three texts, the rule at each edge in columns and in pixels at 10 and at 8.5 pixels a column, the gutter in whole pixels, a form told from the gutter that was kept; `times` 3: the form that is drawn at seven widths, the short and clock texts of the rows, a new session keeping the form; the scroll oracle 2: each form against a full repaint at the scales 1, 1.25 and 2 in dark and in light, and across a resize), one old test (the 101-column rule) replaced, the numbers of others changed (the room of a window of 70 columns); the window test `tests/test_stamps_window.py` has one more test that resizes a real window to the width of each form and of none. The whole binary: 1,346 unit tests pass (12 ignored)
- **Mutation checks**: **46 mutants** of the new code (`blocks.rs`: the constants, the characters and the air of each form, the words, the order of the forms, `px`, `of_gutter`, `form_for`, `gutter_px`, the texts of the three forms; `times.rs`: the form told from the gutter, the form's text, the offset, the dump, the reset on a new session), each one edit on a restored tree whose files were checked by their sha256 after each; the unit tests of `blocks`, `times` and the scroll oracle run against each. The first run: 41 killed, 5 survived (one more did not compile, a match arm made unreachable, redone). The survivors: **`px` rounded instead of rounded up** (no test had a column whose 21 columns end under a half pixel: now `gutter_px(1000, 10.2) == 215` and `gutter_px(780, 10.2) == 164`), **the form dropped when a new session starts the range again** (a test of its own: the dump keeps the form and the gutter the last frame drew until the next frame), **the short form with the year** (my mutant added `0 * year` to the seconds, an identity; redone as the real edit, the form of the year in front: killed), and the redone arm (killed). The second run of these four: 4 of 4 killed. **Two are written down, not killed, as equivalent**: `form_for` with the guard on a column that is no number (`col_px <= 0.0` against `!(col_px > 0.0)`: a column that is not a number gives 0 columns by the cast, and no form either way), and the full form's year written with two digits instead of four (`{:02}` against `{:04}`: no unix time of a person's life has a year under 1,000)
- **The commit hook**: the first run passed the suite (277 tests in 85 files, 154.8 s) and was **refused on the build's peak memory, 1937.8 MiB over the bound of 1536 MiB**, as the runs of P09 to P11d were; the bound stays (never raised or bypassed), the second run, which reuses the receipts, passed (277 passed, 140.7 s, peak 893.1 MiB of the workers) and accepted the commit

## 4. What is not decided (the owner's)

- A window under 70 columns of text still shows no time (the time would cost the text its room); the person widens the window or closes the list on the left.
- The short and clock forms lose the year, or the date, of the time. A hover that says the full time in the bottom line is a small later phase if the owner wants it.
