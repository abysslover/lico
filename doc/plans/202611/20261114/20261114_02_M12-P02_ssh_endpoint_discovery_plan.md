# M12-P02 Plan — The Same Discovery on an ssh Host

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M12-P02-ssh-endpoint-discovery`
- Milestone: [M12](../../../milestones/202611/20261114/20261114_01_M12_local_llm_endpoint_discovery_milestone.md)

## Decisions

- The probe runs as a self-contained Python 3 script through `run_ssh_bounded` with its own time budget (internal deadline 12 s, 16 probes in parallel, 1 s per request, a hard cap on sockets probed, an output cap, `LICO_JSON=` and `LICO_ERR=` lines). The proc root comes from an argument the host adds only when `--llm-proc-root` is set. Linux hosts only; others answer `supported: false`.
- Clean messages and never a hang: `ssh_unreachable`, `remote_python_missing` (`command -v python3` first), `remote_timeout`, `remote_output_too_large`, `unsupported_os`.
- Each endpoint says `bind`: `loopback` means a later use needs a tunnel, `all` does not.
- CLI `--ssh-host`, the dialog's host chooser and the tool's `host` use it; the host name goes through `valid_ssh_host`.

## Steps

- [ ] S01: Red tests with fake hosts (`gpu-box`, `offline-box`, `hang-box`, `nopy-box`, `slow-box`, `huge-box`, `bsd-box`) and a parity test against the local engine.
- [ ] S02: The remote script and its runner; error mapping.
- [ ] S03: CLI, dialog host chooser, tool `host`.
- [ ] S04: Report, mailbox.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The remote script and the local engine give the same endpoints for the same fake servers | parity test |
| 2 | Every failure case ends inside its budget with a clean message | host tests with fake hosts |
| 3 | The result says per endpoint whether it listens on loopback only | host tests |
