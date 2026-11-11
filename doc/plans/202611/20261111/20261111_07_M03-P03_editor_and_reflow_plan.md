# M03-P03 Plan: Prompt editor modularization, mouse/keyboard selection, undo/redo, and resize reflow

- Status: done
- Version: 1.0.0
- Date: 2026-11-11
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Phase: `M03-P03` (`phase/M03-P03-editor-and-reflow`)
- Approval: G1 and G2 Option A approved by the owner on 2026-11-11.

## Why

`src/host/hostd/src/gui/editor.rs` sits at `798` lines (two lines below the `800`-line size ratchet) and lacks everyday desktop editing ergonomics in the multi-line session prompt box:
1. **Mouse & keyboard selection**: Administrators need click/drag caret placement and character selection inside the prompt box (`ed.rect`), `Shift` + movement selection (`Shift+Left/Right/Up/Down/Home/End`, `Ctrl+Shift+Left/Right`), and `Ctrl+C` / `Ctrl+X` to copy or cut an active prompt selection when no conversation lines are selected — while preserving `Ctrl+A` exclusively for `Accounts`.
2. **Bounded per-session undo/redo**: Multi-line prompt drafting needs `Ctrl+Z` (undo) and `Ctrl+Y` / `Ctrl+Shift+Z` (redo), bounded in count and bytes per session and preserved across session switches and panel opens.
3. **Window resize reflow & scroll anchoring**: Resizing `lico gui` must immediately invalidate stale composed frame geometry, reflow wrapped transcript and prompt rows to the new width, and keep the viewed transcript position anchored.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M03-P03` plan and link it in the `M03` milestone and `doc/index_4.md` | Guard checks pass |
| S02 | Split `src/host/hostd/src/gui/editor.rs` and `editor_tests.rs` into `src/host/hostd/src/gui/editor/{mod,selection,app,tests}.rs` (`<= 550` lines each), preserving every existing grapheme, paste, recall, picture-chip, and panel-stash invariant | `cargo test --bin lico` and `tests/test_size_ratchet.py` pass |
| S03 | Implement prompt mouse click/drag selection, `Shift` + movement selection, `Ctrl+C` / `Ctrl+X` prompt copy/cut (`Ctrl+A` kept for `Accounts`), bounded per-session undo/redo (`<= 32` entries/session, `<= 128 KiB` total), selection highlight painting, and `WindowEvent::Resized` reflow + scroll-anchor preservation (`ui.rs <= 1,168` lines) | Rust unit tests (`editor::tests`, `scroll_oracle`), `tests/test_input_box.py`, `tests/test_menu_bar.py`, and `tests/test_editor_selection.py` pass |
| S04 | Generate manual inspection bundle `00009_M03-P03_editor_and_reflow/`, write the `M03-P03` report, update status tables, and commit | Pre-commit guards and changed suite pass |

## Personal data and security

- While a sign-in code or panel owns the input line (`panel.is_some()`), prompt mouse selection, `Ctrl+X`, and `editor` dump fields (`text`, `selection`) remain suppressed (`null`), never exposing sign-in codes in logs, dumps, or undo history.
- Undo/redo stacks live strictly in memory per session (`<= 32` entries, `<= 128 KiB` across all sessions) and are purged immediately when `forget(session)` runs on session deletion.
