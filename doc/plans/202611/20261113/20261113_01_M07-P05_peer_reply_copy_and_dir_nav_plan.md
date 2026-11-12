# M07-P05 Plan — Centered Approval Modal Notification, Session-Persistent `Allow` & Visible Peer-Conversation Cards, Mouse Auto-Copy Across All Right-Pane Screens, and Persistent Directory History & Bookmarks Toolbox

- Milestone: `M07` (`lico 0.2.3`)
- Branch: `phase/M07-P05-peer-reply-copy-and-dir-nav`
- Gate: G2 approved 2026-11-13 (live owner G3 inspection feedback on `M07-P04` and `M07-P05`)

## Goal

Complete the final usability and inter-session workflow items requested during live inspection of `M07-P04` and `M07-P05` while keeping `lico` single-executable and `< 50.0 MiB` idle GUI RSS:

1. **Centered window-level `Allow` / `Deny` modal notification dialog and session-persistent `Allow` permission**:
   - Render pending approvals as an unmistakable centered window-level modal notification dialog over a dimmed window backdrop (`Approval Required`, session/provider header, wrapped prompt, persistence note, `Read Full (V)`, `Allow`, and `Deny` buttons), also surfacing when an open background tab requests approval in `lico gui` two-pane mode.
   - Persist `Allow` (`Allow` click, `Y`/`y`, `Shift+S`, or `Session > Allow approval`) for the remainder of the session on session-keepable tools (`send` and `history`) while keeping one-shot tools (`create`, `computer`, `capture`, `clipboard_read`, `clipboard_write`) single-shot (`keep = false`).
2. **Event-driven visible peer conversation (`1` turn allowed) in caller's chat window and clean default receiver chat view**:
   - Stamp host-reserved `[peer turn started]`, `[peer reply]`, and `[host notice]` markers in `.seg` history (defended by `leads::NOTES` against agent forgery) so every turn and notice remains durably recorded and inspectable via `View > Internal events` (`Ctrl+Shift+E`), while hiding unsolicited agent-initiated peer turns in the receiver's default chat view (`!show_events`) and hiding raw `[host notice]` transport headers in the caller's default chat view.
   - Display outgoing `> [to <id7>] <preview>` inside `[tool] mcp__lico__send` cards and route the receiver's reply (`>> [peer reply from '<label>' (<id7>) | HH:MM | <size>]` + body) visibly into a bordered teal peer-message card in the caller's chat window (`Origin::PeerReply` / `Origin::PeerReplySession`), running `1` event-driven turn on the caller without a second modal or infinite ping-pong loop.
3. **Mouse drag-selection auto-copy to X11 `CLIPBOARD`**:
   - Automatically copy selected UTF-8 text to X11 `CLIPBOARD` on left mouse button release whenever the user finishes dragging a selection in the conversation transcript, any right-pane screen, or the prompt input box (`PRIMARY` remains untouched; plain single clicks without a drag do not overwrite `CLIPBOARD`; `Ctrl+C`, `Ctrl+Shift+C`, `Ctrl+Insert`, and toolbar `Copy` remain active).
4. **Universal text selection and copy across every screen in the right-hand chat area**:
   - Read active `panel_view` lines in `selection_text()` whenever a right-pane screen is open (`Send to session`, `Files` and sidebar inline file preview, `Accounts`, `Actions`, `Login`, `New session`, `Card`, `Rebind`, `Confirm`, `Keys`, `About`, `Start`), clear stale selections on `panel_open` / `panel_close`, keep panels open when clicking toolbar `Copy`, expose `Copy` on `Files` previews when text is selected, and separate `< 4 px` single-click row activation from `>= 4 px` mouse drag-selection on clickable panel rows.
5. **Persistent `[ Directories ]` navigation history (`<` / `>`) and Bookmarks toolbox**:
   - Add a top toolbox bar inside `[ Directories ]` with ordered **Back (`<`)** and **Forward (`>`)** navigation (`Alt+Left` / `Alt+Right` and mouse Back/Forward buttons) across visited `(ssh_host, cwd, global, page)` locations plus **Bookmark toggle (`☆` / `★`)** and a **`Marks (N) v`** quick-jump popup with per-entry jump and `×` removal.
   - Persist `dir_history`, `dir_history_idx`, and `dir_bookmarks` in `settings.json` (`settings.set` / `status`) so ordered directory and page history plus bookmarks survive both `lico gui` and `lico host` restarts.

## Acceptance Criteria

1. Pending approval prompts (both short single-line and long multi-line wrapped prompts, on the active tab or an open background tab) render inside a centered window-level modal notification dialog (`Approval Required`) over a dimmed window backdrop with prominent `Allow`, `Deny`, and `Read Full (V)` buttons, and clicking `Allow` (or pressing `Y`) keeps `send` and `history` permissions active for the rest of the session.
2. When `Claude` sends an approved `mcp__lico__send` message to `Codex` (or vice versa), `Codex`'s default chat view (`!show_events`) stays clean without displaying the unsolicited peer turn (`Ctrl+Shift+E` reveals the full `.seg` record), while `Claude`'s chat view visibly displays outgoing `> [to <id7>] <preview>` inside `[tool] mcp__lico__send` and `Codex`'s incoming reply inside a bordered teal peer-message card (`>> [peer reply from ...]`), driving `1` event-driven turn on `Claude` without a second modal or infinite loop.
3. Selecting text with a mouse drag in the transcript, prompt editor, or any right-hand panel (`Send to session`, `Files` inline preview, `Accounts`, etc.) highlights the lines and copies the exact UTF-8 selection to `CLIPBOARD` upon mouse button release (`PRIMARY == 0`), while `< 4 px` single clicks on interactive panel rows still toggle/activate normally and toolbar `Copy` / `Ctrl+C` copy without closing the open panel.
4. Navigating across folders and pagination pages in `[ Directories ]` records ordered `(ssh_host, cwd, global, page)` history; clicking `<` / `>` (or `Alt+Left` / `Alt+Right`) steps backward and forward through visited folders and page indices; toggling `☆`/`★` and picking from `Marks (N)` jumps directly to saved folder/page entries; and both history and bookmarks persist across `lico gui` and `lico host` restarts.
5. Cold test suite (`<= 180.0 s`), `idle_memory` (`<= 50.0 MiB` at scale 1 and scale 2), `hidden_growth` (`<= +10.0 MiB`), file size ratchets, protocol conformance, and two-clone `suite acceptance` all pass, and the standalone `lico 0.2.3` release binary is updated.
