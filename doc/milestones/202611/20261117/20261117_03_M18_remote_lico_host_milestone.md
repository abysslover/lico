# M18 — The Remote Lico Host (Phase 0)

- Status: P01 (Phase 0, a report only) implemented and on `main` since 2026-11-18 by the owner's G4 and push yes in the chat (a fast-forward of `main` through the M20-P03 tip); G3 (the owner's inspection) is pending: inspection pending; its plan approved by the lead under the owner's delegation (the owner away); later phases are the plan of the report, not approved
- Version: 1.0.0
- Approved: not yet (G1 for the milestone waits for the owner's answers in the report, section 8)
- Project: Lico
- Updated: 2026-11-17
- Base: the M17-P09 tip `35f01bb` (M16 and M17 are not merged: the owner's G3 and G4 wait for their return)
- Implementation: `phase/M18-P01-remote-host-phase0`
- Version of the program: P01 bumps nothing

## Goal

Sessions on an ssh host run under a `lico host` process on that machine, reached over the existing ssh connection, and the window shows them like local ones; each host is approved by the owner and nothing is installed or copied without that approval (see the [P01 report](../../../reports/202611/20261117/20261117_24_M18-P01_remote_lico_host_phase0_report.md)).

## Phases

| Phase | What | Plan or report | Status | Stop |
|---|---|---|---|---|
| M18-P01 | Phase 0: the report | [Plan](../../../plans/202611/20261117/20261117_23_M18-P01_remote_lico_host_phase0_plan.md), [report](../../../reports/202611/20261117/20261117_24_M18-P01_remote_lico_host_phase0_report.md) | done | the owner's answers (section 8 of the report) |
| M18-P02 to P07 | link component and fake, the stdio role and the headless build, the install flow, the remote features, remote sessions, release | the report, section 6 | not started | each stop as in the report |

## Cost

P01: about 1.5 USD (estimate 3 to 4, stop rule 5.5). The whole milestone is estimated at about 38 USD (range 30 to 46) in the report; it does not fit under the ledger's 235 from where it stands.
