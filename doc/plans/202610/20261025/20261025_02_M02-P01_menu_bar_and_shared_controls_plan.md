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
