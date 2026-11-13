# Changelog

Release and milestone history for Lico. Implementation proceeds on reviewed
`phase/*` branches, reaches `main` on owner merge approval (G4), and records a
named `milestone/Mxx-<name>` branch at each milestone's closing commit (see
[CONTRIBUTING.md](CONTRIBUTING.md) and [PLAN.md](PLAN.md)). Resource numbers
below are measured on the Linux `hostd` + hand-drawn native client build
(idle window limit `< 50 MiB` RSS; `10 -> 100` hidden-session growth limit
`<= +10 MiB`), and two clean clones build byte-for-byte identical release
binaries (`python3 -m lico_measure.suite acceptance`).

## 0.2.5 — 2026-11-13 (`M09`)

- **Milestone**: [M09 Clean Call control markers, collapsible tool cards, and cross-provider session creation](doc/milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md) (`milestone/M09-collapsible-tools-and-cross-provider-create`, [M09-P03 report](doc/reports/202611/20261113/20261113_08_M09-P03_verification_and_0_2_5_report.md))
- **Measured resources**: idle window `24.86..25.25` / `35.88..36.32 MiB`, `10 -> 100` hidden growth `+0.38 MiB`
- **Highlights**:
  - Strip internal `[call done]` and 200-character line-clipped `[call done` suffixes across send, reply, Call Room transcript, and default conversation view paths.
  - Defer incoming peer replies chronologically below in-flight turns and render peer cards in a distinct mint-emerald palette (`[peer reply from ...]`).
  - Fold `[tool]`, `$`, and `[patch]` spans by default into clickable `▸ ...` / `▾ ...` cards in an indigo-violet palette (`#161426` / `#1D1934` / `#121020`).
  - Extend `mcp__lico__create` (`kind: "claude" | "codex"`, optional `prompt`, optional `name`, optional `scope: "folder" | "global"`, default `mode: "auto"`) so any `Claude` or `Codex` session can request a new `Claude` or `Codex` tab in `lico gui`.

## 0.2.4 — 2026-11-13 (`M08`)

- **Milestone**: [M08 Persistent Call Rooms, `@AgentName` shared-context mentions, and bounded multi-turn agent collaboration](doc/milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md) (`milestone/M08-call-rooms-and-agent-mentions`, [M08-P04 report](doc/reports/202611/20261113/20261113_05_M08-P04_unified_rename_and_agent_mentions_report.md))
- **Measured resources**: idle window `24.68..24.95` / `35.86..36.31 MiB`, `10 -> 100` hidden growth `+0.28 MiB`
- **Highlights**:
  - Persistent per-pair Call Rooms (`⇄ A · B` stored in `0600` `calls.json`), left-sidebar `[ Calls ]` tab, top-bar Call Room tabs with live human intervention (`call.say`), and clickable transcript jump cards.
  - `@AgentName` and `@all` prompt autocomplete and shared-context mentions attaching up to 8 recent turns (`<= 16 KiB`) plus Call Room history inside host-verified `OWNER-AUTHORIZED` envelopes.
  - Unified `/rename` command across providers, `mcp__lico__rename` self-rename tool, and agent-to-agent `@SessionName` addressing.
  - Bounded 4-round-trip exchanges with `[call done]` early stop, live `Hang up`, and `lico call` CLI + `/v1/calls` HTTP parity.

## 0.2.3 — 2026-11-13 (`M07`)

- **Milestone**: [M07 Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume](doc/milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md) (`milestone/M07-two-pane-workbench-and-past-sessions`, [M07-P03 report](doc/reports/202611/20261112/20261112_13_M07-P03_past_sessions_and_0_2_3_report.md))
- **Measured resources**: idle window `24.65..25.00` / `35.84..35.89 MiB`, `10 -> 100` hidden growth `+0.26 MiB`
- **Highlights**:
  - Maximized default launch, dedicated `Global` general sessions, multi-session top chat tab bar, and color-coded bottom telemetry strip (`Model (Effort)` | `Tok in/out` | `5h` and `Wk` quota + reset | `cwd / Global`).
  - Collapsible left sidebar (`[ Directories | Past ]`) with persistent Back/Forward (`<`/`>`) history, bookmarks toolbox, symlink directory/file navigation, and full-pagination `session.past` discovery with one-click `claude`/`codex` resume.
  - Centered modal approval notifications with session-persistent `Allow`, bordered command/peer cards, and mouse drag-selection auto-copy across all right-pane screens.

