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
Values are from one run at a clean commit (`a065e22`) with both clients on the owned host. Runs differ from each other by a few percent (two earlier complete runs at other commits gave, for example, C2 restart p95 199 and 224 ms and A2 activation p95 59 and 63 ms).

| Target | A2 (primary) | C2 (fallback) | A2 / C2 in M01-P02 |
|---|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches, max) | 22.4 met | 46.3 met | 20.5 / 42.3 |
| same, PSS (reported separately) | 16.6 | 29.3 | 15.4 / 28.9 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB (3 repetitions, max) | 0.12 met | 0.27 met | 0.2 / 0.1 |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 65.5 met | 15.7 met | 24.6 / 14.5 |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 109.6 met | 226.5 met | 77.8 / 60.1 |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met | 0.00 / 0.00 |
| noisy case CPU: visible session producing 100 lines/s (reported separately, one window) | **99.2** | 10.4 | 64.6 / 6.6 |
| cold start, best effort (median of 10, ms) | 121 | 253 | 90 / 104 |
| PERF-06 continuity, 30 s at 100 lines/s, GUI absent 15 s | 3003 records, no gap or duplicate | 3002, same | 3002 / same |
| Korean composition (4 typing conditions, ibus-hangul) | exact | exact | exact |

No target was missed. What the numbers say beyond the table:

- **C2 is now 3.7 MiB below the 50 MiB line** (was 7.7). It was measured with accessibility
  off, so its margin is an upper bound; the A2 margin is 27.6 MiB.
- **A2's noisy case saturates one core** (99.2%, up from 64.6% with hex records; one 10 s window, and
  no lag, gap or cursor check was made while it ran). The noisy
  case has no target in the specification and is reported separately, but a client that
  needs a whole core to show 100 lines per second is a real defect of its renderer: it
  reshapes every visible line on every frame. A frame cap or incremental shaping is the
  likely repair; it was **not** built here (see "Open items").
- Activation and restart grew for both clients because the transcript is 10000 lines
  (A2 activation p95 24.6 → 65.5 ms; C2 restart p95 60.1 → 226.5 ms) and stay under their
  targets. PERF-02 is still true by construction (a client keeps only an id and a cursor per
  hidden session) and its sample count is the 3 repetitions of the 10→100 measurement.
- A client shows the last 40 lines only, so a 10000-line transcript costs the client nothing
  after the first snapshot; the transcript lives in the host and its durable log (`--retain`
  raised so that all 10000 lines are kept; the host default of 2048 would have kept fewer).
- Idle CPU 0.00 means zero scheduler ticks over 60 s (a bound, one window).
- A2 after a page of 2350 Hangul and 2360 ideographs: 22.3 → 25.2 MiB (glyph cache growth).

### Host and provider memory

| Host memory (window 64, 3 repetitions) | RSS | PSS | CPU |
|---|---|---|---|
| 10 sessions, 10000-line transcripts, idle | 3.6 MiB | 1.8 MiB | 0.0% |
| 100 sessions, 50-line histories, idle | 11.3 MiB | 9.5 MiB | 0.0% |
| 100 sessions, 10000-line transcripts, idle | 11.9 MiB | 10.1 MiB | 0.0% |
| 100 sessions, 5 producing, 10 views streaming | 12.6 MiB | 10.8 MiB | 3.6% |

CPU is over a 10 s window; 0.0% means below the resolution of the tick counter in that window.

The host figures are lower than in M01-P02 (20.8 MiB at 100 sessions) only because a
record is now about 110 bytes instead of 1 KiB and the window holds 64 records; this is not
an improvement of the host. Memory does not grow with history: 100 sessions with 50-line histories and with
10000-line transcripts differ by 0.6 MiB. The synthetic agents cost 1.7 MiB each (168.6 MiB for 100) and are
reported apart.

**The real provider program is far heavier**: started and initialized in a private empty
home with no sign-in, no thread and no turn, the installed `codex app-server` (two processes)
held **89 to 123 MiB RSS, 53 to 86 MiB PSS** (3 independent launches: 120.5, 88.6 and 123.1 MiB RSS;
the low launch had one process instead of two). A provider with a conversation holds more. Ten live
provider sessions would therefore cost around 1 GiB outside Lico's own budget; the specification's budgets apply to the GUI and
the host, and the owner should decide whether a provider-memory policy is wanted.

