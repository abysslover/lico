# M04-P02 report — clean conversation view: default-hidden internal control markers and user/agent color differentiation

- Version: 1.0.0
- Status: in_review (awaiting owner G3 acceptance at M04 milestone boundary)
- Date: 2026-11-12
- Plan: [M04-P02](../../../plans/202611/20261112/20261112_02_M04-P02_clean_conversation_view_plan.md) (version 1.0.0, G2 approved by the owner on 2026-11-12)
- Milestone: [M04 — Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](../../../milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md)
- Branch: `phase/M04-P02-clean-conversation-view` (branched from `phase/M04-P01-session-nav-and-toolbar` `69cae56`)
- Inspection bundle: `private/manual_inspection/202611/20261112/00002_M04-P02_clean_conversation_view/`

## What was done

1. **S02 — Default-hidden internal control markers, role coloring, and pixel-exact scroll preservation:**
   - Created `src/host/hostd/src/gui/transcript.rs` (`217` lines) classifying transcript lines into `ControlEvent` (`[session started]...`, `[session closed]`, `[provider reconnected]`, `[provider disconnected]`, `[turn started]`, `[turn completed]`, `[exit 0]`, `[account changed]`), `ErrorNote` (always visible, coral-amber `0xF48C6C`), `User` (`>> ...`, sky-blue `0x82CAFF` with subtle row background tint `0xFF15_2232`), `Tool` (`$ ...`, `> ...`, `[tool] ...`, `[patch] ...`, muted slate `0x94A0AE`), and `Agent` (`0xEAEEF5`). Forged `[agent] ...` lines from `leads::agent_line` are never classified as `ControlEvent` and remain visible.
   - Kept `View::lines` unfiltered so durable `.seg` logs, protocol cursors, and `lines` diagnostics retain every internal lifecycle event, while filtering GUI rendering (`compose` and `selection_text`) when `!show_events`.
   - Keyed `BufferLine` pool coloring to the transcript range (`tx_range`) so non-transcript UI text (including the editor prompt `> ...`) retains standard foreground ink, and updated `paint_user_backgrounds` + fixed signed alpha interpolation in `ui.rs::blend` so incremental `scroll::Shift` matches full repaint pixel-for-pixel across scales `1.0`, `1.25`, and `2.0`.
2. **S03 — `View > Internal events` toggle and `ui_dump` observability:**
   - Added `Cmd::ToggleEvents` (`View > Internal events: hidden|shown`, shortcut `Ctrl+Shift+E`) in `menu.rs` and `panel.rs`, and exposed `"show_events"` and `"shown_lines"` in `ui_dump()` (`ui.rs` at `1,115` lines `<= 1,120` allowlist; `core.rs` unchanged at `988` lines).
3. **S04 — Automated verification and manual inspection bundle:**
   - Added Rust unit tests in `transcript.rs` and `scroll_oracle.rs` (`user_prompt_backgrounds_role_colors_and_hidden_control_events_match_full_repaint_across_scales`), and added `tests/test_conversation_view.py` + `tests/testmap.toml` entry.
   - Captured 2 real Xvfb screenshots and `index.html` in `private/manual_inspection/202611/20261112/00002_M04-P02_clean_conversation_view/`.

## Verification results

| Check / Metric | Result | Budget / Guard |
|---|---|---|
| `cargo test --bin lico` | `574` passed (`1` ignored) | Zero failures |
| `pytest tests/test_size_ratchet.py tests/test_no_hidden_characters.py tests/test_selection.py tests/test_first_view.py tests/test_menu_bar.py tests/test_editor_selection.py tests/test_m02_milestone_review.py tests/test_toolbar_and_nav.py tests/test_conversation_view.py` | `46/46` passed (`41.19 s`) | Zero failures |
| `src/host/hostd/src/gui/ui.rs` / `core.rs` / `transcript.rs` line counts | `1,115` / `988` / `217` lines | `<= 1,120` / `<= 988` / `<= 800` |
