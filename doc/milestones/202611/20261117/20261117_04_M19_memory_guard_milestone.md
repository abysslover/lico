# M19 — The Memory Guard

- Status: P01 in progress (slices A to C built, D not); every approval is the LEAD's under the owner's delegation, to be confirmed by the owner
- Version: 1.0.0
- Project: Lico
- Updated: 2026-11-17
- Base: the M17-P10 tip `1c56353`
- Implementation: `phase/M19-P01-memory-guard`
- Version of the program: not bumped (the release `0.6.0` is not made)

## Goal

Users who run many agents on a machine with little memory get repeated failures and forced restarts. Lico shows each session's memory, warns, lets the person end a heavy session and continue cleanly, and warns the main agent. Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.

## Phases

| Phase | Subject | Plan and state | Status |
|---|---|---|---|
| M19-P01 | the memory guard, slices A to D | [Plan note and State for the lead](../../../plans/202611/20261117/20261117_27_M19-P01_memory_guard_plan.md) | A, B, C built (with the gaps the note lists); D and the release `0.6.0` not done |
