# M24-P02 Plan — One Touch: A Click On A Picture Starts The Session

- Status: done (2026-11-24)
- Version: 1.0.0
- Branch: `phase/M24-P02-one-touch-start` from `doc/M24-one-touch-window` (`14af9ad`, on top of the unpushed P08 and P09); the prefix `phase/M24-P02-` was approved by the owner at commit time (the question on the prefix, 2026-11-24) and is in the local guard; the code is `dee3dd9`
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md) (0.3.0), R3 and R4; the rows and their sources are in section A of [the P01 report](../../../reports/202611/20261124/20261124_01_M24-P01_phase0_one_touch_menu_and_panes_report.md)
- The owner's words (2026-11-24): "새 세션 해서 codex를 치면 그 전까지 아이콘이다가 다시 텍스트 위주 창이 보임, 게다가 두번이나 창을 넘겨야 세션이 시작됨" and "새로운 세션 정보는 ... 각 에이전트의 ... config 파일 ... lico가 조사해서 알려주면 됨"; G1 and the picks of the P01 report were given the same day.

## What this phase changes

A click on the Claude, Codex or Local LLM picture of the new-session wizard (and Enter or Space on the focused picture) **starts the session at once** when the form's own answers are valid, and shows the old second step only when they are not. The Terminal and `agy` pictures already start at once (M23).

## What was found (2026-11-24)

- `wizard_pick` (`gui/newsession_wizard.rs`) sets the provider, picks the account (`first_account`: the provider's default, else its first) and goes to step two; the form was already seeded by `form_open_from` with what the host says was used last (kind, account, mode, model and effort for Claude, the folder `saved_cwd`).
- `form_start` (`gui/newsession.rs`) already does everything a start needs: `form_problem`, the request (`request_of`), the ssh path, `finish_form_start` (closes the panel, shows the session, remembers the folder). A name may be empty (the host names the session). A model and an effort are sent **only when not the default**, so a start with the defaults leaves the choice to the agent's own settings file: Lico need not read those files to honour them (the P01 report, section E, is what the person may be shown later, not what a start needs).
- `terminal_start_dir` (`gui/terminal/view.rs`) is the rule of "the left decides" for the Terminal and `agy` pictures: the folder `Directories` lists (this machine), else the session in front's, else the last folder the window kept.
- About twenty unit tests and five window tests use the pick as the way into step two; they must keep a way in.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **A plain click or Enter or Space on Claude, Codex or Local LLM starts the session at once** when `rows_problem` has nothing to say and the account is not on an ssh host; otherwise the wizard goes to step two exactly as before, with the form's own words saying what is missing | the two cases that keep a screen (no account yet, no server found) fall out of the form's own checks; nothing is checked twice |
| D2 | **Shift with the click, or Shift+Enter, is the old way**: step two, then step three, as before (`wizard_pick`) | the options (account, folder, worktree, name, model, effort, permission) stay reachable until the session header carries them (a later phase); the existing tests keep a door |
| D3 | **The folder is the one on the left**: `terminal_start_dir` (made `pub(crate)`), for a session that is not a Home session; when it finds none, the form's own folder stays | the same rule as the Terminal and `agy` pictures (the owner's word: the left decides) |
| D4 | **The account** is `first_account` as today: the provider's default, else its first | the P01 report's text; an account the person marked default (`F`) wins |
| D5 | **Model, effort, name, worktree are what the form holds** (the default, which the request leaves out): the agent's own settings decide | R4 without reading a settings file |
| D6 | **A start on an ssh account keeps step two** | a folder on this machine is no folder on that host; the ssh path is asynchronous and has its own notice |
| D7 | **A Home session** (the house icon) starts at once with no folder, as the wizard's Start would | the same rule |

## Not done here (named so that nobody expects it)

- Showing the person what a session started with (model, permission) or changing it afterwards: the session header's pictures, a later phase.
- Removing the one-page form and the wizard's steps: they stay for the Shift way until the owner's menu diet (P04) decides about `New session options...`.
- Level C (`agy` as a Lico session).

## Steps

