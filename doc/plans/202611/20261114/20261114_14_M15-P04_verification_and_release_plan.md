# M15-P04 Plan — Verification and Release (`lico 0.3.3`)

- Status: implemented (the lead's instruction of 2026-11-14 said to continue to P04 after the P03 specification was approved; AGENTS.md lets a phase prefix include the installs the plan names, and this plan names one)
- Version: 1.1.0
- Branch: `phase/M15-P04-verification-and-0-3-3` (stacked on the P03 tip `f566ace`)
- Milestone: [M15](../../../milestones/202611/20261114/20261114_04_M15_component_foundation_milestone.md)

## Objective

Release acceptance once and whole, with numbers only as measured: the version `0.3.3` (proposed in mail 135; a small patch for the maximized first start follows after the Local LLM session, not in this milestone); three cold runs of everything with the margin to `2,048 MiB` and `180 s`; the receipts matching; the idle window and hidden growth; the Python and the Rust two-clone acceptance; the tool-list bytes; the binary shown free of the test support; CHANGELOG, PLAN, roadmap, README, manual, index; the install to `~/bin/lico` keeping the `0.3.2` binary at `private/release_0_3_2_backup/lico`. The specification is not edited; any text a requirement would need goes into the report.

## Steps

- [x] 1. The version: the two tests that name `0.3.2` change to `0.3.3` first (red), then `Cargo.toml` and `Cargo.lock`.
- [x] 2. On the clean committed tree, one after the other, each waiting for the load average to fall under 12: Rust units under `umask 022` and `umask 077`; `cargo xtask check` and `verify`; the tool-list bytes; the binary free of the test support.
- [x] 3. Three cold full runs (`suite run --cold`): tests, seconds, peak, margins; a failure in a load-sensitive test on a loaded host is not counted and is said.
- [x] 4. Idle window RSS at scale 1 and 2 (3 launches each) and hidden growth (3 repetitions), with `lico_measure run --candidates a2 --ui --menu --accounts 10 --realistic-text`.
- [x] 5. `suite acceptance` (Python) and `cargo xtask accept` (Rust) on the final commit. Decision on retiring the Python one: only if the Rust one is shown to catch what it catches; otherwise both stay and the report says why.
- [x] 6. Documents: CHANGELOG `0.3.3`, PLAN, roadmap, README, manual, index, milestone and plan statuses.
- [x] 7. Build with `build-release.sh` from the clean commit, hash, keep the `0.3.2` binary at `private/release_0_3_2_backup/lico`, install to `~/bin/lico`, run `lico --version`.
- [x] 8. Report and mail.

## Acceptance Criteria

Every check above has a measured result next to its budget (`<= 180.0 s`, `<= 2,048.0 MiB`, idle `< 50 MiB`, hidden growth `<= +10 MiB`, tool list `<= 5,120` bytes, identical clone builds); anything not met is said as not met with the figure.
