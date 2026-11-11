# M03-P04 Report: Control-surface capabilities parity, loopback HTTP completion, memory pass, and M03 release review

- Status: done (owner G3 accepted and G4 merged on 2026-11-12)
- Version: 1.0.0
- Date: 2026-11-11
- Milestone: [M03 — Linux administrator GUI completeness and control-surface parity](../../../milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md)
- Phase: `M03-P04` (`phase/M03-P04-control-parity-and-review`)
- Plan: [M03-P04 plan](../../../plans/202611/20261111/20261111_08_M03-P04_control_parity_and_review_plan.md)

## Summary

`M03-P04` completes Milestone `M03` by bringing the CLI (`lico`), local Unix socket (`ipc`), and optional loopback HTTP listener (`127.0.0.1`) into full machine-discoverable parity alongside the `M03-P01`..`M03-P03` administrator window upgrades, re-verifying every test, memory, hidden-growth, and clean two-clone reproducibility gate at `lico 0.1.0`, and installing the standalone stripped release binary to `~/bin/lico`.

## Delivered changes

1. **Canonical `capabilities` operation (`core.rs`, `admin.rs`, `ipc.rs`, `http.rs`, `cli.rs`, `src/lico_protocol/v1.py`)**:
   - Added read-only `capabilities` (`lico capabilities`, socket `{"op": "capabilities"}`, `GET /v1/capabilities`, and `POST /v1` `{"op": "capabilities"}`) validated by `v1.py`.
   - Reports `protocol` (`1`), `version` (`"0.1.0"`), `commit`, `build`, full `ops` (`46` operations), `socket_only_ops` (`subscribe`, `unsubscribe`, `view.attach`, `clipboard.write`, `clipboard.done`, `picture.slot`), `http_routes` (`22` routes), `mcp_tools` (`9` tools derived directly from `mcp::TOOLS`), and `limits` (`max_request`, `max_input_text`, `max_sessions`, `retain`, `font_px_min`, `font_px_max`).
   - Kept `src/host/hostd/src/core.rs` at `988` lines (`<= 988` size-ratchet ceiling) and `src/host/hostd/src/gui/ui.rs` at `1,115` lines (`<= 1,120`).
2. **Loopback HTTP non-streaming parity (`ipc.rs`, `http.rs`)**:
   - Extracted `ipc::dispatch_request(shared, op, request, via_http)` as the single source of truth shared between the Unix socket and `POST /v1`.
   - Added `POST /v1` (generic v1 JSON operation dispatch with `id` echoing and strict `400 bad_request` refusal of `SOCKET_ONLY_OPS` and `input` carrying `attachment`), `GET /v1/capabilities`, `GET/POST /v1/accounts`, `GET /v1/sessions/<id>/history?before=N[&limit=M]`, and `GET /v1/search?q=...[&session=ID][&limit=N]` (with strict percent/query validation).
3. **Parallel test determinism hardening (`notify.rs`, `ime.py`, `p02.py`, `tests/`)**:
   - Handled transient Linux `ETXTBSY` (`ExecutableFileBusy`) when spawning custom `LICO_NOTIFY_BIN` scripts under multithreaded `cargo test`.
   - Waited for `ibus-x11` registration in `XIM_SERVERS` on the private `Xvfb` root window before starting GUI clients in `InputMethod._start`, and synchronized async receiver/redraw checks in `test_picture_input.py`, `test_editor_selection.py`, and `test_agent_tools.py`.

## Measured verification (`lico 0.1.0`, commit `5a7b00b`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `570 passed; 0 failed` (both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `458` tests in `80` files, `0` failed, **`94.9 s`**, peak **`1,617.2 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.21 MiB` (`23.96..24.21 MiB`), median PSS `17.52 MiB`, anon `3.94 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.38 MiB` (`35.08..35.38 MiB`), median PSS `23.00 MiB`, anon `4.13 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.37 MiB` (`24.27 MiB` -> `24.55 MiB`; host `8.29` -> `20.13 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | `1 passed` in `148.55 s`; identical `lico 0.1.0` binaries across two clean clones | Byte-for-byte match | Met |
| Standalone release binary (`build-release.sh` -> `~/bin/lico`) | `7,940,328` bytes (`sha256: df82e45e6cc093d227aa4df075299e961c5e5eb3d8a487666bf83d747c5b429d`) | Single self-contained executable | Met |

## Gate status

All four phases of Milestone `M03` (`M03-P01` through `M03-P04`) and `M01-P08` (`lico 0.1.0` acceptance pack) are complete on a linear fast-forward branch chain (`main` -> `phase/M01-P08-acceptance-pack` -> `phase/M03-P01-account-and-session-admin` -> `phase/M03-P02-workspace-navigator` -> `phase/M03-P03-editor-and-reflow` -> `phase/M03-P04-control-parity-and-review`) and await owner G3 inspection acceptance and G4 merge/publication approval.
