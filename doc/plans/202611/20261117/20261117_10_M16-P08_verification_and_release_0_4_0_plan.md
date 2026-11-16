# M16-P08 Plan — Verification and Release (`lico 0.4.0`)

- Status: approved by the lead under the owner's delegation (the owner away), 2026-11-17: mail 139 section 2 names the content and mail 141 says P08 goes on without a stop after P07 and ends with a `done` file; in progress
- Version: 1.0.0
- Branch: `phase/M16-P08-verification-and-0-4-0` (stacked on the P07 tip `0c6d054`)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)

## Objective

Release acceptance once and whole, as M15-P04 did it, with numbers only as measured: the version `0.4.0` (the first release with the Local LLM session); three cold runs of everything with the margin to `2,048 MiB` and `180 s`; the receipts matching; the idle window and hidden growth; the Python and the Rust two-clone acceptance; the tool-list bytes; the binary shown free of the test support; CHANGELOG, PLAN, roadmap, README, manual, index; the release artifact built from the clean commit and installed to `~/bin/lico`, keeping the `0.3.3` binary at `private/release_0_3_3_backup/lico`.

## Steps

- [x] 1. The question file about the `test_adapter_contract` pin (report 123), written BEFORE the three runs. The test and the pin are not touched here unless the lead answers A.
- [x] 2. The version: the two tests that name `0.3.3` change to `0.4.0` first (red), then `Cargo.toml` and `Cargo.lock` in the same commit.
- [x] 3. On the clean committed tree, one after the other, each waiting for the load average to fall under 12: Rust units under `umask 022` and `077`; `cargo xtask check`, `verify` and `system`; the tool-list bytes; the binary free of the test support.
- [x] 4. Three cold full runs (`suite run --cold`): tests, seconds, peak, margins; `test_adapter_contract` is named as the known red; a failure in a load-sensitive test on a loaded host is not counted and is said.
- [x] 5. Idle window RSS at scale 1 and 2 (3 launches each) and hidden growth (3 repetitions), with `lico_measure run --candidates a2 --ui --menu --accounts 10 --realistic-text`.
- [x] 6. `suite acceptance` (Python) and `cargo xtask accept` (Rust) on the final commit: the byte-identical two-clone build.
- [x] 7. Documents: CHANGELOG `0.4.0`, PLAN, roadmap, README, manual, index, milestone and plan statuses; what is left over for the owner (a masked key entry in the window; the two notes without a picture; the pin).
- [ ] 8. Build with `build-release.sh` from the clean commit, hash, keep the `0.3.3` binary at `private/release_0_3_3_backup/lico`, install to `~/bin/lico` (copy beside, then rename), run `lico --version` and a smoke capture on a private display.
- [ ] 9. Report and the `done` mail, and a STOP: the milestone is closed by the lead; push, pull request and merge stay with the owner.

## Estimate

3 to 4 USD (the work is runs and documents; the three full runs and the clean builds are the cost of time, not of tokens). If the phase has used about 6 USD and is not near done, I stop at a green commit and write a `question` file.

## Acceptance Criteria

Every check above has a measured result next to its budget (`<= 180.0 s`, `<= 2,048.0 MiB`, idle `< 50 MiB`, hidden growth `<= +10 MiB`, tool list `<= 5,120` bytes, identical clone builds); anything not met is said as not met with the figure.