## 0.2.2 — 2026-11-12 (`M06`)

- **Milestone**: [M06 Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions](doc/milestones/202611/20261112/20261112_03_M06_hidden_files_and_ssh_remote_sessions_milestone.md) (`milestone/M06-hidden-files-and-ssh-remote-sessions`, [M06-P03 report](doc/reports/202611/20261112/20261112_10_M06-P03_ssh_remote_files_and_sessions_0_2_2_report.md))
- **Measured resources**: idle window `24.19..24.52` / `35.27..35.53 MiB`, `10 -> 100` hidden growth `+0.26 MiB`
- **Highlights**:
  - Default-off `.dotfiles` visibility toggle (`Hidden: off/on`) in the directory viewer.
  - `~/.ssh/config` host discovery, `[ssh: <host>]` provider accounts with supervised remote `Sign in`, SSH remote directory browsing (`Host` combo-box dropdown with non-blocking `ssh_unreachable` handling), UTF-8 file preview, and persistent remote `claude`/`codex` session execution and resume.

## 0.2.1 — 2026-11-12 (`M05`)

- **Milestone**: [M05 Working-directory restore across restarts, paged directory navigation, and unified file opening](doc/milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md) (`milestone/M05-workspace-continuity-and-file-navigation`, [M05-P03 report](doc/reports/202611/20261112/20261112_07_M05-P03_smart_file_open_and_0_2_1_report.md))
- **Measured resources**: idle window `24.23..24.36` / `35.27..35.34 MiB`, `10 -> 100` hidden growth `+0.45 MiB`
- **Highlights**:
  - Automatic working-directory and workspace-view restoration across `lico` restarts.
  - Top-anchored panel rendering with `Page X / Y`, `< Prev`, `Next >`, and mouse-wheel navigation up to `10,000` directory entries.
  - Unified file activation opening UTF-8 text-family files in-window while delegating web, document, image, media, archive, and binary files to `xdg-open`.

## 0.2.0 — 2026-11-12 (`M04`)

- **Milestone**: [M04 Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](doc/milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md) (`milestone/M04-workspace-workbench-and-conversation-ux`, [M04-P04 report](doc/reports/202611/20261112/20261112_04_M04-P04_workspace_file_viewer_report.md))
- **Measured resources**: idle window `24.18..24.42` / `35.29..35.50 MiB`, `10 -> 100` hidden growth `+0.38 MiB`
- **Highlights**:
  - Direct sidebar session switching while panels are open, derived session titles, active-first reverse-chronological sorting, and a clickable action toolbar.
  - Clean conversation view hiding internal control markers by default with user/agent color differentiation, provider weekly (`7d`) quota remaining display, and an on-demand (`F5`-refreshed) working-directory file viewer with `+ New here` and `.html` launch.

## 0.1.0 — 2026-11-11 to 2026-11-12 (`M01` – `M03`)

- **[M03 Linux administrator GUI completeness and control-surface parity](doc/milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)** (`milestone/M03-admin-gui-and-control-parity`, accepted 2026-11-12, [M03-P04 report](doc/reports/202611/20261111/20261111_09_M03-P04_control_parity_and_review_report.md); idle window `23.96..24.21` / `35.08..35.38 MiB`, `10 -> 100` hidden growth `+0.37 MiB`):
  - In-window account rename, session rebind, live permission-mode toggle, workspace-grouped session navigation, `12..=20` px font sizing, multi-line prompt mouse/keyboard selection, cut/copy, undo/redo, resize reflow, and `capabilities` + loopback HTTP parity (`POST /v1` and REST routes).
- **[M02 Agentic-coding basics](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md)** (`milestone/M02-agentic-coding-basics`, accepted 2026-11-11, [planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md); idle window `24.01..24.18` / `35.14..35.23 MiB`):
  - Real-folder and `git worktree` sessions, inter-session sending, agent-callable Lico tools, screen capture and clipboard integration, private virtual-display computer use, attention marks and desktop notifications, conversation search, and per-session memory display.
- **[M01 Measured Linux foundation](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)** (`milestone/M01-measured-linux-foundation`, accepted 2026-11-12, [M01-P08 report](doc/reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md)):
  - Measured selection of the Rust `hostd` runtime and hand-drawn native X11 client, protocol v1 snapshot + delta recovery, OS keyring credential store and multi-account isolation (`M01-P04`, `M01-P06`), single-executable packaging, and byte-for-byte reproducible clean-clone builds.
