# M17-P11: the page looks alive, release 0.6.1

- Version: 1.0.0
- Date: 2026-11-17
- Branch: `phase/M17-P11-page-looks-alive-0-6-1`, stacked on the M19-P01 tip `67aaf28`; release commit `50eb786`
- Source: the owner's reports in the chat, 2026-11-17 (the engine appears but "I cannot click or type inside Google"; "the cursor is not visible, it feels like a picture"; later "typing does run, but I hit the unusual-traffic error"). The lead diagnosed it, planned this patch and built it, under the owner's delegation (recorded as approved by the lead, to be confirmed by the owner). The API worker is finished; this plan and its report are one note because the patch is small.
- Labels: MEASURED (a run of this phase), READ (from the code), COMPUTED.

## 1. The diagnosis (MEASURED, before any change)

The real engine (a copy of the one the owner's Lico installed) was driven in private test homes on a virtual display. Input is NOT broken: click, English typing, links, buttons, forms (Enter and button), a textarea, a checkbox and `onclick` all work, also with the input method on and a 125 Hz mouse flood, and on the real Google home page (a word typed, the search clicked, back, the "Images" link clicked). What the owner met:

| What the owner saw | Cause | Class |
|---|---|---|
| "no cursor, it is a picture" | the engine draws no caret and no focus ring; Lico gave no cue about where typing goes | Lico UX gap, FIXED here |
| the address row at the bottom kept the first address | a non-empty row never followed the page | Lico defect, FIXED here |
| Korean typed in Google's box: boxes | a single-line `<input>` paints Korean as boxes in this engine (a `<textarea>` and plain text are fine; no font helps); the page holds the right text | engine limit, said in the help and the guide |
| the composition showed only in the bottom row | the engine has no composition events | Lico UX gap, FIXED here (the header shows it) |
| a search ends on "unusual traffic" | Google blocks this engine (blank reCAPTCHA box) | NOT fixable by Lico (below) |
| `contenteditable` boxes take no typing | the engine does not focus them | engine limit, said in the help and the guide |

## 2. What the person sees now

- The header of a ready or loading tab says where typing goes: "typing goes to the page (Ctrl+L: address)" or "typing goes to the address row (click the page to type in it)".
- A ring (orange, radius 9, 2 px) marks the last click on the page while the page has the keys. It goes when the page's address changes, on Ctrl+L, on an address Go, on a new tab, and under the F1 help.
- The pointer is a crosshair over the page.
- The address row follows the page's address (a link, Alt+Left, a redirect) when the person has not edited it; a row the person edited or cleared is never overwritten.
- While the page has the keys the composition of the input method shows in the header ("composing: ..."), not in the address row.
- The F1 help and the guide say the limits: no caret is drawn, Korean in a single-line `<input>` may paint as boxes, `contenteditable` takes no typing, Google search refuses this engine.

## 3. Google and Obscura (READ, web research of the same day)

- Obscura's README, its issue list and the GoLogin review say nothing about Google, CAPTCHAs or blocking beyond "CAPTCHA solving is not built in". Its only related feature is an opt-in anti-detect (stealth) mode: `--stealth`, `OBSCURA_STEALTH`, `navigator.webdriver` false, fingerprint randomization.
- BROW-05 stays: Lico passes no stealth switch and sets no stealth variable; the user-agent names Lico and the engine. Using stealth is the OWNER's decision (and a spec change); it is not used, and it may not be enough: a headless Chrome gets the same Google block unless both the user-agent and the webdriver flag are changed, and Google also judges the address's reputation.
- Recommendation: treat Google search as unavailable in this tab; sites that do not block automated clients work.

## 4. How it is built (READ)

- `lico-browser-view` 0.7.0 (pure): `keys_hint(Focus)`, `follows_page(row, last_put, page_url)`, `click_mark(radius, thickness)` with `MARK_RADIUS` 9 and `MARK_THICKNESS` 2, and three more sentences in `help_lines()`. SPEC section "The cues of a live page (0.7.0, M17-P11)".
- Host adapter (`gui/browser_cues.rs` new, 49 lines, and `gui/browser_view.rs`): the ring painted over the frame and clipped to the page, the crosshair set when the pointer enters or leaves the page, the header text, the address row following the page. `browser_view.rs` was 814 lines (over the 800 limit) and is 777 now. The address row of a page with the keys no longer shows the page's pre-edit.
- The follow bug found by the window test: `Editor::put` inserts at the cursor, so the new address was appended to the old one; the row is cleared first.
- Spec: BROW-09 in the product spec; the guide (`doc/manual_end_to_end.md`) says the limits.

## 5. Tests (MEASURED)

| Check | Result |
|---|---|
| `lico-browser-view` contract tests | 39 passed (3 new tests: the hint, the follow rule, the ring; the ring test pins the exact 144 points and the filled disc for a thickness over the radius; the help test names the new sentences) |
| hand-made mutants on the three new functions | 13 made, 12 killed; the survivor (`radius <= 0 \|\| thickness <= 0` reduced to `thickness <= 0`) is equivalent: a radius of 0 or less draws nothing by the loop itself. Five survived the first tests and were killed by the ring tests added |
| window tests `tests/test_browser_tab.py` | 9 passed (about 26 s); the new one reads the header, the ring (`mark`, `mark_pixel`), the crosshair and its change back, and that Ctrl+L, Alt+Left clear the ring and a click gives it back; the navigation test checks the row follows the page after Alt+Left |
| pictures | `private/scratch/202611/20261117/m17_p11_screens/` (`11_click_mark_and_typing_hint`, `12_composing_in_the_header`, `13_typing_goes_to_the_address_row`, `10_f1_help_over_a_page`); read by the lead: the ring is visible, the composition is in the header and the bottom row shows no brackets with text in it |
| `suite run --changed --receipts` | 503 of 503 passed (it found the size ratchet, which led to the split) |
| whole suite, one run | 573 of 573 passed, **160.2 s**, peak **1,901.6 MiB** of 2,048 (margin 146 MiB; one run only), build 58.9 s + 52.8 s test build |
| `cargo xtask check`, `verify`, `graph`, `test` (all components), `accept` | green; graph: the biggest cycle still 48 modules |
| Two clean clones | identical, `4ff192366fd71a6376ec62b678c574fd1a63ed5dce01a96555f845c94a412948` |

## 6. Size (MEASURED)

| | 0.6.0 | 0.6.1 |
|---|---|---|
| binary (the reproducible release script) | 10,820,264 bytes | **10,667,208 bytes (-153,056, -1.41 %)** |

The patch adds code (a module of 49 lines and the component's helpers), so a smaller binary was not expected. The 0.6.0 tree was rebuilt today with the same script and gave 10,820,264 bytes again, so the toolchain did not change; the `.text` section is 8,154,704 against 7,996,032 bytes. The cause is NOT found (it is probably inlining decisions of the optimiser after the changes); the strings of the features checked are all in both binaries (the model list, the memory guard, the install text), the window tests and the system tests passed on release builds of this tree. No memory figure was measured for this patch (no session or per-process data changed; the idle window and host were not re-measured: MEASURED only in the window tests, whose peaks are in the suite run above).

## 7. Installed

`~/bin/lico` is 0.6.1 (`lico 0.6.1 (commit 50eb786, build 6d47f03fc781e028)`, mode 0700, replaced by rename; running hosts keep the old binary until the next start: a window opened after it asks the host to be replaced as before). The 0.6.0 binary is at `private/release_0_6_0_backup/lico`; the 0.6.1 artifact at `private/release_0_6_1/lico`. Recorded as done by the lead under the owner's delegation (the release install); to be confirmed by the owner.

## 8. Known limits and what is open

- The ring is an approximation of "where the page believes the focus is": it marks the last click, not the focus (a click on a link or a Tab key inside the page moves the focus with no ring).
- The engine still paints Korean in a single-line `<input>` as boxes; whether to report it upstream (publication) is the owner's call.
- Google search stays blocked; stealth is the owner's decision.
- Nothing is pushed and nothing is merged (G3 and G4 are the owner's) for M16, M17 (P01 to P11), M18-P01 and M19.
