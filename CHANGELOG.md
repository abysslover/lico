# Changelog

Release and milestone history for Lico. Implementation proceeds on reviewed
`phase/*` branches, reaches `main` on owner merge approval (G4), and records a
named `milestone/Mxx-<name>` branch at each milestone's closing commit (see
[CONTRIBUTING.md](CONTRIBUTING.md) and [PLAN.md](PLAN.md)). Resource numbers
below are measured on the Linux `hostd` + hand-drawn native client build
(idle window limit `< 50 MiB` RSS; `10 -> 100` hidden-session growth limit
`<= +10 MiB`), and two clean clones build byte-for-byte identical release
binaries (`python3 -m lico_measure.suite acceptance`).

## 0.3.1 — 2026-11-14 (`M13`)

- **Milestone**: [M13 Discovery follow-up, chat list polish, and the split of the big GUI tests](doc/milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md) (`milestone/M13-discovery-follow-up-and-chat-list-polish`, [M13-P04 report](doc/reports/202611/20261114/20261114_07_M13-P04_verification_and_0_3_1_report.md))
- **Measured resources**: idle window `25.45..25.64` / `36.27..36.51 MiB`, `10 -> 100` hidden growth `+0.32 MiB`, cold 540-test suite `118.9..124.9 s`, tree peak `1,414.2..1,489.4 MiB` (was `1,964.8..1,998.7 MiB` at `0.3.0`)
- **Highlights**:
  - **The discovery dialog keeps what it found.** The last result of each host and users' choice stays in the window's memory (nothing on disk) with its age, `found 3 min ago`; opening the dialog or choosing the host again shows it without a scan, and Enter or `Refresh` scans again. The command, the HTTP route and the tool still scan fresh.
  - **A faster scan with the same bounds.** Every port gets its silent first contact at once, so the quiet period that protects servers that speak first is waited for once, not once per round of eight: `608 -> 160 ms` on a fixture of 46 sockets, `464 -> 172 ms` for the script on an ssh host.
  - **A quieter, readable list.** Servers that answered but list no model are folded behind one line; a row owned by `ssh` says `via ssh` and is never hidden by its name; rows are two lines with a selection and the selected row whole under the list; the host name is wrapped, never cut; one line says what "no model listed" means.
  - **The Chats tab says when.** Every card starts its second line with when the chat was last written (`10m ago`), a chat that another program writes keeps its title whole and says `[elsewhere] written 4m ago`, and `Native (~/.claude)` is no longer repeated on every row (another account or an ssh host is still named).
  - **A smaller test-suite memory peak.** `build_id()` hashed the program's own file whole (about `200 MiB` in a test binary, in every test that touched it); it is read in pieces now, and the font systems and private X servers of the Rust tests run a few at a time. Two flaky tests were found and fixed on the test side.

## 0.3.0 — 2026-11-14 (`M12`)

