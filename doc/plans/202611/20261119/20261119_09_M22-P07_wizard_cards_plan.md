# M22-P07 Plan — The Wizard's Second And Third Steps As Pictures, Not Text Rows

- Status: done (2026-11-19), commit `40481b9` (the prefix `phase/M22-P07-` approved by the owner at commit time); not pushed
- Version: 1.0.0
- Branch: `phase/M22-P07-wizard-cards` from `phase/M23-P03c-terminal-start-folder` (`341c2ca`; `main` is `708bdd9`, P03c is committed and not pushed)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md), after the release `0.8.0`
- The owner's words (2026-11-19): "세션 -> local LLM -> 텍스트 기반 모드로 다시 가버리는데, 원래 Local LLM 찾을 때 썼던 modal 창이라거나 원래 wizard style의 깔끔하고 단순한 UI를 유지할 수 없나? 지금 텍스트를 많이 보여주기 보다 선택 가능한 옵션을 깔금하게 보여주는 게 좋을 것 같음.(즉 Host에 따라서 선택 가능한 것을 보여주고 그걸로 실행) 다른 Claude라거나 Codex 선택 옵션에서도 텍스트 대신 일종의 UI 형태로 단순하고 simple한 UX/UI를 제공하면 좋겠음."
- This phase changes no operation and no setting. It changes what the window paints for steps two and three of the New session wizard, and adds the click on a picture.

## What was found (2026-11-19, from screenshots of the 0.8.0 build)

