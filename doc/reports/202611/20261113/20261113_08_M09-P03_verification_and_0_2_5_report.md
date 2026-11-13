# M09-P03 Report — End-to-End GUI & MCP Verification, Xvfb Inspection Bundle, Memory Budgets, and `lico 0.2.5` Release

- Status: done (awaiting G3 inspection and G4 merge)
- Version: 1.0.0
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
2. **Active session tab re-focus from Call Room / detached views (`src/host/hostd/src/gui/confirm.rs`)**:
   - Updated `row_click` and `close_tab_by_id` to invoke `switch_to(&id)` when
     `self.active_call_open()` or `self.view.is_none()` even when `id == self.session`,
     so clicking the active session's own top tab from an open `⇄` Call Room tab
     returns immediately to that session's chat view.
3. **End-to-end GUI & MCP integration test (`tests/test_collapsible_tools_and_create_tab.py`, `tests/testmap.toml`)**:
   - Verified in a live Xvfb `ProductWindow` (`3.45 s` runtime, `< 15.0 s` budget):
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
       `scope="global"`, `prompt="..."`), surfacing centered approval modals and
       adding visible tabs in `lico gui`'s top tab bar upon `Allow`.
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
| Rust unit tests (`umask 022` and `umask 077`) | `608 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `492` tests in `96` files, `0` failed, **`108.4 s`**, 4-worker harness tree peak **`1,788.1 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 25.25 MiB` (`24.86..25.25 MiB`), median PSS `16.64 MiB`, anon `3.94 MiB`, host `7.64 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 36.32 MiB` (`35.88..36.32 MiB`), median PSS `22.19 MiB`, anon `4.13 MiB`, host `7.64 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.38 MiB` (`25.09 MiB` -> `25.32 MiB`; host `8.88` -> `20.69 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.5` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`ui.rs` / `core.rs` / `session.rs` / `transfer.rs` / `calls.rs` / `mod.rs` / `codex.rs` / `call_room.rs` / `call_view.rs` / `transcript.rs` / `create/tests.rs` / `mcp.rs` / `create.rs` / `confirm.rs` / `call_room/ops.rs` / `tool_fold.rs`) | `1,087` / `985` / `795` / `795` / `795` / `780` / `752` / `723` / `703` / `687` / `625` / `429` / `404` / `404` / `395` / `209` lines | `<= 1,120` / `<= 988` / `<= 800` | Met |

## Gate status

Implemented on `phase/M09-P03-verification-and-0-2-5` with standalone `lico 0.2.5` installed; awaiting owner G3 inspection and G4 merge approval.
