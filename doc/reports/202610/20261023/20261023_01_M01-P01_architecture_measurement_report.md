# M01-P01 report — architecture and resource measurement

- Status: in_review (inspection verdict pending; the status stays below `success` until the verdict is `accept`)
- Date: 2026-10-23
- Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Plan: [M01-P01](../../../plans/202610/20261023/20261023_01_M01-P01_architecture_measurement_plan.md)
- Branch: `phase/M01-P01-architecture-measurement`
- PR: not opened
- Inspection bundle: `private/manual_inspection/202610/20261023/000_M01-P01_architecture_measurement/` (local, verdict `pending`)
- Gates: G1 and G2 approved 2026-10-23; G3 pending; G4 pending

## What was measured, and what that does and does not show

Every number comes from a synthetic session host, a private virtual X server
(Xvfb: X11, no compositor, software rendering) and minimal clients that show 40
lines of text at 1200x800. No real account, provider session or billable turn
was involved. The clients are not products: the owned clients have no selection,
scrollback, accessibility or real IME composition, and the toolkit clients use a
plain text widget. The results are floors for the architecture choice, not
forecasts of product memory. Wayland, a real compositor, GPU allocation, package
size and real IME composition are **unverified**.

Configuration that shapes the numbers (all candidates run with an explicit
private environment: a private home, no inherited session bus, themes or
input-method settings):

- every client has the accessibility bridge disabled (`NO_AT_BRIDGE=1`); Qt
  candidates also run with `QT_ACCESSIBILITY=0`. **C2's margin is therefore an
  upper bound for an accessible configuration**; measuring with accessibility on
  needs a private accessibility bus and was not done;
- D (WebKitGTK) runs with compositing and the DMA-BUF renderer disabled, so it is
  not "as installed" defaults;
- the host was shared with other work; the one-minute load average during
  experiments ranged from about 6.5 to 20 on 24 logical CPUs and differed per
  candidate, and nothing was pinned or governed. One restart run of candidate B
  coincided with a load spike (26.7) and turned bimodal after 37 launches
  (p95 257.7 ms); it was repeated once at normal load (below) and the first run
  is preserved locally.

## Candidates

| ID | Candidate | Notes |
|---|---|---|
| A | winit 0.30.13 + softbuffer 0.4.8 + fontdue 0.9.5 (Rust) | owned minimal client, CPU pixels; reads its fonts into the heap |
| A2 | A with cosmic-text 0.19.0 for shaping and font fallback | same window layer, realistic text stack; fonts are mapped from the system |
| B | GTK 3.24.41 text widget (C) | installed toolkit |
| B2 | B with GDK GL disabled and a GLX vendor override | same binary; the switches had no effect (one attempt) |
| B3 | B with an empty `libGL.so.1` first on the library path | **experiment only**, not a supported deployment |
| C | Qt 5.15.13 Widgets text widget (C++) | installed toolkit |
| C2 | C with `QT_XCB_GL_INTEGRATION=none` | same binary, one environment switch (its documentation was not re-read: unverified) |
| D | WebKitGTK 2.52.3 web view (C) | HTML page in a web view; web and network helper processes |

Candidate A resolves 81 crates and A2 104 crates for the Linux target (the
lockfiles list more entries because they also cover other platforms); every
declared license is permissive (one crate offers `Apache-2.0 OR GPL-2.0-only`).
Qt and WebKitGTK are measured as installed system packages; their license text
and redistribution terms were not re-read here (unverified).

## Results against the proposed targets

Targets are copied from the product specification. "met", "missed" and
"unverified" describe this scope only, and the latency rows carry the
**X-server-pixel scope** described below. All values are from one final run at a
clean commit (`42e1592`) unless noted.

