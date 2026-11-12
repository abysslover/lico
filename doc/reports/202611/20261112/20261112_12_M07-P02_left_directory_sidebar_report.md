# M07-P02 Report — Two-column workbench left `[ Directories | Past ]` sidebar with highlighted active `cwd`, filter, pagination, and inline preview

- Status: done (owner G3 accepted and G4 merged on 2026-11-13)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Plan: [M07-P02 plan](../../../plans/202611/20261112/20261112_12_M07-P02_left_directory_sidebar_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M07-P02-left-directory-sidebar`
- Inspection bundle: `202611/20261112/00015_M07-P02_left_directory_sidebar/`

## Summary

`M07-P02` reorganizes `lico gui` into a crisp two-pane tabbed workbench:
Column 1 (`LIST_W = 330` logical px, `Ctrl+B` collapsible) provides two default
tabs (`[ Directories ]` and `[ Past ]`) aligned seamlessly with the multi-session
tab bar atop Column 2. `[ Directories ]` integrates local and SSH remote
(`Host: local v` / `Host: <alias> v`) directory navigation, gold `★` visual
highlighting of the active session's working directory (`-> active` jump-back
pill when browsing subfolders) and teal `★ Global (general)` highlighting for
Global sessions, non-blocking async directory synchronization on session tab
switches, instant in-memory filename filtering, `.dot` hidden-file toggling,
mouse-wheel and `< Page X/Y (N) >` pagination, and unified file activation
(wide inline UTF-8 preview in Column 2 with `Esc` returning directly to chat, or
system default opener for external files).

## Delivered changes

1. **S02 — Two-tab left workspace sidebar & pixel-exact rendering (`src/host/hostd/src/gui/{sidebar.rs,sidebar_render.rs,mod.rs,controls.rs,panel.rs,files_ui.rs,files_host.rs,memrow.rs,menu.rs,telemetry.rs,ui.rs}`)**:
   - Added `gui/sidebar.rs` (`744` lines) and `gui/sidebar_render.rs` (`561` lines) implementing the `Ctrl+B` collapsible left column (`LIST_W = 330` px) with two default tabs (`[ Directories ]` and `[ Past ]`), `Host: local v` / `ssh: <alias>` combo-box selector, `.dot` hidden-files toggle, `R` explicit refresh (zero background timers), `+` new session in the viewed folder, instant in-memory `Filter files...` input, gold `★` active `cwd` highlight + `-> active` jump-back pill, teal `★ Global (general)` highlight, async `SidebarDir` auto-sync on top-tab switch, `< Page X/Y (N) >` + mouse-wheel pagination, and unified file click (`Esc` from inline UTF-8 preview returns directly to chat).
   - Preserved `scroll::Shift` pixel identity across DPI scales (`1.0`, `1.25`, `2.0`) by repainting Column 1 cleanly on each frame and keeping top-tab hover memory hints (`memrow.rs`) + bottom memory summary intact (`ui.rs` at `1,092` lines, well within `<= 1,120`).
2. **S03 — Integration tests, top-tab window helpers, and visual inspection bundle (`tests/test_left_directory_sidebar.py`, `tests/testmap.toml`, updated `ProductWindow` tests)**:
   - Updated `ProductWindow` tests to select open sessions via the top session tab bar (`tabs.items`) and added `tests/test_left_directory_sidebar.py` covering default 2-tab layout, active `cwd` & `Global` highlighting, async session-switch sync, subfolder `-> active` jump-back, `.dot` toggle, explicit refresh, pagination, filename filter, wide inline UTF-8 preview (`Esc` straight back to chat), external `.html` opener, `[ Past ]` tab switching, and `Ctrl+B` collapse/expand.

## Measured verification

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Changed-area test suite (`suite run --changed main`) | `445` tests in `82` files + all `cargo` module unit tests, `0` failed | `0` failures | Met |
| Scroll & pixel layout oracle (`gui::scroll_oracle`, scales `1.0`, `1.25`, `2.0`) | `179` GUI unit tests passed across all three DPI scales | Exact pixel match | Met |
| Module size ratchets (`ui.rs` / `sidebar.rs` / `sidebar_render.rs` / `panel.rs`) | `1,092` / `744` / `561` / `794` lines | `<= 1,120` / `<= 800` | Met |

## Gate status

Ready on `phase/M07-P02-left-directory-sidebar`; proceeding directly to `M07-P03`
under the owner's continuous G2 approval, with G3 inspection and G4 merge held
until `M07-P03` completes.
