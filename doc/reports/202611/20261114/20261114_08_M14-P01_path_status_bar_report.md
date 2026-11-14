# M14-P01 Report — Bottom Status Bar with the Full Path

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M14-P01-path-status-bar` (from `main` at `8857a4c`; documentation commit `bdbfb8e`, code commit `d2a60b6`)
- Plan: [M14-P01](../../plans/202611/20261114/20261114_08_M14-P01_path_status_bar_plan.md)

## What changed

- **A bar on the last row**, full window width, below the hint line (`gui/pathbar.rs`, new, 201 lines; the hint line, the telemetry strip and everything above moved up one row, 18 px at scale 1). It shows, in this order: the row the pointer is on (a row of the sidebar's directory list: `dir/name`; a chat card: the chat's folder), else the row the Files panel is on (its cursor) or, when the panel is closed, the directory the sidebar shows, else the working directory of the active session (none for a Global session). `host:path` on an ssh host (the strip above keeps its `ssh:HOST:path`). A path too long for the bar keeps its END behind a leading `…` (`tail_path`, wide characters counted as two columns) and a muted `click: copy` sits at the right edge; a click anywhere on the bar copies the WHOLE text through the existing `copy_text` path.
- **No repaint of its own**: the pointer's row path is remembered and a change of it (and only that) sets `dirty` (`path_hover_moved`, next to `hover_moved`); no timer, thread or polling. Nothing new in the host: no op, tool or command.
- **Layout**: `hint_y_f = height - 2*lh`; one row fewer for transcript text (`rows_avail`), one row fewer for the Files panel (`panel_rows`), the mention popup's lower limit one row higher. The sidebar, the chat list (`calc_chats_ps`), the file list and the modals take `hint_y` and moved with it; the new window test checks that the sidebar entries and the chat cards end above the hint line.
- Window JSON: `path_bar: {text, shown, cut, source, cols, rect, hint_rect, telem_rect}`.
- `ui.rs` grew by `3` lines (`1,090 -> 1,093` of `1,120`); `pathbar.rs` has no `crate::` use at all (only `super::`), so it adds no edge into `core`, `session`, `folders`, `transfer`, `handoff` or `codex`.

## The places that shorten a path (listed before the change, all keep their short form)

`sidebar_render.rs:441` and `sidebar_chats.rs:543` (the directory line, `tail_path`); `newchat_draw.rs:77` (`New chat · place`); `telemetry.rs:327-359` (the strip's directory, `middle_elide`, with `ssh:HOST:` and the branch). Row and card lines clip names, not paths; the Files panel title shows only `[ssh:HOST]`. The bar is where the whole path is read.

## Evidence (as measured)

| Check | Result |
|---|---|
| `tests/test_path_status_bar.py` (3 window tests, private Xvfb, synthetic homes, the fake ssh rig of the ssh tests) | red first: `KeyError: 'path_bar'` (`private/runs/m14_p01_red.log`); green after: the bar at the last row and full width with the session's folder; a directory row and a file row show `cwd/name`; a chat card shows its folder; away from them the folder again; `gpu-box:/.../name` on an ssh host; a path with Korean characters at a 640 px wide window is cut to its end behind `…` within the bar's columns; a click puts the whole text on the clipboard (`xclip`); bar, hint line and telemetry strip do not overlap; the sidebar entries and chat cards end above the hint line; two idle seconds draw nothing (`frames` unchanged) |
| Rust units `gui::pathbar` | `3 passed` (path joining, wide characters and the cut, pointer over selection over session); written with the code, not run red |
| Cold full suite on the code commit (quiet host, load `7.9`) | `543` tests in `115` files, `543 passed`, `122.6 s`, tree peak `1,485.3 MiB` |

Screenshots (private): `private/shots/m14_p01/before_default.png`, `before_narrow.png`, `after_default.png`, `after_narrow.png`.

## Existing tests that depend on the bottom edge (changed on purpose, named)

- `lico_measure/captures.py` `ProductWindow.prompt_ink`: leaves out the hint line and now the bar (`height - 24 - 18`) when it looks for the input line.
- `tests/test_navigation.py` `bottom_ink`: measures the hint line one row above the last row (the last row is the bar).
- `tests/test_frame_cost.py` `without_total_line` (and the test of that helper): the corner that is left out is three lines now (memory total, hint, bar).
These helpers were behind the failures of the first cold run (`test_window_keys`, `test_navigation`, `test_frame_cost`); nothing else changed. `test_first_view` also failed in that run, see the notes.

## Honest notes

- Two further cold runs of the suite on the code failed on a heavily loaded host (load average `62`, `187 s`): `tests/test_llm_discover_speed.py` (remote bound `300 ms`, median `314`) and a wait in `test_peer_reply_copy_and_dir_nav`; the first cold run also had `tests/test_first_view.py` time out in a wait of the session-list test, and it passed in a rerun with its three neighbours. I did not find a cause in the bar for the last one (the click it makes is on a list row, far from the bar) and I did not reproduce it; the timing tests are sensitive to a loaded host. The run I report (`122.6 s`, `543 passed`) was made after the load had fallen.
- The Files panel has no row targets for the mouse (it is driven by its cursor), so the bar follows its cursor row (selection), not the pointer; sidebar rows and chat cards follow the pointer.
- "The directory the sidebar shows" as the selection, before the session folder when nothing is pointed at, is my reading of the owner's "when a directory is shown"; the mail named only the session folder as the fallback. They are the same folder in the normal case.
- Not shown here: the sidebar's own `mem sessions ...` line and the Calls tab have no path to show.

## Next

P02: reproduce the unmaximized first start under a window manager, then fix the cause.
