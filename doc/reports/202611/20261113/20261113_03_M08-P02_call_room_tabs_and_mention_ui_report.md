# M08-P02 Report — Left Sidebar `[ Calls ]` Tab, Top-Bar `⇄ A·B` Call Room Tabs, `@mention` Autocomplete Popup, & Clickable Jump Cards

- Date: 2026-11-13
- Milestone: `M08`
- Phase: `M08-P02`
- Branch: `phase/M08-P02-call-room-tabs-and-mention-ui`
- Status: accepted at G3 and merged to `main` (2026-11-13)

## 1. Summary of Changes

1. **Left Sidebar `[ Calls ]` Tab (`src/host/hostd/src/gui/sidebar_calls.rs`, `sidebar.rs`, `sidebar_render.rs`)**:
   - Extended the left sidebar header to 3 tabs (`[ Directories ]` | `[ Past ]` | `[ Calls ]`, showing `Calls (N)` when Call Rooms exist and highlighting active calls in teal).
   - Added `CallsSidebarState` synchronized directly from `status.calls` (`ui_status`), quick-connect `+ ⇄ <Peer> (<provider>)` buttons for starting a Call Room from the active session, instant Call Room filtering, paginated Call Room cards (`⇄ <Label A> · <Label B>`, `[R/4]` / `[paused]` / `[idle]` status badge, entry count & last speaker preview), 1-click Call Room tab opening, and `×` close (`call.close`).

2. **Top-Bar Call Room Tabs (`⇄ A·B`) & Right-Pane Call Room View (`src/host/hostd/src/gui/call_view.rs`, `tabs.rs`, `ui.rs`, `editor/app.rs`)**:
   - Added open Call Room tabs (`open_tabs`, `active_call`) rendered alongside session tabs in the top tab bar (`⇄ <Label A>·<Label B>` with teal active accent and `×` close tab).
   - Added right-pane Call Room transcript rendering through `self.view.lines` (`>> [Owner | HH:MM]`, `>> [from session ...]`, `>> [peer reply from ...]`, `[tool] [Lico | HH:MM]`), giving Call Room views full `cosmic-text` shaping, bordered speaker cards, scrollback, mouse drag-selection auto-copy to `CLIPBOARD`, `Ctrl+C`, and toolbar `Copy`.
   - Added header action controls (`-> <Label A>`, `-> <Label B>`, and **`Hang up`** / `Ctrl+Shift+K` or `Ctrl+K` on empty input) and wired `Enter` in the Call Room prompt editor to `{"op":"call.say","call":"...","text":"..."}` with full per-room draft persistence (`ed.park` / `ed.resume`).

3. **`@AgentName` Autocomplete Popup & Clickable Transcript Jump Cards (`src/host/hostd/src/gui/mention_popup.rs`, `clip.rs`, `editor/mod.rs`)**:
   - Added `mention_popup.rs` offering matching peer sessions (`@Pri-Avatar`, `@"Quoted Name"`, `@all`) when typing `@<prefix>` in any session tab or Call Room tab, with `ArrowUp` / `ArrowDown` navigation, `Tab` or mouse-click insertion (`replace_span`), and `Esc` dismissal.
   - Wired stationary left-clicks on `[⇄ call ...]` summary cards and `[peer reply ... | call cXXXXXX ...]` cards in session transcripts (`clip.rs:main_mouse_release` -> `try_jump_call_from_line`) to open and focus the corresponding Call Room tab immediately while keeping mouse drag-selection auto-copy intact.
   - Exposed full `"calls"` and `"mention_popup"` telemetry in `ui_dump`.

## 2. Verification

- `cargo test --manifest-path src/host/hostd/Cargo.toml`: `604 passed; 0 failed` (including `mention_popup::tests`).
- `pytest`:
  - `tests/test_size_ratchet.py`, `tests/test_no_hidden_characters.py`, `tests/test_selection.py`, `tests/test_left_directory_sidebar.py`, `tests/test_session_tabs_and_telemetry.py`, `tests/test_peer_reply_copy_and_dir_nav.py`: all green.
