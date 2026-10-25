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

## S04: the real Claude Code login output (urgent)

The lead measured the real `claude auth login` (private home, stdin closed, killed after seconds, nobody signed in): a line `Opening browser to sign in`, a line with an OSC 8 hyperlink
(target and repeated visible text) and a prompt `Paste code here if prompted > ` with no newline. The supervisor read only whole lines, so the prompt never arrived (`needs_code` false, the window never
offered the code field) and the escape stripping knew only CSI, so an OSC link glued its text to the address. Fixed in login.rs: the output is read as it arrives (8 KiB bound, always drained, a non-UTF-8 byte
never stops it), the prompt is recognised in the pending bytes and on complete lines, OSC sequences (ended by BEL or ESC backslash) are stripped, and the address is the link target or the first complete address of the
text (it stops at a space, an escape or a bell). Tests (2, new fixture `real_shape_login.py` with the exact structure and made-up values; 92 pass in all): red on the previous build (`condition not reached`: the code
field never opened; the window test failed likewise); green after: the host reports the clean address and `code`, the fixture receives exactly the code and a newline and ends `succeeded`, the window shows the address
and "paste it below", Ctrl+V of a canary code and Enter succeed, the fake opener is called once with the clean address, the code is nowhere. Which program opens the browser (measured here, shape only, no sign-in, the address never printed):
with a fake `xdg-open` first on the path and no `BROWSER`, the real command printed `Opening browser to sign in` and did NOT call `xdg-open` (0 calls: it uses its own way); with `BROWSER` set to a command of mine it called that command once.
So `BROWSER` is honoured and the login child now gets `BROWSER=/usr/bin/true`: its own attempt does nothing and the window opens one tab. Codex's own opener was not measured; the same variable is set for it. Also measured: under `NO_COLOR=1`,
which the supervisor sets, the real command printed the address as plain text, without the OSC 8 sequences the lead saw without it; both shapes are handled and tested. Idle client RSS at scale 1 on 37b667e: 23.07 MiB (limit 50), host 6.01 MiB.
Cost of the step reached its cap of 2.5 US dollars. Honest note: the P06 fixture did not match the real program, and my plan never captured the real prompt; the same holds for Codex's output, which only the owner's success shows.

## S05: the sign-in code is masked (owner request, addendum 005)

In the code step the prompt row draws one `*` per typed or pasted character (at most 64); the ui dump carries the stars and `input_len`, never the text; the failed-submit notice no longer repeats the code
("The sign-in did not take the code (no code was expected yet)." / "... did not take the code."), the raw error word is in the dump only. Backspace, Ctrl+U and paste act on the real buffer; the label and
session lines are unchanged. Test (1 new, tests/test_mask_code_line.py): after pasting the canary the model holds eleven stars and no canary; the light pixels of the screen equal those after typing eleven `W` (the widest
letter, so the code's own glyphs cannot be what is drawn); 80 characters draw 64 stars. Red by mutation (masking switched off): `assert ('CANARYCODE9' == '***********' ...`; green: whole suite `93 passed in 370.63 s`; the older code tests now
compare `input_len`. A first run of the pixel test failed by 80 pixels because the screen was counted before the frame was presented; the test now waits for the frame. Capture `8_code_step_masked.png` (eleven stars). Idle client RSS at
scale 1 on eee81cb: 22.85 MiB (limit 50), host 6.05 MiB. A hazard found: the reproducibility test, run inside the suite with the shared build directory exported, rebuilds the committed state into that directory, so a binary built before the
suite can be replaced by the committed one; build again before taking captures from it.

## S06: a host of another build (008)

The installed build changes nothing while the old host daemon runs (the login supervisor lives in the host). `status` now carries `build` and `logins_running`, `host.stop` ends a host over its own socket (`busy` while sessions or a sign-in exist unless `--force`), and the window compares builds at start:
idle host of another build -> replaced, header says so; busy -> left alone, header names the way out (`lico host stop`, process id). Tests (2): red with the check switched off (`assert ('older build' in 'select a session')`), green: 2 passed; the test host is "another build" by one appended byte.
A host older than this step cannot be asked to stop (no such operation): its process has to be ended by pid once; the clean-clone reproducibility test now builds in its own target directory because it replaced the shared binary other tests use.

## S08 and 011: permissions by default, approvals, edge margins (010, 011)

- **Cause of "requires approval":** the Claude command line had `--permission-mode default` and no `--permission-prompt-tool stdio`: the program then refuses a tool that needs approval by itself and sends no request, so the window never saw one.
- **Flags chosen:** new Claude session `--permission-mode bypassPermissions --permission-prompt-tool stdio` (`default` instead of `bypassPermissions` in ask mode); the documentation says `--dangerously-skip-permissions` is the same as `--permission-mode bypassPermissions`.
  Evidence (real `claude` 2.1.310, private home, nothing signed in, no model call): the program accepted every combination tried (it waited for input instead of printing an unknown-option error). What it still asks in bypass mode, by the documentation:
  `AskUserQuestion`, plan approval (`ExitPlanMode`), explicit `ask` rules, organization-controlled connector tools, recognized critical-path removals. A real approval request cannot be provoked without a model call, so the request/response shape is the documented and
  already tested one (`can_use_tool`, allow with `updatedInput`, deny with a message) and the simulator now behaves as documented (it refuses with "requires approval" when the prompt tool is missing, and in bypass mode runs ordinary tools without asking). Stated limit: nothing here was seen from a real model turn.
- **Codex:** `approvalPolicy never` and sandbox `danger-full-access` were sent to the real `codex app-server` (private home, no login, no turn) and came back as `never` and `dangerFullAccess`; ask mode keeps `untrusted` and `read-only`.
- **Window:** the header shows `mode: auto (permissions skipped)` or `mode: asks for approval`; the new-session panel has `M` to switch; Y / N answer the first pending approval of the selected session when the input line is empty; the list marks sessions with approvals (`!`); `lico session create --mode default`.
- **Tests (4 new, 3 older ones moved to ask mode where they test the ask path):** default start flags and modes for both providers, exact allow/deny replies and the auto-mode behaviour (an ordinary tool needs no approval, a question still asks), Y/N in the window, and the pixel test for the edges.
  Red before: the session `mode` field was refused (`bad_request`) and, for 011, `AssertionError: (1, 1203, 1199, 1200)` (a light pixel in the last column). Mutation: removing the prompt tool fails two tests (`'--permission-prompt-tool' is not in list`). Green: whole suite `99 passed in 412.80 s`.
- **011 cause:** the text area was as wide as the window although it is drawn 4 px right of the list, so the last 4 px of a line of wide glyphs fell outside the window (the Hangul glyph at the edge); it now keeps 4 px from the list and from the edge for the output, the prompt row and the panel text. The list also had no left margin (its first letter touched the edge): it has 4 px now. Checked at scale 1 and 2 and at a window width of 1203 px.
- Idle client RSS at scale 1 on 6cad8f0: 22.92 MiB (limit 50), host 6.11 MiB. Not done: `A` (allow for the rest of the session), shortening of secrets and paths in the prompt.

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
