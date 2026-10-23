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
synthetic agent processes (all spawned from one long-lived thread, so none dies with
a request thread), keeps a bounded window of recent output in memory, writes every
record to an owner-only segment log with a retention policy, bounds its queues,
connections, sessions and agent output, and refuses stale, foreign and exited targets
before anything reaches an agent. The primary client (A2) and the fallback (C2)
register their view with the host, keep a draft per session and send input with
request ids.

Everything is **synthetic**: the agents are a small program that prints deterministic
records and echoes input; no provider, account, credential, billable turn or real
user data is involved, and agents talk over pipes, not a terminal. Measurements ran on
a private virtual X server (Xvfb: X11, no compositor, software rendering) with minimal
clients. Wayland, a real compositor and GPU, input methods other than ibus,
accessibility, a PTY and real terminal emulation, host crash recovery and packaging are
**not covered** (the last two belong to later phases).

## Protocol and host language

Protocol v1 has machine-readable schemas and a complete operation inventory (`hello`,
`status`, `snapshot`, `subscribe`, `unsubscribe`, `session.create`, `session.list`,
`input`, `input.status`, `approval.answer`, `view.attach`, `view.detach`, `resources`);
requests with an unknown field or a wrong type are refused. A reply with an unexpected
field fails conformance only for the operations the tests validate (the oracle-era
operations plus `input`, `input.status`, `view.attach` and `resources`); the other
replies and the `input` and `approval` events are not validated against the schema.
Session ids are random; approval ids are small per-session numbers. The Python
prototype is the reference oracle; four implementations (the oracle, Rust with the
standard library only, Rust with serde_json, C) passed the same conformance suite (two
tests) for the language decision, and the owned host passes it as the fifth host.

The language was decided by measurement (see the
[decision note](../../../design/202610/20261023/00003_m01_p02_protocol/host_language_decision.md)):
the language floor was small in all three native cases (the C core is 0.6 to 3.1 MiB
smaller than Rust and about 350 KB smaller as a binary), and host memory was governed
by how much output is kept in memory, so the owned host keeps a bounded window and
reads older replay from its log. **Rust with serde_json** was chosen for memory safety
in a process that parses authenticated input.

## Results

Targets are copied from the product specification. "met", "missed" and "unverified"
describe this scope only; latency rows carry the X-server-pixel scope of the earlier
phase. Values are from one run at a clean commit (`64783fa`) with the clients attached
to the owned host; an earlier run before the review fixes gave the same picture.

| Target | A2 (primary) | C2 (fallback) |
|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches) | 20.5 met | 42.3 met |
| same, PSS (reported separately) | 15.4 | 28.9 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB | 0.2 met | 0.1 met |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 24.6 met | 14.5 met |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 77.8 met | 60.1 met |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met |
| noisy case CPU (reported separately) | 64.6 | 6.6 |
| cold start, best effort (median of 10, ms) | 90 | 104 |
| PERF-06 continuity, 30 s of 1 KiB records at 100/s, GUI absent 15 s | 3002 records, no gap or duplicate | same |
| Korean composition (4 typing conditions, ibus-hangul) | exact | exact |

No target was missed. The caveats of the earlier report still apply: the clients are
minimal (no selection, scrollback or accessibility), PERF-02 is true by construction
because a client keeps only an id and a cursor per hidden session, idle CPU 0.00 means
zero scheduler ticks over 60 s (a bound), A2's noisy-case CPU is high because it
reshapes visible lines on every frame, and C2 was measured with accessibility off so
its margin is an upper bound. The primary and the fallback were re-measured here
because the clients now speak the real protocol; the other candidates were not (they
still speak the earlier subscription form).

### Host

| Host memory (window 64, 3 repetitions) | RSS | PSS | peak RSS | CPU |
|---|---|---|---|---|
| 10 sessions, history 500, idle | 4.6 MiB | 2.8 MiB | 3.0 MiB | 0.0% |
| 100 sessions, history 500, idle | 20.8 MiB | 19.0 MiB | 18.4 MiB | 0.0% |
| 100 sessions, history 5000, idle | 21.6 MiB | 19.8 MiB | 19.3 MiB | 0.0% |
| 100 sessions, 5 producing, 10 views streaming (10 s) | 21.4 MiB | 19.6 MiB | 19.1 MiB | 4.6% |

The three native cores of the language comparison held 50 to 54 MiB at 100 sessions
(they kept every record in memory); the bounded window brings the owned host to about
21 MiB, and ten times the retained history adds 0.8 MiB, so host memory does not grow
with history. The agent processes are separate and reported apart: 1.7 MiB each for
these idle synthetic agents (16.9 MiB for 10, 168.8 MiB for 100); real provider
programs will cost far more and are outside this phase. The specification sets no host
budget and none was set by the owner; the figures are reported, not judged. "Peak"
below the instantaneous RSS is the kernel high-water mark lagging.

