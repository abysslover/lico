# M01 — Measured Linux foundation

- Status: done
- Approved: 2026-10-23 (G1, as written in the proposal; M01-P08 G2 approved 2026-11-11)
- Project: Lico
- Updated: 2026-11-12
- Implementation: M01-P01 to M01-P08 (including M01-P07a) are built, reported, accepted by the owner at G3 (2026-11-11 and 2026-11-12), and merged and published on `main` at G4 (2026-11-12)
- Next gate: None (milestone achieved and closed on 2026-11-12)
- Milestone branch: `milestone/M01-measured-linux-foundation` (created at the closing commit on `main`; the phase branches are kept)

## Goal

Establish a measured Linux foundation for a lightweight workbench for agentic
workflows, with execution independent of presentation, supported provider paths,
OS-keyring-only application secrets and isolated accounts.

## Done when

1. The first architecture experiment reports process-tree RSS and PSS, hidden-view
   growth, compositor-visible activation and restart latency before selecting a
   renderer or host integration. Each missed budget returns an explicit design
   decision; a target is never converted to a result.
2. Synthetic host work and bounded output continue while the GUI is hidden,
   destroyed or restarted. Snapshot/cursor recovery proves continuity without
   replaying the complete transcript or retargeting ambiguous input.
3. At least one installed official provider path is tested with its exact version,
   documented authentication limits and real input/approval/reconnect evidence.
   Mock and installed-provider results remain separate.
4. Lico-owned secrets use only the OS keyring; a locked or missing store fails
   closed. Same-provider account A/B routing, refresh and logout are independent,
   with explicit per-session bindings and no silent account or billing fallback.
5. A clean public clone can produce the reviewed Linux package without private
   repositories. Independent review, relevant automated checks, an inspectable
   bundle and the owner's final use support the release decision.

These are proposed acceptance conditions, not completed capabilities. The
[product specification](../../../product/202610/20261023/00001_product_spec/product_spec.md)
defines the full requirements and proposed budgets.

## Why now

Early process and presentation choices determine whether hidden sessions can
remain useful while GUI memory stays bounded. The first approved phase therefore
measures before choosing a framework. The initial source and gate contract also
makes account isolation and public-source privacy visible before implementation.
The [initial primary-source review](../../../design/202610/20261023/00001_bootstrap_design/research.md)
informs this division; it does not establish runtime compatibility.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M01-P01 | Bounded architecture comparison, raw resource table and a proposed renderer/host decision | [Plan](../../../plans/202610/20261023/20261023_01_M01-P01_architecture_measurement_plan.md) | [Report](../../../reports/202610/20261023/20261023_01_M01-P01_architecture_measurement_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P02 | Persistent synthetic host, canonical control surface and UI hide/exit/reattach continuity | [Plan](../../../plans/202610/20261023/20261023_02_M01-P02_persistent_host_control_surface_plan.md) | [Report](../../../reports/202610/20261023/20261023_02_M01-P02_persistent_host_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P03 | One real supported agent, explicit target/input/approval/reconnect and Unicode/IME evidence | [Plan](../../../plans/202610/20261023/20261023_03_M01-P03_first_real_agent_plan.md) | [Report](../../../reports/202610/20261024/20261024_01_M01-P03_first_real_agent_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P04 | OS-keyring-only authentication, verified same-provider accounts and a second provider adapter | [Plan](../../../plans/202610/20261024/20261024_01_M01-P04_accounts_and_second_provider_plan.md) | [Report](../../../reports/202610/20261024/20261024_02_M01-P04_accounts_and_second_provider_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P05 | Measured resources, failure recovery, public-clone Linux packaging and owner end-to-end use | [Plan](../../../plans/202610/20261024/20261024_02_M01-P05_workflow_recovery_and_package_plan.md) | [Report](../../../reports/202610/20261024/20261024_03_M01-P05_workflow_recovery_and_single_executable_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P06 | Session creation, accounts and provider sign-in inside the window | [Plan](../../../plans/202610/20261024/20261024_03_M01-P06_in_window_login_and_session_creation_plan.md) | [Report](../../../reports/202610/20261025/20261025_01_M01-P06_in_window_login_and_session_creation_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P07 | The window shows its first view and every panel, and the documented keys act (fixes the blank window after Ctrl+N) | [Plan](../../../plans/202610/20261025/20261025_01_M01-P07_window_first_view_and_keys_plan.md) | [Report](../../../reports/202610/20261025/20261025_02_M01-P07_window_first_view_and_keys_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P07a | Comparison of lightweight Rust UI toolkits (egui, iced, Slint) against the hand-drawn client on the unchanged 50 MiB budget | [Plan](../../../plans/202610/20261025/20261025_02_M01-P07a_ui_toolkit_comparison_plan.md) | [Report](../../../reports/202610/20261025/20261025_03_M01-P07a_ui_toolkit_comparison_report.md) | Owner G3: accept (2026-11-11) | Fast-forward `main` | done |
| M01-P08 | Acceptance pack: a clean public clone rebuilds the package; one index maps every "Done when" item of this milestone to its evidence; test-suite optimization and `0.1.0` version bump | [Plan](../../../plans/202611/20261111/20261111_04_M01-P08_acceptance_pack_plan.md) | [Report](../../../reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |

Phases run sequentially. Future plans are written from earlier evidence and need
separate approval. A later provider that cannot meet storage or isolation rules
is gated as unsupported, rather than advertised through simulated behavior.

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-10-23 | Milestone scope as written |
| G2 phase plan approval | Approved for M01-P01 to M01-P08 (M01-P08 approved 2026-11-11) | Implementation and experiments of those phases as written |
| G3 inspection acceptance | Approved 2026-11-11 and 2026-11-12 (all phases M01-P01 to M01-P08) | Owner accepted M01-P01 to M01-P07a on 2026-11-11 and M01-P08 on 2026-11-12 |
| G4 merge approval | Approved 2026-10-25, 2026-10-27, 2026-11-11 and 2026-11-12 (merged and published) | `main` fast-forwarded, `milestone/M01-measured-linux-foundation` created, and published on 2026-11-12 |

G1 and G2 were approved together on 2026-10-23, and M01-P08 G2 was approved on
2026-11-11. Setup/publication permission is not phase or merge approval, and
approval of one gate does not carry to the next. No automatic-merge lane is
inherited.

## Blocked on

None. M01 achieved and closed on 2026-11-12.

## Out of scope

Windows implementation is a subsequent milestone with its own credential,
process/windowing and measured acceptance. Full IDE editing, embedded browsing,
remote multi-user access, cloud provisioning, plugin marketplace and a general
DAG scheduler are outside this milestone. No payment, production deployment or
real credentials are required for the architecture experiment.

## When done

Closed on 2026-11-12 at branch `milestone/M01-measured-linux-foundation` after the owner's G3 inspection acceptance and G4 merge and publication approval:
- **Delivered (`5/5` M01 "Done when" items verified across `M01-P01` to `M01-P08`):** measured architecture and toolkit comparisons (`M01-P01` and `M01-P07a`), persistent session host and snapshot/cursor continuity (`M01-P02`), official Claude Code and Codex adapters with OS-keyring-only credentials and per-session account isolation (`M01-P03`, `M01-P04`, `M01-P06`), first-view window rendering (`M01-P07`), single self-contained `lico 0.1.0` executable verified byte-for-byte identical across two clean public clones (`M01-P05` and `M01-P08`), and cold test-suite speedup (`167.7 s -> 92.9 s`).
