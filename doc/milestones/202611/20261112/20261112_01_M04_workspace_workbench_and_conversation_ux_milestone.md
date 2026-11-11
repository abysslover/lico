# M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer

- Status: in_review
- Approved: 2026-11-12 (G1 and G2 approved by the owner for immediate continuous execution through `lico 0.2.0`)
- Project: Lico
- Updated: 2026-11-12
- Implementation: Implemented across `M04-P01` to `M04-P04` (`phase/M04-P04-workspace-file-viewer`, `lico 0.2.0`); awaiting owner G3 and G4
- Next gate: G3 inspection acceptance and G4 merge approval for `M04` (`lico 0.2.0`)

## Goal

Make everyday multi-session human administration in `lico gui` intuitive and
mouse-driven while preserving Lico's single-executable design and `< 50.0 MiB`
idle GUI RSS budget: direct left-sidebar session switching even when a panel is
open, meaningful session titles, reverse-chronological ordering of active
sessions, a clickable action toolbar, clean conversation view hiding internal
host control markers by default while distinguishing user and agent messages by
color, live provider weekly quota remaining (`7d`) alongside working-directory
context, and an on-demand (`F5`-refreshed) working-directory file viewer that
can launch a new session in the viewed folder or open `.html` artifacts in the
external browser.

## Done when

1. Clicking any session row in the left sidebar (or pressing `Alt+1..9` /
   `Ctrl+Tab` / `Session > Session 1..9`) while a panel (`About Lico`, `Keys`,
   `Accounts`, `NewSession`, `Rename`, etc.) is open cleanly closes the panel
   (restoring any parked draft and cancelling unfinished logins) and enters that
   session immediately; sessions derive a human-readable title (`name` -> first
   user prompt summary -> workspace folder -> short ID); active (`running`)
   sessions sort ahead of inactive ones in reverse chronological order of
   activity (`last_active_at`); and a clickable toolbar sits directly beneath
   the menu bar for core actions. (`M04-P01`)
2. Internal host control records (`[session started]...`, `[session closed]`,
   `[provider reconnected]`, `[provider disconnected]`, `[turn started]`,
   `[turn completed]`, `[exit 0]`) remain in durable `.seg` logs and protocol
   streams, are hidden by default in the GUI conversation view (with a `View`
   menu toggle to inspect them, while error/warning notes remain visible), and
   user messages (`>> ...`), agent responses, tool/command lines (`$ ...`,
   `> ...`), and error notes render with distinct colors and subtle user-turn
   background tinting. (`M04-P02`)
3. Both official provider adapters capture rate-limit / weekly quota telemetry
   when reported (`codex` `account/rateLimits/read` and
   `account/rateLimits/updated`; `claude` `rate_limit_event` `seven_day` /
   `five_hour`), expose observed `usage` on `account.list` and `status` (`null`
   when unreported), and display the session's working directory (`cwd` /
   worktree branch) and weekly remaining quota (`7d XX% left`) in the session
   context header and `Accounts` view. (`M04-P03`)
4. An on-demand working-directory file viewer (`folder.list`, `folder.read`,
   `lico files`, `GET /v1/files`, and `View > Working directory files` / toolbar
   `Files`) opens without background `inotify` watchers, refreshes explicitly on
   `F5` (or toolbar `Refresh`), starts a new agent session directly in the
   currently viewed folder (`+ New here`), opens `.html`/`.htm` files and URLs
   in the external browser (`xdg-open`), bumps `lico` to `0.2.0`, passes cold
   suite + two-clone `suite acceptance` + `< 50.0 MiB` idle RSS + `<= +10.0 MiB`
   hidden growth, and installs `~/bin/lico` `0.2.0`. (`M04-P04`)

## Why now

Real daily use of `lico 0.1.0` surfaced concrete human-factors friction:
opening `Help > About Lico` blocked clicking sessions in the left sidebar until
`Esc` was pressed; unnamed sessions fell back to raw IDs or identical folder
names; active sessions did not bubble newest-first; internal lifecycle markers
(`[turn completed]`, `[provider reconnected]`, `[session started]`) cluttered
human-agent reading and user prompts blended into agent prose; primary actions
required hunting through menus or memorizing shortcuts without a toolbar; and
inspecting the active working directory, remaining weekly quota, `.html`
inspection outputs, or starting a session directly from a browsed folder still
required leaving Lico. Addressing these in `M04` (`0.2.0`) completes the human
desktop workbench experience before the separate Windows milestone.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M04-P01 | Direct session switching from panels, derived titles, active reverse-chronological sorting, and clickable toolbar | [Plan](../../../plans/202611/20261112/20261112_01_M04-P01_session_nav_and_toolbar_plan.md) | [Report](../../../reports/202611/20261112/20261112_01_M04-P01_session_nav_and_toolbar_report.md) | `00001_M04-P01_session_nav_and_toolbar/` | — | in_review |
| M04-P02 | Clean conversation view: default-hidden internal control markers and user/agent color differentiation | [Plan](../../../plans/202611/20261112/20261112_02_M04-P02_clean_conversation_view_plan.md) | [Report](../../../reports/202611/20261112/20261112_02_M04-P02_clean_conversation_view_report.md) | `00002_M04-P02_clean_conversation_view/` | — | in_review |
| M04-P03 | Provider weekly quota telemetry (`7d` remaining) and working-directory context display | [Plan](../../../plans/202611/20261112/20261112_03_M04-P03_quota_and_context_bar_plan.md) | [Report](../../../reports/202611/20261112/20261112_03_M04-P03_quota_and_context_bar_report.md) | `00003_M04-P03_quota_and_context_bar/` | — | in_review |
| M04-P04 | On-demand working-directory file viewer (`F5` refresh, `+ New here`, `.html` external browser open) and `0.2.0` release pass | [Plan](../../../plans/202611/20261112/20261112_04_M04-P04_workspace_file_viewer_plan.md) | [Report](../../../reports/202611/20261112/20261112_04_M04-P04_workspace_file_viewer_report.md) | `00005_M04-P04_workspace_file_viewer/` | — | in_review |

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-11-12 | Scope and phases `M04-P01` to `M04-P04`, minor version bump `0.1.0` -> `0.2.0` on completion |
| G2 phase plan approval | Approved 2026-11-12 | Immediate continuous implementation across `M04-P01` to `M04-P04` |
| G3 inspection acceptance | Pending | Awaits owner inspection after `M04-P04` |
| G4 merge approval | Pending | Awaits explicit owner approval after `M04-P04` |