### Continuity, backpressure and input integrity

- **GUI killed with SIGKILL** (both clients): the host kept producing (500 records in
  5 s with no GUI attached), the durable log stayed contiguous, the dead view and its
  subscription were removed by the host, and the restarted GUI was current. The
  restarted client's own counters read 0 gaps, 0 duplicates and 0 resyncs; those are
  its self-reports and are zero by construction after a snapshot attach, so the real
  continuity evidence is the PERF-06 run, where an independent subscriber recorded a
  contiguous sequence while the GUI reattached from a cursor and about 1500 records
  were replayed from disk. "The agent process stayed alive" is trivially true (a GUI
  is not the agent's parent); what the test shows is that the host removed the view.
- **One stalled subscriber**: at 1000 records/s the host produced 1000.2 records/s with
  the stalled subscriber and 1000.0 without (ratio 1.0); the attentive view received
  every record in order; the stalled view was dropped and told to resynchronise. The
  host's output here is paced by the agent, and the test cap was 64 KiB (the default is
  1 MiB), so this shows the drop works rather than the host's ceiling.
- **Input integrity**: 1000 inputs split over two sessions, 480 of them with the reply
  lost and the request sent again; 100 stale-generation and 50 foreign-session
  attempts. Lost 0, duplicated 0, misrouted 0; all 150 refused attempts were refused
  correctly and none reached an agent. A request id is remembered for the last 65536
  inputs of a session; an older id is forgotten and would be dispatched again (a test
  sends more than 4200 and checks the first is still recognised). The three input
  states and an approval round trip are shown on the wire in the bundle; the clients
  read only whether an input was accepted: a refused input returns to its draft, but
  they show no input states and no approvals.
- **A view switched in the middle of a Korean composition** (both clients, real input
  method): the first session received "안녕", the second only "하", and nothing crossed.
  The Qt client resets the input method and lets the answer land in the session being
  left before switching; the winit client has no reset that keeps the input context, so
  it appends the composing text to the old draft and discards the input method's first
  commit afterwards whatever its text. **Known limitation of the winit client**: a key
  typed right after a switch that extends the abandoned composition (a vowel after a
  syllable) can carry the abandoned final consonant into the next syllable of the new
  session as a fragment; the test covers one key sequence per client. The same scenario
  found two real defects before they were fixed, and the review a third (an exact-text
  comparison in the guard that missed a changed commit).

### Hostile requests

35 cases over the local socket and the HTTP listener; 34 were rejected with no effect on
sessions, subscribers or agents, and one is **untested** (the peer-user check: no
second user is available). Cases: foreign and stale targets, an injected request id and
control characters, unknown views, approvals and requests, an unterminated request, a
flood of local connections (11 of 50 extra connections told busy), a flood of sessions
(7 of 10 refused at the cap), a peer that never reads replies (cut off, the host did not
grow), missing, wrong and wrongly-schemed credentials, wrong Host and Origin (including a
cross-site mutation), unknown routes, wrong methods, an oversize body, a burst of 300
requests (241 throttled, then normal service), a request trickled in byte by byte (closed
after 13.6 s, past the 10 s deadline), idle connections beyond the HTTP cap, and canaries
planted in sessions, request ids, paths, queries and headers. A scan of argv, the ready
line, standard error, the durable log and every reply found neither the canary nor the
control credential; the canary was never sent as valid input text (the synthetic agent
echoes input into the log by design), so that scan covers the non-input fields only.
Mutating the host (origin check, connection cap, session cap) made the suite fail. In
four entries the "no effect" flag is set by the suite rather than measured. Not
implemented or not tested: the generation is always 1, so the stale-generation case is a
fabricated mismatch; the throttle is global, not per client; any authenticated caller can
create sessions up to the cap; there is no total disk cap (retention is by record count);
the log is written but never fsynced, so it survives a killed GUI or host process but
not a power cut, and a write error ends that session; the log directory is cleared at
every start unless it holds anything but session segments; drafts are kept in memory and
a killed GUI loses them.

## Independent review

One independent reviewer read the code, the data and the report. It found no blocker and
several major problems, all treated: an agent created through `session.create` would be
killed when the request thread ended (the parent-death signal follows the thread) and is
now spawned from one long-lived thread, with a test that creates a producing session from
a closed connection and checks the agent is gone after the host is killed; reply, approval,
input-channel, agent-output, view and session bounds that were missing; request-id
tracking limited to 4096; a record that could overtake its attach reply and a second
subscription that doubled delivery; a silent log write error and a log directory that
could be wiped; a token file that could be reused; HTTP deadline and throttle ordering;
a client overflow handler that ignored which session it concerned; an input that vanished
when the host refused it; and report and test statements that were stronger than the data
(corrected here). After the fixes every test and experiment was re-run; a second review
round was not run.

