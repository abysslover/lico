# Lico

a lightweight workbench for agentic workflows

Lico is a proposed desktop workbench for persistent command-line agents. The
session host owns execution and output independently of the interface, so
closing or hiding a view should let work continue. Hidden views are intended to
release presentation resources and return from a snapshot plus later events.

Linux comes first; Windows follows through explicit platform interfaces. The
renderer, implementation language and integration strategy are selected from
measurements recorded in the first milestone's reports and accepted by the owner
on 2026-11-11.

## Current status

This repository holds the [product requirements](doc/product/202610/20261023/00001_product_spec/product_spec.md), the milestones,
the phase plans and reports, and the source of the finished phases. Implementation is written on reviewed phase branches. Since
2026-10-27 each finished phase reaches `main` on the owner's merge approval (G4); the phase branches are
kept, and a branch `milestone/Mxx-<name>` marks a milestone once it is achieved (see [CONTRIBUTING.md](CONTRIBUTING.md)). On
2026-11-11, 2026-11-12, and 2026-11-13 the owner accepted G3 and approved G4 merge through M08-P04 (`milestone/M01-measured-linux-foundation` through `milestone/M08-call-rooms-and-agent-mentions`, `lico 0.2.4`). Resource budgets are targets unless a report marks a number
as measured.

The first milestone, the [measured Linux foundation](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md), is complete (`milestone/M01-measured-linux-foundation`, accepted and published on 2026-11-12).
The second milestone, [M02 Agentic-coding basics](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md), is complete (`milestone/M02-agentic-coding-basics`, accepted and published on 2026-11-11):
sessions that run in a real folder or git worktree, sending between sessions, Lico tools that an agent can call, screen capture and clipboard, computer use,
attention marks, notifications, conversation search, and per-session memory in the window, verified against the limit of an idle window under 50 MiB (`24.01..24.18` / `35.14..35.23 MiB`).
The third milestone, [M03 Linux administrator GUI completeness and control-surface parity](doc/milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md), is complete (`milestone/M03-admin-gui-and-control-parity`, accepted and published on 2026-11-12 at `lico 0.1.0`, idle window `23.96..24.21` / `35.08..35.38 MiB`, 10->100 session hidden growth `+0.37 MiB`): in-window account rename, session rebind and live permission-mode toggle, workspace-grouped session navigation and `12..=20` px font sizing, multi-line prompt mouse/keyboard selection, cut/copy, undo/redo and resize reflow, and `capabilities` + loopback HTTP parity (`POST /v1` and REST routes).
The fourth milestone, [M04 Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](doc/milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md), is complete (`milestone/M04-workspace-workbench-and-conversation-ux`, accepted and published on 2026-11-12 at `lico 0.2.0`, idle window `24.18..24.42` / `35.29..35.50 MiB`, 10->100 session hidden growth `+0.38 MiB`): direct left-sidebar session switching while panels are open, derived session titles, active-first reverse-chronological sorting, clickable action toolbar, clean conversation view hiding internal control markers by default with user/agent color differentiation, provider weekly (`7d`) quota remaining display, and an on-demand (`F5`-refreshed) working-directory file viewer with `+ New here` and external `.html` browser launch.
The fifth milestone, [M05 Working-directory restore across restarts, paged directory navigation, and unified file opening](doc/milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md), is complete (`milestone/M05-workspace-continuity-and-file-navigation`, accepted and published on 2026-11-12 at `lico 0.2.1`, idle window `24.23..24.36` / `35.27..35.34 MiB`, 10->100 session hidden growth `+0.45 MiB`): automatic working-directory and workspace-view restoration across `lico` restarts, top-anchored panel rendering with `Page X / Y` / `< Prev` / `Next >` / mouse-wheel navigation up to `10,000` directory entries, and unified file activation opening text-family files directly in-window while delegating web, document, image, media, archive, and binary files to the OS default application (`xdg-open`).
The sixth milestone, [M06 Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions](doc/milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md), is complete (`milestone/M06-hidden-files-and-ssh-remote-sessions`, accepted and merged on 2026-11-12 at `lico 0.2.2`, idle window `24.19..24.52` / `35.27..35.53 MiB`, 10->100 session hidden growth `+0.26 MiB`): default-off `.dotfiles` visibility toggle (`Hidden: off/on`), `~/.ssh/config` host discovery, `[ssh: <host>]` provider accounts with supervised remote `Sign in`, and SSH remote directory browsing (`Host` combo-box dropdown + non-blocking `ssh_unreachable` handling), UTF-8 file preview, and persistent `claude`/`codex` session execution & resume.
The seventh milestone, [M07 Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume](doc/milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md), is complete (`milestone/M07-two-pane-workbench-and-past-sessions`, accepted and merged on 2026-11-13 at `lico 0.2.3`, idle window `24.65..25.00` / `35.84..35.89 MiB`, 10->100 session hidden growth `+0.26 MiB`): maximized default launch, dedicated `Global` general sessions, multi-session top chat tab bar, color-coded bottom telemetry strip (`Model (Effort)` | `Tok in/out` | `5h·Wk` quota + reset | `cwd / Global`), collapsible two-tab left sidebar (`[ Directories | Past ]`) with persistent Back/Forward (`<`/`>`) history & bookmarks toolbox and symlink dir/file navigation, full-pagination `session.past` discovery with one-click `claude`/`codex` resume, centered modal approval notifications with session-persistent `Allow` and bordered command/peer cards, and mouse drag-selection auto-copy across all right-pane screens.
The eighth milestone, [M08 Persistent Call Rooms, `@AgentName` shared-context mentions, and bounded multi-turn agent collaboration](doc/milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md), is complete (`milestone/M08-call-rooms-and-agent-mentions`, accepted and merged on 2026-11-13 at `lico 0.2.4`, idle window `24.68..24.95` / `35.86..36.31 MiB`, 10->100 session hidden growth `+0.28 MiB`): persistent per-pair Call Rooms (`⇄ A · B` in `0600` `calls.json`), `@AgentName` / `@all` editor autocomplete and shared-context mentions (attaching up to 8 recent turns / `16 KiB` plus Call Room history inside host-verified `OWNER-AUTHORIZED` envelopes), unified `/rename` command across providers + `mcp__lico__rename` self-rename tool + agent-to-agent `@SessionName` addressing, bounded 4-round-trip exchanges with `[call done]` early stop and live `Hang up`, a third left-sidebar `[ Calls ]` tab, top-bar Call Room tabs with live human intervention (`call.say`), clickable transcript jump cards, and `lico call` CLI + `/v1/calls` HTTP parity.
Its [planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) gives the M02 scope and order, and the
[roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) shows where each milestone stands.

