# M18-P01 Plan — Phase 0 for the Remote Lico Host

- Status: approved by the lead under the owner's delegation (the owner away), mail 154 of 2026-11-17; a report only
- Version: 1.0.0
- Branch: `phase/M18-P01-remote-host-phase0` (stacked on the M17-P09 tip `35f01bb`)
- Milestone: [M18](../../../milestones/202611/20261117/20261117_03_M18_remote_lico_host_milestone.md); report: [M18-P01](../../../reports/202611/20261117/20261117_24_M18-P01_remote_lico_host_phase0_report.md)

## Objective

Answer, before any code: what a remote `lico` host would be, how the window reaches it, how it gets onto a machine with the owner's approval of THAT machine, what is and is not copied, which components appear, in which phases and at what cost. **No code, no connection to any host, no install, no real service.** Everything is READ from the repository's own code, COMPUTED from measured figures of M16 and M17, or ASSUMED and said so. No host name, personal name or path of the owner's machines is written in a tracked file.

## Steps

- [x] 1. Read what an ssh session and the file browsing do today (`ssh.rs`, `ssh_remote.rs`, `chats/seed.rs`, `llm/remote.rs`, the Claude and Codex adapters).
- [x] 2. Read what the binary needs to run on another machine (the C library it was linked against, the transports the host has).
- [x] 3. Write the report with the owner page first, then the design, the phases, the cost and the questions.
- [x] 4. The milestone note, the index, the link scan, the `done` file (report number 140), STOP.

## Cost

Estimate 3 to 4 USD, stop rule 5.5 (mail 154). Waits of at most 4:50.

## Acceptance Criteria

The report answers the seven questions of mail 154 in plain words first, labels every claim, names no machine, and ends with open questions that each carry a recommendation.
