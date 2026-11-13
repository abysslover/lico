# M08-P03 Report — End-to-End Call Room & `@mention` Verification, Resource Budgets, Xvfb Inspection Bundle, and `lico 0.2.4` Release

- Status: done (awaiting owner G3 inspection acceptance and G4 merge approval)
- Version: 1.0.0
- Date: 2026-11-13
- Milestone: [M08 — Persistent Call Rooms, `@AgentName` Shared-Context Mentions, and Bounded Multi-Turn Agent Collaboration (`lico 0.2.4`)](../../../milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md)
- Plan: [M08-P03 plan](../../../plans/202611/20261113/20261113_04_M08-P03_call_verification_and_0_2_4_plan.md) (G1 and G2 approved by the owner on 2026-11-13)
- Branch: `phase/M08-P03-call-verification-and-0-2-4`
- Inspection bundle: `202611/20261113/00003_M08-P03_call_rooms_and_mentions/`

## Summary

`M08-P03` verifies the complete Milestone `M08` Call Room and `@AgentName`
collaboration workflow end to end across automated GUI/CLI/HTTP tests and live
Xvfb screenshots, confirms all performance and memory budgets well within
target, bumps `lico` from `0.2.3` to `0.2.4`, validates two-clone byte-for-byte
reproducibility, and installs the standalone `0.2.4` executable:

1. **Patch version bump (`0.2.3` -> `0.2.4`)**:
   - Updated `src/host/hostd/Cargo.toml` (`version = "0.2.4"`) and
     `src/host/hostd/Cargo.lock`, along with version assertions in
     `tests/test_control_parity.py` and `tests/test_single_executable.py`.
2. **Exact FIFO per-turn Call Room burst tracking & Call Room GUI polish (`src/host/hostd/src/{calls.rs,session.rs,codex.rs,call_room.rs}`, `src/host/hostd/src/gui/{panel.rs,attn.rs,call_view.rs,mention_popup.rs}`, `src/lico_protocol/v1.py`, `src/host/adapters/claude/simulator.py`)**:
   - Replaced `s.busy`-dependent burst staging on `PeerTurn` with an exact FIFO
     per-turn queue (`turn_fifo: VecDeque<Option<(String, String, u32, bool)>>`),
     populated in `Session::record_input`, merged on Codex `Pending::Steer`, and
     popped in `PeerTurn::finish` / `drop_active`, ensuring multi-round
     `AUTO_CHAIN_TO` exchanges advance deterministically through rounds `1..=4`
     even when a peer reply queues while the recipient's prior turn is still
     settling.
   - Exposed `"active"` and `"paused"` booleans directly in
     `CallRoom::summary_json()` and `src/lico_protocol/v1.py`
     (`CALL_SUMMARY_BODY`), aligned `parse_call_detail` in `gui/call_view.rs`
     with `CallEntry` field names (`hhmm`, `ts`, `speaker`, `speaker_label`,
     `provider`), guarded `panel_view()` when `active_call_open()` is true,
     added a header button backdrop strip in Call Room tabs, and positioned the
     `@mention` autocomplete popup below the prompt line whenever viewport space
     permits (flipping above near the bottom edge).
3. **Comprehensive end-to-end integration suite (`tests/test_call_rooms_and_mentions.py`, `tests/testmap.toml`) & parallel `Pss` sampling stability (`tests/test_host_resources.py`)**:
   - Added `test_mention_popup_shared_context_call_room_tab_say_hangup_and_cli_http`
     and `test_agent_bounded_four_round_burst_and_call_done_early_stop`,
     exercising `@Pri` -> `Tab` autocomplete, zero-modal `@Pri-Avatar` dispatch
     with recent caller context (`<= 8` turns / `16 KiB`) + Call Room transcript
     inside an `OWNER-AUTHORIZED` envelope, automatic `[peer reply]` delivery,
     `0600` `calls.json` persistence, stationary click on caller `[⇄ call ...]`
     jump cards opening top-bar `⇄ Pri-Lico·Pri-Avatar` tabs, `[ Calls ]`
     sidebar navigation, Call Room drag-selection auto-copy to `CLIPBOARD`, live
     human `call.say`, `Hang up` pausing, CLI `lico call ...` & loopback HTTP
     `/v1/calls...` parity, MCP `initialize` host-trust instructions, 4-round
     burst capping (`call_round_limit` on round 5, reset on human turn), and
     `[call done]` early settlement.
   - Bracketed `/proc` `Pss` reads in `tests/test_host_resources.py` against
     concurrent 4-worker shared-inode mapping churn while preserving the strict
     `<= 2048 KiB` assertion.
4. **Xvfb manual-inspection bundle (`202611/20261113/00003_M08-P03_call_rooms_and_mentions/`) & owner documentation**:
   - Captured 4 full-window PNG screenshots under an isolated Xvfb display
     (`01_mention_autocomplete_popup.png`,
     `02_caller_jump_card_and_peer_reply.png`,
     `03_call_room_tab_and_calls_sidebar.png`, and
     `04_call_room_say_and_hung_up_paused.png`) paired with a click-to-open
     `index.html` inspection page.
   - Updated `PLAN.md`, `README.md`, `doc/index_4.md`, `roadmap.md`,
     `doc/manual_end_to_end.md` (`lico 0.2.4`, `v1.8.0`), and the `M08`
     milestone record.

## Measured verification (`lico 0.2.4`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `604 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `489` tests in `95` files, `0` failed, **`106.1 s`**, 4-worker harness tree peak **`1,627.2 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.95 MiB` (`24.68..24.95 MiB`), median PSS `16.57 MiB`, anon `3.94 MiB`, host `7.75 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 36.31 MiB` (`35.86..36.31 MiB`), median PSS `22.26 MiB`, anon `3.94 MiB`, host `7.77 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.28 MiB` (`25.18 MiB` -> `25.09 MiB`; host `8.89` -> `20.70 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.4` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`ui.rs` / `core.rs` / `session.rs` / `transfer.rs` / `mod.rs` / `calls.rs` / `codex.rs` / `call_room.rs` / `call_view.rs` / `transcript.rs` / `sidebar_calls.rs` / `mention.rs` / `mention_popup.rs` / `call_room/ops.rs`) | `1,087` / `985` / `795` / `795` / `779` / `754` / `752` / `709` / `702` / `625` / `515` / `509` / `381` / `321` lines | `<= 1,120` / `<= 988` / `<= 800` | Met |

## Gate status

All three phases of Milestone `M08` (`M08-P01` through `M08-P03`, `lico 0.2.4`)
are implemented, measured, and verified on their phase branches and await the
owner's G3 inspection acceptance and G4 merge approval before merging to `main`.
