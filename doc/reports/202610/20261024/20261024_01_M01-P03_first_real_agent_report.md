# M01-P03 report — first real agent path, Unicode and Korean input

- Status: in_review (inspection verdict pending; the status stays below `success` until the verdict is `accept`)
- Date: 2026-10-24
- Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Plan: [M01-P03](../../../plans/202610/20261023/20261023_03_M01-P03_first_real_agent_plan.md)
- Branch: `phase/M01-P03-first-real-agent`
- PR: not opened
- Inspection bundle: `private/manual_inspection/202610/20261024/002_M01-P03_first_real_agent/` (local, verdict `pending`)
- Gates: G2 approved 2026-10-23; G3 pending; G4 pending. M01-P01 and M01-P02 gates G3 and G4 are also pending.

## What was built and what that does and does not show

The owned host now runs a provider session: `session.create` with kind `codex` starts the
provider program (the Codex app-server) in a private per-session workspace with an
environment of `PATH`, `HOME`, `LANG` and `CODEX_HOME` only, speaks its JSON-lines protocol
and maps it onto host records, input states and approvals. An input becomes one turn, or a
steer of the running turn (a steer the provider refuses becomes a turn of its own, never
both); a provider approval request becomes one host approval that is answered at most once,
and every provider request the client cannot serve is answered with an error; a vanished
provider process makes the session `disconnected` (never idle) and `session.resume` starts
a new process on the same thread with the execution generation raised by one; `session.purge`
removes a session's process, log and workspace and nothing else. Provider sessions carry
UTF-8 text lines (an optional `text` field on records and snapshots); both clients show them,
drop control and bidirectional characters again on their own, and keep Korean and mixed-script
input exact. The provider's protocol schema is pinned by hash in a capability record that
lists what is mapped, refused, unsupported and **unverified**.

**No real provider turn was run by this session.** Every provider result below comes from a
protocol simulator that replays recorded, schema-valid messages (a test double). The one
real-provider fact is the idle memory of the installed program, started in a private empty
home with no sign-in and no turn. The real turn is a command the owner runs (see "Open items").
The installed provider version is 0.163.0; sign-in behavior, the approval events of a real
turn and the Claude Code path are **unverified**. Measurements ran on a private virtual X
server (Xvfb: X11, no compositor, software rendering), so Wayland, a real compositor, GPU,
input methods other than ibus-hangul and accessibility are not covered.

## Results

Targets are copied from the product specification. "met", "missed" and "unverified" describe
this scope only. Workload: synthetic sessions that produce agent-shaped UTF-8 lines (Latin,
Korean, ideographs, Hebrew, an emoji, a combining accent and code, about 110 bytes each),
every transcript 10000 lines long; the displayed record is a line, not the 1 KiB hex record
of the earlier phases, so the figures are not a like-for-like change of the same workload.
Values are from one run at a clean commit (`be5b0cd`) with both clients on the owned host.

| Target | A2 (primary) | C2 (fallback) | A2 / C2 in M01-P02 |
|---|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches, max) | 22.2 met | 46.5 met | 20.5 / 42.3 |
| same, PSS (reported separately) | 16.6 | 31.9 | 15.4 / 28.9 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB | 0.08 met | 0.08 met | 0.2 / 0.1 |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 63.2 met | 24.6 met | 24.6 / 14.5 |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 105.5 met | 199.3 met | 77.8 / 60.1 |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met | 0.00 / 0.00 |
| noisy case CPU: visible session producing 100 lines/s (reported separately) | **99.3** | 10.5 | 64.6 / 6.6 |
| cold start, best effort (median of 10, ms) | 123 | 224 | 90 / 104 |
| PERF-06 continuity, 30 s at 100 lines/s, GUI absent 15 s | 3003 records, no gap or duplicate | 3002, same | 3002 / same |
| Korean composition (4 typing conditions, ibus-hangul) | exact | exact | exact |

No target was missed. What the numbers say beyond the table:

- **C2 is now 3.5 MiB below the 50 MiB line** (was 7.7). It was measured with accessibility
  off, so its margin is an upper bound; the A2 margin is 27.8 MiB.
- **A2's noisy case saturates one core** (99.3%, up from 64.6% with hex records). The noisy
  case has no target in the specification and is reported separately, but a client that
  needs a whole core to show 100 lines per second is a real defect of its renderer: it
  reshapes every visible line on every frame. A frame cap or incremental shaping is the
  likely repair; it was **not** built here (see "Open items").
- Activation and restart grew for both clients because the transcript is 10000 lines
  (A2 activation p95 24.6 → 63.2 ms; C2 restart p95 60.1 → 199.3 ms) and stay under their
  targets. PERF-02 is still true by construction: a client keeps only an id and a cursor
  per hidden session.
- Idle CPU 0.00 means zero scheduler ticks over 60 s (a bound, one window).
- A2 after a page of 2350 Hangul and 2360 ideographs: 22.1 → 25.0 MiB (glyph cache growth).

