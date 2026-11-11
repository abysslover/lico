# M04-P02 Plan: Clean conversation view — default-hidden internal control markers and user/agent color differentiation

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Phase: `M04-P02` (`phase/M04-P02-clean-conversation-view`)
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

In `lico 0.1.0`, every internal host lifecycle record (`[session started] model=...`, `[session closed]`, `[provider reconnected]`, `[provider disconnected]`, `[turn started]`, `[turn completed]`, `[exit 0]`) was rendered inline in the conversation view alongside human prompts and agent responses, and every transcript line shared a single uniform monochrome ink (`0xE0E0E0`). As a result, reading multi-turn conversations required scanning past lifecycle plumbing and distinguishing `>> ` prompts from agent text by prefix alone.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M04-P02` phase plan and link it in the `M04` milestone and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Add `src/host/hostd/src/gui/transcript.rs` classifying transcript records into `ControlEvent` (hidden by default when `!show_events`, never hiding forged `[agent] ...` lines), `ErrorNote` (always visible, coral-amber ink), `User` (`>> ...`, sky-blue ink + subtle dark-blue row background tint `0xFF15_2232`), `Tool` (`$ ...`, `> ...`, `[tool] ...`, `[patch] ...`, muted slate ink), and `Agent` (warm light ink); keep `View::lines` unfiltered for `.seg` logs, protocol cursors, and `lines` diagnostics while filtering `compose` and `selection_text` and preserving pixel-exact `scroll::Shift` behavior | Rust unit tests and `gui::scroll_oracle` pass at scales `1`, `1.25`, and `2` |
| S03 | Add `Cmd::ToggleEvents` (`View > Internal events: hidden|shown`, `Ctrl+Shift+E`) and expose `"show_events"` and `"shown_lines"` in `ui_dump()`; verify `ui.rs <= 1,120` lines | `tests/test_conversation_view.py`, `tests/test_first_view.py`, `tests/test_menu_bar.py`, and `tests/test_size_ratchet.py` pass |
| S04 | Capture manual inspection bundle `00002_M04-P02_clean_conversation_view/`, write the `M04-P02` report, update status tables, and commit | Changed suite and pre-commit guards pass |

## Personal data and security

Internal lifecycle records remain intact in owner-only `0600` `.seg` logs and protocol streams; forged agent lines prefixed with `[agent] ` by `leads::agent_line` are never classified as host `ControlEvent` records and remain visible.