| Target | A | A2 | B | B3 (exp.) | C | C2 | D |
|---|---|---|---|---|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches) | 31.1 met | 20.6 met | 90.4 **missed** | 36.1 met | 93.3 **missed** | 42.0 met | 396.4 **missed** |
| same, PSS (reported separately) | 26.0 | 14.7 | 47.7 | 14.6 | 60.3 | 26.7 | 200.0 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB (3 pairs) | 0.1 met | 0.3 met | 0.2 met | 0.2 met | 0.1 met | 0.2 met | 49.4 **missed** |
| PERF-03 activation p95 ≤ 100 ms (n=100, pixel scope) | 12.7 met | 25.2 met | 11.9 met | 11.5 met | 10.2 met | 9.7 met | 643.2 **missed** |
| PERF-04 restart p95 ≤ 500 ms (n=100, pixel scope) | 74.4 met | 83.9 met | 89.2 met* | 91.9 met | 109.3 met | 74.0 met | 322.9 met |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.20 met |
| noisy case CPU (reported separately) | 13.9 | 70.9 | 10.8 | 10.7 | 7.9 | 7.9 | 49.8 |
| cold start, best effort (median of 10, ms) | 84 | 111 | 125* | 101 | 172 | 114 | 715 |

\* candidate B's restart was repeated once (89.2 ms); the first run, disturbed by
host load, gave 257.7 ms. B2 (same binary as B, not shown) measured 90.4 MiB RSS
and 89.8 ms restart.

How many samples each row really has: PERF-01 is three independent launches (each
the median of five reads of that process); the launches differed by at most
0.2 MiB, so there is almost no run-to-run variance information. PERF-02 is three
paired repetitions. PERF-03 and PERF-04 are 100 observations each. PERF-05 is
**one** 60 s window per candidate; before and after it the harness checked that
the process tree was unchanged and alive and that the view was still shown.

Notes that change how the table reads:

- **Idle CPU 0.00** means zero scheduler ticks over 60 s (resolution about
  0.017%), a bound rather than a measured zero. The noisy case is high for A2
  (70.9%) because the first implementation reshapes all visible lines on every
  frame; an owned client needs incremental shaping.
- **RSS composition.** B and C map the Mesa GL driver stack through libepoxy and
  libGL at start (libLLVM 43.6 MiB resident but only 25.1 MiB PSS, libgallium
  4.6 MiB). That is most of their 72 MiB of file-backed pages; their anonymous
  memory is 17 and 16 MiB. The specification counts "toolkit/driver resident
  pages", so the miss is real for this host's driver stack, but it is inflated by
  page sharing and by the software GL driver used under Xvfb. Another driver stack
  changes it. B has no supported switch that avoids it (one attempt); only the
  unsupported stub (B3) did. Qt has one (C2). WebKitGTK also mapped an NVIDIA
  driver library; the GPU itself is not used by the virtual display.
- **PSS** depends on what other processes share the same pages; it is reported,
  not substituted for the RSS target.
- **PERF-02 is true by construction.** The clients keep only an id and a cursor
  for hidden sessions (no sidebar or per-session widget), so a 0.1–0.4 MiB
  delta says little about an architecture; it shows that views are released and
  that nothing leaks across 100 show/hide cycles. D is the exception: its
  web-process memory is not released (largest of three repetitions 49.4 MiB; the
  cause was not isolated). "Large history" lives only in the host: the snapshot is
  capped at 40 lines, so history size never reaches a GUI. Only 5 of 100
  switches target sessions that produce output while hidden; their samples were
  not slower than the rest, so that condition has no measurable effect here.
- **D** creates a web view, and with it web-process work, each time a view is
  shown; this is the likely reason activation misses by a wide margin (the cause
  was not isolated). Its restart is bimodal (typically 264–310 ms, worst 789 ms).
- **Glyph variety** (two full pages: 2350 Hangul syllables, then 2360 ideographs
  at 14 px): A grows from 30.9 to 32.2 MiB and A2 from 20.4 to 24.3 MiB. A's
  fallback font lacks ideographs, so its second page is drawn as missing-glyph
  boxes (not pixel-checked), which makes "lowest memory" for A2 not like-for-like
  with A. Toolkit candidates were not stress-tested this way.
- **Pixel scope.** Latency is read from X server root-window pixels (a
  session-colour block and a sequence-colour block). There is no compositor and no
  vsync, text content is never read, and values near 10 ms (below one 60 Hz
  frame) fit a framebuffer write, not a presented frame. A frame counts only if it
  shows the target session and a sequence value at least as new as the host's
  when the switch was issued and not implausibly far ahead of it (an earlier
  version accepted a blank page for D; fixed and re-measured). The screen read
  takes about 21 µs.
- Other resident memory is reported separately: the virtual display server is
  67–75 MiB and differs by candidate, and the host grows from 27.5 MiB (10
  sessions) to 77.4 MiB (100 sessions). Shared-memory pages appear in both a client's and
  the display server's RSS, and services activated over a session bus would sit
  outside the process tree (the clients run without one).

