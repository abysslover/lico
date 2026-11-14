# M14-P02 Report — Maximized First Start, and the Maximize/Restore Icon

- Status: implemented, waiting for the owner's G3 and for the owner's check on the real desktop (see the last section)
- Version: 1.0.0
- Branch: `phase/M14-P02-maximized-first-start` (stacked on `phase/M14-P01-path-status-bar` at `2e99f78`; code commit `3ee4071`)
- Plan: [M14-P02](../../plans/202611/20261114/20261114_09_M14-P02_maximized_first_start_plan.md)

## What was known before (lead mail 134, read-only X queries on the owner's running window)

X11 session, GNOME Shell (mutter), two monitors. The window of `lico gui 0.3.1`, started from a terminal as `lico gui`, `LICO_WINDOWED` unset: `_NET_WM_STATE` = `_NET_WM_STATE_FOCUSED` only (no maximized bit), size `1200x800` (the `with_inner_size`), `WM_NORMAL_HINTS` "user specified size". Never maximized in several starts. So the launcher-flag and Wayland hypotheses are out; it is consistent on mutter, not a rare race.

## Reproduction (before any fix) and the cause

- winit `0.30.13` on X11 maps the window first and only then sends the `_NET_WM_STATE` client message (`x11/window.rs:506-537`); it never sets the property before the map, which is the standard's way to ask for an initial state. A window manager is bound to read the property when it takes the window; a client message is only honoured for a window it already manages.
- No window manager is installed here and the owner's session is not to be touched, and the owner declined the download of a real one (`openbox`). So the reproduction uses a MODEL of a window manager (`src/lico_measure/fakewm.py`, a ctypes program over `libX11`, test tool only): it follows the standard and can be told to need time before it takes a window (`--manage-delay`) or to drop the message (`--ignore-messages`, the behaviour the owner's mutter was seen to have). The tests check the OUTCOME: `_NET_WM_STATE` read from the X server with `xprop`, and the window's own `is_maximized`.
- On the unchanged code both models reproduced the failure: with a manager that needs `0.3 s` to take the window, and with one that drops the message, the window never became maximized (`private/runs/m14_p02_red.log`: `condition not reached` twice). A manager that takes the window at once and honours the message maximized it, as the old test setups never could show. That is the cause class: the one message after the map can be lost; the property is what survives.

## What changed

- `gui/windowstate.rs` (new): for the product window the window is created hidden (`with_visible(false)`), `_NET_WM_STATE` = both maximized atoms is set on its X id with `x11rb` on a connection of its own, with a round trip so that the server has it before the window is shown (requests of two connections have no order; without the round trip the tests were flaky, `1` failure in `3` runs), and only then the window is shown; `set_maximized(true)` stays. Wayland, `--windowed` and `LICO_WINDOWED=1` are unchanged (the property is set only on X11 and only when maximized is requested).
- **The icon** (the owner's request in the chat of 2026-11-14): a square at the right end of the menu row (the row with `File Session View Help`), drawn with lines: one square when the window is windowed (click: maximize), two overlapping squares when it is maximized (click: restore); the pointer over it lights it; a click switches in both directions (`set_maximized(!is_maximized())`) through the ordinary click and pointer path; it draws again only when the pointer enters or leaves it or the state changes. Window JSON: `window_btn: {maximized, label, hover, rect}` with the label `Restore window` or `Maximize window`. A windowed start (`--windowed`, `LICO_WINDOWED=1`) shows the single square and the click maximizes.
- Existing test changed on purpose: `tests/test_menu_bar.py` asserted that the menu row holds only its titles (nothing lit right of the middle); it now looks left of the icon. `lico_measure/captures.py` ignores a failed `xdotool windowfocus` (a window that is not mapped yet, which the model with a delay makes possible).
- `ui.rs` `1,093 -> 1,094` of `1,120` lines; `windowstate.rs` has no `crate::` use except `super::` (no edge into a hub).

## Evidence (as measured)

| Check | Result |
|---|---|
| `tests/test_maximized_start.py` (6 window tests: model that takes the window at once, one that needs `0.3 s`, one that drops the message, a windowed start, the icon in both directions, a windowed start and the icon) | red first on the unchanged code (the two model cases, and `KeyError window_btn` for the icon tests); green after, `6 passed` in `4.0 s`, repeated `6` times with no failure |
| Rust units `gui::windowstate` | `1 passed` (the icon is a square inside the menu row at its right end, at scale 1 and 2, and clear of the titles at 300 px); written with the code, not run red |
| Cold full suite on the code (quiet host) | run 1 `549` tests, `1 failed` (see notes), `127.2 s`, peak `1,474.6 MiB`; run 2 `549 passed`, `123.2 s`, peak `1,403.3 MiB` |

Screenshots (private): `private/shots/m14_p02/icon_maximized.png`, `icon_maximized_hover.png`, `icon_windowed.png`; the menu row without the icon is in `private/shots/m14_p01/after_default.png`.

## Honest notes

- What the tests show: a window manager that follows the standard maximizes the window at its first start even when it needs time or drops the message. What they cannot show: that mutter on the owner's screen does. mutter was not tested, and I do not know why it dropped the message (the evidence fits "the message is lost", not a proof). If the real cause on mutter is something else (for example a rule that refuses a maximize for a window with a `user specified size`), this fix will not change it.
- The icon uses the same client message as the old code (`set_maximized`) on a window that is mapped and managed by then. On mutter that message may be dropped too; the owner's click on the icon is therefore also the real test of that question: if the icon does nothing on the owner's screen, the client message is what mutter ignores, and the next step would be another way to ask (for example a message sent from a connection of our own), chosen from the owner's evidence.
- The first cold run had `1` failure, `test_call_rooms_and_mentions` (`IndexError` in a list built from two JSON fields of different moments); it passed in `6` isolated reruns and in the second cold run; I did not find a cause and do not know whether the one row less of text changed it. Two earlier cold runs on a host with load average `60+` failed in load-sensitive tests (`test_llm_discover_speed`, `test_path_status_bar` narrow case, `test_unicode_clients`); the narrow-case test now waits for the bar to be drawn at the new size.
- The model is test tooling (`lico_measure/fakewm.py`), not part of the product binary.

## What the owner should check on the real desktop

Install the build of this milestone (P03), then from a terminal run `lico gui` three times (host not running, host running, once more after closing the window) and look at three things: does the window open maximized; does the small icon at the right end of the menu row switch it to windowed and back; and if either does not work, tell me which one and the output of `xprop -id <window id> _NET_WM_STATE` (the lead measured it before) so that the next step is chosen from evidence. I cannot see the owner's screen and do not claim the maximized start fixed there.

## Next

P03: end-to-end verification, three cold runs, idle and hidden-growth measurements, acceptance, `lico 0.3.2`, install.
