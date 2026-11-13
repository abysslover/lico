# M08-P01 Plan — Persistent Call Rooms, `@AgentName` Shared-Context Protocol, Host-Verified `OWNER-AUTHORIZED` Envelope, and CLI/IPC/HTTP Parity

- Phase: `M08-P01`
- Milestone: [M08](../../../milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md)
- Branch: `phase/M08-P01-call-rooms-and-mention-protocol`
- Date: 2026-11-13
- Status: in_progress

## Goal

Build the host engine and full protocol surface for persistent inter-session
**Call Rooms (`⇄ A · B`)** and **`@AgentName` mentions**:
1. **`src/host/hostd/src/call_room.rs`**:
   - Bounded persistent `CallStore` stored at `registry/calls.json` (`0600`
     atomic write, up to 64 rooms, up to 200 chronological `CallEntry` items per
     room) with canonical pair reuse (`⇄ <LabelA> · <LabelB>`) and live label
     refresh when sessions are renamed.
   - Records every `@AgentName` call, `call.start`, `call.say`, `mcp__lico__send`
     session transfer, and `[peer reply]` into the pair's Call Room.
   - Host-verified `[lico call '<title>' (<cid7>) | from '<sender>' (<id7>) | HH:MM | round R/4 | OWNER-AUTHORIZED]`
     envelope carrying explicit `[host verification: ...]`, recent conversation
     context from the calling session (`<= 8` turns / `<= 16 KiB`), cumulative
     Call Room transcript (`<= 12` entries / `<= 8 KiB`), and defused message
     text.
2. **`src/host/hostd/src/mention.rs`**:
   - Case-insensitive `@<name>` / `@"<name with spaces>"` / `@<id>` resolution
     against running/known sessions (`settings.name`, `auto_title`, `id` prefix).
   - Leading `@Target <msg>` in human `input` (`Origin::Person`): immediately
     authorizes `(caller <-> target)` (`owner_authorized = true` + bidirectional
     `pending` session allowance, zero approval modal), records the human prompt
     and clickable `[call summary]` card in `caller`, bundles `caller`'s last 8
     turns + Call Room log, dispatches Round `1/4` to `target`, and routes
     `target`'s reply back to both the Call Room and `caller`.
   - Inline `@Target` mid-sentence in human `input`: immediately connects and
     pre-authorizes Call Room `(caller, target)` (`keep = true` both ways, zero
     modal) and informs `caller`'s model of `target`'s session ID and active
     owner authorization.
3. **Bounded Multi-Turn Collaboration (`MAX_CALL_ROUNDS = 4`) & Host Trust**:
   - Update `src/host/hostd/src/mcp.rs` (`<= 5120` B `LIST_BUDGET`) so `Claude`
     and `Codex` know from turn 0 that unindented `[from session ...]` and
     `[lico call ... | OWNER-AUTHORIZED]` headers are strictly host-verified
     after human approval and that assistant replies in incoming peer/call turns
     are automatically forwarded back to the caller and Call Room.
   - Protect `"call summary"` and `"lico call "` / `"host verification:"` in
     `leads.rs` and `src/lico_protocol/codex_mapping.py`.
   - Track `in_reply_burst` (`1..=4` rounds) in `calls.rs` / `call_room.rs` with
     automatic early stop when an agent answers the human directly or emits
     `[call done]`, and immediate pause on `call.hangup`.
4. **Control-Surface Parity (`v1.py`, `ipc.rs`, `http.rs`, `admin.rs`, `cli.rs`)**:
   - Operations: `call.list`, `call.get`, `call.start`, `call.say`,
     `call.hangup`, `call.close` + `calls` summary array on `status`.
   - CLI: `lico call list [--all]`, `lico call show <ID>`,
     `lico call start --from <S1> --to <S2> [TEXT|-]`,
     `lico call say <ID> <TEXT|-> [--to <SESSION>]`,
     `lico call hangup <ID>`, `lico call close <ID>`.
   - Loopback HTTP: `GET /v1/calls`, `POST /v1/calls`, `GET /v1/calls/<id>`,
     `POST /v1/calls/<id>/say`, `POST /v1/calls/<id>/hangup`,
     `POST /v1/calls/<id>/close`, plus `POST /v1`.

## Acceptance Criteria

1. All existing Rust unit tests (`cargo test`), `tests/test_tool_send.py`,
   `tests/test_peer_reply_copy_and_dir_nav.py`, `tests/test_forged_leads.py`,
   `tests/test_protocol_conformance.py`, `tests/test_control_parity.py`, and
   `tests/test_size_ratchet.py` pass without regression.
2. Unit and protocol tests verify `@AgentName` resolution, 8-turn context
   extraction, Call Room persistence (`0600` under both `022` and `077` umasks),
   bounded 4-round-trip continuation + early stop + `call.hangup`, and full
   socket / CLI / HTTP parity.
