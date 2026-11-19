# M22-P05a Plan — The Browser Tab's Address Bar At The Top, An Empty First Page, No Click Ring

- Status: done (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M22-P05a-address-bar` from `phase/M23-P03b-test-diet` (`18d06cd`; `main` is `a063ea8`, P03b is committed and not pushed); the prefix `phase/M22-P05a-` approved by the owner at commit time (the question on the prefix, 2026-11-19); the feat commit is `8b30b19`
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md), the browser paragraph of "The owner's answers"
- The owner's words: "그외에 최초 브라우저 URL 위치 및 보이는 형태 개선" (the chat, 2026-11-18); in the milestone: "address bar at the top (`‹ ›  ⟳  address`), cut in the MIDDLE when long (the host name and the end stay), the whole address when the bar has the keys, Enter goes ... a new tab is an EMPTY page with the bar focused; the explanation is on `F1`. The click ring goes."
- P05 is three phases: **P05a** (this one: the bar, the empty first page, the ring), **P05b** (search: text that is not an address is searched with the engine of the setting `browser_search`; the default engine by one measured probe), **P05c** (remembered pages, listed in Chats, reopened by a click, "Reopen closed tab").

## What this phase changes

1. **The address is a bar at the top of the browser tab**, `‹ › ⟳` and the address (D25 to D28), no longer the input line at the bottom.
2. **A new tab is an empty page with the bar focused** (D29); the long explanation is on `F1` only.
3. **The click ring is gone** (D30); the crosshair pointer over the page stays.
4. **The sentence that was the header** (title, phase, where typing goes, notices) is the one row at the bottom, where the address was (D26).

## What was found (2026-11-19, reading the code)

- The host already has **`browser.nav`** (`back`, `forward`, `reload`: `browser/window_ops.rs`) and the window sends it on Alt+Left, Alt+Right and F5. The buttons need no host change. The host does not say whether there is a page to go back to, so a button is never grey for that reason (it is grey while there is no tab).
- The address is the editor of the window (`self.ed`: text, cursor, selection, undo, the drafts parked per `browser:<id>` key, the input method's pre-edit) shown on the prompt row, which is the last line of the text buffer, and the page is painted between the header (the first line) and that row. The editor's geometry (caret, selection, hits for the mouse) is made by `place_layout` from the buffer's lines and assumes the shown row starts with two bytes (`"> "`).
- The terminal tab (M23) already took the pattern this phase uses: the first line is the tab's header, the one row at the bottom is a status row, the middle is painted.
- `ui.rs` is at its line ceiling (1114): what the bar needs there is paid for by moving code out.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D25 | The first line of the browser view is a **bar painted over that line** (a text line of the buffer holding one space, so that the layout keeps its height): three square buttons `‹` back, `›` forward, `⟳` reload (drawn by `icons.rs`, not letters: the font may lack them), then the address field to the right edge. The editor stays where it is (state, keys, drafts, IME); only where it is drawn moves | the owner's words; the least that moves the address to the top with every key path unchanged |
| D26 | The row at the bottom (where the prompt was) shows the sentence that was the header: `ui.header` (and `browser.header` in the `ui` dump) keep the same text, so the notices, the flash and every test that reads them still see them | one place for the sentence; the terminal does the same with its status row |
| D27 | The field shows the address **cut in the middle** when the bar does not have the keys (`https://` dropped; the host name and the end stay: `www.example.com/…/page.html`), and the **whole address with the caret and the selection** when it has them, scrolled sideways so that the caret is in view. An empty field not in use says `Type an address` in a dim ink | the owner's words; the cut is a pure function of the component (`display_address`) |
| D28 | A click on the field gives it the keys and selects the address (a second click places the caret, a drag selects: the editor's own mouse code, fed by the geometry the bar makes); Ctrl+L does the same from the keyboard; a click on a button runs `browser.nav` and does not move the focus; a click on the page gives the page the keys (as now) | the way every browser does it |
| D29 | `browser_prompt` (the toolbar icon, the Session menu): the page area is **blank** (`help` false), the bar is focused and empty; `F1` shows and hides the help as for a tab with a page. A tab that has no frame yet and is not asked for help paints blank, as well | the owner's words: an empty first page, the explanation on F1 |
| D30 | `lico-browser-view` **0.8.0**: `click_mark`, `MARK_RADIUS`, `MARK_THICKNESS` leave the component (a minor change at 0.x) with their contract test; the help lines stop saying that a ring marks the click; the window's `mark`, `mark_pixel` and `browser_cues::browser_paint_mark` go; BROW-09 of the product spec loses the ring; `display_address` joins. The crosshair, the header's where-typing-goes sentence and the follow-the-page rule stay | the owner's words ("the click ring goes"); the component's receipt and CHANGELOG say it |

## Steps

1. [x] **Tests first**: the component's `display_address` (contract test) and the removal of the ring test; the window's pure functions (the bar's layout, the hit test, the sideways scroll) as unit tests; `Editor::select_all` as a unit test; the window tests of `tests/test_browser_tab.py` rewritten for the bar (13 tests: the new tab is blank and its bar focused and empty, F1 shows and hides the help, the bar of a page shows the address without its `https://`, the three buttons run `browser.nav`, a click on the field selects the address and typing replaces it, a long address is cut in the middle and whole with the keys with the caret inside the field, the mouse places the caret and selects in the bar, a click leaves no ring, the bottom row says what the header says).
2. [x] **The component** `lico-browser-view` 0.8.0 (spec, contract tests, receipt by `cargo xtask test lico-browser-view --record`, `cargo xtask check` and `verify` pass). The CHANGELOG line is written with the release (P06): the CHANGELOG has no M22 entry yet.
3. [x] **The window**: `browser_bar.rs` (layout, painting, hit test, the geometry given to the editor), three icons, the bottom row, the blank first page, the ring out (`browser_cues.rs` keeps only the crosshair), `ui.rs` hooks with no new line (1114).
4. [x] **Mutation checks** (below), before the final green gate.
5. [x] **Gates**: `cargo test --bin lico` 960 passed; clippy clean on the new code; the suite (see the commit message and "Results"); the owner's build after the commit; screenshots kept for the owner.

## Results (2026-11-19)

- **What the owner sees** (`lico gui`, a browser tab): at the top, under the tab strip, `‹ › ⟳` and the address field; the buttons are grey while there is no tab. A new tab is an empty page with the field lit and saying `Type an address`; F1 shows the help in the page area and hides it. A page that has the keys shows its address without `https://`, cut in the middle when it is longer than the field (`pages.test/…segment39/the-end.html`); a click on the field, or Ctrl+L, lights it, selects the whole address and shows it whole, scrolled so that the caret is in view; Enter goes; a click on the page gives the keys back to the page. The one row at the bottom says what the header said (title, state, where typing goes, notices). No ring is drawn at a click; the crosshair over the page stays.
- **Sizes**: `browser_bar.rs` 200 lines, `browser_view.rs` 776 (800), `ui.rs` 1114 (1114, no line added), `gui/mod.rs` 800 (no `mod` line added: the module is declared in `browser_view.rs`).
- **Component**: `lico-browser-view` 0.8.0, 39 tests, receipt current.
- **Mutation checks** (37 edits): the component's `display_address` 8 (one survived, the host's end at `?` or `#`: a test case added for each, then caught), the bar's pure functions 9 (the first run used a filter that matched no test, so every one "survived" for nothing: rerun with the right filter; one survived, the field one gap too far from the last button: the test now asserts the gap, then caught; one edit, `.max(left)` on the field's right edge, was dead code and was removed), `select_all` 4 (two are equivalent: an empty text with an anchor at 0 is no selection, and the typing-run flag is reset by every movement and edit that follows; one survived, the drag flag: asserted now, then caught), the window with a real host 16 (the buttons' three actions and their wire action, the focus and the selection of a click on the field, the buttons grey with no tab, the cut, the placeholder, the scroll, the editor's geometry, the focus mirrored in the dump, the bottom row's text, the selection's and the caret's pixels, the glyph boxes; two survived, the caret's and the selection's pixels: a test now reads the pixel, then caught). 35 caught, 2 equivalent.
- **Gates**: `cargo test --bin lico` 960 passed; the changed-files suite 537 tests green in 171.2 s (machine load about 9: other users' jobs) with a peak of 1130.6 MiB, and receipts written; the commit hook found every receipt current; the clean-clone acceptance (Cargo.lock changed with the component's version) passed in 152.7 s; the owner's build was rebuilt after the commit (`lico 0.7.0 (commit 8b30b19)`, `~/bin/lico` links to it). Screenshots of every state were looked at (`LICO_P07_SCREENS` of `tests/test_browser_tab.py`: the empty page, the help on F1, a painted page, the field clicked and selected, a long address cut and whole, the mouse's selection, scale 2).
- **Found on the way**: (1) the test filter of the first mutation run matched no test (the bar's tests live in `gui::browser_view::browser_bar::tests`), so a run that says "SURVIVED" for every edit of a file is the harness, not the tests; (2) the editor's geometry (`place_layout`) assumes a shown prefix of two bytes, so the bar draws `"  "` plus the address and gives the editor that string; (3) `blit` clips only on the right, so the field's text is drawn into a scratch picture of the field's size and copied; (4) `ui.rs` draws the editor's selection and caret at compose time, before the browser paints, so the bar paints its own into the scratch picture and the compose gets no geometry (`usize::MAX` as the first line) while the bar is shown.

## Not decided here

- Whether a button should grey out when there is nothing to go back to (needs the host to say; a host change of its own).
- Tooltips on the three buttons (the other toolbar icons have none either).
- The search and the remembered pages are P05b and P05c; the guide, the manual's browser paragraph (it still speaks of the ring and the header) and the CHANGELOG are P06's.
