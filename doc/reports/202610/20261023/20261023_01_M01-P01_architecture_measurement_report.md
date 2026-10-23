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

Every number below comes from a synthetic session host, a private virtual X
server (Xvfb: X11, no compositor, software rendering) and minimal clients that
show 40 lines of text at 1200x800. No real account, provider session or billable
turn was involved. The clients are not products: the owned client has no
selection, scrollback, accessibility or real IME composition, and the toolkit
clients use a plain text widget. The results are floors for the architecture
choice, not forecasts of product memory. Wayland, a real compositor, GPU
allocation, package size and real IME composition are **unverified**.

## Candidates

| ID | Candidate | Notes |
|---|---|---|
| A | winit 0.30.13 + softbuffer 0.4.8 + fontdue 0.9.5 (Rust) | owned minimal client, CPU pixels |
| A2 | A with cosmic-text 0.19.0 for shaping and font fallback | same window layer, realistic text stack |
| B | GTK 3.24.41 text widget (C) | installed toolkit |
| B2 | B with GDK GL disabled and a GLX vendor override | same binary; the switches had no effect |
| B3 | B with an empty `libGL.so.1` first on the library path | **experiment only**, not a supported deployment |
| C | Qt 5.15.13 Widgets text widget (C++) | installed toolkit |
| C2 | C with `QT_XCB_GL_INTEGRATION=none` | same binary, one environment switch (its documentation was not re-read: unverified) |
| D | WebKitGTK 2.52.3 web view (C) | HTML page in a web view; web and network helper processes |

Candidate A resolves 81 crates and A2 104 crates for the Linux target; every
declared license is permissive (one crate offers `Apache-2.0 OR GPL-2.0-only`).
Qt and WebKitGTK are measured as installed system packages; their license text
and redistribution terms were not re-read here (unverified).

## Results against the proposed targets

Targets are copied from the product specification. "met", "missed" and
"unverified" describe this scope only. Values: A, A2 and B also come from earlier
runs listed below.

| Target | A | A2 | B | B3 (exp.) | C | C2 | D |
|---|---|---|---|---|---|---|---|
| PERF-01 idle GUI process-tree RSS ≤ 50 MiB | 31.0 met | 20.4 met | 91.2 **missed** | 36.7 met | 93.7 **missed** | 41.9 met | 397.8 **missed** |
| same, PSS (reported separately) | 25.9 | 14.8 | 47.9 | 15.2 | 61.0 | 28.5 | 200.4 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB | 0.1 met | 0.1 met | 0.2 met | 0.2 met | 0.2 met | 0.1 met | 28.6 **missed** |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 12.7 met | 32.7 met | 12.3 met | 11.9 met | 10.0 met | 10.1 met | 643.4 **missed** |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 67.0 met | 77.8 met | 105.7 met | 92.0 met | 109.6 met | 67.7 met | 786.9 **missed** |
| PERF-05 idle CPU ≤ 0.5% of a core (60 s) | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.20 met |
| noisy case CPU (reported separately) | 14.6 | 67.2 | 11.8 | 11.0 | 6.7 | 6.8 | 46.4 |
| cold start, best effort (median of 10, ms) | 77 | 111 | 108 | 103 | 176 | 117 | 996 |

Notes that change how the table reads:

- **Idle CPU 0.00** means zero scheduler ticks over 60 s; the resolution is about
  0.017%, so it is a bound, not a measurement of zero. The noisy case is reported
  separately and is high for A2 (67%) because the first implementation reshapes
  all visible lines on every frame; an owned client needs incremental shaping.
- **RSS composition.** B and C map the Mesa GL driver stack through libepoxy and
  libGL at start (libLLVM about 43 MiB resident, libgallium about 5 MiB). That
  accounts for most of their 73 MiB of file-backed pages; their anonymous memory
  is 17 and 16 MiB. The spec counts "toolkit/driver resident pages", so the miss
  is real for this host's driver stack. Another driver stack changes it. B has no
  supported switch that avoids it; only the unsupported stub (B3) did. Qt has one
  (C2). An NVIDIA driver library was also mapped by WebKitGTK; the GPU itself is
  not used by the virtual display.