### Not a number: continuity (PERF-06)

For each candidate: 30 s of 1 KiB records at 100 records/s (3002 records,
3,074,048 bytes), the GUI absent for 15 s, then reattach by cursor. An
independent subscriber recorded a contiguous sequence with no gap and no
duplicate; each client reported 0 gaps and 0 duplicates and finished within two
records of the host. A cursor older than the retained window was answered with a
gap flag and a fresh snapshot.

Hostile requests (foreign session, 50 guessed ids, stale generation, oversize
line, malformed JSON, unknown operation, a canary sent in a request) were all
rejected with no subscription effect. Limits of that evidence: session ids are
derived deterministically (not unguessable); the generation is always 1, so the
stale case is a fabricated mismatch; the peer-user check exists but has no test;
there is no connection cap or throttle (the plan's S9 policy is not
implemented); the canary check shows the host does not echo what it receives, not
that no secret exists elsewhere. The host socket is created with owner-only modes
(directory 0700, socket 0600, both asserted) and there is no HTTP listener, so
Host/Origin handling does not apply.

### PERF-07 inventory

Binary sizes: A 4.3 MB, A2 7.4 MB, B 32 KB, C 63 KB, D 28 KB; the toolkit
binaries load shared libraries (147 mapped files for B, 133 for C, 207 for D, 76
for A2, 20 for A). Package size is unverified (no package is produced in this
phase). Process trees: one process for every candidate except D (three). Display:
Xvfb, X11, 1200x800x24, scale 1 (declared, not observed); fonts as mapped by each
client. Hardware category: x86_64 with 24 logical CPUs.

## Steps and commits

| Step | Commits | Tests (new) |
|---|---|---|
| Approval record | `c12c959` | none (documents) |
| S01 survey, collector, workload | `5fb9e91`, `8b1b188` | 2 |
| S02 host and clients | `f0ba964` | 2 test functions, run for each candidate |
| S03 report, latency, hostile, runner, bundle, candidates, review fixes | `28a05ea` … `61d9a8f` (see the branch log) | 3 |
| S04 decision rules and this report | `dad95d8` and the documentation commits | 1 |

Test count: 8 test functions (2, 2, 3, 1) and 16 runs including the per-candidate
parametrization; 16 passed in about 30 s on the final tree. The plan's "at most
five" is read per step; read as a total it is exceeded, and that reading is left
to the owner. Several assertions in the lifecycle tests are self-reports of the
client under test (resident flag, line count, cursor, gap and duplicate
counters); the externally observed checks are the screen pixels and the host's
own subscriber and sequence counters.

Red evidence (`pytest -q -p no:cacheprovider`, run in a resource-limited job
wrapper): S01 collection error for the missing `procscope` module; S03 collection
errors for the missing report, latency and hostile modules; the memory-composition
assertion failed with an attribute error before its code existed; S04 collection
error for the missing decision module; the review-driven latency and hostile
assertions failed before their fixes. **S02 has no red-before-code run**: the
clients were drafted before the tests. Red was observed afterwards by mutation:
hidden views that keep their resources and a host replay that skips a record both
turned the tests red for every candidate. Every other test was also turned red by
at least one mutation (descendants omitted, overlap allowed, content age ignored
or unbounded, no sample minimum, stale generation accepted, claims unchecked,
unverified accepted). The overflow-and-resync path and the peer-user check have
no test.

Deviations and corrections found by measuring and by the independent review:

- The first activation run timed out 95 of 100 times because a reactivated view
  was blank: reactivation had no snapshot. Reactivation now asks for the latest
  screen; the cursor replay path stays for explicit cursors.
- Reattach after a long absence overflowed the host's bounded queue and the stream
  stopped. The queue now holds a full replay and a client resyncs from a snapshot
  after an overflow notice.
- Candidate B kept a "no session view" label beside a shown view; fixed.
- The independent review found D's latency accepting a blank page, idle CPU with
  no liveness proof, partly vacuous hostile cases, five reads of one process
  counted as five samples, and clients inheriting the user's environment. All were
  fixed and every experiment was re-run from the fixed commit; earlier runs are
  preserved locally and are superseded.
