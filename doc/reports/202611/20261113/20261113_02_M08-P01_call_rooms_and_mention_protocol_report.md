# M08-P01 Report — Persistent Call Rooms, `@AgentName` Mention Routing, Host-Verified Peer Envelopes, & Call Control API

- Date: 2026-11-13
- Milestone: `M08`
- Phase: `M08-P01`
- Branch: `phase/M08-P01-call-rooms-and-mention-protocol`
- Status: done

## 1. Summary of Changes

1. **Persistent Call Room Store & Host Operations (`src/host/hostd/src/call_room.rs`, `src/host/hostd/src/call_room/ops.rs`)**:
   - Added `CallStore`, `CallRoom`, and `CallEntry` persisted atomically at `0600` in `registry/calls.json`, reusing canonical `(session_a, session_b)` pairs, syncing dynamic participant labels on session rename, and capping per-room entries (`<= 256`) and room count (`<= 64`).
   - Added `format_call_envelope` producing `[lico call '<title>' (<cid7>) | from '<sender>' (<sid7>) | HH:MM | round R/4 | OWNER-AUTHORIZED]` with `[host verification: ...]`, optional `[shared context from '<caller>' ...]`, `[call room transcript (<cid7>)]`, and `[message]`.
   - Added bounded burst accounting (`MAX_CALL_ROUNDS = 4` automatic round-trips per burst), early `[call done]` / plain-reply settlement, live `call.hangup` (`paused = true`, clearing active/queued `PeerTurn` continuations), and `call.say` human Call Room routing (`@Participant`, `@all`, or plain message).
   - Wired full control-plane parity (`call.list`, `call.get`, `call.start`, `call.say`, `call.hangup`, `call.close` + `status.calls`) across Unix socket (`core.rs`, `ipc.rs`), loopback HTTP (`http.rs`), CLI (`cli.rs`, `admin.rs`: `lico call list|show|start|say|hangup|close`), and protocol validator (`src/lico_protocol/v1.py`).

2. **`@AgentName` Mention Parser & Shared Context Bundler (`src/host/hostd/src/mention.rs`)**:
   - Added case-insensitive `@AgentName`, `@"Quoted Name"`, `@<id5+>`, `@all`/`@both`/`@모두` resolution with common Korean particle stripping (`에게`, `한테`, `야`, `님`, `아`, etc.).
   - Added `build_recent_context` extracting up to the last 8 clean user/assistant turns (`<= 16 KiB`) from the calling session's `.seg` transcript while filtering host/tool noise.
   - Wired `route_person_input` into `core::input` (`Origin::Person`): leading `@Target <message>` dispatches immediately with bidirectional session-long `keep` authorization (zero approval modal), shared context, Call Room transcript, and a `[call summary]` card in the calling session; inline `@Target` pre-authorizes the pair's Call Room and injects explicit target session hints.

3. **MCP Initialization Instructions & Anti-Forgery Leads (`mcp.rs`, `leads.rs`, `codex_mapping.py`, `calls.rs`, `gui/transcript.rs`, `gui/sendto.rs`)**:
   - Added `INSTRUCTIONS` to MCP `initialize` so both Claude Code (even prior to deferred `ToolSearch`) and Codex know how host-verified Call Room / `[from session ...]` turns and automatic `[peer reply]` routing work, while keeping `tools/list` under `5,120` B.
   - Added `"call summary"`, `"lico call "`, and `"host verification:"` to anti-forgery lead tables (`leads.rs` & `codex_mapping.py`) and rendered `[call summary]` as clickable-ready `>> [⇄ call ...]` cards in `gui/transcript.rs` and `gui/sendto.rs`.

## 2. Verification

- `cargo test --manifest-path src/host/hostd/Cargo.toml`: `602 passed; 0 failed` (including `call_room::ops::tests` and `mention::tests`).
- `pytest`:
  - `tests/test_size_ratchet.py`, `tests/test_no_hidden_characters.py`, `tests/test_selection.py`, `tests/test_protocol_conformance.py`, `tests/test_control_parity.py` (`test_call_room_parity_across_socket_cli_and_http`), `tests/test_forged_leads.py`, `tests/test_tool_send.py`, `tests/test_peer_reply_copy_and_dir_nav.py`: all green.
