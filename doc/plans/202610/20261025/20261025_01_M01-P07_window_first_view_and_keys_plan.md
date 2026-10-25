# M01-P07 — The window's first view and key input (2026-10-25)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P07-window-first-view-and-keys`
> Approval: G2 approved by the owner in the chat of 2026-10-25 for this scope (cause, scope, tests and the cost cap below). G3 and G4 are pending.
> Cost cap: about 6 US dollars for this phase, reported with the result.

## Why

The owner opened `lico gui` and saw only black, and Ctrl+N and Ctrl+A did nothing. Reproduced on a virtual display with captured pixels: the first
screen shows its hint text, but after Ctrl+N the whole text area is empty (no title, no list, not even the input line), although the window
process lives and the window's own model (`ui` dump) lists the panel. The earlier tests looked only at that model, never at the pixels.

## Cause found

When lines are put into the text buffer by hand, new lines placed in front of an already laid-out line were not laid out by `shape_until_scroll`,
and the layout runs stop at the first unlaid line: zero runs, nothing drawn. A panel replaces every line of the screen except the input line,
which is reused, so every panel and every session switch with a changed first line was affected. Empty body lines were checked and are not
the cause (a one-space line gives the same result). Two smaller faults were found on the way: the window quits when its standard input is
closed (a launcher or `nohup` closes it), and a pair of 16-pixel measurement markers (session and sequence colour, used only by the
pixel-latency harness) is drawn at the top left of the product window as a black square when no session is selected.

## Scope

1. Lay out every line of the screen explicitly after the text is set (one place), so a panel, the list and the notices all draw.
2. The product window (`lico gui`) does not draw the measurement markers and does not quit when its standard input closes; the measurement
   client keeps both behaviors.
3. Tests that look at pixels, not only at the model, run against `lico gui` on a private virtual display: the first screen, Ctrl+N and Ctrl+A each
   show text; the top-left corner carries no marker; closing standard input keeps the window. Each is shown red by mutation.
4. The test rig stops what it starts and only that: the host is found through its own socket and stopped by its own pid (never by name), the
   virtual display is stopped on every path.
5. Captures of the three screens for the inspection bundle and a way for the owner to check on the real display.

Out of scope: a menu bar (one paragraph estimate in the report only), Wayland (not reproducible here: no compositor installed), P08 and later.

## Progress

- 2026-10-25: Plan written after the reproduction; fix and tests follow.
- 2026-10-25: Done. Three pixel tests red before and green after (red: `assert 0 >= 4` text rows after Ctrl+N; `corner_is_background()` false; window gone with its input closed);
  suite 84 passed; captures in the bundle; report written (commit eeae7e2 holds the fix). Cost of the phase about 2 US dollars on the harness meter.
- 2026-10-25: S02 added (defect found by the owner inside this scope): Backspace did nothing in the input line. Backspace removes the last character with what is attached to it
  (combining marks, joiners, variation selectors, a decomposed Hangul syllable) when no composition is open; Ctrl+U clears the line; Backspace repeats in text entry. Cost cap 2.5 US dollars.
  Out of scope, follow-ups: the Left, Right, Home and End keys cursor movement, selection, Ctrl+Backspace word delete.
- 2026-10-25: S03 added (usability gap found by the owner inside this scope; cost cap 3 US dollars): the window can copy out but never paste in, so the final code of the Claude Code sign-in
  could not be entered; and the browser should open by itself. **Reversal of the P06 rule "the address is shown and the browser opens only on the person's request":** the window now opens the
  sign-in address in the default browser by itself, once per attempt, because the owner asked for it on 2026-10-25 (Ctrl+O still opens it again, Ctrl+Shift+C still copies it; the environment
  variable `LICO_NO_AUTO_OPEN`, read only by the window, switches it off for tests). Paste stays a person's own act: Ctrl+V and Ctrl+Shift+V (CLIPBOARD), Shift+Insert (PRIMARY), only into a text-entry
  line, nothing read at any other time, a newline ends the paste so a pasted code is never submitted by itself, more than 8 KiB or an incremental transfer is refused, 500 ms deadline off the event loop.
  Out of scope: copy-out of selections, mouse selection, drag and drop, Wayland.
