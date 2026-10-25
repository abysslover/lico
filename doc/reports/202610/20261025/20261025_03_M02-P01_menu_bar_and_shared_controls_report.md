# M02-P01 report — a menu bar and the small shared control set

- Status: in_review (inspection verdict pending; the owner's try of the build is still to come)
- Date: 2026-10-25
- Plan: [M02-P01](../../../plans/202610/20261025/20261025_02_M02-P01_menu_bar_and_shared_controls_plan.md)
- Branch: `phase/M02-P01-menu-bar-and-controls`; inspection bundle `private/manual_inspection/202610/20261025/007_M02-P01_menu_bar/` (closed bar, File menu open, Session menu with greyed entries, Help > Keys; red and mutation logs)
- Gates: the owner's decision of 2026-10-25 (option A: hand-drawn client, small shared controls); G3 and G4 pending.

## What was built

- **The bar** under the window top: File (New session, Accounts, Quit), Session (Next, Previous, Start screen, one entry per listed session with Alt+1 to Alt+9, Allow and Deny the pending approval, Copy selected lines, Paste), View (Older, Newer, Newest output), Help (Keys, About Lico). Each entry shows its key. An entry that cannot act now is greyed and says why in brackets
  ("needs two sessions", "no session is open", "no approval is pending", "nothing is selected", "no text line to paste into", "close the open panel first"); a click on one closes the menu and shows the reason on the first row. The first row (provider, account, model, state, mode) was kept: no status bar was added.
- **Keyboard:** F10 or Alt+F, Alt+S, Alt+V, Alt+H open a menu; Left and Right change menu; Up and Down move over the entries that can act; Enter runs; Esc closes; Ctrl+Q quits (the host keeps running). **Mouse:** a click on a title opens it (again closes it), a click on an entry runs it, a click elsewhere closes it.
- **The shared control set** (`gui/controls.rs`): shaped text labels (cached), buttons, list rows, and one list of hit boxes that is rebuilt with every frame, so a box drawn is the box a click is tested against. Used by the bar and its entries, the Allow / Deny buttons, the session list (headings and rows) and the hint line; the old list text buffer, the button buffers and the separate
  rectangles are gone. The prompt row and the transcript stay in the main text buffer (they are text lines of a different kind).
- **Where:** the product window always; the measurement client only with `--menu` (`lico_measure run --ui --menu`), so the earlier pixel runs keep their layout. The window moves down by one bar row only when the bar is shown.
- No new crate; no new host operation.

## Tests and evidence

5 new tests (`tests/test_menu_bar.py`): the closed bar (four titles, inside the window, not overlapping, the first letter whole, drawn); F10 and Alt+letters, arrows and Enter, greyed reasons (also on pixels), Quit; mouse (open, run, a greyed entry only says why, a click elsewhere closes); with three sessions the menu switches sessions and Allow / Deny answer a real pending
approval (exact reply checked); at scale 2 every box stays inside the window with the 4-logical-pixel margins, clear of the bar. Red before: `KeyError: 'menu'` (five times). Mutation (a greyed entry runs anyway): the click test fails (`'needs two sessions' in 'select a session'`). The product-corner test was updated: the corner now carries the bar, so it checks that no solid marker squares are there.
The sign-in code is still drawn as stars (its tests are unchanged and green). Whole suite: see the final line below.

## Measurement (Xvfb, 10 accounts, 10000-line transcripts, `--ui --menu`, commit 94ed077)

| Target | Scale 1 | Scale 2 | Limit |
|---|---|---|---|
| Idle RSS of the client | 22.76 MiB | 33.93 MiB | 50 MiB |
| Hidden growth | 0.29 MiB | 0.18 MiB | 10 MiB |
| Activation p95 (n=100) | 65.99 ms | 68.49 ms | 100 ms |
| Restart p95 (n=100) | 117.33 ms | 130.80 ms | 500 ms |
| Idle CPU (n=1) | 0.00 % | 0.02 % | 0.5 % |
| Noisy case CPU | 13.8 % | 18.5 % | none set |

Every target is met that had a limit. Two numbers looked worse than in the P06 and P07 reports (the noisy case about 1 and 2 % of a core, the restart p95 about 64 and 72 ms). The guess first written here (labels drawn on every frame, a cold label cache after a start) was wrong: the causes were measured afterwards and are in the next section.

## Frame cost in the noisy case and the restart (task 013, commits 39bd2af and 7e0bedc)

**Measured cause of the noisy case.** The cost came in with P07-S01 (commit eeae7e2), not with the bar: 1.07 % of a core at 93bbb6a, 12.6 % at eeae7e2, 13 to 14 % at the head of that time; with the bar (`--ui --menu`) 13.65 %, without it 14.10 %. The about 1 % in the P06 report belongs to 93bbb6a and probably under-drew the new lines. One frame (10 fps) took 12.9 ms, 10.9 ms of it layout: every new transcript line
is shaped once (unchanged lines were already kept by their text), and shaping with the family `monospace` goes through all monospace faces for each word and scans all installed faces again for each word whose characters the first font lacks (callgrind: 99.8 % of `draw_ui`, in cosmic-text's font fallback and per-word shape plan). Paint was 2.0 ms, present about nothing.

**What changed (same pixels).** (1) The first monospace face is named once and lines are shaped with that name; faces that share family, weight, style and stretch get distinct names at start; the choice is checked at start (the name must give the same glyphs as `monospace` for printable ASCII). (2) A character that face lacks is looked up once, remembered per character and checked once by name; text that needs the old search (combining marks, joiners, variation
selectors, right-to-left joining scripts, Indic, Thai, Jamo, flags) keeps the explicit `monospace` family over the whole cluster. (3) A glyph's bitmap is blended row by row instead of through a callback per pixel. 10 fps pacing, the line text cache and the control path are unchanged. `--plain-fonts` (measurement client only) turns (1) and (2) off so one binary can be compared with itself.
The dump has a `frame` entry (frames, microseconds for layout, paint and present, lines shaped, characters probed, label shapes). A limit of 5 % of a core (state n) for the noisy case is now in the measurement set.

| Commit 7e0bedc, `--ui --menu`, 10 accounts | Scale 1 | Scale 2 | Limit |
|---|---|---|---|
| Noisy case CPU (before: 94ed077) | **3.70 %** (13.8 %) | **6.35 %, missed** (18.5 %) | 5 % |
| Noisy frame at scale 1 / 2 | layout 1.4 + fill 0.2 + blit 1.1 ms | layout 1.4 + fill 1.0 + blit 3.4 ms | |
| Restart p95 (n=100) | 116.42 ms | 118.86 ms | 500 ms |
| Activation p95 (n=100) | 62.66 ms | 64.00 ms | 100 ms |
| Idle RSS / hidden growth | 23.33 / 0.35 MiB | 34.44 / 0.59 MiB | 50 / 10 MiB |
| Idle CPU (n=1) | 0.02 % | 0.02 % | 0.5 % |

**Not met: the noisy case at scale 2 (6.35 % against 5 %).** What is left is real work: a new frame of 4 times the pixels every 100 ms, with 10 new lines shaped per frame (1.4 ms, the same at both scales), a 1.0 ms background fill and 3.4 ms of glyph blending. In this case the lines move every frame, so a cached image or damage tracking of the unchanged transcript would not help; the next levers are a design change (keep the previous frame and scroll it,
or paint fewer pixels) and are for the lead to decide. The step stopped here on purpose.

**Restart.** The restart p95 of 117 and 131 ms was not caused by the new bar or by the change above. Measured with the frame counters (12 to 15 restarts per row, medians): 83 to 90 ms to the first correct frame; the frame work of the first frames is 46 to 50 ms (layout 35 ms, shaping about 41 to 44 lines and probing 24 characters in a cold process, and paint 13 ms for the glyphs
rasterised for the first time), the other 40 ms are the process start, the font scan and the connection. The bar adds 0 to 5 ms (inside the noise); `--plain-fonts` (the old search) takes 104 ms median, so the font change makes the restart a little faster. Against the build at the end of P07 (3220977, built separately and measured with the same harness on the same machine state, `--ui`, n=100): restart p95 114.2 ms there and 111.4 ms now.
So the 64 and 72 ms figures belong to a quieter machine (load average about 10 now); there is no restart regression to repair.

**Pixels.** A test compares a window of Korean, ideograph, Hebrew, emoji, combining-accent, symbol and ASCII lines in one binary with and without `--plain-fonts`: identical pixels. A unit test compares the row blit with the old callback blit on Korean, CJK, Hebrew, emoji, combining and ASCII lines at three offsets and clips: identical. No differing class is known; clusters are left to the old search by construction.

**Tests.** `tests/test_frame_cost.py` (3): an unchanged frame shapes no line, probes no character and shapes no label; the pixel comparison; the limit logic. Mutations: bypassing the line cache fails the first, an unclipped right edge fails the blit test. Whole suite 109 passed, the Rust unit tests pass, the pre-commit hook passed (`{"staged_count": 1, "candidate_count": 0, "errors": [], "pass": true}`).

## Limits and not done

Close and rename session (rename needs a host operation that does not exist; close is a purge and needs a confirmation design); menu letters are not underlined; no sub-menus; the bar is not scrollable on very narrow windows (titles keep their place, an open menu is moved to stay inside); the Help texts are plain panels; Wayland and accessibility remain unverified.
Whole suite on 94ed077: 106 passed in 415.46s (0:06:55). Whole suite on 7e0bedc: 109 passed in 428.98s (0:07:08).
