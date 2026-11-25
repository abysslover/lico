# M24-P11c Plan — The Transcript View: A Click Selects The Whole Block, And A Block Folds

- Status: done (2026-11-26), code `bcd79c1`; not pushed (published with the release 0.8.2)
- Version: 1.0.0
- Branch: `phase/M24-P11c-whole-block-select-and-fold` from `phase/M24-P11b-gutter-and-kind-colours` (`72b4d7e`; P07 to P11b are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-24, the content of 0.8.2), the part of this phase: "대화를 클릭하면 현재 한줄씩 클릭 비슷하게 되는데 해당 대화 전체가 통으로 선택되어 펼치기 닫히기 가능(표시 메모리 줄이기?)"
- Standing authorization of 2026-11-25: the prefix, the commits and the merge are approved in advance; no version bump (the release 0.8.2 carries it).

## What this phase gives the person

Today a click on the conversation selects **one line** of it, and a long reply or a message of several lines is picked line by line. After this phase:

- **A click on a block selects the whole block** (the person's message, a peer's card, the agent's reply: the blocks of P11b), highlighted from its first row to its last.
- **A second click on the block that is selected folds it** to one line, `▸ the first words  · 12 lines`, and a click on that line (the block is selected again as one line) **unfolds it**. A block that is folded takes one row, and the lines it held are not laid out, shaped or drawn while it is folded.
- A command or tool card keeps what it has had since M09: one click folds or unfolds it (it is folded to begin with).
- A drag still selects line by line and is copied as before; shift-click still extends; a panel, the call room and the internal-events view keep the click they have.

## Why it is built this way (what was found)

- The window already has one fold, for the cards of commands and tools (`tool_fold.rs`): `compute_visible_transcript` gives the lines, `tool_fold::apply` turns a card into one pill line whose `(raw index, seq)` are the card's first line's, `expanded_tools` holds the keys `(session, head seq)` of the cards the person opened, and a click toggles the key. The same shape is used here for the other blocks, with the **opposite default**: a block is open until the person folds it (the owner did not ask for the replies to be hidden), and the key set holds the folded ones.
- A block is what P11b defined (`blocks::starts_block`, `kind_of`): the block of a line is found by walking the visible lines of the transcript, so the click and the fold agree with the gutter's times by construction. The time of a folded block is the time of its first line (the pill keeps the first line's raw index and seq).
- A press already overwrites the selection with the pressed line (a drag must start there). The second click can only be told from the first by what was selected **before** the press, so the press remembers it (`BlockState.before`); a drag, a shift-click and a panel never read it.
- A selection is a range of raw indices (`ui.sel`), painted over the lines of `hit_lines` that fall in it; a block's range is its first and last visible raw index. After a fold or an unfold the selection is set again to the range of the block as it now shows, so that "the block is selected" is always `sel == its range`.
- The memory: the lines of a screenful are the only ones shaped (`build` takes `first..end`), but the visible list is made every frame and a long reply holds many lines of it; a folded block is one entry. The set is capped (512 keys) like the tool cards' set.

## Decisions

