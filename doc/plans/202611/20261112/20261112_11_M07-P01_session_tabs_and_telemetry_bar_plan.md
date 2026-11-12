# M07-P01 Plan — Maximized launch, `Global` general sessions, token/quota telemetry, chat session tab bar, and bottom telemetry strip

- Status: done (pending G3 inspection after M07-P03)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Branch: `phase/M07-P01-session-tabs-and-telemetry-bar`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

Everyday agentic sessions benefit from four immediate ergonomics improvements
without adding UI bloat: opening `lico gui` already maximized on the active
monitor, keeping folderless general Q&A sessions in an explicit `Global` scope
for quick lightweight-model questions, switching open sessions via a horizontal
tab bar at the top of the chat pane (since only one conversation is viewed at a
time), and displaying live `Model (Effort)`, `Tok: in / out`, `5h / Wk` quota
with reset countdowns, and the current working directory (`cwd` or `Global`)
directly below the prompt input.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit `M07` milestone and this `M07-P01` plan; update `PLAN.md`, `roadmap.md`, and `doc/index_4.md` | Pre-commit guards pass |
| S02 | Collect per-session token counters (`input_tokens`, `output_tokens`) and `effort` in `Session` (`claude.rs` `assistant`/`result` `usage` + `codex.rs` `thread/tokenUsage/updated` and turn usage), expose `effort`, `is_global`, `tokens`, and `account_usage` reset timestamps across `status`, `session.list`, `session.show`, HTTP `/v1/sessions`, and `src/lico_protocol/v1.py` | Rust unit tests and `test_protocol_conformance.py` pass |
| S03 | Request initial window maximization (`with_maximized(true)` unless `--windowed`) in `gui/mod.rs`; elevate folderless sessions to `Global` (`+ Global` toolbar/tab button and `Scope: Folder \| Global` in `New session`); add modular `gui/tabs.rs` (horizontal multi-session tab bar above the chat area with overflow `< >`, close `x`, and status badges) and `gui/telemetry.rs` (color-coded telemetry bar below the prompt input + multi-account quota summary in the bottom status bar) while keeping `gui/ui.rs` below its size-ratchet bound | Rust unit tests and existing GUI tests pass |
| S04 | Add `tests/test_session_tabs_and_telemetry.py` (registered in `tests/testmap.toml`), capture manual inspection bundle, write `M07-P01` report, and commit cleanly | Changed-area tests, `test_size_ratchet.py`, and `test_selection.py` pass |

## Personal data and security

1. Token counters and quota reset timestamps are plain unsigned integers read
   from structured provider JSON streams; no prompt text, credentials, or local
   machine identifiers are stored or logged in telemetry fields.
2. Public tests and screenshots use synthetic accounts, paths, and session
   names only.