### Adapter against the simulator

Two sessions, 1000 inputs, 489 of them sent again after a lost reply, nine provider
restarts (the provider process vanished, the session said `disconnected`, an input was
refused, `session.resume` raised the generation), 100 stale-generation and 50 foreign-session
attempts, then 50 fixture turns with 100 approvals answered (50 allowed, 50 denied) while a forged
id, a stale generation and a repeated answer were tried around each. Counted from the provider's own
received log and the durable session log (kept in full for this run), not from replies; the
final state of every input is `observed` (1000 of 1000). Not checked: that an approval was
answered for the session that raised it (both sessions share one provider log).

| Count | Result |
|---|---|
| inputs applied by the provider zero times / more than once | 0 / 0 |
| inputs misrouted to the other session | 0 |
| refused inputs that reached the provider | 0 of 150 |
| approvals raised (counted) / answered / provider responses | 100 / 100 / 100 |
| forged, stale and repeated approval answers refused | 100 / 100 / 100 |

Inputs the provider received twice: 0 in the final run, but in a separate
12-run loop 4 runs showed one input received twice as `turn/steer` followed by `turn/start`.
That is the designed fallback (the provider refused the steer because its turn had just
ended, so nothing was applied, and the input became a turn): applied exactly once in all 12
runs. It is "sent twice, applied once", and the first send was an explicit refusal. This holds
under the simulator's refusal semantics; the real provider's are unverified. An input in flight
when the provider vanishes stays `accepted` or `submitted` (the host never resends it; the client
must decide what to do with it).

### Hostile provider and clients

