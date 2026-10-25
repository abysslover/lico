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

Every target is met. Two numbers got worse and are stated plainly: the noisy case went from about 1 and 2 % of a core (P07) to 13.8 and 18.5 %, and the restart p95 from about 64 and 72 ms to 117 and 131 ms. Probable causes (not profiled): the new boxes (bar, list rows, hint) are drawn label by label on every frame instead of from one buffer, and a restarted client shapes its labels again
(the cache is empty after a start). No limit is set for the noisy case; if it matters a cached image of the unchanged parts is the next step.

## Limits and not done

Close and rename session (rename needs a host operation that does not exist; close is a purge and needs a confirmation design); menu letters are not underlined; no sub-menus; the bar is not scrollable on very narrow windows (titles keep their place, an open menu is moved to stay inside); the Help texts are plain panels; Wayland and accessibility remain unverified.
Whole suite on 94ed077: 106 passed in 415.46s (0:06:55).
