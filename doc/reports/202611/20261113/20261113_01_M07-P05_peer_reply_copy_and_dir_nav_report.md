# M07-P05 Report — Styled approval banners, automatic approved peer-reply routing, mouse drag-selection auto-copy across all chat panels, and persistent `[ Directories ]` navigation history & bookmarks

- Status: pending G3 inspection
- Version: 1.0.0
- Date: 2026-11-13
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Plan: [M07-P05 plan](../../../plans/202611/20261113/20261113_01_M07-P05_peer_reply_copy_and_dir_nav_plan.md) (G1 and G2 approved by the owner on 2026-11-13)
- Branch: `phase/M07-P05-peer-reply-copy-and-dir-nav`
- Inspection bundle: `202611/20261113/00001_M07-P05_peer_reply_copy_and_dir_nav/`

## Summary

`M07-P05` resolves all five live owner inspection findings from `M07-P04` to
complete Milestone `M07` (`lico 0.2.3`) while keeping `lico` a single
self-contained executable well inside its `< 50.0 MiB` idle GUI RSS budget:

1. Styled amber-accented `Allow` / `Deny` approval banner with width-bounded
   prompt wrapping left of the buttons and no dangling NBSP line or button
   overlap.
2. Clean default chat views on approved `mcp__lico__send` peer turns (hiding
   unsolicited `[peer turn started] ... [turn completed]` chatter, multi-line
   `>> ` peer echo bodies, and raw `[host notice]` echo headers unless
   `Ctrl+Shift+E` `Internal events` is enabled, while preserving full `.seg`
   durability) plus automatic routing of the receiver's assistant reply back to
   the caller session (`[peer reply from ...]`) without a second manual approval
   card.
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

1. **Styled `Allow` / `Deny` approval banner & button placement (`src/host/hostd/src/gui/{attn.rs,controls.rs,ui.rs}`)**:
   - Wrapped approval prompt text in `attn::approval_prompt` using `controls::wrap_cols` bounded strictly to the column budget left of the `[ Allow ]` and `[ Deny ]` buttons (`cols - btn_cols`), eliminating trailing NBSP (`\u{00A0}`) padding that previously wrapped onto a blank line and collided with the right-anchored buttons.
   - Painted a recessed amber-accented approval banner (`0xFF28_2216` fill, `0xFF4E_3F22` 1 px border, `0xFFE5_A93C` 3 px left accent rail) behind the prompt rows in `ui.rs` and rendered `paint_approval_buttons` after `blit_with` so `[ Allow ]` and `[ Deny ]` sit cleanly inside the right side of the banner at scales `1.0`, `1.25`, and `2.0`.
2. **Approved inter-session (`mcp__lico__send`) automatic reply routing & clean chat views (`src/host/hostd/src/{leads.rs,session.rs,transfer.rs,calls.rs,claude.rs,codex.rs}`, `src/host/hostd/src/gui/transcript.rs`)**:
   - Added `PendingPeerReply { caller_id, hop, armed_by_peer_send }` and `assistant_reply_buf: String` on `Session`, accumulating assistant prose chunks across `claude` and `codex` turns while keeping `.seg` segment persistence completely intact.
   - Armed `pending_peer_reply` when an approved `By::Session` send dispatches to a target session (`hop < MAX_HOPS`); if the user manually steers the receiver mid-turn (`Origin::Prompt`), `pending_peer_reply` clears so human turns stay local, otherwise turn completion automatically routes `[peer reply from '<label>' (<id7>) | HH:MM | <size>]\n<body>` back to the caller via `Origin::Host` without spawning a second approval gate or re-arming an infinite ping-pong loop (`hop + 1`).
   - Updated `transcript::compute_visible_transcript` (and delegated `shown_lines_dump` directly to it for exact `ui()["shown_lines"]` parity) with `in_peer_turn` + `in_peer_echo` state so `[peer turn started] ... [turn completed]` blocks (including multi-line `>> ` peer message bodies) and raw `[host notice]` echo headers stay hidden in default chat view (`!show_events`) and reappear immediately under `Ctrl+Shift+E` (`Internal events: shown`).