- **PSS** depends on what other processes share the same pages; it is reported,
  not substituted for the RSS target.
- **D** starts a new web process per shown view, which is why activation and
  restart miss by a wide margin, and hidden views leave web-process memory behind.
- **Glyph variety** (two full pages: 2350 Hangul syllables, then 2360 ideographs
  at 14 px): A grows from 31.0 to 32.2 MiB and A2 from 20.5 to 24.3 MiB. A's
  fallback font lacks ideographs, so its second page is drawn as missing-glyph
  boxes. Toolkit candidates were not stress-tested this way.
- Latency is read from X server pixels (a session-colour block and a
  sequence-colour block), not from the client's own reply. The screen read takes
  about 21 µs, so values are not limited by the observer. The frame counts only if
  it shows the target session with content at least as new as the host's sequence
  when the switch was issued.

### Not a number: continuity (PERF-06)

For each candidate: 30 s of 1 KiB records at 100 records/s (3002 records,
3,074,048 bytes), the GUI absent for 15 s, then reattach by cursor. An
independent subscriber recorded a contiguous sequence with no gap and no
duplicate; each client reported 0 gaps and 0 duplicates and caught up to within
two records. A cursor older than the retained window was answered with a gap flag
and a fresh snapshot. Seven hostile requests (foreign session, 50 guessed ids,
stale generation, oversize line, malformed JSON, unknown operation, planted canary
in argv, ready line, status and snapshot) were all rejected with no effect.
The host socket is owner-only (directory and socket modes checked) and checks the
peer user; there is no HTTP listener, so Host/Origin handling does not apply.

### PERF-07 inventory

Binary sizes: A 4.3 MB, A2 7.4 MB, B 32 KB, C 63 KB, D 28 KB (the toolkit
binaries load shared libraries: 149 mapped files for B, 133 for C, 209 for D, 76
for A2, 20 for A). Package size is unverified (no package is produced in this
phase). Process trees: one process for A, A2, B and C; three for D. Display:
Xvfb, X11, 1200x800x24, scale 1; fonts DejaVu Sans Mono, NanumSquare and the
system fonts reached through fallback. Hardware category: x86_64 with 24 logical
CPUs; the other host load is recorded per run.

## Steps and commits

| Step | Commits | Tests (new) |
|---|---|---|
| Approval record | `c12c959` | none (documents) |
| S01 survey, collector, workload | `5fb9e91`, `8b1b188` | 2 |
| S02 host and clients | `f0ba964` | 2 test functions, run for each candidate |
| S03 report, latency, hostile, runner, bundle | `28a05ea`, `01f7dc7`, `5871ac9`, `304ee78`, `e8bfcf4`, `bf4b4f1`, `7a25b6a`, `cf20247`, `37035ff`, `c56076b`, `a9794d2` | 3 |
| S04 decision rules | `dad95d8` | 1 |

Test count: 8 test functions (2, 2, 3, 1) and 16 runs including the per-candidate
parametrization; 16 passed in 30 s on the final tree. The plan's "at most five"
is read per step; read as a total it is exceeded, and that reading is left to the
owner.

Red evidence (command `pytest -q -p no:cacheprovider`, run through the
heavy-job wrapper): S01 collection error for the missing `procscope` module; S03
collection errors for the missing report, latency and hostile modules; the
memory-composition assertion failed with an attribute error before its code
existed; S04 collection error for the missing decision module. **S02 has no
red-before-code run**: the clients were drafted before the tests. Red was
observed afterwards by mutation: hidden views that keep their resources and a
host replay that skips a record both turned the tests red for every candidate.
Every other test was also turned red by at least one mutation (descendants
omitted, overlap allowed, content age ignored, no sample minimum, stale
generation accepted, claims unchecked, unverified accepted).

Deviations and corrections found by measuring:

