# M09-P03 Report — End-to-End GUI & MCP Verification, Xvfb Inspection Bundle, Memory Budgets, and `lico 0.2.5` Release

- Status: accepted at G3 and merged to `main` (2026-11-13)
- Version: 1.2.0
- Date: 2026-11-13
- Milestone: [M09 — Clean Call Control Markers, Collapsible Tool Cards, and Cross-Provider Session Creation (`lico 0.2.5`)](../../../milestones/202611/20261113/20261113_02_M09_collapsible_tools_and_cross_provider_create_milestone.md)
- Plan: [M09-P03 plan](../../../plans/202611/20261113/20261113_08_M09-P03_verification_and_0_2_5_plan.md) (G1 and G2 approved by the owner on 2026-11-13)
- Branch: `phase/M09-P03-verification-and-0-2-5`
- Inspection bundle: `202611/20261113/00004_M09-P03_collapsible_tools_and_create/`

## Summary

`M09-P03` completes Milestone `M09` (`0.2.4` -> `0.2.5`) by verifying all three
owner-requested capabilities end to end in a live Xvfb `ProductWindow` and
across the full cold test suite, confirming every memory and module-size budget,
verifying clean two-clone binary reproducibility (`suite acceptance`), and
installing the standalone `0.2.5` executable:

1. **Patch version bump (`0.2.4` -> `0.2.5`)**:
   - Updated `src/host/hostd/Cargo.toml` (`version = "0.2.5"`) and
     `src/host/hostd/Cargo.lock`, along with version assertions in
     `tests/test_control_parity.py` and `tests/test_single_executable.py`.
2. **Active session tab re-focus, empty-tab peer-reply routing, chronological turn-order deferral, mint-emerald peer card palette, and `mode: "auto"` (`bypass`) session creation (`src/host/hostd/src/gui/confirm.rs`, `src/host/hostd/src/calls.rs`, `src/host/hostd/src/calls/tests.rs`, `src/host/hostd/src/claude.rs`, `src/host/hostd/src/codex.rs`, `src/host/hostd/src/core.rs`, `src/host/hostd/src/session.rs`, `src/host/hostd/src/transfer.rs`, `src/host/hostd/src/transfer/tests.rs`, `src/host/hostd/src/create.rs`, `src/host/hostd/src/create/tests.rs`, `src/host/hostd/src/gui/transcript.rs`, `src/host/hostd/src/gui/sendto.rs`, `src/host/hostd/src/mention.rs`)**:
   - Updated `row_click` and `close_tab_by_id` to invoke `switch_to(&id)` when
     `self.active_call_open()` or `self.view.is_none()` even when `id == self.session`,
     so clicking the active session's own top tab from an open `⇄` Call Room tab
     returns immediately to that session's chat view.
   - Replaced the legacy `made_by.is_some() && finished_seq < 0` check with an
     explicit RAII `calls::with_create_prompt` scope around `create`'s founding
     prompt delivery so empty tabs opened via `mcp__lico__create` (`finished_seq < 0`)
     immediately arm `PeerTurn` on incoming `mcp__lico__send` messages and relay
     the peer's reply visibly back to the caller's open window (`[peer reply from ...]`)
     and into the shared Call Room, while resolving `<Name> - made by <Creator>`
     sessions by their base `<Name>` across `@Name` and `to="<Name>"` calls.
   - Added `PeerTurn::deferred_replies` (`try_defer_peer_reply` + `flush_deferred`
     under `MutexGuard<Session>` on `[turn completed]`) and unified authorized
     explicit `mcp__lico__send` replies under `[peer reply from ...]` so peer
     replies arriving while the caller is still running an active turn (such as its
     `approved and done:` turn) never write `>> ...` above that turn's prose.
   - Styled incoming peer cards in a distinct mint-emerald palette (`PEER_INK = #A7F3D0`,
     `PEER_LEAD_INK = #38B2AC`, header name highlight `#5EEAD4` on `#11292B` / `#245256`)
     visually separated from sky-blue human prompts (`#82CAFF`), and switched
     `mcp__lico__create`'s approved session mode from `"default"` to `"auto"`
     (`mode: auto (bypass)`, starting Claude with `bypassPermissions` and Codex
     with `auto` policy).
3. **End-to-end GUI & MCP integration test (`tests/test_collapsible_tools_and_create_tab.py`, `tests/test_tool_create.py`, `tests/test_card_window.py`, `tests/testmap.toml`)**:
   - Verified in a live Xvfb `ProductWindow` (`3.6 s` runtime, `< 15.0 s` budget):
     - `[call done]` and 201-character `LINE_CHARS`-clipped `[call done` tails
       are stripped from `mcp__lico__send` deliveries, Call Room entries, and
       session chat views while still settling Call Room burst state (`active == False`);
     - `[tool] mcp__lico__send` and `$ ls -la` invocations render collapsed by
       default (`▸ ...`) in the indigo-violet pill palette (`#161426`), expand on
       mouse click to `▾ ...` (`#1D1934` header + `#121020` body), and collapse
       back on header click;
     - Cross-provider `mcp__lico__create` opens both an empty `Codex` tab from a
       `Claude` session (`kind="codex"`, no `prompt`) and a `Global` `Claude`
       tab with an initial prompt from a `Codex` session (`kind="claude"`,
       `scope="global"`, `prompt="..."`) in `mode: "auto"` (`mode: auto (bypass)`),
       surfacing centered approval modals and adding visible tabs in `lico gui`'s
       top tab bar upon `Allow`.
4. **Xvfb manual-inspection bundle (`202611/20261113/00004_M09-P03_collapsible_tools_and_create/`) & documentation**:
   - Captured 4 full-window Xvfb screenshots (`01_clean_reply_and_collapsed_tool_pill.png`,
     `02_expanded_indigo_violet_tool_card.png`,
     `03_cross_provider_create_approval_modal.png`, and
     `04_new_codex_and_global_claude_tabs_open.png`) paired with a click-to-open
     `index.html` inspection page.
   - Updated `PLAN.md`, `README.md`, `doc/index_4.md`, `roadmap.md`,
     `doc/manual_end_to_end.md` (`lico 0.2.5`, `v1.9.0`), and the `M09`
     milestone record.

## Measured verification (`lico 0.2.5`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `611 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `492` tests in `96` files, `0` failed, **`137.0 s`**, 4-worker harness tree peak **`2,026.2 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 25.25 MiB` (`24.86..25.25 MiB`), median PSS `16.64 MiB`, anon `3.94 MiB`, host `7.64 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 36.32 MiB` (`35.88..36.32 MiB`), median PSS `22.19 MiB`, anon `4.13 MiB`, host `7.64 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.38 MiB` (`25.09 MiB` -> `25.32 MiB`; host `8.88` -> `20.69 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.5` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`ui.rs` / `core.rs` / `session.rs` / `mention.rs` / `codex.rs` / `transcript.rs` / `calls.rs` / `create/tests.rs` / `claude.rs` / `sendto.rs` / `calls/tests.rs` / `transfer/tests.rs` / `create.rs` / `confirm.rs` / `transfer.rs` / `tool_fold.rs`) | `1,087` / `987` / `800` / `796` / `758` / `695` / `638` / `625` / `598` / `504` / `435` / `407` / `404` / `404` / `393` / `209` lines | `<= 1,120` / `<= 988` / `<= 800` | Met |

## Gate status

Accepted by the owner at G3 and merged and published on `main` (`milestone/M09-collapsible-tools-and-cross-provider-create`) at G4 on 2026-11-13.
