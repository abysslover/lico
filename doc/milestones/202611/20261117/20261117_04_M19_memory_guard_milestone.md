# M19 — The Memory Guard

- Status: P01 done and released as 0.6.0, on `main` since 2026-11-18 by the owner's G4 and push yes in the chat (a fast-forward of `main` through the M20-P03 tip); G3 (the owner's inspection) is pending: inspection pending; (slices A to D by the worker, the rest and the release by the lead after the worker's budget ended); every approval is the LEAD's under the owner's delegation, to be confirmed by the owner. Report: [M19-P01 report](../../../reports/202611/20261117/20261117_28_M19-P01_memory_guard_and_0_6_0_report.md)
- Version: 1.0.0
- Project: Lico
- Updated: 2026-11-17
- Base: the M17-P10 tip `1c56353`
- Implementation: `phase/M19-P01-memory-guard`
- Version of the program: `0.6.0` (release commit `95f39e3`, `10,820,264` bytes, two clean clones identical `88870041...e0a8`)

## Goal

Users who run many agents on a machine with little memory get repeated failures and forced restarts. Lico shows each session's memory, warns, lets the person end a heavy session and continue cleanly, and warns the main agent. Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

## Phases

| Phase | Subject | Plan and state | Status |
|---|---|---|---|
| M19-P01 | the memory guard, slices A to D | [Plan note and State for the lead](../../../plans/202611/20261117/20261117_27_M19-P01_memory_guard_plan.md) | done: A to D built, the gaps closed by the lead (grace, machine close, the note, the machine line, the new-session warning), released as `0.6.0` (see the report) |
