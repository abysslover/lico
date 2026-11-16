# M16-P02 Plan — Phase 0 for the Local LLM Session (Read-Only)

- Status: approved by the owner on 2026-11-17 (the plan shown on the screen); waits for P01
- Version: 1.0.0
- Branch: `phase/M16-P02-local-llm-session-phase0` (stacked on the P01 tip)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)

## Objective

One report, no code and no moves, then stop for the owner's pick: `doc/reports/202611/20261117/20261117_04_M16-P02_local_llm_session_phase0_report.md`, English, an owner page of one screen at its top, every figure marked MEASURED, COMPUTED or ASSUMED.

## Contents

1. What exists and what the session kind builds on: the discovery engine and its dialog (what a result row carries); how a session kind is added today (the Claude and Codex adapters, where New session and + New chat are built, the record, history, resume and chats list per provider); the tool connection (`tools`, `pending`, the tool list and its 5,120-byte budget); which operations an agent can call and which need the person's approval.
2. The model side: OpenAI-compatible `/v1/chat/completions` (streaming, tool calls) and Ollama `/api/chat`; a server without tool calls (text protocol fallback or refusal, with a recommendation); cancellation, timeouts, context limits and truncation; errors (server gone, model missing, key needed) and what the person sees.
3. The Lico tool loop: which tools a local session may call, how approvals reach the person, how results go back, loop limits, what the loop must not be able to do.
4. Security: loopback only, the key in the keyring, what is logged and what is not, prompt-injection exposure of a weaker model with tools, the smallest safe default.
5. The component design: at least four candidate components with downward-only dependencies, public surface, contract tests that need no real server (a fake server on an ephemeral loopback port), the 256 MiB and 30 s check, the order of build with stop points; the thin host adapter and the display-free part of the GUI versus the painter.
6. The estimate: lines, phases, and a cost in USD per phase at the rate measured so far.
7. Open questions for the owner with recommendations: model choice and where it is remembered, the default tool set, showing the reasoning, one record format or its own, CLI and HTTP parity.

## Steps

- [ ] 1. Targeted reading and greps of the code named above (no whole-tree dumps); `cargo xtask graph` for the edges into the hubs.
- [ ] 2. The report; the mail; stop.

## Acceptance Criteria

The report exists with the owner page first and every figure marked; nothing but documents changed; the link scan is clean; the phase stops there.
