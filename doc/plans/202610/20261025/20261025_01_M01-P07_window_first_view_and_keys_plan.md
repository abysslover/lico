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
