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

Results (2026-10-25, commits 39bd2af and 7e0bedc): the noisy case is 3.70 % of a core at scale 1 (limit 5 %, met) and 6.35 % at scale 2 (missed); it was 13.8 and 18.5 %. The blit was made row by row after the font change (scale 2: 9.7 % to 6.3 %); a frame at scale 2 is now layout 1.4 + fill 1.0 + blit 3.4 ms, and the lines move every frame, so a cached image would not help: the next lever is a design change for the lead. Pixels equal (one binary with and without `--plain-fonts`; the row blit against the old blit).
Restart: measured with the frame counters, not a regression: the end of P07 measures 114.2 ms p95 on today's machine state, the head 111.4 ms (the first frames cost 46 to 50 ms in a cold process, the rest is start and connection); the 64 and 72 ms of the P07 report were a quieter machine. Whole suite 109 passed. Details in the report.

## S04 — The window brings the host to its own build and keeps the sessions (tasks 019 and 014, 2026-10-25)

Today (`gui/mod.rs reconcile_host`): a host of another build is replaced only when it holds no session and no sign-in; with sessions the window only says "close it with `lico host stop`", and a session that went `disconnected` cannot be brought back from the window (only `lico session resume ID`). Facts from the code: the registry (`registry.rs`) keeps provider, account, provider session id, generation and mode of every provider session in one file and the next host brings them back `disconnected` with their history and sequence; `session.resume` (`core::session_resume`) takes the current generation, makes it +1 and starts the provider again
(codex `thread/resume`, claude `--resume ID`); a session without a provider session id yet answers `bad_request`; synthetic sessions are not in the registry. `host.stop` is refused with sessions unless `force`.

Options weighed (the owner decided on 2026-10-25 20:5x, so only the last is built): (A) ask in the window "restart the host now?" - an extra question nobody wants; (B) replace automatically only when every session is idle, otherwise ask - still a question; (C) a hand-over from the old host to a new one (the new adopts the old one's sessions and provider processes) - the most work and a new wire between two builds, which is the very thing that differs;
**(D, decided) the host's build hash differs -> the window replaces the host by itself, always, and resumes what was alive.** It uses what exists: registry, `host.stop`, `session.resume`; no new wire except one optional field.

Design.
1. **Where.** `lico gui` without `--socket` (the product). The first check is quick and before the window: same build -> nothing. Another build -> the window opens at once and a worker thread does the rest, so a waiting turn never keeps the window from showing; it reports to the loop with an event (header line, then "the host is a new one": the loop connects to the new socket, starts a reader for it, clears its subscription and shows the open session again).
2. **Settling.** Nothing is cut off silently: a session that works (`activity` working) or waits on an approval is waited for, polling `status` every half second, at most 60 s (the header says "Updating the host after the running turn..."); then the host is restarted anyway and the line says how many turns were interrupted. A sign-in in progress (`logins_running`) is never ended: wait for its end or 5 minutes, then the host is left as it is and the line says so ("...not restarted: a sign-in was still in progress"). The waits are shortened by an environment variable for the tests only.
3. **Stopping.** `host.stop` with `force` and a new optional `build` (stop only if the host still has this build: a compare-and-stop through the socket, so two windows cannot kill the host that the other just started; the answer is `build_changed`). An old host that rejects the field is asked again without it after a second status check; a host too old for `host.stop` (`unknown_op`) is ended once by SIGTERM on the pid its own `status` named (never by process name),
then the socket is waited for to vanish (3 s); if it does not, the line says which process is left.
4. **Starting and resuming.** `ensure_host()` starts the new host (the same call as at the first start). The sessions that were alive in the old host (provider sessions in state `running`) are resumed one by one with the generation the new host reports (+1), in the worker thread. A session that was `disconnected` or `exited` before stays so. The header line: "Restarted the host (new build); resumed N sessions." and, for the ones that did not come back, "...; M could not be resumed (reason)". Never hidden.
5. **Two windows.** The second window sees either the old host (its `host.stop` carries the build it saw: the first one's stop wins, the second gets `build_changed` or no host, looks again and finds the new build, and does nothing more) or the new one. The resuming is done only by the window that stopped. A test starts two windows against one old host.
6. **Resume from the window (task 019 item 1).** Session menu: "Resume session" (Ctrl+R), greyed with a reason ("no session is open", "the session is running", "the session has ended", "the host does not know a conversation to resume yet" for the `bad_request` case); uses `session.resume` with the row's generation; the list shows the new state at the next tick. The same key works from the keyboard, the entry from the bar.
7. **014 inside it.** The "run `lico host stop`" line is shown only when the replacement could not be done, with the reason (a sign-in in progress, the old process did not end). A `bad_request` to `session.create` becomes "The host does not understand this request: it is an older build." (a defensive message); an old host without a build id counts as old (already so: the ids differ).

Tests (red first). A fake codex (the simulator of the codex adapter tests behind a `codex` script on the PATH of host and window) gives real provider sessions on an older-build host (a copy with one byte more). (a) Two sessions, idle: after `lico gui` starts the host is the new build, both sessions are running under generation +1, the header reads "Restarted the host (new build); resumed 2 sessions." and an input to each is answered (history and sequence continue). (b) A session before that was disconnected stays disconnected. (c) A running turn is waited for (with the wait shortened), and one that does not end is cut after the limit and counted in the line.
(d) A sign-in in progress keeps the host and says so. (e) An old host without `host.stop` is ended by pid and replaced. (f) Two windows at once: one host, one set of resumes, no double kill. (g) The menu entry and Ctrl+R resume a disconnected session (greyed otherwise, with the reason). (h) `session.create` answered `bad_request` shows the sentence. The S06 stale-host tests stay green (their expectations change to the new line where they now describe the old behaviour).
Measured, not read from the docs: what resume restores with the real `codex` and `claude` (context kept? a turn that was running? an approval that was waiting?) - one small experiment per CLI on this machine, stated in the report.
