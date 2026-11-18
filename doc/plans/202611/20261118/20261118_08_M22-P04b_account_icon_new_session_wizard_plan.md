# M22-P04b Plan — The Account Icon, A New Session Wizard That Starts With Three Icons, And A Build That Reaches `lico gui`

- Status: done (`192e304`, 2026-11-18)
- Version: 1.1.0
- Branch: `phase/M22-P04b-account-icon-wizard` (stacked on `phase/M22-P04a-tree-menus-three-modes`, `8d498cb`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18, after P04a was installed and opened: "it opens well"):
  1. A build used to reach `lico gui` by itself; make it so again.
  2. Pressing New session draws text; it must show just the three icons, **Claude, Codex, Local LLM**.
  3. Before the plus an **account icon**; New session is not possible without an account.
  4. (The answer to the lead's question) **Local LLM works without an account, of course**: the Claude and Codex icons are grey, Local LLM is not.
  5. **Pressing an icon opens a wizard that goes on step by step.**

## What this phase changes (the window only; the host is not touched)

1. **A build reaches `lico gui` by itself.** `~/bin/lico` is a symbolic link to `target/release/lico`. After `cargo build --release -p lico` the next `lico gui` is the new build (a window of another build asks the old host to be replaced, as at every release). Done 2026-11-18 11:04 by the lead on the owner's request; the `0.7.0` file of the release is kept at `private/release_0_7_0/lico`, the dev build of `8d498cb` at `private/release_m22_p04a_dev/lico`. The release install of P06 replaces the link with a file again (by rename). Risks said to the owner: a broken build is what runs; `cargo clean` or moving the repository breaks the link.
2. **The toolbar is `[Account] [+] [House] [Browser]`** (the terminal's comes with M23). The account icon (a person) opens the Accounts screen (File > Accounts, `Ctrl+A`). With no account at all the account icon is lit (it is the first step). `+` and the house are never greyed (Local LLM needs no account); the browser is as before.
3. **New session (`+`, `Ctrl+N`, File > New session) is a wizard of three steps.** The Home session (the house, `Ctrl+Shift+N`) is the same wizard without a folder.

   | Step | What it shows | Rows (the form's own) |
   |---|---|---|
   | 1 of 3 | **three icons and nothing else**: Claude, Codex, Local LLM (their words only when the pointer or the keys are on one). An icon of a provider with no account is grey and says why when chosen | the tiles |
   | 2 of 3 | Claude, Codex: **Account and folder** (the account, the folder, recent folders, worktree and branch for a git folder, the sign-in row); Home session: the account only. Local LLM: **Server** (the one look at this machine is made at once, the servers found are listed, a click or the keys choose one) | `Account Folder recent.. Worktree Branch SignIn`, or `Server` |
   | 3 of 3 | **Options and start**: Claude, Codex: name, model, effort, permission; Local LLM: model, name | `Name Model Effort Permission Start`, or `Model Name Start` |

   Every step ends with `Back` and `Next` (`Start session` on step 3), as rows that a click or Enter takes. Keys: Enter is Next (Start on the last step; a row with a meaning of its own, the recent folder, the sign-in, the server look, keeps it), **Esc is Back** (closes on step 1), Tab, Up and Down move over the rows, Left and Right change a choice (and move over the icons on step 1), Space and a click act. The title says `New session  ·  step 2 of 3  ·  Account and folder`. A step does not go on while what it shows is not valid (no account, a folder that is not a full path, a branch that git refuses: the same words as the form).
4. **The old single-page form stays** as File > New session options... (the same form, nothing removed: it is the three steps on one page, with the Provider row), and it is what the folder actions of the sidebar and of the Files panel (`+ New here`) open with the folder and the ssh host they name.
5. **The state** (`ui.panel.form`) says `wizard` (`page`, `pages`, `name`, `home`), `tiles` (provider, icon, enabled, focus, rect) and the toolbar says `accounts` as its first button.

## What stays as it is

The keys (`Ctrl+N`, `Ctrl+Shift+N`, `Ctrl+A`), the host, the wire (`session.create` gets the same request from a wizard as from the form for the same answers), the menus, the sidebar. A session is never started by the click on an icon alone: the icon begins the wizard (the owner's choice).

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The wizard is the new-session screen's own panel (`Step::NewSession`) over the same `Form` and the same rows: a page is a window onto `Form::fields()`, with `Back` and `Next` rows added | no function can be lost or duplicated; the validation, the sign-in, the account entry, the recent folders and the Local look are the form's own; the older tests of the form keep a target (File > New session options...) |
| D2 | The icons are drawn by the window from rectangles (`icons.rs`): a person, a spark (Claude), the code brackets `</>` (Codex), a chip (Local LLM); no vendor logo | no font glyph to check, no trademark in a public repository; the owner may ask for other pictures |
| D3 | Step 1 shows no text but the title; the word of the icon the pointer or the keys are on shows under the icons (as the toolbar shows its word) | "just the icons" (the owner); a new person still learns what an icon is |
| D4 | `+` and the house are never greyed; Claude and Codex tiles are grey without an account of their provider, Local LLM never | the owner's answer 4; the account icon is the way to the first account |
| D5 | The wizard does not start a session by a click alone; it goes on step by step | the owner's answer 5 |
| D6 | `Esc` is Back on steps 2 and 3, and closes on step 1 | a wizard's usual key; Esc also still closes when the panel has no wizard |
| D7 | The sidebar's and the Files panel's folder actions keep opening the full form with the folder and the ssh host | they already name a folder (and may name an ssh host that binds the account); a wizard over them is a later step if the owner wants it |

## Steps (tests first; each step is green before the next)

1. [x] **Rust tests.** `icons.rs`: the four new icons ink pixels inside their button only, the seven are seven different pictures, the person has a head over shoulders and is centred, the code brackets are three strokes. `toolbar` (`ui_tests.rs`): the entries are `accounts, new_session, new_home, new_browser`; the account entry is lit with no account and not with one. `newsession_tests.rs`: the wizard opens on step 1 with the three tiles; a tile of a provider with no account is not chosen (a notice says why) and Local LLM is; choosing Claude goes to step 2 with the account and folder rows; Next is refused while the form has a problem and goes on when it has none; Back returns; the last step starts with the very request the one-page form makes for the same answers (`request_of`); Home leaves the folder out; Local looks at this machine when chosen and not again when moved away and back (as P04a).
2. [x] **Window tests** (`tests/test_icon_toolbar_wizard.py`, new; in `tests/testmap.toml`): the toolbar's first button is the account icon, the second the plus, with no account the plus is enabled and the account icon is lit; a click on the plus shows step 1 with three tiles and no text row, Claude and Codex grey and Local LLM enabled without an account; with an account all three are enabled; a click on Claude goes to step 2 of 3, Next, Next, Start makes the session in the folder with the account; Esc goes back; the Local LLM icon lists the servers found (a fake server on a fake process table) and the wizard starts a local session; the house opens the wizard without the folder row; the account icon opens the Accounts screen. The older window tests that drive the form by `Ctrl+N` now reach it by File > New session options... (`tests/gui_helpers.py::open_form`).
3. [x] **Icons** (`icons.rs`: `Account`, `Claude`, `Codex`, `Local`).
4. [x] **State of the accounts in the window** (`attn.rs`: the providers that have an account, read from the `account.list` answers the window already receives; `ui.rs`: the field on the existing line).
5. [x] **Toolbar** (`toolbar.rs`: the account entry first, lit when there is no account, `toolbar_json`).
6. [x] **The wizard** (`newsession_wizard.rs` new: pages, rows, the heading, the pick, next and back; `newsession.rs`: the `Field`s `Pick`, `Back`, `Next`, the rows and lines and keys through the wizard, `form_open` for the wizard and `form_open_full` for the old form; `toolbar/picker.rs` new: the tiles are painted and are click targets; `controls.rs`: `Action::WizardPick`; `menu.rs`: the entry `New session options...`; `panel.rs`: the title).
7. [x] `suite run --changed --receipts`, `cargo xtask check|verify|graph`; [x] a manual-inspection page (the toolbar with and without an account, the three icons, the three steps for each provider, Home, the one-page form); [x] `cargo build --release -p lico` so that `lico gui` shows it; [x] commit `192e304` with the guard on (the owner approved the prefix `phase/M22-P04b-` in the chat).

## Decisions made while doing it (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D8 | The File menu entry for the one-page form is `New session options...` (first written `New session with options...`) | at the doubled scale the dropdown beside the full-height left column has 524 px and the longer label ran 9.6 px past the window (found by `test_menu_bar.py`) |
| D9 | The grey of an icon follows the host's accounts by itself: the toolbar's account icon is lit when `account.list` says there is none (the window asks again every fifth tick), and the wizard reads the accounts again when an icon is chosen | the owner's "reflected automatically": adding an account (the person icon, the CLI, another window) needs no restart and no reopening |
| D10 | A Home wizard's second step says `Home session: it gets a scratch workspace of its own, not a folder.` | the step shows the account only, and a person must see why there is no folder row |
| D11 | When the toolbar row is longer than its room (the Files panel open at a large text size), the padding of the text buttons and the gaps are tightened (14 to 6 and 5 to 3 scale units) before a button is dropped | the account icon pushed `Next >` of the Files panel off a 1200 px window at 20 px text (found by `test_directory_pagination.py`, red before the fix); a button that is not drawn cannot be clicked |
| D12 | The two lines that sent a person to `Ctrl+N` for the first account (the Accounts screen when empty, the Rebind screen) now name the person icon and `Ctrl+A`, and say that Local LLM needs none | the Claude and Codex icons are grey until an account exists, so `Ctrl+N` cannot make the first one |
| D13 | The build reaches `lico gui` by a link (`~/bin/lico` to `target/release/lico`); P06 replaces it by a file (rename) | the owner's first wish; a broken build is what runs, and `cargo clean` or moving the repository breaks the link |

## Results (2026-11-18)

- Unit tests: `cargo test --bin lico gui::` 304 pass (267 before): 37 new, 25 in `newsession_wizard_tests.rs` (the steps, the rows of each step, the icons grey and chosen, Next refused while the form has a problem, Back and Esc, Local looking once, Home without a folder, the very request of the one-page form for the same answers, the account taken when the icon is chosen though the form opened with none), 5 in `picker.rs` (where the tiles are, grey, lit, the ring, a room too small for icons has none), 4 in `icons.rs` (centred, a head over shoulders, the pieces of each picture, the pins of the chip), one for the toolbar (the account first and lit only with no account, the plus and the house never greyed), one for the spacing, one for the empty-account lines. Mutation checks (each fails its test, each restored, files identical): 35, W1 to W20 on the wizard, T1 to T5 on the toolbar, P1 to P6 on the icons, S1 to S3 on the spacing, H1 and H2 on the hints; four survived the first round (W11, W14, W15, W20: Enter on an empty Server row, the focus of the Local step, the settings note, the account fixed up when the icon is chosen) and each got a test or an assertion and was caught on the second.
- Window tests: `tests/test_icon_toolbar_wizard.py` (4 new: the account icon and the plus and the house; the plus shows three icons, one grey, hover and click on a grey one, Esc and the keys, the three steps, Back keeps the text, the session starts, the one-page form is in the File menu; Local LLM with no account looks at a fake server on a fake process table at once and starts a local session; the house is the same wizard without the folder and makes a Home session). The older window tests follow the toolbar of four icons and the wizard through `tests/gui_helpers.py::open_form` (File > New session options...) or through the three steps: 15 test files and `tests/gui_helpers.py`. `test_directory_pagination.py` went red with the account icon and green with D11.
- The changed-files suite: 282 tests in 81 files on 4 workers, all passed, 141.8 s, peak 1980.0 MiB (the bound is 2048 MiB), no test over its 15 s budget, receipts recorded. Two earlier runs recorded none. The first had two reds (`test_directory_pagination.py`, a real regression of the account icon, fixed by D11; `test_first_view.py`, a timing flake under four workers that passes alone and passed in both later runs) and a peak of 2065.3 MiB. The second passed all 282 (peak 2008.2 MiB), but one unrelated test (`test_every_bundled_dependency_has_its_license_notice`, 3.2 s in the first run) took 30.9 s while another agent's `cargo test` loaded the machine (load average 21, I/O pressure 50 %), so the lead waited for a quiet machine and ran it a third time. `tests/budget.toml` was not touched.
- Gates: `cargo xtask check` passes; `cargo xtask verify` passes (every receipt matches its inputs); `cargo xtask graph` 208 modules and the one cycle as before (the biggest has 48 modules).
- Manual inspection: `private/manual_inspection/202611/20261118/00004_M22-P04b_account_icon_new_session_wizard/` (the toolbar with no account and with one, the three icons with Claude and Codex grey, the pointer on Local LLM, the three steps of Claude, Local LLM and its server found, the Home wizard, the File menu and the one-page form).
- What the person may notice: the toolbar starts with a person icon, lit while there is no account; the plus shows three pictures and nothing else (Claude and Codex grey until they have an account, Local LLM always usable), a click on one goes on step by step, Esc goes a step back; `lico gui` of the next build is the build (the link).

## What this phase does not do

The browser tab (P05), the terminal (M23), the release (P06). It removes no menu entry and no key, and it changes no host or wire behaviour.