- Step one is four pictures (Claude, Codex, Local LLM, Terminal) and is the clean style the owner means.
- Step two (Local LLM: "Server", "Back", "Next", a line of keys, the heading "Model servers on this machine", the servers as text lines, a note of two lines) and step three ("Name", "Model", "Effort", "Permission" as `< value >` rows) are the one-page form's text rows shared out over steps (M22-P04b); Claude and Codex step two is "Account" and "Folder" as text rows with the recent folders as text lines. Everything is chosen with Left and Right and Space on a row, which is what the owner calls the text mode.
- The window already has what is needed to paint a choice: `Controls::text` and `button`, the line rectangles of the body (`hit_lines`), the icons (`icons::draw`), the colours of the first step's squares (`picker.rs`). The "+ New chat" modal (`newchat_draw.rs`) and the "Find Local LLM servers..." dialog (`llm_find.rs`) are the other two clean looks.
- The Local LLM server list: the wizard (and the one-page form) look for servers on **this machine only**, because a local session asks the model server on this machine; the Find dialog also looks on an ssh host, but only to show what is there (read-only). So "by Host" is, for sessions, this machine; the ssh host stays the Directories tab's Host choice (`+ New chat` there starts a session on that host).

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **Pictures are painted over the rows; the rows stay the model.** Every row of the form keeps its line of text (so the keys, the dump, the one-page form `File > New session options...` and the tests of the rows are as they were); the window paints, over the line of a row that has choices, the choices as chips, and over a list of lines (the recent folders, the servers) cards. | the smallest change that removes the text mode without removing a function; the owner's "no menu function removed" rule |
| D2 | **Chips** (one row of small boxes, the chosen one filled, the one the keys are on ringed): the account (its provider's picture and its label, then `+ New account`), the model, the effort, the permission (`Skip agent prompts` / `Ask for approval`), the worktree. **Cards** (one box per line, a picture at the left): the recent folders, the model servers, Back, Next and Start. | a short list of words reads best as chips, a list of paths and servers as cards |
| D3 | **A click on a chip sets that value** (`Action::WizardChoose(line, index)`); a click on a card does what a click on that line did (a recent folder is taken, a server is chosen, Back, Next, Start). Left, Right, Space, Enter, Tab and Esc work as before | nothing is removed |
| D4 | **The line of keys is blank in the wizard** (the one-page form keeps it); the two notes (what a folder's own settings do, what a local session can do) stay, they say what the screen cannot | less text, but no loss of a fact the person needs |
| D5 | **No new version number here**; the release with the fix of M23-P03c and this phase is decided after the owner has looked at `lico gui` | the owner decides what is a release |
| D6 | **Not in this phase:** a Host chooser inside the wizard (see the finding above), the big pictures for accounts, and a card for the Find dialog's servers (its "use this server" is a later step) | each needs a decision of its own |

## Steps

1. [x] **Tests first**: Rust unit tests for the cards of each step (`wizard_cards`: which rows, which items, which is chosen) and for the click (`wizard_choose`); a window test that opens the wizard and clicks a chip and a card (the state changes, the painted boxes are where the dump says).
2. [x] **The model** (`newsession_wizard.rs`): `wizard_cards`, `wizard_choose`, the blank line of keys.
3. [x] **The painter** (`wizard_cards.rs`): the veil over the row's text, the chips and cards, the ring, the targets, the rectangles for the dump.
4. [x] **The action** `Action::WizardChoose` in `controls.rs` and its handling.
5. [x] **Gates**: the changed-files suite, `cargo xtask check`, mutation checks, the owner's build (`lico gui` shows the pictures).
6. [x] **The guide** (`doc/manual_end_to_end.md`) says what the steps look like.

## Not decided here

- Whether the pictures should be larger (step one's squares) for the accounts too.
- A Host chooser in the wizard and a "use this server" on the Find dialog.

## Results (2026-11-19)

- **What the window paints now.** Step two and three of the Claude and Codex wizards: the account as chips (its provider's picture and label, then `+ New account`), the recent folders as cards, `Sign in to this account`, Back, Next and Start as cards, the model, effort, permission (`Skip agent prompts` / `Ask for approval`) and worktree as chips. The Local LLM wizard: the model servers as cards, `Look for model servers` / `Look again` as a card, the model and effort chips on step three. A chosen chip is filled, the one the keys are on is ringed. A Next card that cannot be used says why and a click on it changes nothing. The line of keys is blank in the wizard and kept in the one-page form `File > New session options...`.
- **What did not change.** The rows (text) are still the model: the keys (Left, Right, Space, Enter, Tab, Esc), the `ui` dump's rows, the one-page form and the older tests of the rows are as they were; no operation, no setting, no version number (D5).
- **The click.** `Action::WizardChoose(line, index)`: a chip sets that value; a card does what a click on its line did; the chips' row is one row-wide target, so a click beside the chips changes nothing; a second click on the chosen `+ New account` chip goes on to the step that names the new account.
- **Tests first.** Unit tests (`newsession_wizard_tests.rs`, the filter `wizard` 43 of 43) for the chips and cards of every step, the clicks (including the second click on `+ New account`), the disabled Next card with its reason, the worktree chips, the single default chips of Codex, the server cards, the blank keys line and the stray click; the box geometry has 3 unit tests of its own (`wizard_cards.rs`). Two window tests (`tests/test_icon_toolbar_wizard.py`, 5 of 5 in the file): steps two and three of the account wizard (boxes where the dump says, a click chooses, the host received opus, high and the account) and the Local LLM wizard (server card, `Look again` card with the veil checked, Model chips, Start card).
- **Mutation checks.** 26 changes (22 on the model and the painter at unit level, 4 at window level, built into the release binaries: no veil over the row's text, the chosen chip not filled differently, a chip that is not a target, the covered row that is not a target); all 26 caught, the clean runs pass. One window assertion was weak (the veil right of the last chip did not tell "no veil" apart) and was replaced by a check beside the short `Look again` card, after which the mutation was caught.
- **Gates.** The commit's changed-files suite: 244 of 244 in 132.3 s, peak 966.6 MiB of 1536; `cargo xtask check` passes; `cargo clippy --all-targets` gives the same findings as at the parent commit (139 warnings and 5 errors on the program, 155 and 6 on the tests, all errors in code this phase did not touch, such as `gui/editor/app.rs`; none in the changed files).
- **Limits seen** (not changed here): a row is one line high, so the boxes are about 16 px tall (a roomier layout would need spacer lines, which changes the line numbers the keys and the tests use); chips that do not fit in the row's width are not drawn (the keys still reach them); a chip has no hover effect yet. The owner may want any of these changed.
- **The Host question** (D6) stays open: a local session asks the model server on this machine, so the wizard lists this machine's servers; an ssh host stays the Directories tab's Host choice.