### Host and provider memory

| Host memory (window 64, 3 repetitions) | RSS | PSS | CPU |
|---|---|---|---|
| 10 sessions, 10000-line transcripts, idle | 3.7 MiB | 1.9 MiB | 0.0% |
| 100 sessions, 10000-line transcripts, idle | 12.2 MiB | 10.4 MiB | 0.0% |
| 100 sessions, 5 producing, 10 views streaming (10 s) | 12.8 MiB | 11.0 MiB | 3.1% |

The host figures are lower than in M01-P02 (20.8 MiB at 100 sessions) only because a
record is now about 110 bytes instead of 1 KiB and the window holds 64 records; this is not
an improvement of the host. History is bounded by the window (the 10- and 100-session rows both
hold 10000-line transcripts, and M01-P02 measured that ten times the history added 0.8 MiB). The synthetic agents cost 1.7 MiB each (168.6 MiB for 100) and are
reported apart.

**The real provider program is far heavier**: started and initialized in a private empty
home with no sign-in, no thread and no turn, the installed `codex app-server` (two processes)
held **122 MiB RSS, 85 MiB PSS** (3 independent launches: 121.9, 120.3, 122.0 MiB RSS). A
provider with a conversation holds more. Ten live provider sessions would therefore cost
well over 1 GiB outside Lico's own budget; the specification's budgets apply to the GUI and
the host, and the owner should decide whether a provider-memory policy is wanted.

### Adapter against the simulator

Two sessions, 1000 inputs, 489 of them sent again after a lost reply, nine provider
restarts (the provider process vanished, the session said `disconnected`, an input was
refused, `session.resume` raised the generation), 100 stale-generation and 50 foreign-session
attempts, then 50 fixture turns with 100 approvals answered while a forged id, a stale
generation and a repeated answer were tried around each. Counted from the provider's own
received log and the durable session log, not from replies:

| Count | Result |
|---|---|
| inputs applied by the provider zero times / more than once | 0 / 0 |
| inputs misrouted to the other session | 0 |
| refused inputs that reached the provider | 0 of 150 |
| approvals raised / answered / provider responses | 100 / 100 / 100 |
| forged, stale and repeated approval answers refused | 100 / 100 / 100 |

Inputs the provider received twice: 0 in the run at the clean commit, but in a separate
12-run loop 4 runs showed one input received twice as `turn/steer` followed by `turn/start`.
That is the designed fallback (the provider refused the steer because its turn had just
ended, so nothing was applied, and the input became a turn): applied exactly once in all 12
runs. It is "sent twice, applied once", and the first send was an explicit refusal.

### Hostile provider and clients

`test_adapter_survives_malformed_oversize_and_hostile_provider_messages` drives a provider
that sends non-JSON, a message without fields, a 3 MB line, 40 MB of one unending line, 30 MB
in one item that never gets a newline, 3000 items each with a partial line, 300 approval
requests with 100 KB commands full of escape sequences (one id repeated), and an unknown
server request, and then a canary prompt. It found two real defects, now fixed: partial lines
and approval ids were not bounded (without the bounds the host grew by about 150 MiB;
now growth stays under 24 MiB, the test's limit), and approvals beyond 64 were silently
dropped without telling the provider (now answered with an error). After the attack the session is
still running, every record is at most 4096 bytes without control characters, at most 64
approvals are pending with prompts of at most 200 plain characters, and the canary is
found only in the session log and the provider's own received log (not in argv, standard
error or any other file). The clients are tested separately: with the filter of each
client disabled, a stand-in host that sends raw escape sequences makes the shown text contain
the control characters; with the filters, the clipboard has no owner, the window title is unchanged, the screen is not cleared and
no control or bidirectional character is shown. The earlier host-level hostile suite still
passes in the same way (34 of 35 cases rejected with no effect; the peer of another user is
untested because no second user exists).

### Unicode and Korean

Mixed scripts (`hello é é 漢字 שלום 🙂`) typed directly and Korean typed through the
real input method (ibus-hangul) reach the provider as one turn with exactly the typed text,
come back, are recorded and are shown, for both clients. The four P01 typing conditions are
exact for both clients with agent-shaped output, and a view switched in the middle of a
composition gives each session exactly its own part (`안녕` and `하`). Limits: right-to-left
text is shown, but whether its visual order is right is for the owner to judge on the
screens (`screens/echoed_*.png`); the emoji falls back to a monochrome glyph; a line is cut
to 200 characters on screen (the record keeps up to 4096 bytes); the clients still show only
the last 40 lines with no scrollback, so "history on demand" and selection are **not built**.

## Independent review

Pending: one read-only reviewer is checking the measurements and the code; this section is
updated with its findings and their fixes.

## Steps and commits

