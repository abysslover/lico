# M24-P11d: a theme button, dark, light, system

- Version: 1.0.0
- Date: 2026-11-26
- Branch: `phase/M24-P11d-theme-button` from `phase/M24-P11c-whole-block-select-and-fold`; code `6c9af5e`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-26 (quoted in the [plan](../../../plans/202611/20261126/20261126_02_M24-P11d_theme_button_plan.md)): "상단 아이콘 중에 theme 버튼을 하나 추가해서 누를 때마다 어두움, 밝음, 시스템 돌아가면서 바뀌게 해줘."
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test and in four screenshots of the window)

| | Before | Now |
|---|---|---|
| The top icons | Accounts, New session, New Home session, Open browser | **one more, last: the theme** |
| A press on it | | goes to the next of **dark, light, system** and back to dark; the picture is the state (a crescent, a sun, a disc half solid and half a ring); the word beside the icons says it: `Theme: dark`, `Theme: light`, `Theme: system (now dark)` or `(now light)` |
| Dark | the only look | the look of today, pixel for pixel (the mapping is the identity) and the state a window starts in |
| Light | | the whole window light: the ground and the bands pale, the text dark, the hues kept (the person's blue band a pale blue, a peer's card a pale teal, a command's box a pale violet); the terminal pane and the editor too |
| System | | dark or light as the desktop says, and follows it while the window is open (asked again about every 30 seconds) |
| The choice | | kept by the host with the other view settings; the next window has it |
| A panel open (Accounts, Files) | | stays open; the press never closes it and the icon is never greyed |

The browser tab's page keeps its own colours (it is a picture the engine paints); a pasted picture and a colour glyph too.

## 2. How it is built (READ)

- **The mapping** (`gui/theme.rs`): a channel moves by `255 - (max + min)` of the colour's three channels, so the lightness is mirrored, the hue and the chroma are kept, nothing is clamped, and applying it twice gives the colour back. Dark is the identity.
- **Where it is applied**: the colours reach the pixels in very few places: `ui::rect` (every fill), `ui::blit_with` (the ink of every glyph) and a few direct writes of whole bands (`transcript.rs`, `times.rs`, `ui.rs`). The mode is a thread-local set at the top of `compose`, so the tests that run in many threads do not meet.
- **A switch** drops the kept frame (the scroll step keeps rows of the last one) and draws once from scratch.
- **The host** keeps `theme` (`dark`, `light`, `system`; `dark` removes the key; `status` reports it when it is not dark; any other word is refused by `settings.set` and by the shape check of the request).
- **System**: the window system's own answer where it gives one (it does not on X11), else `gsettings` (`color-scheme`, then the theme's name), asked **off the window's thread**, once when System is chosen and every 30 seconds while it stays chosen; a question that has not come back in two minutes is forgotten; an answer of nothing keeps what was known; before any answer System is dark.
- The module graph is unchanged. The size ratchet holds (`ui.rs` 1114, `mod.rs` 798, `cli.rs` 797).

## 3. Checks (MEASURED)

- **Tests first**: run red on stubs, then green.
- **The tests now**: unit 46 new; the scroll oracle in light at the scales 1, 1.25 and 2 and the census of the paint sites; the window test `tests/test_theme.py`. The whole binary: 1,328 unit tests pass.
- **The census** found one site that was missed (the divider between the panes), now mapped; it finds none other, with and without an open menu.
- **Mutation checks**: two passes, **every mutant one edit on a restored tree whose files were checked by their sha256 after each**. The first pass, **128 mutants** of the new code (`theme.rs`, `theme_host.rs`, the host's setting, the lines added to `ui.rs`, `mod.rs`, `menu.rs`, `icons.rs`): 106 killed, 21 survived, 1 did not compile (a removed match arm, redone as an empty arm). The survivors were of five kinds and each was closed in the code or in a test: the question to the desktop could not be seen from a unit test (the window has no event loop there), so the asking was split from the window (`ask_with`, `on_window_theme`, `is_dark`, `probe_with`, `run`) and the questions started are counted (`asked`, also in the dump); the two constants of the cadence (30 s, 2 min) were only used by name and are pinned; the scope that puts the mode back had no test of its own; the redraw marks had no test; the moon's centring and the sun's rays were only looked at (now the bounding boxes). The second pass, **65 mutants** (everything of `theme_host.rs` after its change and every survivor of the first): 59 killed, 6 survived; two of them (`dump` without the theme, the desktop's answer dropped in `user_event`) are killed by the window test `tests/test_theme.py` run against each mutant (2 of 2 killed). **Four are written down, not killed**: the window system's own answer is not read (`window.theme()` is none on X11 and a unit test has no window), the arm that takes the window system's `ThemeChanged` is dropped (the event is not sent on X11; the method it calls is tested), and two on the same line of `keep_rows` (the list column's background after a scroll step: the line is **dead**, the column is drawn whole afterwards, the line is older than this phase and was only given `tone`; a magenta fill in its place also survives)
- **The commit hook**: the first run **refused**: the suite passed (630 tests in 130 files, 184 s) but the build peak was **1962.7 MiB over the bound of 1536 MiB**, and the guard refuses a peak over the bound even when the tests pass; **the bound stays** (the owner's word of 2026-11-25), nothing was raised or bypassed, the same commit was run again by hand with the receipts of the passed tests reused, and the second run accepted it (`6c9af5e`)
- **Looked at**: the four screenshots (dark, light, a block selected in light, system).

## 4. What is not decided (the owner's)

- The light palette is a mapping of the dark one, not a hand-drawn second palette: it is consistent and a colour that looks wrong in light is one line to fix (an exception in `tone`). A hand-tuned palette is a later phase if the owner wants one.
- A page in the browser tab keeps its own colours; telling pages the preference (`prefers-color-scheme`) is the engine's and is not done.
- The title bar and the borders of the window belong to the window manager.
- System is read through `gsettings` (GNOME and the desktops that keep that key); another desktop answers dark until the window learns its way (the portal's `color-scheme`).
- There is no command line flag for the theme: `cli.rs` is at 797 lines of the limit of 800 and the setting is reachable by `settings.set`.
- The marker lines the harness writes into the conversation are text and are mapped like all text; nothing in them was made a special case.
- **The size of the unit suite and the component method (the owner's doubt of 2026-11-26, "단위 시험 갯수가 1000개 넘어간 것 보니 원래 원칙 component 기반 구현이 제대로 된 것인지 확신이 안듬")**: READ and MEASURED. The 16 components under `components/` (`png`, `vt`, `ws`, `cdp`, `engine`, `proxy`, `sandbox`, and the others) follow the rules of `component_rules.md` (own tests, own budget, not re-run while unchanged). The **1,338 unit tests are those of the `lico` binary** (the host and the window), which is not split: the module graph has 268 modules, **one cycle of 50 modules with 59 two-way pairs**, which the component rules forbid (no cycle, only downward edges). All the work of M22 to M24 on the window lands in that cluster, which is why the suite grows, why the commit hook's build peak was over the bound several times (1,701 to 1,961 MiB) and why a window change re-runs so much. This phase kept its new code in a pure module and a thin one (`theme.rs`, `theme_host.rs`) and **added no edge to the cycle (it stays at 50)**, which limits the damage and does not shrink the cluster. **Proposal (the owner decides the order)**: after the release `0.8.2` and before or after level C (`0.9.0`), a milestone that breaks the cycle of the window's core with the `/refactor` method (Phase 0 report first: which modules have no upward dependency and can become components, interface spec before any move). Nothing of it is started; the standing order (0.8.2, then level C) goes on.
