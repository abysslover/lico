# M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)

- Status: done (closed 2026-11-13)
- Approved: 2026-11-12 (G1 and G2 approved by the owner for continuous execution; G3 and G4 approved 2026-11-13)
- Project: Lico
- Updated: 2026-11-13
- Milestone branch: `milestone/M07-two-pane-workbench-and-past-sessions`
- Implementation: Merged to `main` across `phase/M07-P01-session-tabs-and-telemetry-bar` through `phase/M07-P05-peer-reply-copy-and-dir-nav` (`lico 0.2.3`)
- Next gate: Completed (G1..G4 accepted and merged to `main` on 2026-11-13)

## Goal

Reorganize `lico gui` (`0.2.2` -> `0.2.3`) into a fast, uncluttered two-pane
tabbed workbench that opens maximized on the active monitor while preserving
Lico's single-executable design and `< 50.0 MiB` idle GUI RSS budget:
(1) a right-hand primary chat area topped by a multi-session tab bar (with
dedicated `Global` general-purpose sessions for lightweight folderless Q&A) and
anchored below the prompt input by a color-coded session telemetry strip
(`Model (Effort)` | `Tok: in / out` | `5h / Wk quota + reset` | `cwd / Global`),
(2) a left-hand sidebar providing two default tabs (`[ Directories ]` and
`[ Past ]`) where `[ Directories ]` browses local and SSH remote directories,
highlights the active session's working directory, and supports instant filename
filtering, pagination, and unified file preview/opening, and (3) full-pagination
discovery (`session.past`, no top-N truncation) and one-click resume of all past
`claude` and `codex` conversations associated with the selected working
directory or `Global` scope across GUI, CLI, IPC, and loopback HTTP.

## Done when

1. `lico gui` opens maximized on the active monitor by default (`--windowed` opt
   out), elevates folderless sessions to a first-class `Global` general scope
   (`+ Global` quick creation and `Scope: Folder / Global` in `New session`),
   tracks live per-session `input_tokens`, `output_tokens`, `effort`, and quota
   reset countdowns (`5h` and `Wk`), renders a horizontal multi-session tab bar
   at the top of the chat pane (`Ctrl+Tab`, `Alt+1..9`, click, close `x`,
   overflow `< >`), and renders a color-coded session telemetry bar directly
   below the prompt input (`Model (Effort)` | `Tok: in / out` | `5h / Wk` |
   `cwd / Global`) plus multi-account quota summary in the bottom status bar.
   (`M07-P01`)
2. `lico gui` presents a two-column workbench whose left column (`Ctrl+B`
   collapsible) defaults to two tabs (`[ Directories ]` and `[ Past ]`), with
   `[ Directories ]` integrating local and SSH (`Host: local v` / `ssh:<host>`)
   directory browsing, clear visual highlighting of the active session's working
   directory (`cwd` or `Global`), automatic async directory sync when switching
   session tabs, instant in-memory filename filtering, pagination, and unified
   file activation (wide inline UTF-8 text preview with `Esc` back to chat, or
   system default opener for non-text files). (`M07-P02`)
3. `session.past` enumerates all past `claude` (`projects/*/*.jsonl`) and
   `codex` (`sessions/**/rollout-*.jsonl`) conversations matching a working
   directory (`cwd`) or `Global` scope across local and `[ssh: <host>]` accounts
   with full `offset` / `limit` / `total` pagination (no top-N cap), surfaces
   paginated `[ Past ]` navigation in the left sidebar and `New session` with
   one-click resume (`--resume` / `thread/resume` + recent transcript seeding, or
   focusing an already-open tab), bumps patch version `0.2.2` -> `0.2.3`, passes
   the full cold suite + idle RSS (`<= 50.0 MiB`) + hidden-growth (`<= +10.0 MiB`)
   + two-clone `suite acceptance`, and installs `~/bin/lico` `0.2.3`. (`M07-P03`)

## Why now

Since only one session's conversation is read at a time, placing open sessions
in a horizontal tab bar above the chat area frees the left sidebar to hold a
persistent two-tab workspace navigator (`[ Directories ]` and `[ Past ]`)
without squeezing the conversation between three columns. Combining maximized
launch, `Global` general Q&A tabs, a live token/quota/directory telemetry bar
under the prompt, highlighted working-directory navigation, and full paginated
resume of directory-associated `claude` and `codex` histories turns `lico 0.2.3`
into an ergonomic daily workbench while keeping memory around `~25-36 MiB`.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M07-P01 | Maximized launch, `Global` general sessions, token/quota reset telemetry, chat session tab bar, and bottom telemetry strip | [Plan](../../../plans/202611/20261112/20261112_11_M07-P01_session_tabs_and_telemetry_bar_plan.md) | [Report](../../../reports/202611/20261112/20261112_11_M07-P01_session_tabs_and_telemetry_bar_report.md) | `00014` | — | done (merged 2026-11-13) |
| M07-P02 | Two-column workbench left `[ Directories \| Past ]` sidebar with highlighted active `cwd`, filter, pagination, and inline preview | [Plan](../../../plans/202611/20261112/20261112_12_M07-P02_left_directory_sidebar_plan.md) | [Report](../../../reports/202611/20261112/20261112_12_M07-P02_left_directory_sidebar_report.md) | `00015` | — | done (merged 2026-11-13) |
| M07-P03 | Full-pagination `session.past` discovery and one-click `claude`/`codex` resume (local + SSH, `cwd` + `Global`) and `0.2.3` release | [Plan](../../../plans/202611/20261112/20261112_13_M07-P03_past_sessions_and_0_2_3_plan.md) | [Report](../../../reports/202611/20261112/20261112_13_M07-P03_past_sessions_and_0_2_3_report.md) | `00016` | — | done (merged 2026-11-13) |
| M07-P04 | Bidirectional `Claude` <-> `Codex` MCP communication, bordered command/tool cards, Claude `get_usage` `5h`/`Wk` quota, symlink dir/file navigation, and `/exit` tab dismissal | [Plan](../../../plans/202611/20261112/20261112_14_M07-P04_session_comms_command_boxes_and_fixes_plan.md) | [Report](../../../reports/202611/20261112/20261112_14_M07-P04_session_comms_command_boxes_and_fixes_report.md) | `00017` | — | done (merged 2026-11-13) |
| M07-P05 | Centered modal approval window, session-persistent `Allow`, visible peer-conversation cards (`1`-turn event-driven) & clean receiver chat view, mouse-selection auto-copy & universal right-pane copy, and persistent `[ Directories ]` history (`<`/`>`) + bookmarks toolbox | [Plan](../../../plans/202611/20261113/20261113_01_M07-P05_peer_reply_copy_and_dir_nav_plan.md) | [Report](../../../reports/202611/20261113/20261113_01_M07-P05_peer_reply_copy_and_dir_nav_report.md) | `20261113/00001..00002` | — | done (merged 2026-11-13) |

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-11-12 | Scope and phases `M07-P01` to `M07-P05`, patch version bump `0.2.2` -> `0.2.3` on completion |
| G2 phase plan approval | Approved 2026-11-12 / 2026-11-13 | Continuous implementation across `M07-P01` to `M07-P05` |
| G3 inspection acceptance | Approved 2026-11-13 | Owner inspection acceptance across `M07-P01` through `M07-P05` |
| G4 merge approval | Approved 2026-11-13 | Fast-forward merged to `main` (`milestone/M07-two-pane-workbench-and-past-sessions`) on 2026-11-13 |
