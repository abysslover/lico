# M14-P02 Plan — Maximized First Start

- Status: implemented (see report)
- Version: 1.1.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M14-P02-maximized-first-start` (stacked on `phase/M14-P01-path-status-bar`)
- Milestone: [M14](../../../milestones/202611/20261114/20261114_03_M14_path_status_bar_and_maximized_start_milestone.md)

## Objective

Reproduce first. The window is created with `with_maximized(true)` and `set_maximized(true)` (`gui/mod.rs:572-583`); winit `0.30.13` maps the window and only then sends the `_NET_WM_STATE` client message, and never sets the property before the map, so a window manager that has not yet taken the window may ignore the message (a hypothesis, not a finding). The existing test checks only the request (`maximized_requested`), and Xvfb has no window manager. Reproduce under a real light window manager in a private display (the distro `openbox`, downloaded as `.deb` files pinned by exact version and extracted under `private/wm/`, never installed; only with the owner's approval of the download) and under a small EWMH model written for the test (Rust, `x11rb`, no download), never the owner's session; test the OUTCOME (`_NET_WM_STATE` and size read from X); fix the cause only; say what the test can and cannot show; tell the owner what to check on their screen.

Scope changes after the owner's answers (lead mail 134): the launcher flag and Wayland hypotheses are out (terminal `lico gui`, `LICO_WINDOWED` unset, X11); the owner declined the download of `openbox`, so only the window-manager model is used; and an in-window icon that switches between maximized and windowed mode (right end of the menu row) is part of this phase.

## Steps

- [x] S01: The outcome test (real `lico gui` child, private host socket, private Xvfb, the window manager(s)), run red on the unchanged code; output kept. If nothing reproduces, stop and ask with three options.
- [x] S02: The cause, from the evidence (client message before the window is managed, missing initial `_NET_WM_STATE` property, the fixed `with_inner_size`, a launcher flag, Wayland chosen over X11).
- [x] S03: The fix of the cause (leading candidate: create invisible, set the `_NET_WM_STATE` property on the X id, then show; ask once more on the first `Resized` if still not maximized; no timer).
- [x] S04: Green; the existing `maximized_requested` test kept; report `111` with the owner's check list.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The failure is reproduced on a window manager before the fix, and the cause is named from evidence | red output |
| 2 | After the fix the window is maximized at its first start under the same window manager(s) | outcome test |
| 3 | The report says what the test shows and what it cannot (the owner's mutter) | report |
| 4 | `--windowed` and the Wayland path are unchanged; no new timer or repaint | tests, review |
