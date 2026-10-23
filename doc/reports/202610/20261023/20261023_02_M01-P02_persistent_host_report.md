# M01-P02 report — persistent host, control surface and UI continuity

- Status: in_review (inspection verdict pending; the status stays below `success` until the verdict is `accept`)
- Date: 2026-10-23
- Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Plan: [M01-P02](../../../plans/202610/20261023/20261023_02_M01-P02_persistent_host_control_surface_plan.md)
- Branch: `phase/M01-P02-persistent-host`
- PR: not opened
- Inspection bundle: `private/manual_inspection/202610/20261023/001_M01-P02_persistent_host/` (local, verdict `pending`)
- Gates: G2 approved 2026-10-23; G3 pending; G4 pending. M01-P01 gates G3 and G4 are also pending.

## What was built and what that does and does not show

An owned session host in Rust (serde_json is its only dependency) with a versioned
local protocol, a command line, and an optional loopback HTTP listener. It owns
synthetic agent processes, keeps a bounded window of recent output in memory, writes
every record to an owner-only segment log with a retention policy, bounds every
subscriber queue, and refuses stale, foreign and exited targets before anything
reaches an agent. The primary client (A2) and the fallback (C2) register their view
with the host, keep a draft per session and send input with request ids.

Everything is **synthetic**: the agents are a small program that prints
deterministic records and echoes input; no provider, account, credential, billable
turn or real user data is involved, and agents talk over pipes, not a terminal.
Measurements ran on a private virtual X server (Xvfb: X11, no compositor, software
rendering) with minimal clients. Wayland, a real compositor and GPU, input methods
other than ibus, accessibility, a PTY and real terminal emulation, host crash
recovery and packaging are **not covered** (the last two belong to later phases).

## Protocol and host language

Protocol v1 has machine-readable schemas and a complete operation inventory
(`hello`, `status`, `snapshot`, `subscribe`, `unsubscribe`, `session.create`,
`session.list`, `input`, `input.status`, `approval.answer`, `view.attach`,
`view.detach`, `resources`); requests with an unknown field or a wrong type are
refused and a reply with an unexpected field fails the conformance suite.
Session ids are random. The Python prototype is the reference oracle; four
implementations (the oracle, Rust with the standard library only, Rust with
serde_json, C) pass the same conformance suite (two tests).

The language was decided by measurement (see the
[decision note](../../../design/202610/20261023/00003_m01_p02_protocol/host_language_decision.md)):
the language floor was small in all three native cases (the C core is about
350 KB and 1 to 3 MiB smaller than Rust), and host memory was governed by how much
output is kept in memory, so the owned host keeps a bounded window and reads older
replay from its log. **Rust with serde_json** was chosen for memory safety in a
process that parses authenticated input.

## Results

Targets are copied from the product specification. "met", "missed" and
"unverified" describe this scope only; latency rows carry the X-server-pixel scope
of the earlier phase. Values are from one run at a clean commit (`512c1fb`) with the
clients attached to the owned host.

| Target | A2 (primary) | C2 (fallback) |
|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches) | 20.5 met | 42.3 met |
| same, PSS (reported separately) | 15.3 | 28.1 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB | 0.2 met | 0.1 met |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 26.7 met | 11.0 met |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 79.0 met | 59.5 met |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met |
| noisy case CPU (reported separately) | 68.1 | 5.8 |
| cold start, best effort (median of 10, ms) | 101 | 105 |
| PERF-06 continuity, 30 s of 1 KiB records at 100/s, GUI absent 15 s | 3002 records, no gap or duplicate | same |
| Korean composition (4 typing conditions, ibus-hangul) | exact | exact |

No target was missed. The caveats of the earlier report still apply: the clients
are minimal (no selection, scrollback or accessibility), PERF-02 is true by
construction because a client keeps only an id and a cursor per hidden session,
idle CPU 0.00 means zero scheduler ticks over 60 s (a bound), A2's noisy-case CPU
is high because it reshapes visible lines on every frame, and C2 was measured with
accessibility off so its margin is an upper bound. The primary and the fallback were
re-measured here because the clients now speak the real protocol; the other
candidates were not (they still speak the earlier subscription form).

### Host

| Host memory (history 500 records per session, window 64) | RSS | PSS |
|---|---|---|
| 10 sessions, idle | 4.2 MiB | 2.4 MiB |
| 100 sessions, idle | 17.8 MiB | 16.0 MiB |
| 100 sessions, 5 producing, 10 views streaming | 18.3 MiB | 16.5 MiB |

The three native cores of the language comparison held 50 to 54 MiB at 100 sessions
(they kept every record in memory); the bounded window brings the owned host to
17.8 MiB, so host memory no longer grows with retained history. The agent processes
are separate and reported apart: 1.7 MiB each for these idle synthetic agents (16.9
MiB for 10 and 168.8 MiB for 100); real provider programs will cost far more and are
outside this phase. The specification sets no host budget and none was set by the
owner; the figures are reported, not judged.

### Continuity, backpressure and input integrity

- **GUI killed with SIGKILL** (both clients): the same agent process stayed alive,
  the host kept producing (501 records in 5 s with no GUI attached), the durable log
  stayed contiguous, the dead view and its subscription were removed, and the
  restarted GUI was current with 0 gaps, 0 duplicates and 0 resyncs.
