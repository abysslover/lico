# M01-P01 — Architecture and resource measurement (2026-10-23)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P01-architecture-measurement`
> Motivation: measure lifecycle and resource costs before committing to a renderer.
> Approval: G1 approved 2026-10-23; G2 approved 2026-10-23; G3 pending; G4 pending.

## Proposed outcome

A reproducible Linux architecture comparison, a supported-protocol/license
matrix and a renderer/host recommendation supported by the first measurements.
No renderer, language or host reuse decision is made in this draft. A candidate
may use an existing adapter only if its license, supported protocol, state and
account isolation meet the product contract. Another candidate may use an owned
minimal host. Do not turn a native-toolkit label into a performance result.

## Research before execution

The [initial research note](../../../design/202610/20261023/00001_bootstrap_design/research.md)
records official Linux memory, native OAuth and keyring sources checked on
2026-10-23. After approval, survey official renderer/toolkit and candidate-host
licenses, versions, process model, Linux display/IME support and public build
inputs. Pin the exact candidate revisions. Inspect official provider capability
documentation and installed versions without signing in, copying credentials or
starting billable model turns. Mark anything unavailable as unverified.

## Proposed steps

| ID | Work after approval | Meaningful checks (proposed; not present yet) |
|---|---|---|
| S01 | Record at most two feasible architecture candidates and explicit resource categories; establish a publicly reproducible synthetic workload | `test_resource_process_tree_scope`; omitted GUI helper or double-counted attribution makes it fail |
| S02 | Build the smallest comparable visible/hidden view and a bounded synthetic host stream; lifecycle follows separate host/UI ownership | `test_hidden_view_releases_presentation`; `test_gui_exit_preserves_output_cursor`; retained hidden buffers or lost/duplicated sequence makes them fail |
| S03 | Measure cold/warm start, hidden growth and compositor-visible correct-frame latency; create a versioned report and inspection generator | `test_measurement_report_requires_scope_and_samples`; `test_latency_uses_correct_frame_event`; missing scope/sample or API-only timestamps makes them fail |
| S04 | Compare all results and license/isolation constraints, document one recommendation or a budget decision, then produce the phase report and inspection bundle | `test_architecture_decision_references_observed_budgets`; a recommendation that calls an unmet/unmeasured budget passed makes it fail |

Tests are named here to define failure behavior; no test or implementation is
created by approving the project scaffold. Each executed step observes red,
implements the minimum, observes green and commits only a green result. Reuse
existing checks; add 1–3 core tests per step, at most five. Report the actual count.

## Measurement contract and acceptance

All thresholds below are **targets copied from the product specification**.
None has been measured or achieved. First report the observations; then classify
each target as met, missed or unverified with its scope.

| Criterion | Workload / required evidence | Fails or stays unverified when |
|---|---|---|
| GUI idle RSS target ≤50 MiB | One visible synthetic session at 1200×800; GUI/helper/renderer process tree; PSS reported separately | Any GUI descendant is omitted, a frontend buffer is relabeled host, or RSS exceeds the target |
| Hidden sessions target ≤10 MiB added GUI RSS for 10→100 sessions | Same visible view after hidden view trees, buffers and textures are released; host memory separately reported | Hidden display resources remain resident, workload changes, or delta exceeds target |
| Warm activation target p95 ≤100 ms | At least 100 switches including large history and active hidden output; first correct frame on GUI/compositor path | Only API latency is timed, sample count <100, incorrect frame shown, or target missed |
| Warm GUI restart target p95 ≤500 ms | At least 100 bounded reattach observations under declared warm filesystem; cold start separately measured | Host/agent restarts unexpectedly, filesystem condition is absent, wrong target is shown, or target missed |
| Idle GUI CPU target ≤0.5% of one core | Declared 60 s idle interval; a noisy synthetic-agent case separately reported | Continuous UI polling obscures idle, CPU scope differs, or target missed |
| Output continuity | Synthetic stream: proposed 1 KiB records at 100 records/s for 30 s, bounded and sequenced; GUI absent for a recorded interval | Snapshot/cursor evidence has a gap/duplicate or backpressure stalls the host |
| Full observation inventory | Binary/package size, peak and idle RSS/PSS, process tree, display server, viewport, fonts, scale, hardware category and observable GPU allocation | Missing values are represented as zero or unstated exclusions; unsupported values must be unverified |
| Clean build and license boundaries | Documented public inputs and candidate license/redistribution notes | Build needs another private repo or notices/license compatibility are unknown |

