# M07-P05 Report — Centered modal approval window, session-persistent `Allow`, visible peer-conversation cards (`1`-turn event-driven), mouse drag-selection auto-copy across all chat panels, and persistent `[ Directories ]` navigation history & bookmarks

- Status: pending G3 inspection
- Version: 1.1.0
- Date: 2026-11-13
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Plan: [M07-P05 plan](../../../plans/202611/20261113/20261113_01_M07-P05_peer_reply_copy_and_dir_nav_plan.md) (G1 and G2 approved by the owner on 2026-11-13)
- Branch: `phase/M07-P05-peer-reply-copy-and-dir-nav`
- Inspection bundles: `202611/20261113/00001_M07-P05_peer_reply_copy_and_dir_nav/` and `202611/20261113/00002_M07-P05_modal_approval_and_visible_peer_reply/`

## Summary

`M07-P05` resolves all live owner inspection findings from `M07-P04` and
`M07-P05` to complete Milestone `M07` (`lico 0.2.3`) while keeping `lico` a
single self-contained executable well inside its `< 50.0 MiB` idle GUI RSS
budget:

1. Centered window-level `Approval Required` modal notification dialog over a
   dimmed workspace backdrop (`Allow`, `Deny`, and `Read Full (V)` buttons,
   session/provider badge, and background-tab alert surfacing in `lico gui`)
   paired with session-persistent `Allow` permission (`keep = true` on
   keepable tools `send` and `history`, single-shot `keep = false` on
   `create`, `computer`, `capture`, `clipboard_read`, `clipboard_write`).
2. Event-driven visible peer conversation (`1` round-trip turn allowed, zero
   infinite loop) in the caller's chat window — rendering outgoing
   `> [to <id7>] <preview>` inside `[tool] mcp__lico__send` cards and incoming
   `>> [peer reply from '<label>' (<id7>) | HH:MM | <size>]` + reply body inside
   a unified teal peer-message card (`Origin::PeerReply` /
   `Origin::PeerReplySession`) while hiding internal `[host notice]` echoes on
   the caller and hiding unsolicited `[peer turn started] ... [turn completed]`
   turns on the receiver (`Ctrl+Shift+E` reveals full `.seg` history).
3. Automatic X11 `CLIPBOARD` copy on mouse left-button drag-selection release in
   the conversation transcript, prompt editor, and right-pane panels (leaving
   `PRIMARY` untouched and ignoring `< 4*s` px stationary clicks).
4. Universal mouse drag-selection highlight (`0xFF2A_4A7A`) and copy (`Ctrl+C`,
   `Ctrl+Shift+C`, toolbar `Copy`, menu `Copy`, and mouse-release auto-copy)
   across every right-hand chat area screen rendered via `panel_view()`
   (`Send to session`, `Files` and inline sidebar preview, `Accounts`,
   `Actions`, `Login`, `New session`, `Card`, `Rebind`, `Confirm`, `Keys`,
   `About`, `Start`), keeping open panels open on `Copy` and preserving
   stationary single-click row activation.
5. Persistent `[ Directories ]` navigation history (`[ < ]` Back / `[ > ]`
   Forward across both directories and pagination pages, `Alt+Left` /
   `Alt+Right`, `MouseButton::Back` / `MouseButton::Forward`) and bookmarks
   toolbox (`[ ☆/★ ]` toggle + `[ Marks (N) v ]` quick-jump and `×` removal
   popup), persisted in `settings.json` across `lico gui` and `lico host`
   restarts.

## Delivered changes

1. **Centered `Allow` / `Deny` modal notification window & session-persistent `Allow` (`src/host/hostd/src/gui/{approval_modal.rs,cardview.rs,controls.rs,menu.rs,mod.rs,row.rs,ui.rs}`)**:
   - Replaced inline top-strip approval banners with `approval_modal::paint_approval_modal`, painting a dimmed full-window backdrop (`alpha = 156/255`) and an integer-aligned centered modal notification window (`Approval Required · <session> (<provider>)`, wrapped prompt lines, persistence note, `Read Full (V)`, `Allow`, and `Deny` buttons), also surfacing pending approvals from open background tabs when `tabs_shown()` is active in `lico gui`.
   - Added `row.screen_tools` (`Asks::ScreenTool`) so single-shot screen/clipboard tools stay `no_keep() == true`, and updated `answer_approval(true)` / `card_key('y' | 'Y')` to send `keep = !asks.no_keep()`, keeping `send` and `history` approvals active for the rest of the session after a single `Allow`.
