# M25-P09 Plan: Answers Begin Folded, agy Begins In `auto`, A Handed-Over agy Conversation Is Its Number

- Status: done (2026-11-27)
- Version: 1.0.0
- Branch: `phase/M25-P09-agy-auto-default-and-id-handoff` from `main` (`648eb50`, `lico 0.9.0`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), rows P09 to P12 (the `0.9.1` patch release)
- The owner's words (2026-11-26, after trying `0.9.0`): (1) "이제 좋긴 한데 기본적으로 "내가 적은 글" 외에 다른 창들은 숨겨진 상태로 시작할 수 있도록 할 수 있나?" (2) "auto 는 모든 요청에 allow_once로 답하므로 원래 기본값은 모두 허용 bypass permission on이 기본값이어야 함." (3) "대화 전체 넘겨주기는 정확하게 모르겠는데 기존 대화의 세션 번호를 알려줄 뿐 전체 대화를 알려주는 형태를 원하는 게 아님."
- The owner's answers to the two questions of this phase: fold scope = "내 글 빼고 전부 접힘"; publication = "0.9.1로 끝까지 진행" (the guard prefixes, the commits, the merge and the push, as for `0.9.0`).

## What this phase changes

1. **Answers and peers begin folded.** In the transcript only the text the person wrote begins open. An answer of an agent (the one that has just arrived too) and a text that came from a peer session begin as the one-line pill `> first words  / N lines`; a click opens one and a click folds it again (the fold of `M24-P11c`, reversed as a default). A short answer of one line is not foldable and stays open; the cards of a tool call or a command already began folded (`M24-P06`). A switch in *More > Settings*, **Start answers folded: on / off** (default on), brings back the old behaviour: with it off every block begins open and a click folds it. Changing the switch forgets what the person turned over, so that the screen always agrees with the switch. The setting is the host's (`fold_answers`, reported by `status` only when it is off, so a status of an older window is unchanged).
2. **Select and Send to give the text, not the pill.** A selection that holds a folded block gives the whole text of the block (copy, Send to, `Ctrl+C`); until now it gave the pill. This is the one change of an earlier phase's behaviour (`M24-P11c` pinned the pill) and the owner's rule that Send to sends what the person sees as text.
3. **An agy session begins in `auto`.** The window's agy picture, a Chats card of an agy conversation and the form's default start an agy session in `auto`: the host answers each permission request of the program once with the narrowest yes (`allow_once`) and writes `[auto-allowed] <prompt>` first (P03, unchanged). This is the owner's "bypass permission on" read as the host's own `auto`; **the program's own flag that skips its permissions is still never passed** (the security note, gap 2, is amended). `default` stays one switch away in the Session menu and asks the person.
4. **Handing over an agy conversation gives its number, never its text.** For a Claude conversation *Hand over conversation* is unchanged (the whole text, as a file or inline). For an agy session it now sends the receiver one short note: `agy conversation number: <id>` and the line "only the number is given: the text of the conversation is not sent". The panel row says so before the person confirms ("Conversation number (only the number is sent, never its text)"). A session with no conversation yet says `no_conversation_yet`; an id of an odd shape is refused (`bad_request`); the receiver is checked as before.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | "다른 창들" is read as the **blocks of the conversation that are not the person's own** (agent answers and peer texts), not other tabs or panes | the sentence follows "이제 좋긴 한데" about the transcript of `0.9.0`; the owner chose "내 글 빼고 전부 접힘" |
| D2 | A streaming answer that grows past one line **collapses to its pill** while it arrives | the owner chose the just-arrived answer folded too; the click opens it and it stays open until the person folds it |
| D3 | The setting default is **on** and the switch is in More > Settings | the owner asked for it as the default; the old look stays one click away |
| D4 | "bypass permission on" is the host's `auto` (`allow_once` to every request), **not** the program's skip flag | the flag would make the program stop asking itself and the host could neither see nor record the request; `auto` keeps the `[auto-allowed]` line in the transcript |
| D5 | Hand-over of an agy conversation is the **number only** | the owner's words; the program's own files are never read for it |
| D6 | The rig of the host tests seeds `fold_answers: false` unless a test asks for folded answers | the old window tests were written for open blocks; the new tests set `answers_folded=True` |