- 2026-10-25: S04 added (urgent, cost cap 2.5 US dollars): the real `claude auth login` could not be finished in the window. Its prompt for a code ends without a newline and the
  supervisor read only whole lines, so the window never offered the code field; its address may come as an OSC 8 hyperlink whose visible text repeats it, and the escape stripping knew
  only CSI, so the address was corrupted. The login fixture of P06 printed a plain address and a prompt with a newline: a fixture that did not match the real program (the real prompt had never been captured).
  Fixed: output is read as it arrives (bounded, always drained), the code prompt is looked for in the bytes so far, OSC sequences are stripped and the address is taken from the link target or the
  first complete address; the login child gets a harmless BROWSER so the window is the only opener. New fixture with the real byte structure and made-up values.
- 2026-10-25: S05 added (owner request): the sign-in code was drawn in clear text in the prompt row (the P06 "no echo" rule had covered output, logs and the model, not the screen). In the code step the prompt row
  now shows one star per character (at most 64), the ui dump holds the stars and the length instead of the text, and a failed submit says plainly "The sign-in did not take the code" without the code (the raw error word
  only in the dump). The label line and the session line stay clear text. Cost reported per step; no cost stop (owner rule of 2026-10-25).
- 2026-10-25: S06 added (owner's installed build was not in use because the old host daemon kept running; the login supervisor lives in the host). `status` carries `build` (a hash of the running executable)
  and `logins_running`; `host.stop` / `lico host stop [--force]` end a host over its own socket (refused with `busy` while sessions or a sign-in exist, unless forced; never by name). The window compares builds when it starts:
  an idle host of another build is replaced and the header says so; a busy one is left alone and named with the way to close it. A host older than S06 cannot answer `host.stop`: its process has to be ended by pid once.
- 2026-10-25: S08 added (owner rule of 2026-10-25: a session starts with permissions skipped, "as Orca does", and the window must still handle approvals). **Decision and risk, written as the owner asked:** a new Claude session starts with
  `--permission-mode bypassPermissions` and Codex with approval policy `never` and sandbox `danger-full-access`, because with the previous strict defaults every tool call failed with "requires approval" and the window could
  not answer: the Claude command line was missing `--permission-prompt-tool stdio`, which is now always present so what the program still asks about (a question to the person, a plan, an ask rule) reaches the window.
  What contains the risk: the session runs under the account's private home and its own private workspace directory, with the host's environment cleared to a path and a locale; what it does not contain: a skipped-permission session can
  still read and write anything the user can (the files of the user outside the workspace, the network), so the workspace is a starting directory, not a jail. Ask mode (`default`, `untrusted` + `read-only`) is one key (M in the new-session panel) or `--mode default` away
  and is shown in the header. Y allows and N denies the first pending approval of the selected session while the input line is empty. Not done: "allow for the rest of the session" (the reply shape was not verified), shortening of secrets and paths in the prompt.
  Also in this step (011 and the list): the output text keeps a 4 px margin from the list and from the window edge (the text area was as wide as the window although drawn 4 px to the right, so a wide glyph at the edge was cut), the list has a left margin, the product window is titled "Lico".
- 2026-10-25: S07 added (owner could not find how to leave a session, switch or start a second one). Actions first, so a later menu bar only calls them: a permanent hint line at the bottom of every view
  (`Ctrl+N new session   Ctrl+A accounts   Esc session list   Ctrl+Tab next session   Alt+1-9 session by number`, fewer items when the window is narrow, `Esc back out` while a panel is open, `Y allow  N deny` while an approval is pending);
  Ctrl+Tab / Ctrl+Shift+Tab next / previous session (wrapping, drafts stay with their sessions); Alt+1..9 by the number shown in the list; Esc in a session with an empty input line returns to the start screen
  (the session keeps running; Esc with text or a selection keeps its meaning). The menu bar was not started.
