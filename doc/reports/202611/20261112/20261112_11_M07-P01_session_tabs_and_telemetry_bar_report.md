# M07-P01 Report — Maximized launch, `Global` general sessions, token/quota telemetry, chat session tab bar, and bottom telemetry strip

- Status: pending G3 inspection
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Plan: [M07-P01 plan](../../../plans/202611/20261112/20261112_11_M07-P01_session_tabs_and_telemetry_bar_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M07-P01-session-tabs-and-telemetry-bar`
- Inspection bundle: `202611/20261112/00014_M07-P01_session_tabs_and_telemetry_bar/`

## Summary

`M07-P01` delivers the right-hand primary chat workbench improvements of Milestone
`M07`: `lico gui` opens maximized on the active monitor by default (`--windowed`
or `LICO_WINDOWED=1` opt-out), folderless scratch sessions are elevated to a
first-class `Global` general scope (`G·<title>` tab badge, `+G` tab-bar button,
`Ctrl+Shift+N`, and `Session > New Global session`), open sessions sit in a
horizontal multi-session tab bar above the conversation view (`[+]` / `[+G]`,
overflow `< >`, `×` close, and status badges), and a color-coded per-session
telemetry strip sits directly below the prompt input displaying
`Model (Effort) | Tok: in / out | 5h & Wk quota + reset countdown | CWD / Global`
alongside a compact multi-account quota summary in the bottom status bar.

## Delivered changes

1. **S02 — Per-session token counters, `effort`, `is_global`, and quota reset metadata (`src/host/hostd/src/{tokens.rs,claude.rs,codex.rs,folders.rs,registry.rs,unified.rs}`, `src/lico_protocol/v1.py`, `src/host/adapters/{claude,codex}/simulator.py`)**:
   - Added `tokens.rs` (`TokenUsage` + `SessionTokens`) tracking per-turn and cumulative `input_tokens` (including `cache_read_input_tokens` / `cache_creation_input_tokens`) and `output_tokens` across Claude stream-json (`assistant` and `result` `usage`) and Codex JSON-RPC (`thread/tokenUsage/updated` and `turn/completed`).
   - Exposed `effort`, `is_global`, `tokens` (`{"input", "output"}` when observed), and `account_usage` reset timestamps across `status`, `session.list`, `session.show`, HTTP `/v1/sessions`, and `src/lico_protocol/v1.py`.
2. **S03 — Maximized launch, `Global` general sessions, multi-session top tab bar, and color-coded bottom telemetry strip (`src/host/hostd/src/gui/{mod.rs,tabs.rs,telemetry.rs,controls.rs,row.rs,attn.rs,menu.rs,toolbar.rs,newsession.rs,panel.rs,confirm.rs,transcript.rs,scroll_oracle.rs,ui.rs}`)**:
   - Requested `WindowAttributes::with_maximized(true)` and `window.set_maximized(true)` on initial `lico gui` launch unless `--windowed` or `LICO_WINDOWED=1` is passed (`layout.maximized_requested` in UI dump).
   - Added `gui/tabs.rs` rendering a `26`-px horizontal multi-session tab bar (`G·`, `C·`, `X·` badges, activity/attention dots, `×` close button, `< >` horizontal tab scrolling, `[+]` new session, and `[+G]` new Global session) that switches sessions even while modal panels are open.
   - Added `gui/telemetry.rs` rendering a `22`-px color-coded telemetry strip directly below the prompt input (`Model (Effort)` | `Tok: in / out` in `k`/`M` notation | green/amber/crimson `5h` & `Wk` quota with reset countdown | clickable `cwd` or `Global (general)`) plus multi-account quota summary in the bottom status bar and crisp tool-call badge tinting in `transcript.rs`, while shrinking `gui/ui.rs` from `1,115` to `1,079` lines (`<= 1,120` ratchet).
3. **S04 — Integration test, testmap registration, and visual inspection bundle (`tests/test_session_tabs_and_telemetry.py`, `tests/testmap.toml`)**:
   - Added `tests/test_session_tabs_and_telemetry.py` pinning maximized launch metadata, `+G` Global general session creation, live Claude (`k`-scale) and Codex (`M`-scale + crimson critical quota) telemetry updates, panel-dismissing tab clicks, clickable telemetry `cwd_rect`, and tab `×` close.

## Measured verification

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Changed-area test suite (`suite run --changed main`) | `444` tests in `81` files + all `cargo` module unit tests, `0` failed, **`99.8 s`** | `0` failures | Met |
| Scroll & pixel layout oracle (`gui::scroll_oracle`, scales `1.0`, `1.25`, `2.0`) | Passed across all three DPI scales | Exact pixel match | Met |
| Module size ratchets (`ui.rs` / `tabs.rs` / `telemetry.rs` / `tokens.rs`) | `1,079` / `342` / `427` / `280` lines | `<= 1,120` / `<= 800` | Met |

## Gate status

Ready on `phase/M07-P01-session-tabs-and-telemetry-bar`; proceeding directly to
`M07-P02` under the owner's continuous G2 approval, with G3 inspection and G4
merge held until `M07-P03` completes.