- The first activation run timed out 95 of 100 times because a reactivated view
  was blank: reactivation had no snapshot. Reactivation now asks for the latest
  screen; the stale-cursor replay path stays for explicit cursors.
- Reattach after a long absence overflowed the host's bounded queue and the stream
  stopped. The queue now holds a full replay and a client resyncs from a snapshot
  after an overflow notice.
- Candidate B kept a "no session view" label beside a shown view; fixed and
  re-measured, the earlier run is preserved.
- One commit (`5871ac9`) was made while the test run had reported no free heavy
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
| 1 | `01f7dc7` | A, B | all seven |
| 2 | `304ee78` | B (after the label fix) | all seven; replaces B from run 1 |
| 3 | `e8bfcf4` | A, B | idle memory and inventory with the RSS composition fields |
| 4 | `bf4b4f1` | C, D, B3 | all seven (aborted at a later candidate; its receipt was written afterwards) |
| 5 | `7a25b6a` | C2, B2 | all seven |
| 6 | `cf20247` | A2 | all seven |
| 7 | `37035ff` | A, A2 | glyph-variety memory |

All runs started from a clean tree. Raw samples (100 per latency metric, 5 per
memory metric, 3 per growth metric) stay local until the review is done; the
bundle's receipt lists every command, commit and the workload. p95 is nearest
rank on a monotonic clock. A latency timeout is counted at the 2000 ms cap, never
dropped; there were none in the reported runs.

## Design choices (self-audited)

| Choice | Decision | Alternatives kept as settings or open |
|---|---|---|
| RSS definition | process-tree sum, each pid once, scopes disjoint; PSS beside it | PSS as the target basis is an owner decision |
| p95 method | nearest rank | other estimators change values by well under one sample step at n=100 |
| Hidden growth | RSS(100 sessions) minus RSS(10), paired per repetition, after every session was shown once then hidden | longer cycles |
| Targets | never relaxed | revising PERF-01 is listed as an option below |
| Variants | env-only variants labelled; the stub variant is marked experiment-only | none |

## Decision

No candidate was selected automatically. Four candidates meet every measured
target in this scope (A, A2, C2, and the experiment-only B3); B, C and D as
installed do not, and D misses by a wide margin.

| Option | Evidence | Trade-off |
|---|---|---|
| 1. Owned client on winit + softbuffer + cosmic-text (A2) | 20.4 MiB RSS, activation p95 32.7 ms, restart p95 77.8 ms, glyph pages +3.8 MiB | lowest memory; IME composition, accessibility, selection and incremental shaping are unbuilt and unmeasured |
| 2. Qt 5 Widgets with its XCB GL integration off (C2) | 41.9 MiB RSS (8 MiB of margin), restart p95 67.7 ms | built-in IME and accessibility; relies on a launch-time switch, Qt 5 support status and licensing unverified, margin shrinks as features are added |
| 3. GTK 3 | 91.2 MiB RSS as installed, PSS 47.9 MiB | misses PERF-01 unless the target basis changes; no supported switch avoids the Mesa/LLVM mapping |
| 4. WebKitGTK | 397.8 MiB RSS, activation p95 643 ms | rejected by every latency and memory target |
| 5. Change PERF-01 to a PSS basis | B 47.9, C 61.0, C2 28.5 | weakens the product promise; an owner decision, not made here |

Recommendation: keep A2 as the primary candidate and C2 as the fallback into
M01-P02. P02's host and control surface do not depend on the renderer, so the
choice does not need to be locked now. Lock it at the end of M01-P03, when real
Korean IME composition and accessibility are exercised on a real display; if the
owned client cannot reach acceptable IME behaviour there, the Qt route remains
measured and ready. This recommendation depends on three things that this phase
did not verify: IME composition on X11 and Wayland, behaviour under a real
compositor and GPU, and the cost of the remaining owned-client features.

## Open items for the owner

- Inspection of the local bundle (G3) and the decision above.
- Whether "at most five tests" is per step or in total.
- Whether PERF-01 stays on an RSS basis.
- Publishing the branch and opening the phase pull request (this session cannot).
