# M08-P04 Report — Unified `/rename` Command, `mcp__lico__rename` Self-Rename Tool, and Agent-to-Agent `@SessionName` Addressing

- Status: accepted at G3 and merged to `main` (2026-11-13)
- Version: 1.0.0
- Date: 2026-11-13
- Milestone: [M08 — Persistent Call Rooms, `@AgentName` Shared-Context Mentions, and Bounded Multi-Turn Agent Collaboration (`lico 0.2.4`)](../../../milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md)
- Plan: [M08-P04 plan](../../../plans/202611/20261113/20261113_05_M08-P04_unified_rename_and_agent_mentions_plan.md) (approved by the owner on 2026-11-13)
- Branch: `phase/M08-P04-unified-rename-and-agent-mentions`

## Summary

`M08-P04` implements the three live-inspection workflow enhancements requested
by the owner on `lico 0.2.4`:

1. **Unified `/rename` command across `Claude` & `Codex` (`src/host/hostd/src/gui/editor/app.rs`, `src/host/hostd/src/mention.rs`, `src/host/hostd/src/folders.rs`, `src/host/hostd/src/call_room.rs`)**:
   - Typing `/rename <Name>` in any `Claude` or `Codex` session prompt renames
     Lico's session directly (`--clear` clears the name) without forwarding
     `/rename` to the provider LLM, while keeping `F2` and toolbar `Rename`
     unchanged; typing bare `/rename` in `lico gui` opens the inline `Rename`
     prompt (`F2`).
   - Host `input` (`Origin::Person`) routes `/rename <Name>` identically for
     CLI, IPC, and loopback HTTP callers, and `folders::rename` invokes
     `call_room::sync_labels` so open Call Room titles (`⇄ A · B`) update
     immediately whenever a participant renames.
2. **`mcp__lico__rename` self-rename tool (`src/host/hostd/src/mcp.rs`, `src/host/hostd/src/tools.rs`)**:
   - Exposed `rename(name)` on Lico's loopback MCP server for both `Claude` and
     `Codex` sessions while keeping `tools/list` strictly below the `5,120`-byte
     token budget (`LIST_BUDGET`).
   - Validates `valid_name` (`<= 60` chars, no control or hidden characters),
     updates `s.settings.name`, marks the registry dirty, and syncs open Call
     Room titles immediately so natural-language self-rename requests work out
     of the box.
3. **Agent-to-agent `@SessionName` addressing (`src/host/hostd/src/calls.rs`, `src/host/hostd/src/history/mod.rs`, `src/host/hostd/src/mention.rs`, `src/host/hostd/src/mcp.rs`, `src/host/hostd/src/call_room.rs`)**:
   - Added `mention::resolve_agent_target` to `calls::send` (`to`) and
     `history::history` (`session`) so agents can pass `"@Codex-L"` (or exact
     `"Codex-L"`) directly without calling `sessions` first to look up a hex ID.
   - Added `mention::dispatch_assistant_mentions` wired through `PeerTurn` /
     `calls::deliver_peer_reply`: when a `Claude` or `Codex` turn completes
     without having already called `mcp__lico__send`, unindented column-0
     `@ExactSessionName <message>` paragraphs outside fenced code blocks
     automatically route through `calls::send` (immediate Call Room delivery
     when authorized, or centered `Allow / Deny` modal on first unapproved
     contact) and append a clickable `>> [⇄ call ...]` summary card in the
     sender's transcript.
   - Updated `call_room::record_session_transfer` so explicit `mcp__lico__send`
     calls carrying `[call done]` settle `room.active = false` cleanly, and
     updated MCP `initialize` `INSTRUCTIONS` to advertise `mcp__lico__rename`
     and `@PeerName` addressing.
4. **End-to-end verification (`tests/test_call_rooms_and_mentions.py`, `tests/test_tool_*.py`, `tests/test_agent_tools.py`, `tests/test_control_parity.py`)**:
   - Added `test_unified_slash_rename_mcp_rename_and_agent_at_mentions` covering
     GUI `/rename Codex-L`, bare `/rename` inline-bar opening, `/rename --clear`,
     IPC `/rename`, `mcp__lico__rename` + Call Room title synchronization,
     `mcp__lico__send(to="@Codex-L")` + `[call done]` settlement,
     `mcp__lico__history(session="@Codex-L")`, and direct assistant `@Codex-L`
     dispatch (while verifying fenced code blocks do not trigger sends).

## Measured verification (`lico 0.2.4`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `605 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `490` tests in `95` files, `0` failed, **`142.5 s`**, 4-worker harness tree peak **`1,812.4 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.4` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`ui.rs` / `core.rs` / `calls.rs` / `folders.rs` / `mention.rs` / `call_room.rs` / `tools.rs` / `mcp.rs` / `gui/editor/app.rs` / `history/mod.rs`) | `1,087` / `985` / `784` / `751` / `737` / `721` / `701` / `424` / `404` / `194` lines | `<= 1,120` / `<= 988` / `<= 800` | Met |

## Gate status

Accepted by the owner at G3 and merged to `main` (`milestone/M08-call-rooms-and-agent-mentions`) at G4 on 2026-11-13.
