# M10-P01 Plan — Tab Colors and the Full `past` -> `chats` Rename

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M10-P01-tab-colors-and-chats-rename`
- Milestone: [M10](../../../milestones/202611/20261113/20261113_03_M10_chats_tab_rename_and_resume_milestone.md)

## Final names

| Surface | Old | New |
|---|---|---|
| Label | Past | Chats |
| Wire op | `session.past` | `session.chats` |
| HTTP | `/v1/sessions/past` | `/v1/sessions/chats` |
| CLI | `lico session past ...` | `lico chat list [flags]` (bare `lico chat` lists); `lico chat latest` arrives in P02 |
| Code | `past/`, `sidebar_past.rs`, `PastItem`, `session_past`, `AsyncJob::SidebarPast`/`PastResume`, `ui.sidebar.past`, UI json `past_*` and tab `"past"` | `chats/`, `sidebar_chats.rs`, `ChatItem`, `session_chats`, `SidebarChats`/`ChatResume`, `ui.sidebar.chats`, `chats_*` and `"chats"` |

No top-level `chat` command exists today, and `<noun> <verb>` matches `call list` and `account list`. Files are moved with `git mv`.

## Old and new build mismatch

An old host answers `unknown_op` to `session.chats`, and an old CLI gets `unknown_op` from a new host. The CLI prints one stderr line (`this host (build X) does not know OP; this lico is build Y; run lico host stop and retry`, reading `status.build`) and exits 2; the GUI sidebar shows the same sentence instead of an empty list; the window-start stale-build prompt already covers the usual case. An already installed old binary cannot be changed.

## Steps

- [ ] S01: Red tests: tab colors and labels in the UI json, label contrast `>= 4.5:1`, retired-names scan, `session.past` -> `unknown_op`, CLI mismatch sentence.
- [ ] S02: Tab colors (Directories blue, Chats amber, Calls teal; faint own-color tint when inactive).
- [ ] S03: The rename across Rust, Python protocol, tests, `tests/testmap.toml`, user strings, capabilities lists and living docs.
- [ ] S04: Mismatch sentence in the CLI and the sidebar; Xvfb screenshot of the three tabs; report.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Three distinct tab colors, inactive tint, contrast `>= 4.5:1` | UI json test and unit test |
| 2 | No retired name left; `session.past` gets `unknown_op` | retired-names test and protocol test |
| 3 | Mismatch is explained in one sentence by CLI and GUI | CLI and GUI tests against a host fixture without the op |
| 4 | Changed-suite check green on the staged tree | commit hook |