| Step | Commits | New tests |
|---|---|---|
| Approval record | `aa11983` | none |
| S01 capability record, reference mapping, fixtures | `d537871` | 2 |
| S02 adapter on the host against the simulator | `6d86595` | 3 |
| S03 environment allowlist, private workspaces, purge | `da2b8f2` | 2 |
| S04 UTF-8 text sessions, client defences, Unicode and Korean round trip | `b006af3` | 2 (run for two clients) |
| S05 hostile provider, text workload, adapter scenarios, real-turn command, bundle | `71cc757`, `6ed2520`, `be5b0cd`, `c629d14`, `9f834a8` | 1 |

Ten test functions (2, 3, 2, 2, 1), as approved, and 53 runs in the whole suite including the
earlier phases; 53 passed on the final tree (about 185 s). The measurement commands carry
their own checks (the adapter scenarios and the real-turn dry run) and added no test.

Red evidence: S01 and S02 were written before their tests, so red was observed by mutation
(and disclosed in the plan's progress notes); S03's environment test was first run against
existing code and its purge test was red before purge existed; **S04's tests were red before
the code** (4 of 4 failed), and the client-only half was strengthened after it passed on
the first run by showing it fails with each client's filter weakened; S05's hostile test
was red at the approval bound before the fix, and mutations of the partial-line bound and the
repeated-id check were each caught. The mutation outputs are local scratch files.

## Deviations and corrections

- A first version of the S04 test passed even with both client filters removed, because the
  host sanitizes first; a stand-in host that sends raw sequences was added to test the
  clients themselves.
- The measurement commands labelled every scope with a hard-coded history and record size; the
  labels now state the real history length and the output kind, and the recorded record size
  is the real one (an earlier draft of the run showed 1024 for 110-byte records).
- The first adapter run counted "sent to the provider twice" as a failure; the count now also
  reports what the provider applied, and the repeated-send detail (see above).
- One debug run of the hostile check failed once with no recorded cause (a dirty tree, before
  failure details were recorded); 50 later runs passed. The cause is unknown.
- Clients scrape JSON by key with simple string search; agent text is JSON-escaped so it
  cannot contain a raw key, and rec/snapshot text is parsed with a real JSON parser, but the
  scraping remains.
- A2 gained the `serde_json` crate (the host already uses the same pinned version), so its
  lock file changed; nothing new was downloaded (offline cache).

## Measurement runs

| Run | Commit | Content |
|---|---|---|
| GUI (`run --text`) | `be5b0cd` | all experiments for A2 and C2 with agent-shaped output |
| P02 (`p02 --text`) | `be5b0cd` | host memory, backpressure, input integrity, hostile suite, GUI kill, view switch |
| P03 (`p03`) | `c629d14` | adapter scenarios, hostile adapter check, provider evidence, provider idle memory, real-turn dry run, simulated session |
| Bundle | `9f834a8` | `p03-bundle`: screens and the inspection page |

Each started from a clean tree; the receipts carry the command, commit, binary SHA-256
values and workload. The GUI and P02 runs used the host and clients built before `be5b0cd`
(unchanged since); the P03 stage was run at a later commit that only added measurement and
bundle code. An earlier set of runs at `6ed2520` is superseded.

## Decision

Nothing was missed, so no budget decision is required. The recommendation of the earlier
phases stands: A2 is the primary candidate and C2 the fallback. Two facts argue for keeping
both alive: A2's noisy-case CPU (99%) and C2's thin PERF-01 margin (3.5 MiB). The renderer
lock planned for the end of this phase is the owner's decision at G3; this phase supplies the
evidence, not the lock.

## Open items for the owner

1. **Real turn (needs the owner).** From the repository, with the host binaries built
   (`private/build/host_hostd/release`, or `LICO_BUILD_ROOT`):
   `cd src && PYTHONPATH=. python3 -m lico_measure real-turn --accept-billing --turns 1 --out DIR`.
   It starts the owned host with the real provider found on `PATH`, sends one short prompt
   (at most 3 turns, never more), **denies every approval it is asked**, and writes an
   owner-only transcript. It refuses to run without `--accept-billing`; the provider finds
   its own sign-in exactly as when run by hand, and Lico does not read or copy it. Each turn
   can be billed. Not covered by any evidence yet: the real approval events, whether the
   read-only sandbox works on this host (the provider warned that it needs user namespaces),
   and how a not-signed-in provider answers (the command reports `[input refused]` and stops).
   `--provider-bin PATH` runs the same command against the simulator for free.
2. Decide the A2 noisy-case repair (frame cap or incremental shaping), if wanted before M01-P04.
3. Decide whether provider memory (about 122 MiB idle per provider) needs a policy.
4. Selection, scrollback/history on demand, accessibility and bidirectional layout checks
   remain unbuilt or unverified in both clients.
5. The three branches (`phase/M01-P02-persistent-host`, `doc/M01-P03-plan`,
   `phase/M01-P03-first-real-agent`) still need to be published by Pri; this session cannot push.
6. G3 (inspection of M01-P01, P02 and P03) and G4 (merges) are the owner's. M01-P04 needs its
   own plan and approval.