## Steps and commits

| Step | Commits | New tests |
|---|---|---|
| Approval record | `b0797e1` | none |
| S01 protocol, oracle, conformance, language comparison and decision | `42ea32f`, `b3b12ee`, `c3f5df9` | 2 (run for five hosts) |
| S02 host core | `97bea50` | 3 |
| S03 control surface | `f99e13b` | 3 |
| S04 client integration | `7ac2640` | 2 (run for two clients) |
| S05 resources, hostile suite, runner, bundle, review fixes | `73d45fc`, `512c1fb`, `64783fa` and the documentation commits | 1 |

Eleven test functions (2, 3, 3, 2, 1), as approved, and 41 runs in the whole suite
including the earlier phase; 41 passed on the final tree (about 165 s). The review fixes
extended existing tests instead of adding new ones. The P02 tests check the host from
outside (the durable log read directly from disk, `/proc`, the host's own counters) with
some timing-based waits that can be flaky on a loaded machine; several assertions of the
lifecycle tests of the earlier phase and the restarted client's counters are self-reports
of the client under test.

Red evidence: S01 failed to import the schema module and then failed against the
prototype before it was rewritten; S02 failed because the host binary did not exist; S03
failed with `unknown_op` before the operations existed. **S04 and S05 have no
red-before-code run**: the client changes were written before their tests, and the
resource-report test was first run against an existing implementation (its first red was
a race in the test itself, fixed by using idle sessions). Red was observed by mutation for
the tests, among them a log that skips a record, a queue that never overflows, a retention
that is ignored, a generation that is not checked, a duplicate that is dispatched again,
an origin or credential that is not checked, a dead view that stays registered, a Qt draft
that loses the composing text, an old composition that is not recognised, a resource
report with halved log bytes or a wrong agent pid, request validation that is skipped, an
agent spawned from the request thread, no subscriber dedup, a 4096-entry tracking window,
and no hard cap on queued replies; one mutation (no socket shutdown for a peer that never
reads) was **not** detected by its test. These mutation outputs are local scratch files,
not part of the public source. The peer-user check, the HTTP throttle and the connection
caps have no pytest test of their own (the hostile suite exercises the caps and the
throttle).

## Deviations and corrections

- The commit guard of the repository (a local hook) was extended with the P02 branch
  prefix after the P02 approval; its other checks are unchanged.
- The language comparison needed two fixture corrections (a connection reset on an
  oversize request counts as closed; a log-size comparison needs idle sessions).
- Retention keeps at least the configured number of records and trims whole segments, so
  a host retains up to a quarter more than configured.
- `session.create` and the synthetic agents use pipes; non-producing preloaded sessions
  are idle agents that stay alive to accept input, so an exited session exists only after
  `session.create` with no rate.
- The receipts carry binary SHA-256 values, but the binaries come from an ignored build
  directory, so the commit alone does not prove which binary ran; the hashes do.

## Measurement runs

| Run | Commit | Content |
|---|---|---|
| GUI | `64783fa` | all experiments for A2 and C2 against the owned host, including Korean input |
| P02 | `64783fa` | host memory, backpressure, input integrity, CLI transcripts, input states, the hostile suite, GUI kill and view switch for both clients, screens |

Both started from a clean tree; the receipts carry the command, commit, binary SHA-256
values and workload. Raw samples stay local until the review is done. An earlier pair of
runs at `512c1fb` (before the review fixes) is preserved locally and is superseded.

## Decision

Nothing was missed, so no budget decision is required. The recommendation of the earlier
phase stands: A2 is the primary candidate and C2 the fallback; the renderer is locked at
the end of M01-P03, when the installed agent path is exercised with Korean input on a real
display. The winit client's composition limitation above is one reason to keep the
fallback measured and working. The host decision is made: Rust with serde_json, memory
bounded by design.

## Open items for the owner

- Inspection of the local bundle (G3) and acceptance of M01-P01 (G3, G4).
- M01-P03 needs its own plan and approval; it will let real agent output reach the durable
  log, so its plan must re-check the personal-data controls, and it needs a PTY and terminal
  emulation. It should also decide how drafts survive a GUI crash, and whether a second
  review round of the fixes is wanted.
- Whether the HTTP listener, which no client uses yet, stays in the first release.
- Publishing the branch and opening the phase pull request (the development session has no
  repository credentials).
