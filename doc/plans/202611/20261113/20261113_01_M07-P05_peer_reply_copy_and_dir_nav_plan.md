# M07-P05 Plan — Approval Banner Layout, Approved Peer-Turn Reply Routing, Mouse Auto-Copy Across All Right-Pane Screens, and Persistent Directory History & Bookmarks Toolbox

- Milestone: `M07` (`lico 0.2.3`)
- Branch: `phase/M07-P05-peer-reply-copy-and-dir-nav`
- Gate: G2 approved 2026-11-13 (live owner G3 inspection feedback on `M07-P04`)

## Goal

Complete the five final usability and inter-session workflow items requested during live inspection of `M07-P04` while keeping `lico` single-executable and `< 50.0 MiB` idle GUI RSS:

1. **Styled `Allow` / `Deny` approval card banner and button alignment**:
   - Wrap pending approval prompt text cleanly inside the width left of the `Allow` and `Deny` buttons without appending trailing NBSP padding that wraps onto a dangling blank line.
   - Paint a dedicated approval card banner (`0xFF28_2216` fill, subtle border, amber accent rail) behind the prompt and render `Allow` and `Deny` on top of the banner after text blitting so buttons never float over unrelated lines or collide with text.
2. **Approved inter-session (`mcp__lico__send`) reply routing and clean default chat view**:
   - Stamp host-reserved `[peer turn started]` and `[host notice]` markers in `.seg` history (defended by `leads::NOTES` against agent forgery) so every turn and notice remains durably recorded and inspectable via `View > Internal events` (`Ctrl+Shift+E`), while hiding unsolicited agent-initiated peer turns in the receiver's default chat view (`!show_events`) and hiding raw host transport echo headers in the caller's default chat view.
   - Automatically route the receiver's assistant reply from an approved `By::Session` peer turn back to the caller session (`[peer reply from '<label>' (<id7>) | HH:MM | <size>]`) as a host notice (`Origin::Host`) without asking for a second approval card, and auto-authorize an explicit `mcp__lico__send` back to that caller during the approved peer turn while suppressing duplicate auto-reply at turn completion.
3. **Mouse drag-selection auto-copy to X11 `CLIPBOARD`**:
   - Automatically copy selected UTF-8 text to X11 `CLIPBOARD` on left mouse button release whenever the user finishes dragging a selection in the conversation transcript, any right-pane screen, or the prompt input box (`PRIMARY` remains untouched; plain single clicks without a drag do not overwrite `CLIPBOARD`; `Ctrl+C`, `Ctrl+Shift+C`, `Ctrl+Insert`, and toolbar `Copy` remain active).
4. **Universal text selection and copy across every screen in the right-hand chat area**:
   - Read active `panel_view` lines in `selection_text()` whenever a right-pane screen is open (`Send to session`, `Files` and sidebar inline file preview, `Accounts`, `Actions`, `Login`, `New session`, `Card`, `Rebind`, `Confirm`, `Keys`, `About`, `Start`), clear stale selections on `panel_open` / `panel_close`, keep panels open when clicking toolbar `Copy`, expose `Copy` on `Files` previews when text is selected, and separate `< 4 px` single-click row activation from `>= 4 px` mouse drag-selection on clickable panel rows.
5. **Persistent `[ Directories ]` navigation history (`<` / `>`) and Bookmarks toolbox**:
   - Add a top toolbox bar inside `[ Directories ]` with ordered **Back (`<`)** and **Forward (`>`)** navigation (`Alt+Left` / `Alt+Right` and mouse Back/Forward buttons) across visited `(ssh_host, cwd, global, page)` locations plus **Bookmark toggle (`☆` / `★`)** and a **`Marks (N) v`** quick-jump popup with per-entry jump and `×` removal.
   - Persist `dir_history`, `dir_history_idx`, and `dir_bookmarks` in `settings.json` (`settings.set` / `status`) so ordered directory and page history plus bookmarks survive both `lico gui` and `lico host` restarts.

## Acceptance Criteria

1. Pending approval prompts (both short single-line and long multi-line wrapped prompts) render inside a bordered approval banner with `Allow` and `Deny` vertically centered on the right and zero text overlap or dangling blank wrapped lines.
2. When `Claude` sends an approved `mcp__lico__send` message to `Codex` (or vice versa), `Codex`'s default chat view (`!show_events`) stays clean without displaying the unsolicited peer turn (`Ctrl+Shift+E` reveals the full `.seg` record), and `Codex`'s assistant reply is automatically delivered back to `Claude` without a second manual approval prompt.
3. Selecting text with a mouse drag in the transcript, prompt editor, or any right-hand panel (`Send to session`, `Files` inline preview, `Accounts`, etc.) highlights the lines and copies the exact UTF-8 selection to `CLIPBOARD` upon mouse button release (`PRIMARY == 0`), while `< 4 px` single clicks on interactive panel rows still toggle/activate normally and toolbar `Copy` / `Ctrl+C` copy without closing the open panel.
4. Navigating across folders and pagination pages in `[ Directories ]` records ordered `(ssh_host, cwd, global, page)` history; clicking `<` / `>` (or `Alt+Left` / `Alt+Right`) steps backward and forward through visited folders and page indices; toggling `☆`/`★` and picking from `Marks (N)` jumps directly to saved folder/page entries; and both history and bookmarks persist across `lico gui` and `lico host` restarts.
5. Cold test suite (`<= 180.0 s`), `idle_memory` (`<= 50.0 MiB` at scale 1 and scale 2), `hidden_growth` (`<= +10.0 MiB`), file size ratchets, protocol conformance, and two-clone `suite acceptance` all pass, and the standalone `lico 0.2.3` release binary is updated.
