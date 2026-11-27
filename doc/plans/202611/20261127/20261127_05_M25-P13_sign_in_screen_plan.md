# M25-P13 Plan — The Sign-In As A Screen Of Pictures, And A Plain Title For The Wizard

- Status: in progress (2026-11-27), nothing pushed
- Version: 1.0.0 (this document's own version; the product is `0.9.1` and this phase changes no product version. The owner's words of 2026-11-27: the product is far from `1.0.0` and stays in `0.x`, and the patch climbs to `99` before the minor moves, so the next release is `0.9.2`)
- Branch: `phase/M25-P13-sign-in-screen` from `main` (`0947358`, `lico 0.9.1`, published)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), the line after `0.9.1` (the owner's rule: the patch climbs to `99` before the minor moves; the next release is `0.9.2`, and it is cut when the owner asks)
- The owner's words (2026-11-27), after the sign-in of an agy account had worked:
  1. "agy login 까지 완료 되었는데 그 과정을 조금 더 gui 형태로 보여주고, 붙여 넣기 하는 위치도 뭔가 창에 붙여 넣는 느낌의 UI로 수정할 수 없나?" and "만약 다른 account 도 그런 형태가 아니라면 모두 그렇게 보이도록.."
  2. On the line under the icon of a new session: "New Bot session - step 1 of 3 - Choose | Restarted the host (...)" is mixed; only the part before `|` is right, and because an icon makes the session at once, "New Bot session" alone is enough ("this state, press agy: the window is open, so the step is of no meaning").
- The standing yes of 2026-11-25 ended with `0.9.1`: this phase is local (commit allowed under the hook, **no push and no merge** until the owner says so).

## What this phase changes

1. **The sign-in is a screen of pictures for every provider** (Claude, Codex, agy). The text lines of the sign-in stay the model (the dump, the keys `Ctrl+C`, `Ctrl+O`, `D`, and the words that the tests read); a veil of the window's own ground covers them and the pictures are painted over it, as the pages of the accounts are (`account_cards.rs`):
   - a row of pictures of the wizard's size for the steps: **Open** (the page), **Sign in**, **Code** (Codex has no Code picture until its program asks for a code); a step done has a tick, the step to do now has the ring, the ones to come are grey;
   - under them the **address** in a box with two buttons (open it, copy it);
   - under that a **box to paste the code into** with two buttons (paste from the clipboard, send); what is typed or pasted is drawn as stars, never as text, and only its length is in the dump;
   - a sign-in that ended without success shows a cross on the first picture, a sentence of what happened (red) and one more picture, **Again**, that starts the sign-in again (the line `Code sent.` of the panel is dropped then); Codex's second way (the device code, key `D`) has its sentence where the box would be.
2. **The window asks the host nothing new.** The states are the host's own (`waiting`, `browser`, `code`, `succeeded`, `failed`, `timed_out`, `cancelled`); a click on a button is the same as the key that does it (`LoginAct`).
3. **Four new pictures** of the small set of icons (`check`, `cross`, `copy`, `paste`), drawn from rectangles and lines like the others (one colour, no file).
4. **The wizard's title is only what is made** (`New Bot session` or `New session`). The step ("step 2 of 3 - Account and folder") is not drawn; it stays in the state (`panel.form.wizard.title`, `wizard.name`) for the tests and the keys. While the wizard is open the host's notice and the message are not appended to its title after a `|`.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | One screen for all three providers, three steps (two for Codex until it asks for a code) | the owner asked for it for every account |
| D2 | The text lines and the keys stay; the pictures are painted over a veil | the dump, the keys and about forty older tests read the text |
| D3 | The bottom prompt of the code (`> ****`) is blank while the box is painted | the box is the place that one pastes into; the stars are in the box |
| D4 | The wizard's step is only in the state | the owner: the step has no meaning when an icon makes the session at once |
| D5 | No new host operation, no new setting, no change of the protocol | the screen reads what `account.login.status` already says |
| D6 | The model/effort picker and the catching of `/model` are **not** in this phase | a design of its own: the host has no operation for it yet (see "Not decided here") |

## Steps

1. [x] Icons `check`, `cross`, `copy`, `paste`.
2. [x] `login_cards.rs`: the pure view (`login_view`), the pure layout (`layout`), the painting, the dump.
3. [x] The wizard's title (`form_title`, `step_title`, `wizard_open`), the header without the notice while the wizard is open.
4. [x] Unit tests (`login_cards_tests.rs`, the wizard tests changed to say the plain title and the step in the state).
5. [x] A real window test (`tests/test_login_screen.py`): the three providers, the double of the sign-in program, the paste box, the stars, the buttons, a failed sign-in and Again.
6. [x] The older tests that read the sign-in or the wizard title still pass; mutation checks of the new rules.
7. [x] The guide, the CHANGELOG, the milestone row, the index rows; clippy; the by-hand changed-files run (324 passed, the build's peak over the bound, told); the commit (the hook).
8. [ ] The owner's build (`~/bin/lico` rebuilt) so the screen can be seen.

## Not decided here

- **`/model` and a right-click menu for the model and the effort** (the owner's third request of 2026-11-27): in a Claude or Codex session `/model` is typed to the program itself; in an agy session the program answers that it is "a regular message". The host has **no** operation that changes the model or the effort of a live session (they are chosen at `session.create` only). A picker needs, per provider, a way to change them (agy: the `models` and `configOptions` of `session/new` and a request to set one; Codex: the app server's model field per turn; Claude: the program's own command). It is a phase of its own and needs the owner's yes after the lead's proposal.
- The periodic "not responding" of the window (a sampler is running; nothing is changed until it reproduces).

## Found so far (2026-11-27)

- `panel.rs` was at its limit of 800 lines: the body of the sign-in moved into `login_cards.rs` (`login_body`), which also keeps the text lines in one place.
- The window already showed the sign-in code as stars in the bottom prompt; with the box painted the prompt is blank (the box has them).

## Results (2026-11-27)

- **Unit tests**: `21` new in `login_cards_tests.rs` (the steps for every state and provider, the layout, the text lines that stay the model, the stars counted in characters, the bottom line blank while the box is painted) and `1` new in `newsession_wizard_tests.rs` (`panel_header`: the wizard's title stands alone, another panel's has the notice and the message after a bar); `8` assertions of the wizard tests changed to say the plain title and the step in the state. The whole unit run of the host: `1,453` passed (before the last two additions; the changed-files run is the last word).
- **Window tests** (`tests/test_login_screen.py`, `3` tests): the agy sign-in with the Paste button (the clipboard is set with `xclip`), the Copy button, typing, the Send button; a failed sign-in with its cross, its sentence and Again; Codex with two steps and the second way. The older sign-in and wizard tests (`test_login_window.py`, `test_agy_account_window.py`, `test_icon_toolbar_wizard.py`, `test_one_touch_window.py`) pass with the step read from `form.wizard.title`.
- **Mutants**: `32`, all killed (one was first a build error of the mutant itself, a borrow, and was made again). None survives.
- **Looked at**: window screenshots of the code asked, the code typed, a failed sign-in and the wizard's first step.
- **Found on the way**: after a failed sign-in the line `Code sent.` stayed under the pictures; it is dropped when the host reports the end.
- **Clippy**: one finding on a line of this phase (`unnecessary_lazy_evaluations`) was fixed before the run; the other findings in the files touched are older lines and were left as they are.
- **The changed-files run by hand on the staged tree** (the hook's own line, limit 300 s): `324` tests in `92` files, `324` passed, `0` skipped, `154.6 s`, peak `1,074.9 MiB` for all workers together; the build `87.8 s` and the test build `48.3 s`. **Told, not raised:** the run ended with the guard's exit 3 because the build's peak was `2,129.2 MiB`, over the bound of `1,536 MiB` (the host's load average was `26` when the run began, from other users; no cause is claimed, and the bound stays). No test was red.
- **Not pushed, not merged.** The product is `0.9.1`; this phase changes no product version.
