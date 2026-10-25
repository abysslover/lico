# M01-P07 report — the window's first view and key input

- Status: in_review (inspection verdict pending; the owner's look at the real display is still to come)
- Date: 2026-10-25
- Plan: [M01-P07](../../../plans/202610/20261025/20261025_01_M01-P07_window_first_view_and_keys_plan.md)
- Branch: `phase/M01-P07-window-first-view-and-keys`
- Inspection bundle: `private/manual_inspection/202610/20261025/006_M01-P07_window_first_view_and_keys/` (three captures: start, after Ctrl+N, after Ctrl+A; `captures.json`)
- Gates: G2 approved by the owner in the chat of 2026-10-25; G3 and G4 pending.

## What was wrong and what was done

- **Cause.** Lines put into the text buffer by hand were not all laid out when new lines stood in front of a reused one; the layout stops at the first
  unlaid line, so nothing was drawn. Every panel (Ctrl+N, Ctrl+A) replaced all lines but the input line, so the whole text area went empty while the window's model
  still listed the panel. Empty body lines were checked and are not the cause (a one-space spacer gave the same blank result). Fix: every line of the screen is laid out
  in the one place that builds the text. Earlier tests read the model, not the pixels; the new ones read the pixels.
- **The black square** at the top left is a pair of 16-pixel measurement markers (session colour, sequence colour) that the pixel-latency harness reads;
  with no session selected the session colour is black. The product window no longer draws them; the measurement client does.
- **Closed standard input.** The window quit when its standard input closed (a launcher or `nohup` does that). The product window now keeps running; the measurement client still ends with its input.
- **Tests** (3, `tests/test_window_keys.py`, on a private virtual display, looking at pixels): the first screen, Ctrl+N and Ctrl+A each show text rows; the corner carries no marker;
  a closed standard input keeps the window. Red before the fix, as logged: `assert 0 >= 4` (text rows after Ctrl+N), `assert False = corner_is_background()`, the window gone with its input closed.
  Green after: 3 passed; the whole suite 84 passed in 310.9 s.
- **Rig.** The test rig finds a host through the window's own socket and ends it by its own pid, never by name; its virtual display is stopped on every path and at interpreter exit.

## S02: Backspace and Ctrl+U in the input line (found by the owner)

The input line only grew: no Backspace handling existed. Now Backspace removes the last character with what is attached to it (combining marks, joiners, variation selectors, a decomposed Hangul
syllable) when no input-method composition is open (an open one belongs to the method, whose Backspace edits it); Ctrl+U clears the line; Backspace repeats in text entry (the panel's
repeat filter, meant for confirmations, no longer applies there); nothing typed is echoed. Tests: 3 window tests on pixels (`ab` then Backspace gives exactly the pixels of `a`; Ctrl+U and a held
Backspace; real ibus Hangul: a committed syllable goes in one press and a Backspace during a composition leaves committed text alone), 1 Rust unit test for the edit, and the P06 sign-in
code test now types a typo and corrects it with Backspace. Red before: `assert 'ab' == 'a'`, `assert ('abcu' == '')`. Suite: 87 passed. Not covered: the guard "no composition open" cannot be
tested end to end (the method consumes the key itself). Follow-ups, not done: the Left, Right, Home and End keys, selection, Ctrl+Backspace word delete, marks of scripts other than the ones listed.

## S03: paste into the input line, and the sign-in address opens by itself (owner request)

Paste (X11 only, no new crate, no helper program): Ctrl+V and Ctrl+Shift+V paste the clipboard, Shift+Insert the primary selection, only into a text-entry line (session input, account label, sign-in code),
only on the person's own key, never during an input-method composition. The selection is requested through a helper window of our own on a connection of its own, off the event loop, with a 500 ms deadline;
an incremental transfer or more than 8 KiB is refused with a notice (nothing is cut), a newline ends the paste (a trailing one is dropped), control characters are removed, no owner gives a notice. A pasted code is never
echoed, never submitted (Enter stays the person's). Auto-open reverses the P06 rule (written in the plan, owner request 2026-10-25): once per attempt, Ctrl+O again on request, a missing opener shows the notice and keeps the address
on screen; `LICO_NO_AUTO_OPEN` switches it off for tests. Tests (3, a real client with a fake `xdg-open` that records its argument and a real selection held by `xclip`): a pasted canary code is accepted like a typed one and appears nowhere;
the other keys and the refusals (9 KiB, newline, trailing newline, no owner, primary) and a visible label line with Backspace after; the browser is asked once, again on Ctrl+O, again for a new attempt, never with the code, and a
missing opener says so. Red before (previous build): the three failed; green after: 3 passed in 27.82 s; the whole suite once showed one failure of the restart-recovery test that did not repeat (it passed alone and with its
neighbours: 11 passed); idle client RSS at scale 1: 22.82 MiB (limit 50), host 6.06 MiB. Limit: a failing opener (it starts but exits with an error) is not noticed. Cost of the step about 2.5 US dollars against the cap of 3.

## Mistakes of mine, plainly

- While reproducing I stopped a host by name (`pgrep -f` of a host command line), which stopped the owner's own running host (pid 878470). Sessions registered with it come back
  as `disconnected` after a start and resume on request; nothing else was touched. The script that did it is gone; the rig now cannot do it.
- The earlier window tests (P06) passed while the window drew nothing, because they only read the model.
- The Xvfb processes left over on the machine (`Xvfb :99 -screen 0 1024x700x24`, 14 of them, and one 1920x1080) are not started by this rig (it starts `-displayfd` displays of 1200x800 and stops them); none of ours is left.
  They are only reported.

## Not reproduced

Wayland: no compositor is installed here, so the owner's session type could not be tried. On the owner's display the fix is verified by the steps below.

## Menu bar: a judgement, not built

A classic top menu (File, Session, Account, Help) is possible in this window but it is new drawing code, not a setting: a strip of text drawn with the existing text buffers, hit boxes for the labels,
a dropdown list for the open menu, Alt+letter and arrow keys, and each entry bound to an action that exists (new session, accounts, sign in, licenses, quit). Estimate: about 400 to 600 lines of Rust and
three tests (pixels of the bar, click opens and chooses, keyboard), roughly 8 to 12 US dollars with one review; a native toolkit menu would bring a toolkit (tens of MiB, against the 50 MiB limit and
the one-file rule) and is not recommended. Effect on the limits: memory a few hundred KiB (the idle client is 23 and 34 MiB against 50), switching latency near zero because unchanged text lines are
reused (70 ms against 100 ms; the dropdown costs a frame only while open), idle CPU unchanged. Recommended: first look at how the owner uses the keys and the on-screen hint after this fix; if a menu is still wanted,
build the drawn one as its own small phase and re-measure both scales. A cheaper step now would be a permanent one-line hint at the bottom of the window (about one US dollar).

## How the owner checks on the real display

1. `~/bin/lico gui` (this build: sha256 `2fc23138...`). Expected: dark window, in the right area three lines: `select a session`, `Ctrl+N  new session`, `Ctrl+A  accounts and sign-in`, then `> []`; no black square at the top left.
2. Click once inside the window (to give it the keyboard focus), press Ctrl+N: the title `New session: which provider?` with `1  codex` and `2  claude`; Esc; Ctrl+A: `Accounts`.
3. If keys still do nothing, tell which session type you use (`echo $XDG_SESSION_TYPE`) and try the other path for comparison: `env -u WAYLAND_DISPLAY ~/bin/lico gui` (X11 through XWayland).
