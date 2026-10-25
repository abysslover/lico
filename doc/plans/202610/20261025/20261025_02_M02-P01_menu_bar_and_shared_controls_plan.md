# M02-P01 — A menu bar and the small shared control set (2026-10-25)

> Status: in_progress
> Branch: `phase/M02-P01-menu-bar-and-controls` (from the M01-P07 tip)
> Approval: the owner's decision of 2026-10-25 (UI toolkit option A: keep the hand-drawn client and build a small shared control set; the menu bar comes next). G3 and G4 are pending.

## Goal

One sentence: the window gets a classic top menu bar (File, Session, View, Help) whose entries are exactly the actions that exist today, each showing its key, reachable by keyboard and mouse, drawn by one small shared control set that the Allow / Deny buttons, the session list and the hint line use too.

## Scope

1. **Menus.** File: New session (Ctrl+N), Accounts (Ctrl+A), Quit (Ctrl+Q). Session: Next / Previous session (Ctrl+Tab, Ctrl+Shift+Tab), Start screen (Esc), one item per listed session (Alt+1 to Alt+9), Allow / Deny the pending approval (Y, N), Copy selected lines (Ctrl+Shift+C), Paste (Ctrl+V).
   View: Older output (Page Up), Newer output (Page Down), Newest (End). Help: Keys, About. Nothing is listed that does not work: an entry that cannot act now is greyed with its reason written beside it ("needs two sessions", "nothing is selected", "no approval is pending").
   The first-row text (provider, account, model, state, mode) stays as it is: a status bar was not needed.
2. **Keyboard.** F10 opens the bar, Alt+F, Alt+S, Alt+V, Alt+H open a menu, Left / Right change menu, Up / Down move over the entries that can act, Enter runs, Esc closes; Ctrl+Q quits. **Mouse:** a click on a title opens it, a click on an entry runs it, a click elsewhere closes it; every hit box is measured and kept inside the window margins.
3. **Shared controls** (`gui/controls.rs`): one drawing and hit-test module for labels, buttons and list rows (the bar, the entries, the Allow / Deny buttons, the session list, the hint line); no second drawing path for these. The prompt row stays in the transcript text. No new crate; the idle client stays under 50 MiB and the activation under 100 ms.
4. **Where it shows:** the product window always; the measurement client only with `--menu` (so the earlier pixel measurements keep their layout), and the final measurement is repeated with `--menu`.
5. Small follow-ups that fit: none planned (close and rename session need host operations that do not exist yet for rename; close is a purge and needs a confirmation design). Not started: M01-P08.

## Tests (red first, pixels and the ui dump)

Closed bar drawn and not clipped at the left; F10 and Alt+letter open menus (dump: open menu, items, keys, enabled and reasons); arrows and Enter run New session and Accounts; Esc closes; a click on a title and on an entry works and a click on a greyed entry shows its reason and does nothing;
with sessions: next / previous and a session item switch sessions, Allow / Deny are greyed without an approval and answer with one; Ctrl+Q ends the window; at scale 1 and 2 every hit box lies inside the window and no title boxes overlap; the sign-in code is still never drawn; whole suite.

## Measurement and risks

Re-measure idle RSS, hidden growth, activation and restart p95 with `--menu` at scale 1 and 2 against the same limits. Risk: moving the text area down by one bar row changes every y coordinate (click boxes, selection, approval buttons); the dump carries window coordinates so the older tests keep reading them.

## Progress

- 2026-10-25: Plan written; steps follow.
- 2026-10-25: Done. The guard first refused the branch name (a mailbox note went to the lead); the prefix was allowed and the work committed unchanged (0460de4). The bar (File, Session, View, Help), the entries with their keys and greyed reasons, F10 / Alt+letter /
  arrows / Enter / Esc, mouse, Ctrl+Q, Help panels (Keys, About) and `gui/controls.rs` (labels, buttons, list rows) used by the bar, the entries, Allow / Deny, the session list and the hint line. Tests: 5 new (`tests/test_menu_bar.py`); red `KeyError: 'menu'` x5;
  a mutation (a greyed entry runs anyway) fails the click test; the whole suite is green. The measurement with `--menu` at both scales met every target (see the report). Not done: close and rename session (no host operation for rename; close is a purge that needs a confirmation design).

## S03 — The frame cost in the noisy case (task 013, 2026-10-25)

Measured first (Xvfb, 10 accounts, one session printing 100 mixed-script lines/s, `--ui`): the noisy case costs about 13 % of a core since P07-S01 (commit eeae7e2; 1.07 % at 93bbb6a), with and without the menu bar (`--ui` 14.10 %, `--ui --menu` 13.65 %), so the bar and the label drawing are not the cause. One frame (10 fps) is 12.9 ms: layout 10.9 ms (the 10 new lines, about 1.1 ms each, are shaped once;
unchanged lines are already kept by text), paint 2.0 ms, present about 0. Callgrind: 99.8 % of `draw_ui` is cosmic-text shaping; the font search (`FontFallbackIter`, a scan over the installed faces and every monospace face for each word whose characters the default font lacks) and the per-word shape plan are the two parts.
A bench outside the product (private) gives per line: 250 to 850 us for ASCII, 500 to 1900 us with Korean / ideographs, about 110 us if one named font is forced (but then Korean and ideographs come from other fonts).

Design (same pixels, less search; amended after the first build, 2026-10-25): (1) the first monospace face is named once and every line is shaped with that name instead of the family `monospace` (checked at start: the name must give the same glyphs as `monospace` for all printable ASCII, else the old family stays). The family `monospace` made every word go through all monospace faces; with the name, an Advanced line of ASCII costs about 12 us instead of about 240 us in the bench.
(2) A character that face lacks is shaped alone once with `monospace`, the face it lands on is remembered per character (a face is addressed by family name, weight, style and stretch, so faces that share all four get distinct family names once at start), checked once with the name, and the line gets one attribute span per run of characters on another face. A character that does not check out keeps the old search.
(3) Left to the old search, by an explicit `monospace` span over the whole cluster: combining marks, joiners, variation selectors and skin tones with the character before them, right-to-left joining scripts, Indic, Thai, Jamo, flags. So the pixels are the old ones by construction, and the lookup is only used where it is a lookup per character.
The first draft also gave `Shaping::Basic` to simple lines; the bench showed the named face makes Advanced as cheap (workload 57 us a line either way), so Basic was dropped: no ligature or right-to-left risk to check.
Not changed: 10 fps pacing, the line text cache, one control path. A switch `--plain-fonts` (measurement client only) turns it off, so a test can compare pixels in one binary.

Tests (red first): the dump gets a `frame` entry (frames, microseconds per part, lines shaped, characters probed, label shapes). (a) An unchanged frame shapes no line and probes no character (mutation: bypass the line cache, the test fails). (b) Pixels: a window with Korean, ideograph, Hebrew, emoji, combining-accent, symbol and ASCII lines is the same with and without `--plain-fonts`, or the test names the differing lines.
(c) The noisy case, measured by the harness with a limit of 5 % of a core (state n). Restart p95 is measured with the same counters before any claim about its cause.
