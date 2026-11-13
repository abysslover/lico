# M08 — Persistent Call Rooms, `@AgentName` Shared-Context Mentions, and Bounded Multi-Turn Agent Collaboration (`lico 0.2.4`)

- Status: awaiting_g3_g4
- Approved: 2026-11-13 (G1 and G2 approved by the owner via explicit design choices A1/A2/A3/A4)
- Project: Lico
- Updated: 2026-11-13
- Milestone branch: `milestone/M08-call-rooms-and-agent-mentions` (to be created at milestone close on G4 merge approval)
- Implementation: Implemented across `phase/M08-P01-call-rooms-and-mention-protocol` through `phase/M08-P04-unified-rename-and-agent-mentions` (`lico 0.2.4`); awaiting owner G3 inspection acceptance and G4 merge approval
- Next gate: G3 inspection acceptance and G4 merge approval

## Goal

Elevate Lico's inter-session communication (`0.2.3` -> `0.2.4`) from raw one-shot
`mcp__lico__send` transfers into a first-class **"Phone Call / Shared Call Room" +
`@AgentName` mention architecture** across `Claude` and `Codex` sessions while
preserving Lico's single-executable design and `< 50.0 MiB` idle GUI RSS budget:
(1) persistent per-pair **Call Rooms (`⇄ A · B`)** with chronological speaker
transcripts, host-verified `OWNER-AUTHORIZED` call envelopes (so receiving
`Claude` and `Codex` models recognize human-authorized peer collaboration and
know their normal assistant replies are automatically routed back to the Call
Room and caller), `@AgentName` shared-context bundling (attaching up to the
calling session's last 8 turns / `16 KiB` plus cumulative Call Room history),
immediate authorization on human `@mention` (zero approval modal), single-modal
session connection on agent-initiated sends, bounded multi-turn exchanges (up to
4 automatic round-trips per burst with early stop on task completion), and full
CLI + IPC + loopback HTTP parity (`M08-P01`);
(2) a dedicated **`[ Calls ]` tab** in the left sidebar alongside `[ Directories ]`
and `[ Past ]`, **top tab bar Call Room tabs (`⇄ Pri-Lico·Pri-Avatar`)** with live
human chat intervention (`call.say`) and `[ Hang up ]` (`Ctrl+K`), `@AgentName`
autocomplete popup in the prompt editor, and clickable Call Room jump cards in
session transcripts (`M08-P02`); and
(3) end-to-end automated & live Xvfb manual-inspection verification, resource
budget confirmation (`< 50.0 MiB` idle RSS, `<= +10.0 MiB` hidden growth,
`<= 180.0 s` cold suite), two-clone `suite acceptance`, and standalone `0.2.4`
installation to `~/bin/lico` (`M08-P03`).

## Done when

1. Persistent `CallStore` (`registry/calls.json`, `0600`), `@AgentName` mention
   resolution (`@Pri-Avatar`, `@"Pri Avatar"`, `@<id>`), recent-context bundler
   (last 8 turns / `<= 16 KiB` + Call Room transcript), host-verified
   `[lico call ... | OWNER-AUTHORIZED]` envelope + `mcp.rs` host-trust guarantee,
   immediate bidirectional authorization on human `@mention`, up to 4 automatic
   round-trips per burst (`MAX_CALL_ROUNDS = 4`) with early stop and `Hang up`,
   and full `call.list` / `call.get` / `call.start` / `call.say` / `call.hangup` /
   `call.close` parity across IPC, CLI (`lico call ...`), and loopback HTTP
   (`/v1/calls...`) are implemented and green. (`M08-P01`)
2. Left sidebar `[ Dir | Past | Calls ]` tab, top tab bar `⇄ A·B` Call Room tabs
   with chronological speaker-badged cards + `[ Hang up ]` + live human input
   (`call.say`) + universal selection/copy, `@AgentName` autocomplete popup in
   the prompt editor, and clickable caller `[⇄ call ...]` jump cards in session
   transcripts are implemented and green. (`M08-P02`)
3. Comprehensive integration tests (`tests/test_call_rooms_and_mentions.py`),
   cold test suite (`<= 180.0 s`), scale 1 & scale 2 idle RSS (`<= 50.0 MiB`),
   10->100 hidden growth (`<= +10.0 MiB`), Xvfb manual-inspection screenshots,
   and reproducible `lico 0.2.4` standalone installation pass end to end.
   (`M08-P03`)
4. Unified `/rename <Name>` (`--clear` and bare `/rename` inline-bar opening
   alongside `F2` and toolbar `Rename`), `mcp__lico__rename` self-rename tool
   with live Call Room title sync, and agent-to-agent `@SessionName` addressing
   (`mcp__lico__send(to="@Name")`, `mcp__lico__history(session="@Name")`, and
   unindented column-0 `@ExactSessionName <message>` assistant output routing)
   work across `Claude` and `Codex` sessions. (`M08-P04`)

## Why now

Live testing of `0.2.3` showed two fundamental gaps in point-to-point
`mcp__lico__send`: (a) receiving `Claude` sessions refused peer tasks when the
input header lacked explicit host attestation of human-owner authorization and
explanation of automatic `[peer reply]` forwarding, and (b) inter-session
collaboration had no persistent shared Call Room where `@AgentName` mentions
share conversational context, agents exchange up to a bounded number of turns
automatically, and the human administrator can open the room tab to read, steer,
or hang up the call directly.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M08-P01 | Persistent Call Rooms, `@AgentName` shared-context protocol, host-verified `OWNER-AUTHORIZED` envelope, bounded 4-round-trip engine, and CLI/IPC/HTTP parity | [Plan](../../../plans/202611/20261113/20261113_02_M08-P01_call_rooms_and_mention_protocol_plan.md) | [Report](../../../reports/202611/20261113/20261113_02_M08-P01_call_rooms_and_mention_protocol_report.md) | — | — | done |
| M08-P02 | Left sidebar `[ Calls ]` tab, top-bar `⇄ A·B` Call Room tabs with live human intervention & `Hang up`, `@mention` autocomplete popup, and clickable jump cards | [Plan](../../../plans/202611/20261113/20261113_03_M08-P02_call_room_tabs_and_mention_ui_plan.md) | [Report](../../../reports/202611/20261113/20261113_03_M08-P02_call_room_tabs_and_mention_ui_report.md) | — | — | done |
| M08-P03 | End-to-end Call Room & `@mention` verification, resource budgets, Xvfb inspection bundle, and `lico 0.2.4` release | [Plan](../../../plans/202611/20261113/20261113_04_M08-P03_call_verification_and_0_2_4_plan.md) | [Report](../../../reports/202611/20261113/20261113_04_M08-P03_call_verification_and_0_2_4_report.md) | `202611/20261113/00003_M08-P03_call_rooms_and_mentions/` | — | done |
| M08-P04 | Unified `/rename` command across providers, `mcp__lico__rename` self-rename tool, and agent-to-agent `@SessionName` addressing | [Plan](../../../plans/202611/20261113/20261113_05_M08-P04_unified_rename_and_agent_mentions_plan.md) | [Report](../../../reports/202611/20261113/20261113_05_M08-P04_unified_rename_and_agent_mentions_report.md) | — | — | done |

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-11-13 | Scope and phases `M08-P01` to `M08-P04` (owner choices A1/A2/A3/A4 + live inspection enhancements), patch version bump `0.2.3` -> `0.2.4` |
| G2 phase plan approval | Approved 2026-11-13 | Continuous implementation across `M08-P01` to `M08-P04` |
| G3 inspection acceptance | Pending | Awaits owner review after `M08-P04` |
| G4 merge approval | Pending | Awaits owner approval after G3 |