2. **Event-driven visible peer-conversation cards & clean receiver chat views (`src/host/hostd/src/{leads.rs,session.rs,transfer.rs,calls.rs,claude.rs,codex.rs,mcp.rs}`, `src/host/hostd/src/gui/{sendto.rs,transcript.rs}`, `src/lico_protocol/{claude_mapping.py,codex_mapping.py}`)**:
   - Added `Origin::PeerReply` and `Origin::PeerReplySession(u32)` stamping `[peer reply]` (protected by `leads::NOTES`), `peer_reply_authorized` (`s.busy && s.peer_turn.is_answering(to) && pending::kept(shared, to, "send", caller)`), and single-round-trip termination (`is_reply` skips re-arming `peer_turn` on the caller so the exchange completes after `1` event-driven turn on the caller with zero ping-pong loop).
   - Emitted `> [to <id7>] <first_line>` inside `[tool] mcp__lico__send` cards in `claude` and `codex`, matched `[peer reply from 'NAME' (ID) | HH:MM | SIZE]` in `sendto::header_look`, and grouped peer headers plus their contiguous `>> ` echo body lines into a unified `ROLE_PEER_BG` teal bordered card in `transcript.rs`, while keeping `[host notice]` and unsolicited `[peer turn started] ... [turn completed]` turns hidden under `!show_events` (`Ctrl+Shift+E` reveals all `.seg` lines).
3. **Mouse drag-selection auto-copy & universal right-pane panel selection (`src/host/hostd/src/gui/{mod.rs,ui.rs,clip.rs,panel.rs,toolbar.rs,sendto.rs,memrow.rs}`, `src/host/hostd/src/gui/editor/app.rs`)**:
   - Added `panel_drag`, `panel_sel`, `panel_view_lines`, `panel_body_rect`, `panel_scroll_px`, and `tx_drag_moved` tracking on `UiState`, plus `editor_drag_origin` on `App`.
   - Wired mouse-drag line selection (`0xFF2A_4A7A` highlight painted behind `panel_view` rows) across all non-transcript right-pane views; stationary clicks (`< 4*s` px) clear `panel_sel` and trigger clickable row actions (`[1]`, `[L]`, `[C]`, directory/file rows, account actions), whereas dragging `>= 4*s` px selects rows without accidentally firing `hit_row` on mouse release.
   - Auto-copied non-empty selections to X11 `CLIPBOARD` (`Selection::Clipboard` only, keeping `PRIMARY` untouched) on left-button drag release across transcript lines, prompt editor text, and `panel_view` rows, and extended `copy_selection`, `Ctrl+C`, `Ctrl+Shift+C`, toolbar `Copy`, and menu `Copy` to copy `panel_sel` without dismissing open panels.
4. **Persistent `[ Directories ]` navigation history & bookmarks toolbox (`src/host/hostd/src/{host.rs,ipc.rs}`, `src/host/hostd/src/clipboard/settings.rs`, `src/host/hostd/src/gui/{sidebar.rs,sidebar_render.rs,sidebar_nav.rs,ui.rs}`, `src/lico_protocol/v1.py`)**:
   - Persisted `dir_history`, `dir_history_idx`, and `dir_bookmarks` in `settings.json` (`settings.set` / `status`) across `lico gui` and `lico host` restarts (updating `host.rs` `arg()` to `.rfind(...)` so later `--state-dir` CLI overrides take precedence).
   - Added a compact navigation and bookmark toolbox row at the top of `[ Directories ]` (`[ < ]`, `[ > ]`, `idx/len`, `[ ☆/★ ]`, `[ Marks (N) v ]`), tracking both directory transitions (`enter_dir`, `[..]` parent, root entry, `cwd` sync) and `[ < ] Page X/Y [ > ]` pagination changes, supporting `Alt+Left` / `Alt+Right` and `MouseButton::Back` / `MouseButton::Forward`, and rendering an interactive bookmarks quick-jump + `[×]` removal popup.

## Measured verification (`lico 0.2.3`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `599 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `486` tests in `94` files, `0` failed, **`104.4 s`**, 4-worker harness tree peak **`1,697.3 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 25.00 MiB` (`24.65..25.00 MiB`), median PSS `14.54 MiB`, anon `3.94 MiB`, host `7.64 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.89 MiB` (`35.84..35.89 MiB`), median PSS `20.17 MiB`, anon `3.94 MiB`, host `7.57 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.26 MiB` (`24.76 MiB` -> `24.92 MiB`; host `8.81` -> `20.57 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.3` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`ui.rs` / `transfer.rs` / `session.rs` / `mod.rs` / `codex.rs` / `calls.rs` / `transcript.rs` / `claude.rs` / `cardview.rs` / `approval_modal.rs` / `controls.rs`) | `1,071` / `795` / `792` / `776` / `751` / `642` / `624` / `597` / `537` / `199` / `191` lines | `<= 1,120` / `<= 800` | Met |

## Gate status

All five phases of Milestone `M07` (`M07-P01` through `M07-P05`, `lico 0.2.3`)
are implemented, measured, and installed to `~/bin/lico` for live testing,
awaiting owner G3 inspection acceptance and G4 merge/publication approval.