Use a monotonic clock and publish the p95 method and sample counts. Scope GUI,
host, agent and external-service cost separately. RSS sums count resident mappings;
PSS provides proportional shared-page attribution. Record both instead of claiming
either is total machine cost. Retain bounded raw samples locally and publish
sanitized aggregates plus their generation command after review.

A candidate is accepted only with scoped evidence. If neither meets the resource
targets, stop before framework selection and return a concrete decision with at
least three substantive alternatives, their tradeoffs and a recommendation.
Do not relax a target or select a heavy framework automatically. Repeat only a
changed candidate, failed measurement or unresolved mismatch; preserve prior runs.

## Personal data & security

This phase uses synthetic sessions and canary credentials only. No real account,
OAuth login, billable agent turn or inherited credential store is exercised.
Application credentials in later phases remain OS-keyring-only and fail closed.
Official CLI sign-in does not authorize extracting subscription tokens for another
client. Same-provider refresh, profile and session bindings must remain separate.

Relevant controls for an experimental local control surface: explicit response
schemas (S1); minimum ephemeral synthetic metadata (S2); target/account/generation
authorization (S3); opaque IDs (S4); canaries absent from URL/argv/log/errors (S5,
S7); masked labels (S6); a complete route inventory (S8); bounded requests and
explicit throttle policy (S9); authenticated owner-only IPC or protected loopback
with exact Host/Origin handling (S11). Webhooks and biometric data are not in scope
(S10/S12). No shared private helper is imported to apply these controls.

Proposed hostile-input cases: foreign target/account, stale generation, absent
control credential, guessed ID, wrong Host/Origin and planted canary in collected
nonsecret sinks. A rejected mutation must not dispatch work. The phase plan is
rechecked if candidate research adds endpoints or stored user data.

## Manual inspection

Proposed local bundle:
`private/manual_inspection/202610/20261023/000_M01-P01_architecture_measurement/`.
The execution date is chosen when the approved phase runs; no bundle exists now.
It will show two candidates at the same viewport, visible→hidden→reattached state,
large-history and active-output switches, Unicode/Korean IME cases supported by
the candidate, process categories, timing samples and continuity counters.

Looks right when the correct session/account remains identifiable, hidden work
continues, view resources are released and reactivation shows current content
without input landing in a stale target. Include synthetic rejected control
exchanges as an attacker's view. A screenshot alone does not prove IME input or
correct-frame latency.

The approved implementation establishes a public CLI under `src/` that generates
this bundle from a clean commit and writes its command, exact commit and workload
in a receipt. No generator or claimed command exists at draft time. An independent
reviewer checks the measurements; the inspection verdict remains pending until
actually observed. The owner's final product acceptance stays separate.

## Out of scope

Production UI, real-provider feature implementation, live login/account changes,
persistent secrets, packaging a release, Windows implementation and automatic
merges. M01-P02 through P05 require their own plans and approvals.

## Progress

- 2026-10-23: Draft prepared only. No branch, implementation, test, experiment,
  benchmark, report or inspection bundle had run.
- 2026-10-23: G1 and G2 approved as written. Branch created from the reviewed
  documentation commit; this documentation-only commit records the approval.
  No step has run: S01 to S04 are not started. G3 and G4 remain pending.