## Steps

1. [x] **Tests first**: pure tests of `begins_folded`, the key that turns a block over, a short answer that stays open, the setting change; window tests (default folded, click opens, click folds, a later answer, the menu entry, the sync from the settings); the selection test says the whole text; the handoff tests (host unit, window panel, host Python); the mode tests expect `auto`.
2. [x] `gui/block_fold.rs` (`start_open`, `begins_folded`, `open_block_lines`), `gui/transcript.rs`, `gui/clip.rs`, `gui/attn.rs`, `gui/menu.rs`, `clipboard/settings.rs`, `ipc.rs`.
3. [x] `transfer.rs` and `gui/sendto.rs` (the number only), `gui/sidebar_chats.rs`, `gui/newsession_wizard.rs`, `new_session.rs` (the mode).
4. [x] The host tests (`tests/test_agy_chats.py`, `test_agy_tile.py`, `test_agy_peers.py`, `test_block_select.py`, `test_menu_tree.py`) and the unit tests green; the release build.
5. [x] Mutation checks of the new logic; the plan's results; the guide, the security note (gap 2, decision D5), the milestone rows, `PLAN.md`, the index; the commit (the hook); `~/bin/lico` rebuilt.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| C1 | with the default setting an answer of 2 or more lines and a peer text begin as a pill; the person's own text, a one-line answer and the cards of tools do not | unit tests (`block_fold_tests.rs`), host test (`tests/test_block_select.py`) |
| C2 | a click on a pill opens the block, a click folds it, and a later answer arrives folded | window tests |
| C3 | the menu entry flips the setting, the host keeps it, the window follows (`settings` reply) and the toggles the person made are forgotten | window tests, host test |
| C4 | a selection or a copy of a folded block is the whole text | unit test (`sendto_tests.rs` selected lines, `block_fold_tests.rs`) |
| C5 | the agy picture, a card and the form's default start `auto`; `default` can still be chosen | host tests (`tests/test_agy_tile.py`, `tests/test_agy_chats.py`) |
| C6 | handing over an agy conversation delivers the number and no text, with the note under the header of the sender; no id, an odd id and a program other than Claude or agy are refused | unit tests (`transfer/tests.rs`, `sendto_tests.rs`), host test (`tests/test_agy_peers.py`) |
| C7 | no middle dot, no local path, no secret in any new text; no size cap crossed | the guards of the commit |

## Personal data and security

- The setting `fold_answers` is a boolean in the host's settings file; no personal data.
- `auto` for agy is the existing P03 behaviour (the narrowest yes, `[auto-allowed]` recorded, never an `always` choice). The default changes who answers a request the first time; it is written in the security note (gap 2) and in the guide, and `default` is one switch away.
- The conversation number is an id the program made; it is validated (`valid_provider_session`) before it is sent and carries no text, path or account.

## Not decided here

