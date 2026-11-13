# M09-P01 Report — Strip `[call done]` Control Markers and Add Collapsible Tool Cards

- Status: accepted at G3 and merged to `main` (2026-11-13)
- Date: 2026-11-13
- Branch: `phase/M09-P01-clean-markers-and-collapsible-tools`
- Plan: [M09-P01 Plan](../../../plans/202611/20261113/20261113_06_M09-P01_clean_markers_and_collapsible_tools_plan.md)
- Milestone: [M09](../../../milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md)

## Summary of Changes

1. **Internal `[call done]` and Clipped-Tail Sanitization**:
   - Added `call_room::strip_call_done` and `call_room::strip_call_done_display_line` (`src/host/hostd/src/call_room/ops.rs`), removing case-insensitive `[call done]` markers as well as 200-char line-clipped `[call done` tails while returning `marked_done = true`.
   - Wired `strip_call_done` into `calls::send_with` (via `CALL_DONE_FLAG` -> `after_peer_deliver` -> `call_room::record_session_transfer`), `calls::deliver_peer_reply`, `gui/call_view.rs`, and `gui/transcript.rs` (`!show_events`), settling Call Room auto-rounds (`room.active = false`) while keeping user-visible messages completely free of control markers.
2. **Default-Collapsed Clickable Tool / Command / Patch Cards & Distinct Indigo-Violet Palette**:
   - Added `src/host/hostd/src/gui/tool_fold.rs` and integrated it with `transcript.rs`, `ui.rs`, and `clip.rs`.
   - Contiguous `[tool]`, `$`, and `[patch]` `ROLE_BOX` invocations in session chat views fold by default (`!show_events`) into 1-line clickable pills (`▸ [tool] ... · N lines`, with `error` badge when failed).
   - Stationary mouse clicks (`!moved && !shift`) toggle `(session_id, head_seq)` in `UiState::expanded_tools`, expanding to `▾ ...` + full body and collapsing back on click while preserving mouse drag-selection and auto-copy.
   - Styled collapsed pills (`TOOL_FOLD_BG = 0xFF16_1426`, `TOOL_FOLD_INK`), expanded headers (`TOOL_HEAD_BG = 0xFF1D_1934`, `TOOL_HEAD_RAIL = 0xFF9B_8AFB`, `TOOL_HEAD_INK`), and expanded bodies (`CMD_BG = 0xFF12_1020`, `CMD_BORDER = 0xFF2E_2852`, `CMD_RAIL = 0xFF7C_6CF6`, cool periwinkle ink) to stand clearly apart from normal conversation prose.

## Acceptance Verification

| # | Criterion | Result |
|---|---|---|
| 1 | `[call done]` and clipped `[call done` suffixes hidden across send, peer reply, Call Room, and GUI transcripts while settling `room.active = false` | Pass (`call_room::ops::tests::strip_call_done_removes_full_and_clipped_markers_and_settles_transfer_room`) |
| 2 | Default-collapsed `▸ ...` tool/command/patch pills toggle to `▾ ...` + body on click and respect `show_events` | Pass (`gui::tool_fold::tests`, `tests/test_peer_reply_copy_and_dir_nav.py`) |
| 3 | Distinct indigo-violet tool card palette matches full repaint across scales `1.0`, `1.25`, `2.0` | Pass (`gui::scroll_oracle`) |
| 4 | Line-count ratchets and testmap selection clean | Pass (`tests/test_size_ratchet.py`, `tests/test_selection.py`) |

## Suite Results

- `cargo test -- --test-threads=4`: `608 passed; 0 failed` (`26.99 s`).
- Targeted Python GUI & guard suites (`test_size_ratchet.py`, `test_selection.py`, `test_conversation_view.py`, `test_peer_reply_copy_and_dir_nav.py`, `test_call_rooms_and_mentions.py`): `37 passed` (`24.47 s`).
