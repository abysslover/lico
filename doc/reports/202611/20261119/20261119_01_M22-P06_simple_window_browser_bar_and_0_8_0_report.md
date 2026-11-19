# M22: a simpler window, a browser tab that looks like a browser, release 0.8.0 (with the terminal tab of M23)

- Version: 1.0.0
- Date: 2026-11-19
- Branches: one `phase/*` branch per phase (`phase/M22-P01-chats-of-all-sessions` to `phase/M22-P05c-remembered-pages`, `phase/M23-P01-vt-engine` to `phase/M23-P03b-test-diet`), the release on `phase/M22-P06-release`; of these P03b, P05a, P05b, P05c and P06 are not pushed; release commit `c587de6`
- Source: the owner's words in the chat, 2026-11-18 and 2026-11-19, kept in [the M22 milestone](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md): "Simple is best"; the click ring is not needed; Global is one virtual directory; Chats shows every past session, newest first, and a click goes to its folder; two left tabs and the Calls out of them; the full path at the very bottom; the menu simpler; the URL at the top of the browser tab and an empty first page that looks like a browser; words in the bar are a search; a page opened before can be opened again; and, for M23, "choose Terminal in New session".
- Labels: MEASURED (a run of this release), READ (from the code or the plan notes), COMPUTED.

## 1. What the person sees now (READ from the guide and the code, MEASURED in the phases' window tests)