The program is one executable, `lico` (`0.2.4`); its build is described in [src/host/README.md](src/host/README.md) and end-to-end verification is in [doc/manual_end_to_end.md](doc/manual_end_to_end.md). Two clean clones build byte-for-byte identical release binaries (`python3 -m lico_measure.suite acceptance`, [M01-P08 report](doc/reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md), [M03-P04 report](doc/reports/202611/20261111/20261111_09_M03-P04_control_parity_and_review_report.md), [M04-P04 report](doc/reports/202611/20261112/20261112_04_M04-P04_workspace_file_viewer_report.md), [M05-P03 report](doc/reports/202611/20261112/20261112_07_M05-P03_smart_file_open_and_0_2_1_report.md), [M06-P03 report](doc/reports/202611/20261112/20261112_10_M06-P03_ssh_remote_files_and_sessions_0_2_2_report.md), [M07-P03 report](doc/reports/202611/20261112/20261112_13_M07-P03_past_sessions_and_0_2_3_report.md), and [M08-P04 report](doc/reports/202611/20261113/20261113_05_M08-P04_unified_rename_and_agent_mentions_report.md)). The build works from a public clone with
documented, publicly available dependencies.
Provider tools are independently installed unless redistribution is permitted.

## Project map

- [Documentation](doc/README.md)
- [Current status and approval gates](PLAN.md)
- [Contributing](CONTRIBUTING.md)
- [Experiments](exp/README.md)
- [Source](src/README.md) and [tests](tests/README.md)

## Authentication and accounts

The intended design uses official provider authentication only. Application-owned
secrets belong in the operating system credential store, with no plaintext
fallback. Accounts for the same provider must have separate credentials,
refresh state and session bindings. A default-account change must not retarget
running work, and an error must not silently switch accounts or billing modes.
The phases that implement them (M01-P04 and M01-P06) were accepted by the owner on 2026-11-11.

## License

[MIT](LICENSE). Provider names identify integrations and do not imply endorsement.
