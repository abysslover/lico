# M08-P03 Plan — End-to-End Call Room & `@mention` Verification, Resource Budgets, Xvfb Inspection Bundle, and `lico 0.2.4` Release

- Phase: `M08-P03`
- Milestone: [M08](../../../milestones/202611/20261113/20261113_01_M08_call_rooms_and_agent_mentions_milestone.md)
- Branch: `phase/M08-P03-call-verification-and-0-2-4`
- Date: 2026-11-13

## Why

`M08-P01` and `M08-P02` added persistent per-pair Call Rooms (`⇄ A · B`), `@AgentName` shared-context routing, `OWNER-AUTHORIZED` call envelopes, bounded 4-round-trip collaboration, CLI/IPC/HTTP parity, the `[ Calls ]` sidebar tab, top-bar Call Room tabs, `@mention` editor autocomplete, and clickable transcript jump cards. `M08-P03` bumps `lico` to `0.2.4`, verifies the entire workflow end to end across automated tests and real Xvfb screenshots, measures all memory and suite-time budgets, runs two-clone reproducibility acceptance, updates documentation, and installs the standalone `0.2.4` binary.

## Scope

1. **Patch version bump (`0.2.3` -> `0.2.4`)**:
   - Update `src/host/hostd/Cargo.toml` (`version = "0.2.4"`) and `src/host/hostd/Cargo.lock`, plus the version assertions in `tests/test_control_parity.py` and `tests/test_single_executable.py`.
2. **End-to-end automated verification (`tests/test_call_rooms_and_mentions.py` + `tests/testmap.toml`)**:
   - Verify `@AgentName` autocomplete popup (`@Pri` -> `Tab` -> `@Pri-Avatar `), zero-modal `@mention` routing with prior conversational turns (`<= 8` turns / `16 KiB`) + Call Room transcript inside an `OWNER-AUTHORIZED` envelope, automatic `[peer reply]` forwarding, `0600` `calls.json` persistence, stationary click on caller `[⇄ call ...]` jump card opening the top-bar `⇄ Pri-Lico·Pri-Avatar` Call Room tab, `[ Calls ]` sidebar tab navigation, mouse drag-selection auto-copy in the Call Room view, live human `call.say` intervention, `[ Hang up ]` pausing, CLI `lico call ...` & loopback HTTP `/v1/calls...` parity, MCP `initialize` instructions, bounded 4-round-trip burst limit (`call_round_limit` on round 5, reset on human turn), and `[call done]` early settlement.
3. **Budgets, cold suite, Xvfb inspection bundle, documentation, and `0.2.4` release**:
   - Confirm `cargo test` under `umask 022` and `umask 077`, cold full test suite (`<= 180.0 s`, 4-worker peak `<= 2,048 MiB`), idle window RSS at scale 1 and scale 2 (`< 50.0 MiB`), and 10->100 hidden growth (`<= +10.0 MiB`).
   - Capture Xvfb screenshots and an `index.html` inspection page, update status/docs (`PLAN.md`, `README.md`, `doc/index_4.md`, `roadmap.md`, `doc/manual_end_to_end.md`, milestone & report), run `lico_measure.suite acceptance`, and install `lico 0.2.4`.
