# M24-P11c: a click selects the whole block, and a block folds (the transcript view, third part)

- Version: 1.0.0
- Date: 2026-11-26
- Branch: `phase/M24-P11c-whole-block-select-and-fold` from `phase/M24-P11b-gutter-and-kind-colours` (`72b4d7e`); code `bcd79c1`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-24 (quoted in the [plan](../../../plans/202611/20261126/20261126_01_M24-P11c_whole_block_select_and_fold_plan.md)): "대화를 클릭하면 현재 한줄씩 클릭 비슷하게 되는데 해당 대화 전체가 통으로 선택되어 펼치기 닫히기 가능(표시 메모리 줄이기?)" and "이게 0.8.2의 내용"; and of 2026-11-26: the window he ran showed only the cards of tools folding
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test and in two screenshots of the window)

| | Before | Now |
|---|---|---|
| A click on a line of your message, a peer's card or the agent's reply | selects that one line | **selects the whole block** (from its first row to its last, the room with the time included); nothing is copied by the click |
| A second click on the block that is selected | selects the same line again | **folds it** to one row, `▸ the first words  · 12 lines`, in the dim ink of the tool cards' rows; the time beside it is the first line's |
| A click on the folded row | | **opens it** again (and the block is selected) |
| A command or tool card | one click folds or opens it | the same |
| A drag, a shift-click, a panel, the call room, the internal-events view | as they were | as they were |
| A note of the window, a short line that has nothing to fold | one line | the line click it had |

A block is foldable when it has two lines or more, or one line longer than 84 characters. The folds are the window's own: they are not saved, a block that is folded stays folded while the window runs, and at most 512 are kept (the set starts again when it is full).

## 2. How it is built (READ)

- `gui/blocks.rs`: `block_span(roles, pos)`, the first and the last line of the block that holds a line, found with the same rule (`starts_block`) that gives the gutter its times, so that the click and the time agree by construction.
- `gui/block_fold.rs` (new): `State { folded, before }`, `foldable`, `pill`, `apply` (pure, over the visible lines, only when the set is not empty: a window that never folds pays nothing) and `App::try_block_click`.
- `gui/transcript.rs`: `ROLE_BLOCK_FOLDED` (a new bit) and the pill's ink; `visible_transcript_lines` folds the cards first and then the blocks.
- `gui/clip.rs`: a press overwrites the selection with the pressed line (a drag has to start there); it now keeps the selection it overwrote, and the release (stationary, no shift) asks, in this order, the tool card, the jump from a call and the block: the second click is the click on the block that **was** the selection.
- **The memory**: a folded block is one entry of the visible list, so its lines are not laid out, shaped or drawn while it is folded (the window shapes only the lines of a screenful, but the list is made in every frame and a long reply holds many entries of it).
- The module graph is unchanged (the cycle stays at 50). The size ratchet holds (`ui.rs` 1114, `mod.rs` 797).

## 3. Checks (MEASURED)

- **Tests first**: the pure tests and the click tests were run on stubs and failed, then green.
- **The tests now**: unit 28 new (`blocks` 2, `block_fold` 26), the scroll oracle 1 new (at the scales 1, 1.25 and 2: a folded block draws the same pixels with the scroll step as with a full repaint, and a toggle drops the kept frame), the window test `tests/test_block_select.py` (a real window). The whole binary: 1,281 unit tests pass.
- **Mutation checks**: **49 mutants** of the new code (`block_span` in `blocks.rs`, all of `block_fold.rs`, the pill's ink and the call in `transcript.rs`, the lines added to `clip.rs`): **49 killed, none survived** (the first run of 25 was cut short by the need to build the release for the owner's window, which cannot be built while a mutant stands in the tree; the other 24 were run on the restored tree, whose files were checked by their sha256 after every mutant). The tests that killed them include three found while writing them: a pill of a block that is already folded is foldable by its own marker (else it could never be opened), a growing reply keeps its key and its count grows, and a full set starts again.
- **The commit hook**: the first run refused: two window tests were red under the load of four workers (`test_unicode_clients`: the typing tool dropped the characters `漢字` and the first letter of `שלום` of the text it typed; `test_agy_tile`: the tab's picture was not drawn yet when the test read it); neither touches what this phase changed, and both passed alone by hand (5 tests in 2 files, 8.9 s). The first run: 310 tests in 85 files, 308 passed, 192.8 s, the build's peak 1485.0 MiB under the bound of 1536 MiB. The second run of the hook took the receipts of the files that had passed and accepted the commit; the limits were not touched.

## 4. What is not decided (the owner's)

- The first click selects the block but does not copy it (a finished drag does, one line clicked did not); Ctrl+C or the Copy menu copies the selection. Whether the first click should copy is the owner's.
- A block that is folded and selected copies as its row (as the cards' rows do); to copy the text of a block, select it with the first click and copy before the second.
- A double click is two clicks: it selects and then folds (there is no word selection in the transcript).
- Nothing is folded by the window by itself to spare memory (a block is folded only by the person); the screenful is the only part that is shaped already.
- The ink of the folded row and its form (`▸ ` and `  · N lines`) are the lead's choice, one place each in `gui/block_fold.rs` and `gui/transcript.rs`.
- The theme button (P11d) and the file under the pointer (P11e) come next; the release is `0.8.2`.
