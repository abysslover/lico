# M03-P03 report — prompt editor modularization, mouse/keyboard selection, undo/redo, and resize reflow

- Version: 1.0.0
- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Date: 2026-11-11
- Plan: [M03-P03](../../../plans/202611/20261111/20261111_07_M03-P03_editor_and_reflow_plan.md) (version 1.0.0, G2 approved by the owner on 2026-11-11)
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Branch: `phase/M03-P03-editor-and-reflow` (branched from `phase/M03-P02-workspace-navigator` `b4771ce`)
- Inspection bundle: `private/manual_inspection/202611/20261111/00009_M03-P03_editor_and_reflow/`

## What was done

1. **S02 — Modularized `gui/editor` well under the 800-line size ratchet and tightened `ui.rs`:**
   - Replaced monolithic `src/host/hostd/src/gui/editor.rs` (`798` lines) and `editor_tests.rs` with `src/host/hostd/src/gui/editor/{mod,selection,app,tests}.rs` (`703`, `353`, `344`, and `410` lines), preserving every grapheme-cluster, tab-expansion, paste-normalisation, recall, picture-chip, and panel-masking invariant.
   - Consolidated editor copy/cut/paste glue into `editor/app.rs`, shrinking `src/host/hostd/src/gui/ui.rs` from `1,166` lines down to **`1,115` lines** and ratcheting `tests/size_allowlist.toml` down from `1,168` to `1,120`.
2. **S03 — Mouse & keyboard selection, `Ctrl+C`/`Ctrl+X`, bounded per-session undo/redo, and resize reflow:**
   - Added grapheme-aligned prompt selection (`anchor`, `selection_range`, `selected_text`, `sel_rects`) driven by both `Shift` + movement keys (`Left`, `Right`, `Ctrl+Left`, `Ctrl+Right`, `Home`, `End`, `Up`, `Down`) and mouse click/drag inside `ed.rect` (mapping expanded tabs, wide CJK glyphs, and combining marks accurately).
   - Wired `Ctrl+C` / `Ctrl+Insert` and `Session > Copy` (`Copy selected text`) to copy an active prompt selection when no conversation lines are selected, and added `Ctrl+X` / `Session > Cut selection` to cut onto X11 `CLIPBOARD` without losing text if over the single-transfer limit — while keeping `Ctrl+A` bound exclusively to `Accounts`.
   - Added word-coalesced, bounded per-session undo/redo (`Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z`; `<= 32` snapshots/session and `<= 128 KiB` across all sessions), preserved across session switches (`park`/`resume`) and panel opens (`take`/`restore`), and purged on `forget(session)`.
   - Handled `WindowEvent::Resized` in `ui.rs` to invalidate stale composed frames, clamp scrollback, reflow wrapped lines immediately, and preserve the viewed scroll anchor.
3. **S04 — Automated verification and manual inspection bundle:**
   - Added unit tests in `src/host/hostd/src/gui/editor/tests.rs` and Xvfb integration tests in `tests/test_editor_selection.py`, and captured bundle `00009_M03-P03_editor_and_reflow/`.

## Verification results

| Check / Metric | Result | Guard |
|---|---|---|
| `cargo test --bin lico gui::` (including `scroll_oracle` at scales `1`, `1.25`, `2`) | `173/173` passed | Zero failures, zero `gui` warnings |
| `pytest tests/test_editor_selection.py tests/test_input_box.py tests/test_menu_bar.py tests/test_selection.py tests/test_size_ratchet.py tests/test_no_hidden_characters.py` | `41/41` passed | Zero failures |
| `src/host/hostd/src/gui/ui.rs` / `editor/*.rs` line counts | `1,115` / `<= 703` lines | `<= 1,120` allowlist / `<= 800` ratchet |
