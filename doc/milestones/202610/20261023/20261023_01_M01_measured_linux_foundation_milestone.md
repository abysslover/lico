# M01 — Measured Linux foundation

- Status: in_progress
- Approved: 2026-10-23 (G1, as written in the proposal)
- Project: Lico
- Updated: 2026-10-27
- Implementation: M01-P01 to M01-P07 and the comparison M01-P07a have reports (all `in_review`, inspection verdicts pending); M01-P08 (acceptance pack) is not started
- Next gate: G3 inspection of the M01 reports and bundles (G4 was given for two moves of `main`, on 2026-10-25 and 2026-10-27, each without G3; since 2026-10-27 G4 is asked for each finished phase on its own); the account owner's first real sign-in and end-to-end use are still to come
- Milestone branch: `milestone/M01-measured-linux-foundation`, created at the closing commit when the milestone is achieved (not yet; the phase branches are kept)

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
| M01-P01 | Bounded architecture comparison, raw resource table and a proposed renderer/host decision | [Plan](../../../plans/202610/20261023/20261023_01_M01-P01_architecture_measurement_plan.md) | [Report](../../../reports/202610/20261023/20261023_01_M01-P01_architecture_measurement_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P02 | Persistent synthetic host, canonical control surface and UI hide/exit/reattach continuity | [Plan](../../../plans/202610/20261023/20261023_02_M01-P02_persistent_host_control_surface_plan.md) | [Report](../../../reports/202610/20261023/20261023_02_M01-P02_persistent_host_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P03 | One real supported agent, explicit target/input/approval/reconnect and Unicode/IME evidence | [Plan](../../../plans/202610/20261023/20261023_03_M01-P03_first_real_agent_plan.md) | [Report](../../../reports/202610/20261024/20261024_01_M01-P03_first_real_agent_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P04 | OS-keyring-only authentication, verified same-provider accounts and a second provider adapter | [Plan](../../../plans/202610/20261024/20261024_01_M01-P04_accounts_and_second_provider_plan.md) | [Report](../../../reports/202610/20261024/20261024_02_M01-P04_accounts_and_second_provider_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P05 | Measured resources, failure recovery, public-clone Linux packaging and owner end-to-end use | [Plan](../../../plans/202610/20261024/20261024_02_M01-P05_workflow_recovery_and_package_plan.md) | [Report](../../../reports/202610/20261024/20261024_03_M01-P05_workflow_recovery_and_single_executable_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P06 | Session creation, accounts and provider sign-in inside the window | [Plan](../../../plans/202610/20261024/20261024_03_M01-P06_in_window_login_and_session_creation_plan.md) | [Report](../../../reports/202610/20261025/20261025_01_M01-P06_in_window_login_and_session_creation_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P07 | The window shows its first view and every panel, and the documented keys act (fixes the blank window after Ctrl+N) | [Plan](../../../plans/202610/20261025/20261025_01_M01-P07_window_first_view_and_keys_plan.md) | [Report](../../../reports/202610/20261025/20261025_02_M01-P07_window_first_view_and_keys_report.md) | Local bundle, verdict pending | None | in_review |
| M01-P07a | Comparison of lightweight Rust UI toolkits (egui, iced, Slint) against the hand-drawn client on the unchanged 50 MiB budget | [Plan](../../../plans/202610/20261025/20261025_02_M01-P07a_ui_toolkit_comparison_plan.md) | `doc/reports/202610/20261025/20261025_03_M01-P07a_ui_toolkit_comparison_report.md` on its phase branch | Awaiting review | None | in_review |
| M01-P08 | Acceptance pack: a clean public clone rebuilds the package; one index maps every "Done when" item of this milestone to its evidence; the owner's end-to-end checklist | Not written | Not produced | Not produced | None | not_started |

Phases run sequentially. Future plans are written from earlier evidence and need
separate approval. A later provider that cannot meet storage or isolation rules
is gated as unsupported, rather than advertised through simulated behavior.

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-10-23 | Milestone scope as written |
| G2 phase plan approval | Approved 2026-10-23 for M01-P01, M01-P02 and M01-P03; approved 2026-10-24 for M01-P04 to M01-P06 as drafted; approved 2026-10-25 for M01-P07 and M01-P07a by the owner's decisions in chat; M01-P08 has no plan yet | Implementation and experiments of those phases as written |
| G3 inspection acceptance | Pending | Reports and local bundles exist for the phases above; every inspection verdict is pending |
| G4 merge approval | Approved for two moves of `main`, otherwise pending | `main` was moved by fast-forward on the owner's approval, without G3: on 2026-10-25 to pull request 1 and M01-P01 to M01-P05, on 2026-10-27 to the finished work up to M02-P02; every other phase merge is asked for on its own when the phase is finished and verified (CONTRIBUTING.md) |

G1 and G2 were approved together on 2026-10-23. Later phase plans need their own
approval. Setup/publication permission is not phase or merge approval, and
approval of one gate does not carry to the next. No automatic-merge lane is
inherited.

## Blocked on

- G3: owner inspection of the reports and bundles of M01-P01 to M01-P07 and M01-P07a; every verdict is pending.
- G4: explicit owner approval is required before any phase merge and is asked for each finished phase (given for two moves of `main`: 2026-10-25 and 2026-10-27).
- "Done when" item 3: an installed official provider path exercised with real sign-in evidence (the account owner's first real login).
- "Done when" item 5: a clean public clone produces the reviewed Linux package (M01-P08), and the account owner has used it end to end.
- M01-P08: its plan is not written; it needs its own approval (G2) before it starts.

## Out of scope

Windows implementation is a subsequent milestone with its own credential,
process/windowing and measured acceptance. Full IDE editing, embedded browsing,
remote multi-user access, cloud provisioning, plugin marketplace and a general
DAG scheduler are outside this milestone. No payment, production deployment or
real credentials are required for the architecture experiment.

## When done

Not filled: M01-P01 to M01-P07 and M01-P07a have reports that wait for the owner's inspection (G3); M01-P08 (acceptance pack) is not started, and the account owner's first real sign-in and end-to-end use are still to come. When the milestone is achieved, the branch `milestone/M01-measured-linux-foundation` is created at its closing commit and named here.
