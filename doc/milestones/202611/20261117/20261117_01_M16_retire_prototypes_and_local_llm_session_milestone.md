# M16 — Retire the Prototypes, and a Found Local LLM Server as Its Own Session Kind (Phase 0)

- Status: P01 and P02 implemented; the owner accepted the design on 2026-11-17 (ten recommendations of report 119); P03 and P04 in progress (P01 and P02 approved by the owner on the screen on 2026-11-17; later phases are sketches until the owner picks the design)
- Version: 1.0.0
- Approved: 2026-11-17 (the owner's approval of the short plan for P01 and P02)
- Project: Lico
- Updated: 2026-11-17
- Base: `main` at `6712fe5` (M15 accepted, merged and published); P02 is stacked on P01
- Implementation: `phase/M16-P01-retire-prototypes`, `phase/M16-P02-local-llm-session-phase0`; later phases get their prefix when the owner has picked the design
- Version of the program: P01 and P02 bump nothing; `0.4.0` (a new kind of session) is PROPOSED for the release phase of the implementation, the lead confirms

## Goal

Two things, in this order. First, the one item M15 left open: the retired prototype clients and hosts are deleted (the owner's answer A to question 114, given on 2026-11-14 and 2026-11-17). Second, a Phase 0 report for the owner's purpose of 2026-11-14: Claude and Codex keep their OAuth sign-in and are not routed to local models; a Local LLM server that discovery (M12 and M13) found becomes its own kind of session, chosen in New session and + New chat next to Claude and Codex; Lico runs the tool loop itself (the model answers, asks for a tool, Lico runs it through the same operations and approvals the other sessions have, and feeds the result back). Servers on this machine only: an ssh host's server waits for the remote lico host milestone.

## Rules that bind

The component rules 1.0.1 (new code is new components with a version, downward-only dependencies, `SPEC.md`, contract tests, `RECEIPT.json`, 256 MiB and 30 s per component run; the short named system-test list at 512 MiB and 60 s); no new upward edge into the hubs (`core`, `session`, `folders`, `transfer`, `handoff`, `codex`); the whole suite runs only at the release phase; no new Python; idle RSS below 50 MiB, one executable, a GUI for the human with CLI and tool parity; tests never touch the owner's real services, hosts, sockets, `~/.claude`, `~/.codex`, and no billable turn; the discovery rules stay (GET-only, raw responses never persisted, nothing secret on disk or in a transcript, a server's key only in the OS keyring); no spec edit without the owner's yes (the proposed LLM-01..03 text still waits). The maximized first start (open since M14) is scheduled after this milestone and is not touched here.

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M16-P01 | Retire the prototypes | [Plan](../../../plans/202611/20261117/20261117_02_M16-P01_retire_prototypes_plan.md) | pending | implemented (`575f757`) |
| M16-P02 | Phase 0 for the Local LLM session (read-only; a report, then stop) | [Plan](../../../plans/202611/20261117/20261117_03_M16-P02_local_llm_session_phase0_plan.md) | [Report](../../../reports/202611/20261117/20261117_04_M16-P02_local_llm_session_phase0_report.md) | implemented; waits for the owner's pick |
| M16-P03 | `llm-wire` (the pure formats) | [Plan](../../../plans/202611/20261117/20261117_05_M16-P03_llm_wire_plan.md) | pending | approved, in progress |
| M16-P04 | `llm-http` and `llm-fake` | [Plan](../../../plans/202611/20261117/20261117_06_M16-P04_llm_http_and_fake_plan.md) | pending | approved |
| M16-P05.. | `convo`, `toolloop`, the driver port and adapter, the GUI, the release `0.4.0` | sketch in the P02 report | | the lead's next pointer |

## Cost

The owner's API credit is limited (124.8 of 150 USD used at the lead's count of 2026-11-17). Estimates: P01 about 1.5 to 2.5 USD, P02 about 6 to 9 USD; the running total is reported after each step.
