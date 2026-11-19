# M23-P04c Plan — The `agy` Picture In New Session: One Click Starts `agy` In A Terminal In The Folder On The Left

- Status: in_progress (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M23-P04c-agy-tile` from `phase/M23-P04b-agy-conversations`; the prefix `phase/M23-P04c-` is approved by the owner (2026-11-19, with P04a and P04b)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), row P04, the end of level B of [the P04 plan](20261119_10_M23-P04_agent_in_terminal_plan.md)
- The owner's words (2026-11-19): "agy 는 Claude 다음 agy 다음 Codex 위치로 하면 될 것 같고" (the `agy` picture goes after Claude and before Codex), "A+B만 먼저하고 C는 향후로", and "모든 기본 아이콘 크기는 세션 시작할 때 보이는 Claude, Codex 아이콘을 기본으로 하고 그정도 간결성(거의 텍스트가 없는 수준)" / "거의 컴맹 수준의 사람도 쓸 수 있는 직관성"
- This phase adds one picture to the first step of New session and the one thing it does: it opens a terminal in the folder on the left and types `agy` and Enter into it. It reads no record, sends nothing to `agy`, and changes no setting.

## What is true before this phase

- The first step of New session paints four pictures over the blank lines of the panel's body: Claude, Codex, Local LLM, Terminal (`PICKS` in `newsession_wizard.rs`, drawn by `picker.rs`; each is a square of the same size, at most 112 logical pixels, with a word under it where it is chosen). Claude and Codex are grey when no account of that provider exists, with the notice "needs an account first"; Local LLM and Terminal are never grey. The Terminal picture opens a shell at once in the folder on the left and closes the panel (`M23-P03a`, `M23-P03d`).
- A terminal tab shows the picture of `claude`, `codex` or `agy` while that program runs in it (P04a), and the conversations of `agy` are cards in Chats (P04b). A person who wants `agy` starts a Terminal and types `agy`: nothing in New session knows it.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **Five pictures, in the owner's order: Claude, `agy`, Codex, Local LLM, Terminal.** `PICKS` gains `"agy"` at the second place; the keys (Tab, the arrows, Enter, Space) and the clicks work over five as over four; the focus when the step opens is still the picture of the provider the form opened on. | the owner's order; the form's own providers (`PROVIDERS`) stay Claude, Codex and Local LLM: `agy` is no provider of a Lico session (that is level C, later) |
| D2 | **The picture is the `agy` picture of P04a** (an upward chevron over a dot), the same size as the others, with the word `agy`. | the owner's philosophy: every picture the size of the Claude and Codex ones, almost no text |
| D3 | **A click on it closes the panel and opens a terminal, as the Terminal picture does** (`terminal_start_dir`: the folder Directories shows, else the session's, else the last folder, else the home), **and types `agy` and Enter into it.** Plain `agy`: nothing is added to the line, never its flag that skips permissions. The tab then shows the `agy` picture (P04a). | no new door on the host: the window types what a person could type; the person sees what runs and `agy` keeps its own default of asking |
| D4 | **The picture is grey when `agy` is not found on this machine, and a click on it says why** ("agy is not installed on this machine."), as the picture of a provider with no account says it needs one. Found means an executable regular file named `agy` (a link to one counts) in a folder of the window's `PATH`, or in `~/.local/bin` or `~/bin`. It is looked for when New session opens, not at every frame. | the window's `PATH` can be shorter than the shell's (a launcher's), and `agy` installs itself in `~/.local/bin`; the two extra folders keep a grey picture from lying about an installed program; a program the shell finds by a rule of its own (an alias) is not found, and the person can still choose Terminal |
| D5 | **Not in this phase:** `agy` as a session of Lico (level C), the Home scope (the Terminal picture there is as it was: a shell in the home), a choice of `agy`'s mode, and a setting for where `agy` is. | each needs a decision of its own |

## Steps

1. [ ] **Tests first**: Rust unit tests (the order of the pictures and their words, the focus over five, `agy` enabled only when found and grey with the reason otherwise, the search over `PATH` and the two folders with a link, a file that is not executable and a folder of the same name; a click on the enabled picture opens a terminal in the folder on the left and queues `agy` and Enter, a click on the grey one opens nothing and says why; `tile_rects` for five pictures); the window tests that index the pictures (`tests/test_icon_toolbar_wizard.py`, `tests/test_login_window.py`, `tests/test_terminal_tab.py`) take the new order; one window test (the `agy` picture starts a fake `agy` in the folder on the left, the tab shows its picture; a window whose `PATH` has no `agy` shows it grey and says why).
2. [ ] **The window**: `PICKS`, the word and the picture, the search, the click.
3. [ ] **The docs**: the guide.
4. [ ] **Gates**: mutation checks, the changed-files suite, `cargo xtask check`, the owner's build.

## Not decided here

- Whether the picture should read `agy` or the product's own name (`Antigravity`): the guide and the picture say `agy` as the command is called; the owner may want the other.
- Whether a click on the picture should offer to resume the last conversation in that folder (Chats has them; P04b).
