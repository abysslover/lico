# M24-P03 Plan — The Account Screen In Pictures

- Status: done (2026-11-24)
- Version: 1.0.0
- Branch: `phase/M24-P03-account-screen` from `phase/M24-P02-one-touch-start` (`6c45a98`, which is `main` too: P08, P09 and P02 are published, PR #9); the prefix `phase/M24-P03-` was approved by the owner at commit time (the question on the prefix, 2026-11-24) and is in the local guard; the code is `45e6caf`
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md) (0.4.0), R1 and R5's account half; the shape was the owner's pick on 2026-11-24 ([P01 report](../../../reports/202611/20261124/20261124_01_M24-P01_phase0_one_touch_menu_and_panes_report.md), sections B and G)
- The owner's words (2026-11-24): "계정으로 오면 다시 텍스트가 보이고"; the pick: "카드 + 작은 그림 5개" (a card per account, five small pictures, a plus card).

## What this phase changes

The account screen (`Ctrl+A`, the person icon of the toolbar) was a list of text lines and, for one account, five lettered lines (`S sign in`, `R rename`, `F make it the default`, `D remove`, `E remove and end its sessions`). It is now:

1. **The list**: a card for each account (the provider's picture, its label, an orange star on the default one, a disc in the colour of what is left of its quota) and a card with a plus for a new account. No account yet: the plus card alone.
2. **An account's page**: five pictures of the size of the new-session pictures, in a row: a key (sign in), a pencil (rename), a star (make it the default), a bin (remove it, its sessions stay) and a bin with a cross (remove it and end its sessions). A word under each; the one asked for once is lit and says `Sure?` until the second click.

## What was found (2026-11-24)

- The wizard's boxes (`M22-P07`, `gui/wizard_cards.rs`) are the pattern: the text lines stay the model (keys, dump, notices) and a veil of the window's ground covers them while the pictures are painted over; the pictures are targets of their own. The same is done here.
- A click on a text line of the list already opens the account or adds one (`accounts_or_label_click`): a card covers the line of its text, so **no target of its own is needed** for the cards. (The first version had one; the mutation checks showed it to be dead code, and it was removed.)
- The page's five lines have no click of their own: the five pictures are targets, `Action::AccountAct(key)`, which run the key.
- The icon set (`gui/icons.rs`) had no key, pencil, star or bin; each is drawn from rectangles, strokes, discs and (new) a polygon filler, and the existing tests (inside the button, centred, all different, none for a too small button) cover them through `Icon::ALL`.
- The quota figures are the percent **left** (`7d 4% left`); the host's own words are `CRITICAL` under 5 and `LOW` under 20, so the disc is red under 5, orange under 20, green above, by the least of the two figures.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The text lines stay under the pictures; every key (`1`..`9`, `A`, `S`, `R`, `F`, `D`, `E`) works as before | the dump, the keys and the tests read them; nothing is lost for a person who types |
| D2 | The sentences that explain (`provider says: ...`, `Sessions bound to it: ... Press D again ...`, the notice after a rename or a removal) stay as text | they carry what the person must know before a removal, and the host's answer; the owner's picture rule is for what can be a picture |
| D3 | Nine cards at most (the digit keys name nine accounts, as before) | the list was already numbered `1`..`9` |
| D4 | The word under the bin with a cross is `Wipe` and the second click's word is `Sure?` | the longer words did not fit under a picture (seen on the screenshots) |
| D5 | The five pictures use the new-session tiles' drawing (`tile_rects`, `draw_tiles`), so they have the same size and the same lit-on-pointer look | the owner's rule: every picture is the size of the Claude and Codex ones |

## Not done here

- The state of the provider's login as a picture (a disc for signed in or not): the sentence `provider says: ...` stays (it needs a check of the provider, which the page makes on opening).
- The menu entry `Accounts`, the toolbar icon and the sign-in screens (`Login`, `Label`): unchanged.

## Steps

1. [x] **Tests first**: unit tests of the model (`tone_of`, the cards with their lines and the plus two lines under the last account or under the sentence, nine at most, no cards on another screen, the five pictures with their keys and the pending one, a card covers the line whose click does what the card says, a picture runs its key) and the window tests on a real host (the list with its cards, a click on a card opens its page, the star makes the account the default, the bin asks and the second click removes, the plus card, the screen with no account). Red first.
2. [x] **The code**: five icons, `gui/account_cards.rs`, the wiring (`Action::AccountAct`, the paint call, the click chain, the dump).
3. [x] **Mutation checks**, before the final green gate.
4. [ ] **Gates and documents**: `cargo test --bin lico`, clippy on the new code, the changed-files suite, `cargo xtask check`, the owner's build; the guide (the accounts paragraph), the milestone row, `PLAN.md` (also: P08, P09 and P02 are published), the index row; the screens in `private/`.

## Results (2026-11-24)

- **What the owner sees** (`lico gui`, `Ctrl+A` or the person icon): the list is a card per account and a plus card; a click on a card opens its page, five big pictures (key, pencil, star, bin, bin with a cross) with a word under each; the bin clicked once is lit and says `Sure?`. Pictures of the four screens: `private/scratch/202611/20261124/m24_p03_screens/` (the list, an account's page, a removal asked once, no account yet).
- **Code**: five new icons in `gui/icons.rs` (500 lines; `Key`, `Pencil`, `Star`, `Trash`, `TrashX`, and a polygon filler); the new `gui/account_cards.rs` (the model: `tone_of`, `account_cards`, `account_buttons`; the painting over the veiled lines; `account_click`; the dump `acct_json`); `Action::AccountAct(char)`; the call in `paint_main_overlays`; the field `ToolbarGeom.acct`; the dump field `panel.acct`; `acct_display` and `accounts_or_label_click` made `pub(crate)`. No host, protocol or `Cargo.lock` change.
- **Tests (red first: the window tests could not pass before the screen existed)**: 8 unit tests in `account_cards.rs` (the tone by the least of the two figures with both limits, the cards with their lines and the plus two lines under the last account or under the sentence, nine at most and the plus after them, no cards on another screen, the five pictures with their keys and the pending one, a card covers the line whose click does what the card says, a picture runs its key, an action that is not ours is not taken) and 3 window tests on a real host (`tests/test_account_screen.py`: the list and a click on a card, the star makes the account the default and the bin asks twice before it removes, the plus card and the screen with no account); the five icons pass the icon tests that cover every icon (inside the button, centred, all different). Whole `cargo test --bin lico`: 1058 passed, 12 ignored; the changed-files suite 290 tests passed.
- **Found by looking at the screenshots**: two captions were cut at the edge of their picture (`Remove + end`, `Click again`): they are `Wipe` and `Sure?` now.
- **Found by the mutation checks**: a click target on every card was dead code, because a click on the line of a card already opens the account or adds one (`accounts_or_label_click`); the targets and their two `Action` variants were removed, and a test now ties each card to the line whose click does what the card says.
- **Mutation checks (45 runs in two rounds; every mutant of the final code is caught)**: unit level 19 mutants in the first round and 17 in the second (the tone limits and the least-or-most, nine cards, the plus line, cards on the wrong screen, the picture of a provider, the star of the default, the keys of the five pictures, the pending one, the picture's screen, every action taken or none), window level 6 and then 3 (no rectangle for the dump of the list or of the pictures, every picture the key `S`); the two survivors of the first window round were the dead targets above.
- **Gates**: clippy as at the parent (139 warnings in the lib, 155 in the test build), `cargo xtask check` passes, the changed-files suite passes (its first run failed only the build-memory guard, 1819 MiB over 1536, with every test green; the rerun, with the build skipped, peaked at 1137 MiB). `tests/testmap.toml` lists the new test file.
- **Decisions that stand until the owner says otherwise**: D1 to D5 above.
- **Not done, said again**: the provider's login state as a disc (the sentence stays); the menu diet (P04) and the pane tool (P05).
