# M03-P04 Plan: Control-surface capabilities parity, loopback HTTP completion, memory pass, and release review

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-11
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Phase: `M03-P04` (`phase/M03-P04-control-parity-and-review`)
- Approval: G1 and G2 Option A approved by the owner on 2026-11-11.

## Why

With `M03-P01` through `M03-P03` completing the Linux administrator window (`lico gui`), agents and scripts driving Lico over the command line (`lico`), local owner-only socket (`ipc`), or optional loopback HTTP listener (`127.0.0.1`) need two remaining pieces for complete control-surface parity:
1. **Machine-readable `capabilities` inventory**: A single read-only operation (`lico capabilities`, `GET /v1/capabilities`, and socket `{"op": "capabilities"}`) reporting the protocol version (`1`), package version (`0.1.0`), build commit, full `ops` inventory, `socket_only_ops`, loopback `http_routes`, `mcp_tools`, and runtime `limits`.
2. **Loopback HTTP non-streaming parity**: Alongside the convenience REST routes (`GET /v1/capabilities`, `GET/POST /v1/accounts`, `GET /v1/sessions/<id>/history`, `GET /v1/search`, and existing session/settings/approval/view routes), `POST /v1` dispatches every non-streaming v1 operation through the shared IPC dispatcher while strictly refusing socket-bound streaming/attachment operations (`subscribe`, `unsubscribe`, `view.attach`, `clipboard.write`, `clipboard.done`, `picture.slot`, and `input` carrying `attachment`).

As the closing phase of `M03`, `M03-P04` also re-verifies the full cold test suite, idle GUI RSS (`< 50.0 MiB` at scales 1 and 2), 10-minute hidden growth (`<= +10.0 MiB`), and clean two-clone `lico 0.1.0` reproducible build (`suite acceptance`), and installs the standalone release binary to the local user `bin/lico` path.

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M03-P04` plan and link it in the `M03` milestone and `doc/index_4.md` | Guard checks pass |
| S02 | Add `capabilities` across `core.rs`, `admin.rs`, `ipc.rs`, `http.rs`, `cli.rs`, and `src/lico_protocol/v1.py`; extract shared non-streaming `ipc::dispatch_request` for `POST /v1`; add `GET/POST /v1/accounts`, `GET /v1/sessions/<id>/history`, and `GET /v1/search`; add `tests/test_control_parity.py` | `tests/test_control_parity.py`, `tests/test_protocol_conformance.py`, `tests/test_host_control.py`, and `tests/test_size_ratchet.py` pass (`core.rs <= 988` lines) |
| S03 | Run full `cargo test` under `umask 022` and `umask 077`, `suite run --cold` (`<= 180.0 s`, `<= 2,048 MiB`), `idle_memory` at scales 1 and 2 (`< 50.0 MiB`), `hidden_growth` (`<= +10.0 MiB`), clean two-clone `suite acceptance` at `0.1.0`, and atomic release install to local user `bin/lico` | Zero failures; byte-for-byte identical two-clone `lico 0.1.0` sha256; RSS budgets satisfied |
| S04 | Generate manual inspection bundle `00010_M03-P04_control_parity_and_review/`, write the `M03-P04` report, update `M03` milestone, `doc/manual_end_to_end.md`, `PLAN.md`, `README.md`, `roadmap.md`, and `doc/index_4.md`, and commit | Pre-commit guards pass |

## Personal data and security

- Loopback HTTP remains strictly bound to `127.0.0.1`, gated by exact `Host` and `Origin` checks, constant-time `Bearer` token comparison, failed-auth rate limiting, and `MAX_BODY` caps before routing.
- Socket-only operations and local file attachments (`input` with `attachment`, `clipboard.write` with `path`) are refused over HTTP (`400 bad_request`), preserving the local-socket boundary.
