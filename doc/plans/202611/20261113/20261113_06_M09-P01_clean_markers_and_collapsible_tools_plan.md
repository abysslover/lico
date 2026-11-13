# M09-P01 Plan — Strip `[call done]` Control Markers and Add Collapsible Tool Cards

- Status: done
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M09-P01-clean-markers-and-collapsible-tools`
- Milestone: [M09](../../../milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md)

## Objective

1. **Hide `[call done]` and clipped `[call done` suffixes**:
   - Add `call_room::strip_call_done` + display-line sanitizer to strip case-insensitive `[call done]` markers and line-clipped `[call done` suffixes across `calls::send_with`, `calls::deliver_peer_reply`, `mention::dispatch_assistant_mentions`, `call_room::record_session_transfer`, `gui/call_view.rs`, and `gui/transcript.rs` (`!show_events`), while preserving `marked_done` so Call Room auto-rounds still settle cleanly.
2. **Default-collapsed clickable tool/command cards with distinct colors**:
   - Introduce `src/host/hostd/src/gui/tool_fold.rs` integrated with `transcript.rs`, `ui.rs`, and `clip.rs`.
   - Group contiguous tool/command/patch invocations (`[tool] ...`, `$ ...`, `[patch] ...` + `> ...`, stdout, `[tool error]`, `[exit N]`, fold notes) in session chat views when `!show_events`.
   - Collapse each invocation by default to a 1-line clickable summary pill (`▸ [tool] ...` / `▸ $ ...` / `▸ [patch] ...`) with line count and error indicator when failed.
   - Toggle expand (`▾ ...`) / collapse (`▸ ...`) on stationary mouse click in the chat viewport, keeping drag-to-select/copy intact.
   - Render collapsed pills and expanded tool header/body cards with a distinct indigo-violet background, rail, border, and cool periwinkle ink clearly separated from warm-white `AGENT_INK`, blue `USER_BG`, and teal `PEER_BG`.

## Steps

- [x] S01: Implement `strip_call_done` and clipped-tail sanitization in `call_room.rs`, `calls.rs`, `mention.rs`, `gui/call_view.rs`, and `gui/transcript.rs`.
- [x] S02: Implement `gui/tool_fold.rs` + distinct tool card styles/colors in `gui/transcript.rs`, state & dump support in `gui/ui.rs`, and stationary click toggle in `gui/clip.rs`.
- [x] S03: Add Rust unit tests and update/extend `test_conversation_view.py` and `test_peer_reply_copy_and_dir_nav.py`; verify `scroll_oracle`, `test_size_ratchet.py`, and targeted suites pass, then commit `M09-P01`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | `[call done]` and 200-char-clipped `[call done` suffixes are hidden in `send`, `peer reply`, Call Room entries, and GUI transcripts (`!show_events`), while settling `room.active = false` | `call_room::tests`, `transcript::tests` |
| 2 | `[tool]`, `$`, and `[patch]` cards are collapsed by default (`▸ ...`), toggle to `▾ ...` + full body on stationary click, collapse again on header click, and show all raw records when `show_events = true` | `tool_fold::tests`, `test_conversation_view.py`, `test_peer_reply_copy_and_dir_nav.py` |
| 3 | Expanded and collapsed tool cards use distinct indigo-violet backgrounds, borders, rails, and ink colors separated from normal agent prose, and `scroll_oracle` passes pixel-for-pixel at scales `1.0`, `1.25`, and `2.0` | `scroll_oracle`, `transcript::tests` |
| 4 | Every source and test file stays within line-count ratchets | `tests/test_size_ratchet.py` |
