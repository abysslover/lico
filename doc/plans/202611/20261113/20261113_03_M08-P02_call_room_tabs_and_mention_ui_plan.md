# M08-P02 Plan — Left Sidebar `[ Calls ]` Tab, Top-Bar `⇄ A·B` Call Room Tabs, `@mention` Autocomplete Popup, & Clickable Jump Cards

- Date: 2026-11-13
- Milestone: `M08`
- Phase: `M08-P02`
- Branch: `phase/M08-P02-call-room-tabs-and-mention-ui`
- Status: in_progress

## 1. Objective

Implement the complete GUI administration surface for shared Call Rooms and `@AgentName` mentions while keeping every Rust source file strictly under its line-count ratchet (`<= 800` default; `gui/ui.rs <= 1,120`) and preserving `< 50.0 MiB` idle GUI RSS:

1. **Left Sidebar 3-Tab Switcher (`[ Directories ]` | `[ Past ]` | `[ Calls ]`) (`src/host/hostd/src/gui/sidebar_calls.rs`)**:
   - Extend the left sidebar tab bar from 2 tabs (`0 = Directories`, `1 = Past`) to 3 tabs (`0 = Dir/Directories`, `1 = Past`, `2 = Calls` with live active/total badge e.g. `Calls (1)`).
   - Sync Call Room summaries directly from `status.calls` (`ui_status`) with zero extra background polling.
   - Render Call Room cards showing `⇄ <Label A> · <Label B>`, live status pill (`[active R/4]`, `[paused]`, `[idle]`), participant providers, entry count, last speaker preview, a `[ + Start call ]` quick-connect strip for the active session + open peers, and a `×` close button (`call.close`). Clicking a Call Room row opens and focuses its top-bar tab (`⇄ A·B`).

2. **Top-Bar Call Room Tabs (`⇄ A·B`) & Right-Pane Call Room View (`src/host/hostd/src/gui/call_view.rs`)**:
   - Support open Call Room tabs alongside session tabs in `tabs.rs` (`open_calls: Vec<String>`, `active_call: Option<String>`), rendered as `⇄ <Label A>·<Label B>` (with `●` when active) and `×` tab close.
   - When `active_call` is selected, fetch full room detail asynchronously via `{"op":"call.get","call":"..."}` on open and whenever `status.calls` `updated_at` / `entry_count` / `status` changes.
   - Populate `self.view.lines` with formatted Call Room transcript lines (`[call turn]` / `[peer reply]` / `$` / speaker headers) so the existing `cosmic-text` layout, scrollback, mouse drag-selection auto-copy to `CLIPBOARD`, `Ctrl+C`, toolbar `Copy`, and `Ctrl+F` find work identically to session transcripts.
   - Draw a crisp Call Room control banner with clickable **`[ Hang up ]`** (`call.hangup`, also bound to `Ctrl+Shift+H` / button click when active) and participant jump buttons (`[ -> <Label A> ]`, `[ -> <Label B> ]`).
   - Route `Enter` in the bottom prompt editor of an active Call Room tab to `{"op":"call.say","call":"<cid>","text":"..."}` so the human administrator can speak directly into the Call Room (`@Agent`, `@all`, or plain message) at any time.

3. **`@AgentName` Autocomplete Popup (`src/host/hostd/src/gui/mention_popup.rs`)**:
   - Detect an active `@<prefix>` token at the editor cursor in any session tab or Call Room tab.
   - Render a compact floating popup above the prompt editor listing matching running/open peer sessions (`@Pri-Avatar  codex · idle`, plus `@all` in a Call Room), supporting `ArrowUp` / `ArrowDown` navigation, `Tab` (or mouse click) insertion (`@Name ` or `@"Name With Spaces" `), and `Esc` dismissal.

4. **Clickable `[⇄ call ...]` & `[peer reply ... | call cXXXXXX ...]` Jump Cards (`clip.rs` / `call_view.rs`)**:
   - Clicking (without dragging) a Call Room summary card (`>> [⇄ call ... (cXXXXXX) ...]`) or a peer reply header (`>> [peer reply ... | call cXXXXXX ...]`) in any session transcript opens and focuses that Call Room tab in 1 click.

5. **Deterministic UI Telemetry (`ui_dump`)**:
   - Expose `"calls"` (active call id, open call tabs, hangup/participant rects, entries, sidebar `calls_tab_rect` & `call_entries`) and `"mention_popup"` in `ui_dump` for automated Xvfb verification.