- **One stalled subscriber**: at 1000 records/s the host produced 1000.0 records/s
  with the stalled subscriber and 1000.1 without (ratio 1.0); the attentive view
  received all 6001 records in order; the stalled view was dropped and told to
  resynchronise.
- **Input integrity**: 1000 inputs split over two sessions, 480 of them with the
  reply lost and the request sent again; 100 stale-generation and 50
  foreign-session attempts. Lost 0, duplicated 0, misrouted 0; all 150 refused
  attempts were refused correctly and none reached an agent. The three input states
  (accepted, submitted, observed) and an approval round trip are shown on the wire
  in the bundle.
- **A view switched in the middle of a Korean composition** (both clients, real
  input method): the composing text is committed to the session it was typed for and
  the input method's late echo is ignored; the first session received "안녕", the
  second only "하", and nothing crossed. The same scenario found two real defects
  before they were fixed: the Qt client leaked the old composition into the new
  session, and the winit client lost its first key after an input-context reset.

### Hostile requests

32 cases over the local socket and the HTTP listener were each rejected with no
effect on sessions, subscribers or agents: foreign and stale targets, an injected
request id and control characters, unknown views, approvals and requests, an
unterminated request, a flood of local connections and of sessions (both capped),
missing, wrong and wrongly-schemed credentials, wrong Host and Origin (including a
cross-site mutation), unknown routes, wrong methods, an oversize body, a burst of
300 requests (245 throttled, then normal service), idle connections beyond the
HTTP cap, and canaries planted in sessions, request ids, paths, queries and
headers. A scan of argv, the ready line, standard error, the durable log and every
reply found neither the canary nor the control credential. Mutating the host
(origin check, connection cap, session cap) made the suite fail. **Untested**: the
peer-user check (no second user is available); the generation is always 1, so the
stale-generation case is a fabricated mismatch; the throttle is global, not per
client; session creation by any authenticated caller is allowed up to the cap.

## Steps and commits

| Step | Commits | New tests |
|---|---|---|
| Approval record | `b0797e1` | none |
| S01 protocol, oracle, conformance, language comparison and decision | `42ea32f`, `b3b12ee`, `c3f5df9` | 2 (run for five hosts) |
| S02 host core | `97bea50` | 3 |
| S03 control surface | `f99e13b` | 3 |
| S04 client integration | `7ac2640` | 2 (run for two clients) |
| S05 resources, hostile suite, runner, bundle | `73d45fc`, `512c1fb`, and the later commits on the branch | 1 |

Eleven test functions (2, 3, 3, 2, 1), as approved, and 41 runs in the whole suite
including the earlier phase; 41 passed in about 165 s on the final tree. Several
assertions in the lifecycle tests of the earlier phase are self-reports of the
client under test; the P02 tests check the host from outside (the durable log read
directly from disk, `/proc`, the host's own counters).

Red evidence: S01 failed to import the schema module and then failed against the
prototype before it was rewritten; S02 failed because the host binary did not exist;
S03 failed with `unknown_op` before the operations existed. **S04 and S05 have no
red-before-code run**: the client changes were written before their tests, and the
resource-report test was first run against an existing implementation (its first
red was a race in the test itself, a log read while producers were writing, which
was fixed by using idle sessions). Red was observed by mutation for every test:
a log that skips a record, a queue that never overflows, a retention that is
ignored, a generation that is not checked, a duplicate that is dispatched again, an
origin or credential that is not checked, a dead view that stays registered, a Qt
draft that loses the composing text, an old composition that is not recognised, a
resource report with halved log bytes or a wrong agent pid, and request validation
that is skipped. The peer-user check, the HTTP throttle and the connection caps have
no pytest test of their own (the hostile suite exercises the caps and the throttle).

## Deviations and corrections

- The commit guard of the repository was extended with the P02 branch prefix after
  the P02 approval; its other checks are unchanged.
- The language comparison needed two fixture corrections (a connection reset on an
  oversize request counts as closed; a log-size comparison needs idle sessions).
- Retention keeps at least the configured number of records and trims whole segments,
  so a host retains up to a quarter more than configured.
- `session.create` and the synthetic agents use pipes; non-producing sessions are
  idle agents that stay alive to accept input, so an exited session exists only
  after `session.create` with no rate.

## Measurement runs

| Run | Commit | Content |
|---|---|---|
| GUI | `512c1fb` | all experiments for A2 and C2 against the owned host, including Korean input |
| P02 | `512c1fb` | host memory, backpressure, input integrity, CLI transcripts, input states, the hostile suite, GUI kill and view switch for both clients, screens |

Both started from a clean tree; the receipts carry the command, commit, binary
SHA-256 values and workload. Raw samples stay local until the review is done.

## Decision

Nothing was missed, so no budget decision is required. The recommendation of the
earlier phase stands: A2 is the primary candidate and C2 the fallback; the renderer
is locked at the end of M01-P03, when the installed agent path is exercised with
Korean input on a real display. The host decision is made: Rust with serde_json,
memory bounded by design.

## Open items for the owner

- Inspection of the local bundle (G3) and acceptance of M01-P01 (G3, G4).
- M01-P03 needs its own plan and approval; it will let real agent output reach the
  durable log, so its plan must re-check the personal-data controls, and it needs a
  PTY and terminal emulation.
- Whether the HTTP listener, which no client uses yet, stays in the first release.
- Publishing the branch and opening the phase pull request (the development session
  has no repository credentials).