Whether the owner wants a Trash for what a fold hides, or a fold of long agent answers by length instead of by kind; whether `auto` for an agy session should be marked in the tab (it is on the strip and the header as for the others); handing over the text of an agy conversation (the program's files are not read; needs the real program).

## Results

(2026-11-27)

What was made: the list above. The one change of an earlier phase's behaviour is the selection of a folded block (item 2): `M24-P11c` pinned the pill, the tests now say the whole text.

### Tests (against the owner's budget of about 100 unit tests per turn)

- 10 new unit tests: `block_fold_tests.rs` (what begins folded, the key that turns a block over from the way it began, a short answer that stays open, only peers and agent answers begin folded, the setting changing forgets the toggles and the same word changes nothing, the window flow of default-folded / click opens / click folds / a later answer begins folded, the menu entry and the host's answer, the sync from the settings), `transfer/tests.rs` (an agy conversation is its number, never its text, no other program is added) and `sendto_tests.rs` (the panel says that no text goes). The existing tests that pinned the old behaviour were changed (the selection of a folded block, the menu now `23` entries, the oracle window starts with every block open).
- 2 new host tests (`tests/test_block_select.py`: the conversation begins with the answer folded and the setting turns that off; `tests/test_agy_peers.py`: handing over an agy conversation gives the receiver its number and never its text) and the host tests of the mode now expect `auto` (`test_agy_tile.py`, `test_agy_chats.py`, `test_menu_tree.py`). The rig of the host tests seeds `fold_answers: false` unless a test asks for folded answers (D6).
- The unit set of the touched modules (`block_fold`, `sendto`, `transfer::`, `menu_tests`, `pane_cue`, `scroll_oracle`): 111 passed unmutated; the six host files: green.

### Mutation checks

17 mutants, one edit each of `block_fold.rs`, `clip.rs`, `attn.rs`, `clipboard/settings.rs`, `ipc.rs`, `transfer.rs`, `sendto.rs`, `newsession_wizard.rs` or `sidebar_chats.rs` (the script and the log are in the session's scratchpad, not in the repository): **17 killed, none survived** (the four of `begins_folded`, the three of the setting change and the toggle, the selection, the window's reading of the setting, the host's default and its allowed key, the three of the hand-over, the panel, the tile's mode and the card's mode). The unmutated tests were run first. The sources are byte-identical to their state before the round (checked with `cmp` against the backups).

### Found on the way

- One host test of the hand-over (`test_agy_peers`) failed once with `session_disconnected` under load and passed alone four times: it now waits for the receiver's `[session started]` before the hand-over.
- The window's own tests had assumed open blocks: the oracle of the scroll tests starts with `start_open: true` so that its pictures stay the ones it was written for.

- **The first changed-files run of the staged tree was red: 14 of 610 failed** (the by-hand run before the commit, as in the earlier phases). Three causes, none of them excused: (1) the protocol's schema (`src/lico_protocol/v1.py`) did not know the setting `fold_answers`, so every window test that checks the host's `settings` reply against it failed (`$.settings.fold_answers: unknown field`): the field is now in the `settings` reply, in `settings.set` and in the list of fields of which one is required, and the protocol note is `0.1.16` (a paragraph says what it is and that `status` carries it only when it is off); (2) eight window tests that read the text of answers and peers met pills: the rig of `ProductWindow` seeded `fold_answers: false` after a host of another build had started (a host reads its settings file once, at its start) and in the wrong folder for a host the test starts with its own `--state-dir`, and three tests that start a bare host (`test_first_view`, `test_editor_selection`, `test_frame_cost`) had no seeding at all: `rig.open_answers(folder)` is the one helper, `ProductWindow` calls it before any host starts and for the test's own `--state-dir`, and the five bare-host tests call it; the rig does **not** seed every host, because `status` would then carry `fold_answers: false` for all of them; (3) **the installed `agy` is `1.5.18` now** (it updated itself on this machine) and the contract test of the pin (D9 of the milestone) failed on the version, as it is meant to: the record, the simulator, the three fixtures, the README line and the test's constant were made again for `1.5.18` after the test's own probe (`initialize`, `session/new`, the modes, the error codes, in an empty home) answered exactly as the record says; nothing else of the record changed.
- The second changed-files run by hand on the staged tree: `677` tests in `138` files, `677` passed, **`194.5 s`** (over the figure of `180 s`, as in the earlier phases; the hook's limit is `300 s`), peak `1,031.1 MiB`; nothing was raised or bypassed.

### Not done here

The real turn of `agy` (G3). `auto` as the default is the owner's words read as the host's own `auto` (D4); the program's own flag that skips its permissions is still never passed.