1. [x] **Tests first**: `gui/newsession_touch_tests.rs` (a plain pick starts at once and builds the request the form would; the folder on the left wins; a Home session has none; no account, a missing server and an ssh account keep step two; the Shift way is the old one; Enter and Space and a click are the same), and the window tests (`tests/test_icon_toolbar_wizard.py`, `tests/test_login_window.py`) that go through step two use the Shift way; one new window test starts a session by one click on a real host. Red first.
2. [x] **The code**: `wizard_touch`, the two call sites, `terminal_start_dir` visibility.
3. [x] **Mutation checks**, before the final green gate.
4. [ ] **Gates and documents**: `cargo test --bin lico`, clippy on the new code, the changed-files suite, `cargo xtask check`, the owner's build; the guide (the new-session paragraph), the milestone row, `PLAN.md`, the index row.

## Results (2026-11-24)

- **What the owner sees** (`lico gui`): click the Claude, Codex or Local LLM picture of New session (or the house for a Home session): the session opens at once, with no second or third screen. The account is the provider's default, the folder is the one on the left, the model and effort are the program's own unless the person chose one. When it cannot start by itself the old step two appears: no account (the picture stays grey and says so), a Local LLM with no server known, an account on an ssh host, a folder that is not valid; a start the host refuses ends on step two with the reason. Shift with the click, or Shift+Enter, is the old three-step way.
- **Code**: `gui/newsession_wizard.rs` (553 lines): `wizard_touch` (Shift or not), `pick` (the old `wizard_pick`, now with a `touch` flag), `one_touch_ready`, `one_touch`; the two call sites (Enter and Space, a click) call `wizard_touch`; `terminal_start_dir` is `pub(crate)`. The form's own `form_start` does the start: nothing was written twice. No protocol, host or `Cargo.lock` change.
- **Tests (red first against a stub)**: 12 unit tests in the new `gui/newsession_touch_tests.rs` (a plain pick starts at once for Claude and Codex; a start the host refuses leaves step two with the reason; the folder on the left; a left side that lists an ssh host leaves the form's folder; a Home session has no folder; no account stays grey; Local LLM with no server shows step two and no refusal; an ssh account keeps step two; a form with a problem shows step two and no refusal; Shift is the old way; Enter, Space and a click are the same touch; the Terminal picture is untouched), and 3 window tests on a real host (`tests/test_one_touch_window.py`: a click starts a session in the folder the left lists, not in the one the window kept; Enter does too; Shift with the click shows step two and starts nothing). Whole `cargo test --bin lico`: 1050 passed, 12 ignored.
- **Tests that had to change**: nine places in four window test files (`test_icon_toolbar_wizard.py`, `test_login_window.py`, `test_toolbar_icons_and_bottom_rows.py`, `test_session_tabs_and_telemetry.py`) entered step two with a plain Enter or click on a picture; they use Shift now (`shift+Return`, and a `_shift_click` helper in `test_icon_toolbar_wizard.py`). The changed-files suite found the last two; the unit tests of the steps call `wizard_pick`, kept for tests, and did not change.
- **A thing found on the way**: the selection guard (`tests/test_selection.py`) failed once `tests/testmap.toml` was touched: `tests/test_agy_chats.py` and `tests/test_agy_tile.py` (M23-P04b and P04c) were never listed under `[areas]`. They are listed now (with the new test file).
- **Mutation checks (30 runs in two rounds; every mutant of the final code is caught)**: unit level 14 mutants of the new code in the first round and the 12 live ones again in the second (Shift ignored or inverted, never a touch, the ssh test dropped, the form's problem ignored, the Home folder, the left folder not set, a refused start staying on step one, the reason lost, `one_touch` answering false, the two call sites left on the old way, the touch tried before the provider is set); one survivor was equivalent (`f.local ||`: a Local LLM form has no account, so the ssh test is false anyway) and the check was removed from the code, one was a test hole (the form's problem ignored gave the same page; the tests now say that no refusal is shown for a start that was not tried); window level 3 mutants (never a touch, Shift ignored, the left folder not set).
- **Gates**: clippy as at the parent (139 warnings in the lib, 155 in the test build; `wizard_pick` is `#[cfg(test)]` so that it is no dead code), `cargo xtask check` passes.
- **Decisions that stand until the owner says otherwise**: D1 to D7 above; the Shift way is the only way into steps two and three apart from `File > New session options...` (the one-page form, which the owner's menu picks mark as a candidate to drop, in P04).
- **Not done, said again**: showing and changing what a session started with in the session's header (model, permission), and the account screen, the menu and the pane tools (M24-P03 to P05).