- A run's receipt is now written before the first experiment and records binary
  SHA-256 values. The binaries come from an ignored build directory, so a clean
  source tree does not by itself prove which binary ran; the hashes do.
- One commit (`5871ac9`) was made while the test run had reported no free job
  slot, so the green gate had not run for that commit; the same tree was run
  green immediately after (9 passed).
- Qt with an empty GL stub failed to start (its GLX plugin needs real symbols);
  the supported switch was measured instead.
- The memory-composition test first used a 256 kB tolerance and failed
  intermittently; the status counters lag the smaps total by about 0.4 MiB per
  process, so the tolerance is 1024 kB and the record says so.

## Measurement runs

| Run | Commit | Candidates | Experiments |
|---|---|---|---|
| final | `42e1592` | A, A2, B, B2, B3, C, C2, D | all seven, plus the glyph-variety memory run for A and A2 |
| inventory | `61d9a8f` | all | window-geometry check corrected |
| B restart | `61d9a8f` | B | repeat of the restart experiment (see load note) |

All runs started from a clean tree. Raw samples (100 per latency metric) stay
local until the review is done; the bundle's receipt lists every command, commit
and the workload. p95 is nearest rank on a monotonic clock. A latency timeout is
counted at its cap (2000 ms for activation, 10000 ms for restart), never dropped;
there were none in the reported runs. Candidate asymmetries: A and A2 rebuild
their glyph cache and shaped buffer on every show (pessimistic for them) and are
much thinner than B, C and D; D batches updates on a 16 ms timer; A uses fixed
font files while the others use system fonts.

## Design choices (self-audited)

| Choice | Decision | Alternatives kept as settings or open |
|---|---|---|
| RSS definition | process-tree sum, each pid once, scopes disjoint; PSS beside it | PSS as the target basis is an owner decision |
| p95 method | nearest rank | other estimators change values by well under one sample step at n=100 |
| Hidden growth | RSS(100 sessions) minus RSS(10), paired per repetition, after every session was shown once then hidden | a client with a per-session UI element |
| Targets | never relaxed | revising PERF-01 is listed as an option below |
| Variants | env-only variants labelled; the stub variant is marked experiment-only | none |

## Decision

No candidate was selected automatically. Four candidates meet every measured
target in this scope (A, A2, C2, and the experiment-only B3); B, C and D as
configured here do not, and D misses three targets, two by a wide margin.

| Option | Evidence | Trade-off |
|---|---|---|
| 1. Owned client on winit + softbuffer + cosmic-text (A2) | 20.6 MiB RSS, activation p95 25.2 ms, restart p95 83.9 ms, glyph pages +3.9 MiB | lowest memory; IME composition, accessibility, selection and incremental shaping are unbuilt and unmeasured (noisy-case CPU already 70.9%) |
| 2. Qt 5 Widgets with its XCB GL integration off (C2) | 42.0 MiB RSS (8 MiB of margin, measured with accessibility off), restart p95 74.0 ms | built-in IME and accessibility; relies on a launch-time switch, Qt 5 support status and licensing unverified, the margin shrinks as features and accessibility are added |
| 3. GTK 3 | 90.4 MiB RSS as installed, PSS 47.7 MiB | misses PERF-01 unless the target basis changes; no supported switch avoids the Mesa/LLVM mapping |
| 4. WebKitGTK | 396.4 MiB RSS, activation p95 643 ms, growth 49.4 MiB | misses memory, growth and activation targets |
| 5. Change PERF-01 to a PSS basis | B 47.7, C 60.3, C2 26.7 | weakens the product promise; an owner decision, not made here |

Recommendation: keep A2 as the primary candidate and C2 as the fallback into
M01-P02. P02's host and control surface do not depend on the renderer, so the
choice does not need to be locked now. Lock it at the end of M01-P03, when real
Korean IME composition and accessibility are exercised on a real display; if the
owned client cannot reach acceptable IME behaviour there, the Qt route remains
measured and ready. This recommendation depends on things this phase did not
verify: IME composition on X11 and Wayland, behaviour under a real compositor and
GPU, accessibility cost for any candidate, and the cost of the remaining
owned-client features.

## Open items for the owner

- Inspection of the local bundle (G3) and the decision above.
- Whether "at most five tests" is per step or in total.
- Whether PERF-01 stays on an RSS basis.
- Publishing the branch and opening the phase pull request (the development
  session has no repository credentials).