| # | Decision | Detail |
|---|---|---|
| D1 | **Which blocks** | `Kind::User`, `Kind::Peer`, `Kind::Response` select as a block and fold; `Kind::Command` keeps the card's own click (tool_fold); `Kind::Note` is one line and keeps the line click |
| D2 | **First click** | a stationary click (no drag, no shift) on a line of a block that is not the one selected: `ui.sel = (first raw, last raw)` of the block. It is **not copied** (as the click on one line is not now; a drag still copies and Ctrl+C or the Copy menu copy the selection) |
| D3 | **Second click** | a stationary click on the block that is the selection (`sel == the range of the block`, read from what was selected before the press): if the block is foldable, its key is toggled in the folded set and the selection is set to the range the block has now; a block that cannot be folded stays selected |
| D4 | **Foldable** | a block of two lines or more, or of one line longer than 84 characters (the pill's width); a short single line has nothing to fold |
| D5 | **The pill** | `▸ ` + the first line clipped to 84 characters + `  · N lines` (`N` = the lines of the block; `1 line` for a long single line), the role of the first line (so the colour says who spoke, and the gutter shows the first line's time) with `ROLE_BLOCK_FOLDED` (new bit, `1 << 12`) and, for a peer's card, both borders (`ROLE_BOX_TOP \| ROLE_BOX_BOT`) so that it is one closed row; its ink is the dim ink of the tool cards' pills |
| D6 | **The key** | `(session, seq of the first line of the block)`; a reply that goes on growing keeps its key and the count in its pill grows; a block that is gone from the log is simply never looked up (the set is capped) |
| D7 | **Where it applies** | `visible_transcript_lines`, after `tool_fold::apply`, only when the set is not empty (no cost for a window that never folds) and not in the internal-events view or the call room (as the tool cards) |
| D8 | **The dump** | `blocks` in the state dump: `folded` (keys count) and `selected` (the range) so that the window test can read them; `selection` (the text) is there already |

## The pieces

1. `gui/blocks.rs`: `block_span(roles, pos) -> (first, last)` (pure: the entries of the block that holds `pos`), tests in `blocks_tests.rs`.
2. `gui/block_fold.rs` (new): `State { folded, before }`, `foldable`, `pill`, `apply` (pure over the visible entries), `App::try_block_click`, the dump; tests in `block_fold_tests.rs`.
3. `gui/transcript.rs`: `ROLE_BLOCK_FOLDED`, the pill's ink in `tint_role`, the call in `visible_transcript_lines`.
4. `gui/clip.rs`: the press remembers the selection it overwrites; the release asks `try_block_click` after the tool card and the call jump.
5. `gui/ui.rs` (pinned at 1114: only joined text, no line added): the field `block_fold` of `UiState`, the dump key.
6. Tests: unit (the pure functions and the click on a built `App`), the scroll oracle (a folded block draws the same pixels with the scroll step as with a full repaint; a toggle drops the kept frame), a window test (`tests/test_block_select.py`: a real window, a session with a two-line reply and a one-line reply; the click selects the whole reply, the second folds, the dump says so).

## Steps

1. [x] **Tests first, run red.**
2. [x] **The change.**
3. [x] **Mutation checks**, one edit at a time on `blocks.rs` (the new function), `block_fold.rs` and the lines added to `transcript.rs` and `clip.rs`.
4. [x] **Other tests**: the existing click, selection and tool-card tests, the oracle, the suite.
5. [x] **Words**: the guide.
6. [x] `cargo xtask check` and `graph`, the hook, `~/bin/lico` rebuilt, the report.

## Not decided here (the owner's, written in the report)

- Whether the first click should also copy the block (the window copies a finished drag; one line clicked is not copied now, so a block is not either).
- Whether a folded block should keep its time shown (it does: the first line's) and whether blocks older than some age should fold by themselves to spare memory (not done: nothing is folded that the person did not fold).
- A block that is folded and selected copies as its pill line (as the tool cards' pills do); to copy the text of a block, select it with the first click and copy before the second.
- The theme button the owner asked for on 2026-11-26 (P11d) comes next, then the file under the pointer (P11e).

## Results (2026-11-26)

- **Tests first**: the pure tests (`block_span`, `foldable`, `pill`, `apply`) and the click tests were written before the code and run red on stubs; then green. The oracle test and the window test were added with the change.
- **The commit** `bcd79c1`: `gui/blocks.rs` (`block_span`), `gui/block_fold.rs` (new: the state, `foldable`, `pill`, `apply`, `App::try_block_click`), `gui/transcript.rs` (`ROLE_BLOCK_FOLDED`, the pill's ink, the call), `gui/clip.rs` (the press remembers the selection it overwrites; the release asks the block), `gui/ui.rs` (the state and the dump key, joined into existing lines), `tests/test_block_select.py` and its map entry, the guide `1.29.0`. D1 to D8 held; one wording changed: a pill is foldable by its own marker (D4), so that a short folded block can be opened again.
- **Tests now**: unit 28 new (`blocks` 2, `block_fold` 26: the pure functions 10, the click on a built window 14, the look 2), the scroll oracle 1 new (a folded block draws the same pixels with the scroll step as with a full repaint, at the scales 1, 1.25 and 2), the window test `tests/test_block_select.py` (a real window: the first click selects the whole block, the second folds it, the third opens it). The whole binary: 1,281 unit tests pass.
- **Mutation checks**: **49 mutants** of the new code (`block_span` in `blocks.rs`, all of `block_fold.rs`, the pill's ink and the call in `transcript.rs`, the lines added to `clip.rs`): **49 killed, none survived** (the first run of 25 was cut short by the need to build the release for the owner's window, which cannot be built while a mutant stands in the tree; the other 24 were run on the restored tree, whose files were checked by their sha256 after every mutant). The tests that killed them include three found while writing them: a pill of a block that is already folded is foldable by its own marker (else it could never be opened), a growing reply keeps its key and its count grows, and a full set starts again.
- **The commit hook**: the first run refused: two window tests were red under the load of four workers (`test_unicode_clients`: the typing tool dropped the characters `漢字` and the first letter of `שלום` of the text it typed; `test_agy_tile`: the tab's picture was not drawn yet when the test read it); neither touches what this phase changed, and both passed alone by hand (5 tests in 2 files, 8.9 s). The first run: 310 tests in 85 files, 308 passed, 192.8 s, the build's peak 1485.0 MiB under the bound of 1536 MiB. The second run of the hook took the receipts of the files that had passed and accepted the commit; the limits were not touched.
- **The owner's window**: the window the owner ran on 2026-11-26 was the build of P11b (`72b4d7e`), which has no fold of the ordinary blocks (only the cards of commands and tools fold, since M09); `~/bin/lico` was rebuilt from this tree and the window has to be restarted to show it.
- **Next**: P11d (the theme button the owner asked for on 2026-11-26: dark, light, system), then P11e (a file under the pointer is underlined and opens in a tab).
