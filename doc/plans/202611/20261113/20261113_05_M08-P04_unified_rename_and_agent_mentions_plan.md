# M08-P04 Plan — Unified `/rename` Command, `mcp__lico__rename` Self-Rename Tool, and Agent-to-Agent `@SessionName` Addressing

- Phase: `M08-P04`
- Milestone: [M08](../../../milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md)
- Branch: `phase/M08-P04-unified-rename-and-agent-mentions`
- Date: 2026-11-13

## Why

Live owner testing of `lico 0.2.4` confirmed `@Codex-L` Call Room routing and automatic `[peer reply]` + `[call done]` handling, and surfaced three natural workflow enhancements:
1. Typing `/rename <Name>` in any session (`Claude` or `Codex`) should uniformly rename Lico's session tab (while keeping `F2` and the toolbar `Rename` button), rather than forwarding `/rename` to a provider CLI that does not update Lico's registry.
2. Asking an agent in natural language to rename its own window (`mcp__lico__rename`) should update Lico's session name directly.
3. Agents should also be able to address peer sessions by `@Name` (`@Codex-L`) — both as `to: "@Codex-L"` in `mcp__lico__send` (and `session: "@Codex-L"` in `mcp__lico__history`) and by writing an unindented leading `@PeerName <message>` line in their assistant response.

## Scope

1. **Unified `/rename` command across `Claude` & `Codex` (`gui/editor/app.rs`, `mention.rs`, `folders.rs`, `call_room.rs`)**:
   - In `lico gui`, typing `/rename <Name>` renames the active session immediately via `session.rename` (`--clear` clears the name) without sending `/rename` to the LLM; typing bare `/rename` opens the inline `Rename` prompt (`F2`).
   - In `core::input` (`Origin::Person`), `/rename <Name>` also renames the session directly for CLI/HTTP callers and syncs Call Room participant titles (`call_room::sync_labels`).
2. **`mcp__lico__rename` tool (`mcp.rs`, `tools.rs`)**:
   - Expose `rename(name)` on Lico's loopback MCP server for both `Claude` and `Codex` (`LIST_BUDGET <= 5,120` B preserved), validating `valid_name` (`<= 60` chars, no control/invisible chars), updating `s.settings.name`, and syncing Call Room titles immediately.
3. **Agent-to-agent `@Name` addressing (`calls.rs`, `history/mod.rs`, `mention.rs`, `mcp.rs`, `call_room.rs`)**:
   - Resolve `@SessionName` / exact `SessionName` in `mcp__lico__send(to=...)` and `mcp__lico__history(session=...)`.
   - Detect unindented column-0 `@ExactSessionName <message>` lines outside fenced code blocks in an assistant's completed turn when `mcp__lico__send` was not already called in that turn, routing through `calls::send` (immediate Call Room delivery when authorized, centered `Allow` modal on first unapproved contact).
   - Settle `room.active = false` in `record_session_transfer` when an explicit `mcp__lico__send` carries `[call done]`, and update MCP `initialize` `INSTRUCTIONS`.
4. **Tests, documentation, reproducibility, and `~/bin/lico` `0.2.4` reinstall**:
   - Add automated unit and end-to-end tests covering `/rename`, `mcp__lico__rename`, `mcp__lico__send(to="@...")`, and assistant `@PeerName` output routing; run cold suite, `suite acceptance`, and reinstall `lico 0.2.4`.