3. **Mouse drag-selection auto-copy & universal right-pane panel selection (`src/host/hostd/src/gui/{mod.rs,ui.rs,clip.rs,panel.rs,toolbar.rs,sendto.rs,memrow.rs}`, `src/host/hostd/src/gui/editor/app.rs`)**:
   - Added `panel_drag`, `panel_sel`, `panel_view_lines`, `panel_body_rect`, `panel_scroll_px`, and `tx_drag_moved` tracking on `UiState`, plus `editor_drag_origin` on `App`.
   - Wired mouse-drag line selection (`0xFF2A_4A7A` highlight painted behind `panel_view` rows) across all non-transcript right-pane views; stationary clicks (`< 4*s` px) clear `panel_sel` and trigger clickable row actions (`[1]`, `[L]`, `[C]`, directory/file rows, account actions), whereas dragging `>= 4*s` px selects rows without accidentally firing `hit_row` on mouse release.
   - Auto-copied non-empty selections to X11 `CLIPBOARD` (`Selection::Clipboard` only, keeping `PRIMARY` untouched) on left-button drag release across transcript lines, prompt editor text, and `panel_view` rows, and extended `copy_selection`, `Ctrl+C`, `Ctrl+Shift+C`, toolbar `Copy`, and menu `Copy` to copy `panel_sel` without dismissing open panels.
4. **Persistent `[ Directories ]` navigation history & bookmarks toolbox (`src/host/hostd/src/{host.rs,ipc.rs}`, `src/host/hostd/src/clipboard/settings.rs`, `src/host/hostd/src/gui/{sidebar.rs,sidebar_render.rs,sidebar_nav.rs,ui.rs}`, `src/lico_protocol/v1.py`)**:
   - Added `SidebarNavLoc { host, path, page }` (`MAX_SIDEBAR_HISTORY = 64`, `MAX_SIDEBAR_BOOKMARKS = 32`) persisted in `settings.json` (`sidebar_history`, `sidebar_history_idx`, `sidebar_bookmarks`) via `settings.get` / `settings.set` and hydrated on GUI connect and reconnect (also updating `host.rs` `arg()` to `.rfind(...)` so later `--state-dir` CLI overrides take precedence).
   - Added a compact navigation and bookmark toolbox row at the top of `[ Directories ]` (`[ < ]`, `[ > ]`, `idx/len`, `[ ☆/★ ]`, `[ Marks (N) v ]`), tracking both directory transitions (`enter_dir`, `[..]` parent, root entry, `cwd` sync) and `[ < ] Page X/Y [ > ]` pagination changes, supporting `Alt+Left` / `Alt+Right` and `MouseButton::Back` / `MouseButton::Forward`, and rendering an interactive bookmarks quick-jump + `[×]` removal popup.

## Measured verification (`lico 0.2.3`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `597 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `486` tests in `94` files, `0` failed, **`103.5 s`**, 4-worker harness tree peak **`1,767.2 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.67 MiB` (`24.37..24.67 MiB`), median PSS `17.16 MiB`, anon `4.13 MiB`, host `7.79 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.80 MiB` (`35.50..35.80 MiB`), median PSS `22.80 MiB`, anon `3.94 MiB`, host `7.70 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.36 MiB` (`24.91 MiB` -> `25.06 MiB`; host `8.90` -> `20.80 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.3` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`ui.rs` / `panel.rs` / `transfer.rs` / `session.rs` / `mod.rs` / `sidebar.rs` / `codex.rs` / `sidebar_nav.rs` / `calls.rs` / `sidebar_render.rs` / `clip.rs` / `claude.rs` / `transcript.rs` / `settings.rs` / `attn.rs` / `ipc.rs`) | `1,087` / `795` / `795` / `788` / `775` / `772` / `740` / `715` / `627` / `619` / `588` / `587` / `571` / `515` / `507` / `487` lines | `<= 1,120` / `<= 800` | Met |

## Gate status

All five phases of Milestone `M07` (`M07-P01` through `M07-P05`, `lico 0.2.3`)
are implemented, measured, and installed to `~/bin/lico` for live testing,
awaiting owner G3 inspection acceptance and G4 merge/publication approval.
