# M01 — Measured Linux foundation

- Status: in_progress
- Approved: 2026-10-23 (G1, as written in the proposal)
- Project: Lico
- Updated: 2026-10-23
- Implementation: M01-P01 measurement may start; no result exists yet
- Next gate: G3 inspection of the M01-P01 report, then G4 for any merge

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
| M01-P04 | OS-keyring-only authentication, verified same-provider accounts and a second provider adapter | [Plan](../../../plans/202610/20261024/20261024_01_M01-P04_accounts_and_second_provider_plan.md) | Not produced | Not produced | None | in_progress |
| M01-P05 | Measured resources, failure recovery, public-clone Linux packaging and owner end-to-end use | Not drafted; depends on P04 | Not produced | Not produced | None | not_started |

Phases run sequentially. Future plans are written from earlier evidence and need
separate approval. A later provider that cannot meet storage or isolation rules
is gated as unsupported, rather than advertised through simulated behavior.

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-10-23 | Milestone scope as written |
| G2 phase plan approval | Approved 2026-10-23 for M01-P01, M01-P02 and M01-P03; approved 2026-10-24 for M01-P04 as drafted | Implementation and experiments of those phases as written; not P05 |
| G3 inspection acceptance | Pending | No output bundle or acceptance verdict exists |
| G4 merge approval | Pending | No milestone or phase merge authorized |

G1 and G2 were approved together on 2026-10-23. Later phase plans need their own
approval. Setup/publication permission is not phase or merge approval, and
approval of one gate does not carry to the next. No automatic-merge lane is
inherited.

## Blocked on

- G3: owner inspection of the M01-P01 report and bundle once they exist.
- G4: explicit owner approval is required before any milestone or phase merge.
- M01-P02: G2 approved 2026-10-23; its report and inspection (G3) and any merge (G4) are pending.
- M01-P03: G2 approved 2026-10-23; its report and inspection (G3) and any merge (G4) are pending.
- M01-P04: G2 approved 2026-10-24 as drafted; its report and inspection (G3) and any merge (G4) are pending.
- M01-P05: a separate plan written from earlier evidence, needing approval.

## Out of scope

Windows implementation is a subsequent milestone with its own credential,
process/windowing and measured acceptance. Full IDE editing, embedded browsing,
remote multi-user access, cloud provisioning, plugin marketplace and a general
DAG scheduler are outside this milestone. No payment, production deployment or
real credentials are required for the architecture experiment.

## When done

Not filled: M01-P01 has been approved and is starting; no phase has produced a result.