| Place | Before (`0.7.0`) | Now (`0.8.0`) |
|---|---|---|
| Left side | three tabs: Directories, Chats, Calls; a `Global (general)` row; Chats listed one folder or Global | two tabs: Directories and Chats; one virtual directory `Home` (`lico://home`); Chats lists **every past session of this machine** at once, newest first, a click goes to the chat's folder |
| Calls | a sidebar tab with quick-connect buttons | out of the sidebar; a room two sessions share has its own tab and a small `⇄` mark on each of the two session tabs (the agents' tools, `@AgentName`, the CLI and REST are unchanged) |
| Top | nine toolbar buttons (`+ New`, `Accounts`, `Files`, `Close`, `Rename`, `Mode`, `Group`, `Send`, `Find`) and `+ +B +BL +G` on the tab row | four icons (account, plus, house, browser); the tab row has no buttons; the rest is in the menus, which are trees (groups open in place; no entry removed, no key changed) |
| New session | a one-page form | a wizard: four pictures (Claude, Codex, Local LLM, Terminal), Claude and Codex grey without an account; `File > New session options...` still opens the whole form |
| Bottom | the path on the last row, empty in Home and with no session | the path row above the hint row, never empty (`lico://home` in Home) |
| Chats card | a text row with pills | the provider's picture, the title (two lines when it needs them), a time coloured by its age, the folder, a state picture (disc, ring, square) |
| Browser tab | a header with the address, a ring on your last click, a text explanation on a new tab | the address bar at the top (`‹ › ⟳` and the field, cut in the middle), an empty first page with the bar focused, no ring, one bottom row; **words in the bar are searched** (setting `browser_search`, Google by default); **pages are remembered** (100, `browser_pages.json`), listed in Chats, reopened by a click, `Reopen closed tab`, `Clear remembered pages` |
| Terminal | none | a terminal tab (M23 P01 to P03b): a shell of yours in a tab, opened at once by the Terminal picture or `Session > New terminal tab`; `lico terminal ...` on the command line |

## 2. The phases

| Phase | What | Commit | Plan note |
|---|---|---|---|
| M22-P01 | host: `session.chats` scope `all`, CLI and REST | `0900c95` | [plan](../../../plans/202611/20261118/20261118_03_M22-P01_chats_of_all_sessions_plan.md) |
| M22-P02 | host: the chat index (background, incremental, every scope reads it), `.jsonl` only, the provider's own home on by default | `4cb06c9` | [plan](../../../plans/202611/20261118/20261118_04_M22-P02_chat_index_plan.md) |
| M22-P03 | window: two left tabs, Home, Chats with the jump, Calls out | `7361c18` | [plan](../../../plans/202611/20261118/20261118_05_M22-P03_window_two_tabs_home_chats_plan.md) |
| M22-P04 | window: the toolbar of icons, the bottom rows, the tab row without buttons | `8f9cfb7` | [plan](../../../plans/202611/20261118/20261118_06_M22-P04_window_toolbar_icons_bottom_rows_plan.md) |
| M22-P04a | window: the menus as trees; New session with three modes | `e69adb0` | [plan](../../../plans/202611/20261118/20261118_07_M22-P04a_tree_menus_three_modes_plan.md) |
| M22-P04b | window: the account icon, the wizard of icons | `192e304` | [plan](../../../plans/202611/20261118/20261118_08_M22-P04b_account_icon_new_session_wizard_plan.md) |
| M22-P04c | window: the Chats cards | `2223fe5` | [plan](../../../plans/202611/20261118/20261118_09_M22-P04c_chats_rows_plan.md) |
| M23-P01 to P03b | the terminal: `lico-vt`, the host's pty, the window's tab, the Terminal picture, the test diet | see the [M23 milestone](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md) | |
| M22-P05a | browser tab: the address bar at the top, the empty first page, no ring (`lico-browser-view` 0.8.0) | `8b30b19` | [plan](../../../plans/202611/20261119/20261119_04_M22-P05a_address_bar_plan.md) |
| M22-P05b | browser tab: search with `browser_search` (`lico-browser-view` 0.9.0) | `68ca2c3` | [plan](../../../plans/202611/20261119/20261119_05_M22-P05b_search_plan.md) |
| M22-P05c | browser tab: remembered pages | `69b16c6` | [plan](../../../plans/202611/20261119/20261119_06_M22-P05c_remembered_pages_plan.md) |
| M22-P06 | the guide, the README, the CHANGELOG, this report, the version | `c587de6` | [plan](../../../plans/202611/20261119/20261119_07_M22-P06_release_plan.md) |

## 3. The search probe (MEASURED in P05b, 2026-11-19)

One run of three requests through the real engine and the host's fence, the same query with a Latin and a Korean word, percent-encoded: Google (`https://www.google.com/search?q=%s`) opened with real results and the Korean intact, DuckDuckGo's html page opened (`ready`, real results), Bing opened (still `loading` at the sample, real results). Google is the default because it worked and it is the owner's own first wish; the other two are what to write in `browser_search` if Google stops answering. One run of three requests is not a promise: the guide says that no engine is promised to work.

## 4. Measured for the release

MEASURED on this machine, 2026-11-19 (a shared one: the other users' work shows in the load).

| What | Result |
|---|---|
| The whole cold suite, three runs | `616/616` tests in `130` files each; `176.5`, `173.1` and `174.4 s`; peak `1,010.1`, `1,125.1` and `1,121.4 MiB` against the bound of `1,536` (the bound was lowered from `2,048` in M23-P03a); the 1-minute load at the start of each run `5.4`, `6.9` and `6.8` (the gate waits for a load under 7) and `8.7`, `7.2` and `9.4` at its end |
| `cargo xtask check`, `graph`, `verify` | pass (`233` modules, `75` nodes, `444` production edges, `120` test-only edges) |
| `cargo xtask system` | pass: `10` system tests, each far under its budget (the longest, `browser_input_reaches_the_engine_at_once_and_nothing_wakes_while_a_still_page_is_shown`, `23.2 s` as it waits for the quiet of a still page); the command `90.6 s` with a peak of `1,495 MB` for the whole command (the build included) |
| The release binary | commit `c587de6`: `11,033,672` bytes (`+157,696`, `+1.45 %` over the `0.7.0` file of `10,875,976`); two clean clones in two places built the same file, SHA-256 `2b71def030eecb0c6e112678be5cce572588b7a9446fd8768f89678e2e1f95ef`; `lico --version` says `lico 0.8.0 (commit c587de6, ...)`; the acceptance test (`suite acceptance`: two clones, the same bytes, no path of the build machine in the file) passed in `199.2 s` on the commit before the message of this one was corrected (the same tree) |
| Idle window and host memory (`lico_measure run --candidates a2 --ui --menu --accounts 10 --realistic-text`, 3 launches each, `--allow-dirty`: only documents were uncommitted, the program is the commit `c587de6`) | window scale 1 `25.92..26.13 MiB` (`0.5.0`: `25.54..26.07`), scale 2 `36.67..37.10` (`36.67..37.01`); host `8.63..8.92` (`8.28..8.52`); no tab open; the limit `< 50 MiB` is met |
| Hidden growth, 10 to 100 sessions (3 repetitions) | scale 1 `+0.13..+0.86 MiB`, scale 2 `+0.01..+0.18` (`0.5.0`: `-0.06..+0.27`); the limit `<= +10 MiB` is met |
| Idle CPU (60 s, nothing producing) | `0.03 %` of a core at both scales; met |
| The noisy case, first run (a visible session producing 100 records a second, 60 s) | scale 1 `5.45 %`, scale 2 `7.80 %` of a core, flagged `missed` against the `5 %` of PERF-05; the machine's 1-minute load while it ran was `16` to `17`: see section 4a |
| The host's unit tests | `978` pass (`cargo test --bin lico`, P05c) |
| Mutation checks of the last phases | P05a `37` edits (`35` caught, `2` equivalent), P05b `52` caught (`3` equivalent not run), P05c `69` (`68` caught, `1` equivalent at the integration level); the earlier phases' counts are in their plan notes |
| This phase's own change | six files: the version in `Cargo.toml` and `Cargo.lock`, the README, two tests, the guide; no source file |

The size grew `1.45 %`; it was not broken down by feature (the release adds the terminal, the search and page store, the wizard and the menu trees, and takes away the buttons and the Calls tab).

### 4a. The noisy case (PERF-05, at most 5 % of one core) read again

The first run of the experiment gave `5.45 %` at scale 1 and `7.80 %` at scale 2 while the machine's load was `16` to `17` (other users' work), and the harness flagged both `missed`. The experiment was repeated twice at each scale after the 1-minute load had fallen to `4.8` to `4.9` (the 5- and 15-minute loads were still `8` to `14`, so the machine was quieter, not quiet):

| Scale | First run (load 16 to 17) | Repetition 1 | Repetition 2 | Limit | Earlier reading (M02-P01, 2026-10-25) |
|---|---|---|---|---|---|
| 1 | `5.45 %` (missed) | `3.32 %` | `4.13 %` | `<= 5 %` | `3.70 %` |
| 2 | `7.80 %` (missed) | `5.63 %` (missed) | `5.38 %` (missed) | `<= 5 %` | `6.35 %` (missed) |

So: **at scale 1 the limit is met** when the machine is not crowded and was missed under heavy load; **at scale 2 the limit is not met**, as it was not met in M02-P01 (`6.35 %`, "not met: the noisy case at scale 2", its cause being a frame of four times the pixels every 100 ms). What this report cannot say is whether `0.8.0` costs more or less than `0.7.0` in this case: the `0.7.0` binary was not run beside it, and the readings move by a point with the load. A comparison run of the two builds at the same time is the way to find out, and is not done here. Idle CPU is `0.02` to `0.03 %` in every run.

## 5. What is not decided (the owner's)

- **The 180 s wall-clock limit of the suite.** The cold runs above are the whole suite; the first gate of P05c (the changed files only, 616 tests) took `206.9 s` under a machine load of 9 to 26 and the limit was the thing that failed. The options stay: judge the limit only when the machine is quiet (the lead's recommendation: a run that started under load is repeated, and the load is printed beside the time), count CPU seconds instead, or a fifth worker.
- **The noisy case at scale 2** (`5.4` to `5.6 %` of a core against the limit of `5 %`, section 4a): an open miss since M02-P01, not new in this release as far as the readings show. Whether to work on it, and whether to compare `0.8.0` with `0.7.0` on the same machine, is the owner's call.
- **Push, pull request, merge.** Nothing of P03b, P05a, P05b, P05c and P06 is pushed. Each needs the owner's yes after a dry run.
- **`~/bin/lico`** is a link to `target/release/lico` (the owner asked for it on 2026-11-18, so that a build reaches `lico gui` by itself); the earlier plan said P06 would put a file in its place. The lead kept the link; a file is the owner's choice.
- **G3** (the owner's acceptance of the milestone) has not been asked.

## 6. Known limits (READ)

- The engine's control socket has no password and `navigator.platform` is `Win32` (unchanged from `0.7.0`; the token waits for M21).
- A page is remembered once it has loaded (a page still loading is not), and only an explicit `http` or `https` address is kept (no user name, nothing local).
- The terminal is not fenced (it runs as you); an agent that is started in it does not come into Lico yet (M23-P04, later).
- The search engines were probed once; Google or any other can send the page to a check page.

## 7. What changed in the files of this phase

The version (`Cargo.toml`, `Cargo.lock`, the README, the two tests that name it), `doc/manual_end_to_end.md` 1.10.0 (the browser tab's bar, search and remembered pages; the terminal tab; the window's items without the buttons that are gone), `CHANGELOG.md`, this report, the plan note, `PLAN.md`, the index. No source file of the program changed in this phase (D55).