- **Milestone**: [M12 Find OpenAI-compatible Local LLM endpoints on this machine and on ssh hosts](doc/milestones/202611/20261114/20261114_01_M12_local_llm_endpoint_discovery_milestone.md) (`milestone/M12-local-llm-endpoint-discovery`, [M12-P03 report](doc/reports/202611/20261114/20261114_03_M12-P03_verification_and_0_3_0_report.md))
- **Measured resources**: idle window `24.76..25.11` / `35.82..36.23 MiB`, `10 -> 100` hidden growth `+0.41 MiB`
- **Highlights**:
  - **Find Local LLM servers, read-only and on demand.** `Session > Find Local LLM servers...` (a dialog that scans only on Enter or a click), `lico llm discover [--ssh-host HOST] [--all-users]`, `POST /v1/llm/discover`, and the tool `llm_endpoints` (which asks the person each time). It lists the listening TCP ports of your own processes (all users' ports only when asked) that answer like an OpenAI-compatible server (`GET /v1/models`) or an Ollama server (`GET /api/tags`): kind, address and whether it listens on loopback or all addresses, whether it wants a key (reported, never tried), its model ids and the name of its process.
  - **Careful by construction.** Only sockets that listen on the machine itself, at the machine's own address; a quiet connection first, so a server that speaks first (SSH, a database) is sent nothing; HTTP/1.0 GET with no body, credential or cookie, no redirect followed, at most 64 KiB read, at most 64 ports, 8 seconds; nothing runs in the background, nothing is stored, no other program's configuration or key file is read.
  - **ssh hosts the same way**: a bounded Python 3 script run in batch mode (20 seconds, 128 KiB of output) with clean words for an unreachable host, a missing `python3`, a timeout, too much output, a host that is not Linux. Using a found server is a later step.

## 0.2.7 — 2026-11-13 (`M11`)

- **Milestone**: [M11 The provider's own default home as a visible "Native" choice, running-elsewhere safety, and the empty-Resume hint](doc/milestones/202611/20261113/20261113_04_M11_native_provider_home_chats_milestone.md) (`milestone/M11-native-provider-home-chats` once accepted, [M11-P03 report](doc/reports/202611/20261113/20261113_15_M11-P03_verification_and_0_2_7_report.md)); stacked on the M10 branches, which are not yet accepted
- **Measured resources**: idle window `25.01..25.10` / `35.91..36.29 MiB`, `10 -> 100` hidden growth `+0.23 MiB`
- **Highlights**:
  - **Earlier chats are found again.** The provider's own default home (`~/.claude`, `~/.codex`, or `CLAUDE_CONFIG_DIR` / `CODEX_HOME`) is a visible entry named `Native (~/.claude)` / `Native (~/.codex)` next to the managed accounts: in `+ New chat`, the Chats tab, beside the input target, in `lico session create --account native-claude|native-codex`, `lico chat --account`, and `mcp__lico__create` (`account: "native"`). It is closed until you choose (first-run question once per window start: Yes / No / Ask later, counts only before the answer; View menu entries; `lico settings --native-claude|--native-codex`). The provider runs in place with its own login: Lico never reads, copies, links or decrypts it. The old rule "default home only when no managed account exists" is gone.
  - **Running elsewhere.** A chat that another program is writing right now (modified in the last 180 s, or a Claude Code or Codex process of yours that Lico did not start names it or runs in its folder since before its last write) is marked `[elsewhere]`; its Resume is refused (`chat_running_elsewhere`) in the dialog, the CLI and the tool, and nothing is adopted or started; `New` always works; closed chats of the same folder resume normally.
  - **Empty-Resume hint.** When Resume finds no chat for the chosen account, the dialog, `--resume-latest` and the tool say how many chats of the folder exist under other accounts and homes (counts only).
  - A Native caller's new session of the same kind through the tool stays Native (it no longer moves to the managed default). Host tests start with a private synthetic `HOME`, so no test can read the real default homes.

## 0.2.6 — 2026-11-13 (`M10`)

- **Milestone**: [M10 Chats tab, full `past` -> `chats` rename, "+ New chat" with Resume, and resume parity](doc/milestones/202611/20261113/20261113_03_M10_chats_tab_rename_and_resume_milestone.md) (`milestone/M10-chats-tab-rename-and-resume` once accepted, [M10-P04 report](doc/reports/202611/20261113/20261113_12_M10-P04_verification_and_0_2_6_report.md))
- **Measured resources**: idle window `24.80..25.18` / `35.82..36.08 MiB`, `10 -> 100` hidden growth `+0.35 MiB`
- **Highlights**:
  - The three sidebar tabs have their own colors: Directories blue, Chats amber, Calls teal, each with a faint own-hue tint when inactive.
  - **Breaking rename, no alias**: `Past` is `Chats` everywhere. Wire op `session.past` -> `session.chats`, HTTP `/v1/sessions/past` -> `/v1/sessions/chats`, CLI `lico session past` -> `lico chat [list]`. The old names answer `unknown_op` or a usage error; a command line and a host of different builds say so in one sentence (`run lico host stop and retry`).
  - A `+ New chat` row above the parent row of the Directories tab opens a dialog: choose an account, then `Resume` (default) or `New`. `Resume` shows `Last Session : <provider> / <YYYY-MM-DD HH:MM> / <title> (<N> turns)` and continues the latest chat of the directory for that account (a tab that already holds it is shown again); with none it opens a new one and says so. Up/Down, Left/Right, Enter, Esc; `Options...` opens the full form.
  - Chats carry `turns` (bounded count, `null` when unknown). One host rule, `latest chat of a folder and account that has a saved transcript`, serves the list, `lico chat latest`, the dialog and creation. `lico session create --resume-latest` and `mcp__lico__create` with `resume: true` use it (default off).

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