`test_adapter_survives_malformed_oversize_and_hostile_provider_messages` drives a provider
that sends non-JSON, a message without fields, a 3 MB line, 40 MB of one unending line, 30 MB
in one item that never gets a newline, 3000 items each with a partial line, 300 approval
requests with 100 KB commands full of escape sequences (one id repeated), and an unknown
server request, and then a canary prompt. It found two real defects, now fixed: partial lines
and approval ids were not bounded (without the bounds the host grew by about 150 MiB;
now growth stays under 24 MiB, the test's limit), and approvals beyond 64 were silently
dropped without telling the provider (now answered with an error). The review added more, all fixed
and now part of the same test: a line break or invisible line separator inside a command or reason
forged a second record (now a space or removed), a cut approval prompt hid its tail (now ends with an
ellipsis), and an oversize provider request id was echoed back (now ignored). After the attack the session is
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

One read-only reviewer read the code, the raw data and the bundle (nothing was run by it). Its
findings, and what was done:

| Finding | Action |
|---|---|
| Approval ids restarted at 1 after a resume, so an old prompt number could approve a new request | fixed: a per-session counter that is never reused; the exit test now checks it and fails without it |
| A provider that never became ready was left running and its log orphaned; a reaped pid could be signalled; only the direct child was stopped; a provider that closed stdout blocked the session | fixed: own process group stopped as a whole, a session that never became ready leaves no process, log or workspace, a reaped pid is cleared. Checked by hand against a program that never answers (no process, log or workspace left); not a pytest test |
| Provider-controlled ids and item keys not bounded in size | fixed and tested (the hostile test sends a 100 KB id) |
| Line breaks and line separators survived sanitizing, forging records | fixed in the host, the Python reference and both clients; tested |
| The `--text` workload labelled 10000-line transcripts while the host kept 2048 records | fixed: the durable log keeps them; labels and recorded sizes state the real values |
| Two host-resource rows were duplicates in the text workload; idle CPU used a 0.5 s window | fixed: a real short-history contrast row, a 10 s window |
| `p03` receipt called the whole run a simulator; the provider version came from the record, not the binary; some counts were computed | fixed: the binary's own version and hash are recorded; counts are observed |
| `real-turn` ran the provider with the owner's real HOME, left directory and file modes at defaults, had no receipt | fixed: empty private HOME with the provider's own `CODEX_HOME` passed as a path, owner-only modes, a receipt, the session purged at the end. Residual risk below |
| Vacuous assertions (`or` in the one-turn check, `or len(lines) >= 5`) | fixed; the clipboard and title checks are weak (neither client has code that touches them) and say so |
| PERF-02 is classified `met` from 3 repetitions; continuity ran with the synthetic agent; the A2 noisy case has no lag check; the C2 emoji is a missing-glyph box; scraped JSON depends on field order | not changed; stated in this report |
| Status markers such as `[turn completed]` are in-band (agent text can equal them) | not changed; the real-turn command relies on them, which a hostile agent could fake only against its own owner |

The reviewer's verdict was that the adapter's core handling is sound (exactly-once input by request
id, a refused steer falling back to a turn, approvals answered once under the session lock) and
that the measurement narrative needed the corrections above before being cited.

## Steps and commits

| Step | Commits | New tests |
|---|---|---|
| Approval record | `aa11983` | none |
| S01 capability record, reference mapping, fixtures | `d537871` | 2 |
| S02 adapter on the host against the simulator | `6d86595` | 3 |
| S03 environment allowlist, private workspaces, purge | `da2b8f2` | 2 |
| S04 UTF-8 text sessions, client defences, Unicode and Korean round trip | `b006af3` | 2 (run for two clients) |
| S05 hostile provider, text workload, adapter scenarios, real-turn command, bundle, review fixes | `71cc757`, `6ed2520`, `be5b0cd`, `c629d14`, `9f834a8`, `548fe27`, `a065e22`, `a288785` | 1 |

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
- The scenarios first started a new input when a turn's input state was `observed`, which can be before
  the turn ends: a `crash` or `fixture` input then became a steer. They now wait for the turn's completed
  record (this flake appeared once in the final run's p03 stage and was fixed before the reported run).
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
| GUI (`run --text`) | `a065e22` | all experiments for A2 and C2 with agent-shaped output |
| P02 (`p02 --text`) | `a065e22` | host memory, backpressure, input integrity, hostile suite, GUI kill, view switch |
| P03 (`p03`) | `a288785` | adapter scenarios, hostile adapter check, provider evidence, provider idle memory, real-turn dry run (with two approvals), simulated session |
| Bundle | `a288785` | `p03-bundle`: screens and the inspection page |

Each started from a clean tree; the receipts carry the command, commit, binary SHA-256
values and workload (the binaries come from an ignored build directory, so the hashes, not the commit,
prove which binary ran). The P03 stage ran at a later commit that changed only the scenario code. Earlier
complete sets of runs (at `6ed2520` and `be5b0cd`, before the review fixes) are superseded; they gave the same picture.

## Decision

Nothing was missed, so no budget decision is required. The recommendation of the earlier
phases stands: A2 is the primary candidate and C2 the fallback. Two facts argue for keeping
both alive: A2's noisy-case CPU (99%) and C2's thin PERF-01 margin (3.7 MiB). The renderer
lock planned for the end of this phase is the owner's decision at G3; this phase supplies the
evidence, not the lock.

## Open items for the owner

1. **Real turn (needs the owner).** From the repository, with the host binaries built
   (`private/build/host_hostd/release`, or `LICO_BUILD_ROOT`):
   `cd src && PYTHONPATH=. python3 -m lico_measure real-turn --accept-billing --turns 1 --out DIR`.
   It starts the owned host with the real provider found on `PATH`, sends one short prompt
   (at most 3 turns, never more), **denies every approval it is asked**, writes an owner-only
   transcript and receipt, and purges its session. It refuses to run without `--accept-billing`.
   The provider finds its own sign-in through `CODEX_HOME` (passed on as a path; Lico does not read
   or copy it) while its `HOME` is an empty private directory, so a command it runs does not find
   your ssh keys or shell history under your real home. **Residual risk:** other files your account
   can read are still reachable by a command the provider may run without asking (whether the
   read-only sandbox allows that is unverified); the default prompt asks for no command. Each turn
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
