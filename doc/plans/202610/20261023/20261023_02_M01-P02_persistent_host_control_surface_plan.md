# M01-P02 — Persistent host, control surface and UI continuity (2026-10-23)

> Status: draft
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P02-persistent-host` (not created)
> Motivation: turn the measured prototype into the owned session host and its canonical control surface, so a GUI can exit, crash or return while synthetic work continues.
> Approval: G2 for this plan pending. M01-P01 gates: G3 pending, G4 pending.

## Basis recorded from M01-P01

The owner's direction after the P01 report: follow its recommendation (the owned
client on winit, softbuffer and cosmic-text as the primary candidate, the Qt
client with its GL integration off as the measured fallback) and **do not relax
any product target**; the GUI idle RSS target stays at 50 MiB of process-tree RSS.
The renderer is locked at the end of M01-P03, so this phase stays renderer
independent and keeps both clients working against the real host.

Evidence this plan is written from (see the P01 [report](../../reports/202610/20261023/20261023_01_M01-P01_architecture_measurement_report.md)):

- The Python host in `src/lico_host` is a prototype and becomes the **reference
  oracle** for conformance tests; it is not shipped.
- Defects P01 found that the contract here must make impossible: a reactivated
  view without a snapshot is blank; a stalled subscriber must be resynchronised,
  not left stuck; a client must handle both input-method commits and unfiltered
  key text (a lost-key defect was found once).
- Not yet covered and carried into this plan: input drafts and composition state
  when a view is switched mid-composition; unguessable ids (the prototype derives
  them); a connection cap and throttle policy.

## Proposed outcome

An owned session host with a versioned, authenticated control protocol, a CLI and
an optional loopback REST listener; durable sequenced output with a retention
policy; snapshot plus cursor reattach; target and generation validated input with
request ids; view attach and detach; a resource report. The GUI clients (primary
and fallback) attach and detach through it. A GUI killed, exited or restarted does
not stop the synthetic agent processes, and no output is lost or duplicated.

The proposals below are decisions to approve together with this plan:

- The host language is **decided by measurement at the first step**, not now.
  Rust is the working assumption (memory safety for a process that parses
  authenticated input, shared code with the owned client). A minimal socket server
  measured in P01 preparation gave a floor of 1.8 MiB RSS and 308 KB for Rust
  (size-optimized) and 1.1 MiB and 14 KB for C, so the language floor is small
  either way; the real host adds parsing, a runtime and a log. Dependencies are a
  small, pinned set announced before download (candidates: a serialization crate,
  a command-line parser, a minimal async runtime); exact versions and licenses
  are recorded at the first step.
- Host memory is governed by design, not language: per-session memory holds only
  a bounded recent window of records, older output is read from the durable log,
  subscriber queues and snapshots are bounded, and the host's RSS at 10 and at 100
  sessions is reported. The product specification sets no host budget; none is
  invented here. If the owner sets one before or during the phase it becomes a
  target and a miss stops the phase like any other.
- The synthetic agent is a small program under `src/` that emits sequenced
  records, answers input lines and raises synthetic approval prompts. No provider
  CLI, account, credential, billable turn or real user data is used.
- A host that restarts or crashes recovering its sessions, packaging and the
  real terminal emulation of provider programs are **not** in this phase.

## Proposed steps

| ID | Work after approval | Meaningful checks (proposed; not present yet) |
|---|---|---|
| S01 | Write protocol v1 (messages, opaque random ids, generations, cursors, snapshots, errors, size limits), explicit response schemas and the complete route inventory; a conformance suite run against the Python oracle; build the protocol core three ways (Rust with the standard library only, Rust with the proposed crates, C) and report binary size and idle and loaded RSS of each, then record the language decision with its evidence and any owner-set budget | `test_protocol_rejects_unknown_fields_and_oversize`; `test_cursor_contract_no_gap_no_duplicate` (an extra or missing field, a gap or a duplicate makes them fail) |
| S02 | Rust host core: session registry, ownership of synthetic agent processes, bounded queues with backpressure, a durable sequenced log with a retention policy, snapshot generation | `test_output_persists_and_stays_sequenced_without_gui`; `test_slow_subscriber_cannot_stall_host`; `test_retention_boundary_reports_gap_and_snapshot` |
| S03 | Control surface: owner-only local IPC, the `lico` CLI, optional loopback REST with a control credential and exact Host/Origin handling; input with request id and target generation (accepted, submitted, observed shown separately); approval responses; view attach and detach; resource report | `test_stale_target_input_is_refused_without_dispatch`; `test_reconnect_never_resends_ambiguous_input`; `test_http_requires_credential_host_and_origin` |
| S04 | Client integration: both clients speak the real protocol; GUI exit, kill and restart against the real host; per-session input draft and composition state across view switches, defined and tested | `test_gui_kill_and_reattach_keeps_agent_and_sequence`; `test_view_switch_never_sends_composition_to_another_session` |
| S05 | Re-measure the targets with the real host and clients, run the hostile-input suite, produce the phase report and the inspection bundle | `test_resource_report_matches_process_scope` |

Tests are named here to define failure behavior; none exists until approved work
runs. Each executed step observes red, implements the minimum, observes green and
commits only a green result. Reuse existing checks; 1–3 new core tests per step.
This plan proposes 11 in total and asks the owner whether "at most five" in the
P01 plan was per step or per phase; the actual count is reported.

## Measurement contract and acceptance

All thresholds are **targets copied from the product specification**. None has
been met by this phase yet. Observe first, then classify each as met, missed or
unverified with its scope. A missed target stops the phase and returns a decision
with at least three alternatives; no target is relaxed and no heavy framework is
chosen automatically.

| Criterion | Workload and required evidence | Fails or stays unverified when |
|---|---|---|
| GUI idle RSS ≤ 50 MiB (PERF-01) | Primary and fallback clients on the real protocol, one visible session at 1200x800, process-tree RSS with PSS beside it, three independent launches | any descendant omitted, RSS above 50 MiB, or the P01 numbers cannot be reproduced |
| Hidden growth ≤ 10 MiB, 10 to 100 sessions (PERF-02) | same view; the client now holds a per-session navigator entry so the cost is real | the delta is measured without session metadata, or exceeds the target |
| Activation p95 ≤ 100 ms and restart p95 ≤ 500 ms (PERF-03, PERF-04) | at least 100 observations each against the real host, read from the display, with active hidden output | fewer than 100 samples, API-only timing, or a target missed |
| Idle GUI CPU ≤ 0.5% (PERF-05) | declared 60 s interval with liveness checked; noisy case separate | the process tree changed during the interval |
| Output continuity (PERF-06) | 1 KiB records at 100 records/s for 30 s; the GUI killed with SIGKILL for 15 s; the agent process id unchanged; the durable log and the reattached view contiguous | any gap, duplicate, restarted agent or stalled host |
| Backpressure | a deliberately slow subscriber while another view keeps up; host output rate unchanged | the host or another subscriber stalls |
| Input integrity | at least 1000 synthetic inputs with induced reconnects and stale targets; zero duplicated, lost or misrouted | any ambiguous input resent or sent to a stale target |
| Korean composition | the P01 composition test (four conditions, ibus-hangul in a private session) against the real host path, plus a defined behavior for a view switched mid-composition | text differs from the keys, committed text shrinks, or composition reaches another session |
| Host resources | host idle and peak RSS and PSS at 10 and at 100 sessions, with and without producing sessions, reported separately; the language comparison from the first step; no target is proposed (an owner-set budget becomes one) | a missing value shown as zero, or the host grows with the retained history instead of the bounded window |

p95 uses a monotonic clock and nearest rank with the sample count published. GUI,
host, agent and display-server costs are scoped separately. Raw samples stay local
until review; sanitized aggregates and the generating command are published after.

## Personal data & security

This phase adds endpoints and persists data, so the controls apply: explicit
response schemas (S1); minimum ephemeral synthetic metadata (S2); target and
generation authorization on every mutation (S3, account binding is a fixed
synthetic placeholder here and becomes real in P04); opaque random ids, not
derived from an index (S4); canaries absent from argv, URLs, logs, errors **and the
durable event log** (S5, S7); masked labels (S6); a complete route inventory with
a test that anything undocumented is refused (S8); bounded requests, a connection
cap, timeouts and an explicit throttle policy (S9); owner-only authenticated IPC
and a protected loopback listener with exact Host and Origin handling (S11).
Webhooks and biometric data do not exist (S10, S12). No shared private helper is
imported.

Persisted data is synthetic output only: the durable log is a private directory
with owner-only modes, a documented retention limit and no real user data. A later
phase that lets real agent output reach the log re-checks this plan first.

Hostile-input cases: foreign target, stale generation, absent or wrong control
credential, guessed id, wrong Host and Origin, oversize and slow requests, a peer
of another user, and a canary in every collected sink. A rejected mutation must
not dispatch work, and each case is shown in the inspection bundle.

## Manual inspection

Proposed local bundle: `private/manual_inspection/202610/YYYYMMDD/001_M01-P02_persistent_host/`
(date chosen when the approved phase runs). It shows CLI transcripts (status, list,
watch, send, view attach and detach), the GUI killed and reattached with sequence
counters side by side, the hidden-to-visible transition, input and approval
exchanges with their three states, the Korean input transcripts, the resource
tables with host memory, and the rejected hostile exchanges as an attacker's view.
A screenshot alone does not prove input, continuity or latency.

It looks right when the correct session and account stay identifiable, hidden work
continues, a restarted GUI shows current content, and no input lands in a stale
target. The bundle is generated by a public command under `src/` from a clean
commit, with its command, exact commit, binary hashes and workload in a receipt.
An independent reviewer checks the measurements; the inspection verdict stays
pending until actually observed.

## Out of scope

Real provider agents, login, accounts, keyring storage and any billable turn
(P03, P04); host crash recovery, packaging and a clean-clone release (P05);
terminal emulation of provider programs; remote access; Windows. M01-P03 to P05
need their own plans and approvals.

## Progress

- 2026-10-23: Draft prepared from the P01 evidence. No branch, implementation,
  test or experiment exists for this phase. G2 for this plan, and G3 and G4 for P01,
  are pending.
