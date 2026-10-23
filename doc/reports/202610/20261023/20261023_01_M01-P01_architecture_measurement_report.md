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
lines of text and one input line at 1200x800. No real account, provider session
or billable turn was involved. The clients are not products: the owned clients
have no selection, scrollback or accessibility, and the toolkit clients use plain
text widgets. The results are floors for the architecture choice, not forecasts
of product memory. Wayland, a real compositor, GPU allocation, package size,
accessibility and input methods other than ibus are **unverified**.

Configuration that shapes the numbers (all candidates run with an explicit
private environment: a private home, no inherited session bus, themes or
input-method settings):

- every client has the accessibility bridge disabled (`NO_AT_BRIDGE=1`); Qt
  candidates also run with `QT_ACCESSIBILITY=0`. **C2's margin is therefore an
  upper bound for an accessible configuration**; measuring with accessibility on
  needs a private accessibility bus and was not done;
- D (WebKitGTK) runs with compositing and the DMA-BUF renderer disabled, so it is
  not "as installed" defaults;
- the host was shared with other work and nothing was pinned or governed; the
  one-minute load average during experiments ranged from about 3 to 30 on 24
  logical CPUs and differed per candidate (candidate D's last experiments ran at
  loads of 28–30). An earlier run of candidate B's restart coincided with a load
  spike (26.7), turned bimodal and was repeated; the earlier runs are preserved
  locally and superseded by the single final run reported here.

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
**X-server-pixel scope** described below. All values are from one run of every
candidate at a clean commit (`46f271d`).

| Target | A | A2 | B | B3 (exp.) | C | C2 | D |
|---|---|---|---|---|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches) | 31.0 met | 20.5 met | 91.2 **missed** | 36.1 met | 93.0 **missed** | 41.8 met | 396.5 **missed** |
| same, PSS (reported separately) | 26.0 | 14.9 | 49.2 | 15.4 | 60.1 | 28.2 | 203.8 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB (3 pairs) | 0.2 met | 0.0 met | 0.3 met | 0.0 met | −0.1 met | 0.1 met | 2.4 met (erratic, see below) |
| PERF-03 activation p95 ≤ 100 ms (n=100, pixel scope) | 15.4 met | 33.7 met | 25.3 met | 14.7 met | 13.5 met | 16.5 met | 643.0 **missed** |
| PERF-04 restart p95 ≤ 500 ms (n=100, pixel scope) | 58.8 met | 77.2 met | 101.5 met | 99.2 met | 97.6 met | 61.8 met | 324.1 met |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.00 met | 0.20 met |
| noisy case CPU (reported separately) | 13.6 | 68.7 | 11.2 | 12.8 | 7.4 | 6.8 | 49.9 |
| cold start, best effort (median of 10, ms) | 66 | 89 | 105 | 398 | 172 | 124 | 1072 |

B2 (same binary as B, not shown) measured 90.5 MiB RSS and 93.3 ms restart p95.

How many samples each row really has: PERF-01 is three independent launches (each
the median of five reads of that process); the launches differed by at most
0.4 MiB, so there is almost no run-to-run variance information. PERF-02 is three
paired repetitions. PERF-03 and PERF-04 are 100 observations each. PERF-05 is
**one** 60 s window per candidate; before and after it the harness checked that
the process tree was unchanged and alive and that the view was still shown.

Notes that change how the table reads:

- **Idle CPU 0.00** means zero scheduler ticks over 60 s (resolution about
  0.017%), a bound rather than a measured zero. The noisy case is high for A2
  (68.7%) because the first implementation reshapes all visible lines on every
  frame; an owned client needs incremental shaping.
- **RSS composition.** B and C map the Mesa GL driver stack through libepoxy and
  libGL at start (libLLVM 43.6 MiB resident but only 25.1 MiB PSS, libgallium
  4.6 MiB). That is most of their 72 MiB of file-backed pages; their anonymous
  memory is 17 and 16 MiB. The specification counts "toolkit/driver resident
  pages", so the miss is real for this host's driver stack, but it is inflated by
  page sharing and by the software GL driver used under Xvfb. Another driver stack
  changes it. B has no supported switch that avoids it (one attempt); only the
  unsupported stub (B3) did. Qt has one (C2). B's PSS (49.2 MiB) sits just under
  the number but the target is RSS, so it stays missed. WebKitGTK also mapped an
  NVIDIA driver library; the GPU itself is not used by the virtual display.
- **PERF-02 is true by construction.** The clients keep only an id and a cursor
  for hidden sessions (no sidebar or per-session widget), so a delta near zero
  says little about an architecture; it shows that views are released and that
  nothing leaks across 100 show/hide cycles. "Large history" lives only in the
  host: the snapshot is capped at 40 lines, so history size never reaches a GUI.
  Only 5 of 100 switches target sessions that produce output while hidden; their
  samples were not slower than the rest, so that condition has no measurable
  effect here.
- **D's hidden growth is erratic.** Across three full runs its largest repetition
  was 28.6, 49.4 and 2.4 MiB, and in the last run one repetition was −30 MiB
  (web-process memory dropped). The classification above is not trustworthy for D.
  D also creates a web view, and with it web-process work, each time a view is
  shown; this is the likely reason activation misses by a wide margin (not
  isolated). Its restart is bimodal (typically 264–310 ms, worst 804 ms).
- **Glyph variety** (two full pages: 2350 Hangul syllables, then 2360 ideographs
  at 14 px): A grows from 30.9 to 32.2 MiB and A2 from 20.4 to 24.2 MiB. A's
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
  takes about 21 µs. B's activation moved from about 12 ms to 25 ms when the input
  line was added; the other candidates moved by a few ms.
- Other resident memory is reported separately: the virtual display server is
  67–75 MiB and differs by candidate, and the host grows from 27.5 MiB (10
  sessions) to 77.4 MiB (100 sessions). Shared-memory pages appear in both a
  client's and the display server's RSS, and services activated over a session
  bus would sit outside the process tree (the clients run without one).

### Korean composition through a real input method

Setup: a private bus and a private ibus 1.5.29 daemon with the ibus-hangul engine
(2-set keyboard), one per virtual display, sharing nothing with any desktop
session; keys are sent as ordinary key events. Candidates A, A2, B and C2 each
have an input line. The sentence "dkssudgktpdy gksrmf " (spelling 안녕하세요 한글
and two spaces) was typed in four conditions: 80 ms between keys, 8 ms, with
100 records/s streaming into the visible session, and 8 ms with that stream.
Result: **all 16 runs ended with exactly "안녕하세요 한글 ", and in every run the
committed text only ever grew** (nothing already committed was removed or
rewritten). The test was also run repeatedly during development with the same
outcome.

What this found along the way: the first owned client lost every space
("안녕하세요한글"), because a key the input method does not take reaches the
client by a different path than the input method's own commits. Both paths must
be handled. This is the class of defect behind "characters vanish" in other
applications, and it is the main risk of an owned client. What is **not**
covered: other input methods (fcitx5, kime), the Wayland text-input protocol,
candidate popups, cursor-area placement, input drafts and composition state when
a view is switched or hidden mid-composition, and composition while a real
compositor is present.

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
| S03 report, latency, hostile, runner, bundle, candidates, review fixes, Korean input | `28a05ea` … `46f271d` (see the branch log) | 4 |
| S04 decision rules and this report | `dad95d8` and the documentation commits | 1 |

Test count: 9 test functions (2, 2, 4, 1) and 20 runs including the per-candidate
parametrization; 20 passed in about 108 s on the final tree. The plan's "at most
five" is read per step; read as a total it is exceeded, and that reading is left
to the owner. Several assertions in the lifecycle tests are self-reports of the
client under test (resident flag, line count, cursor, gap and duplicate
counters); the externally observed checks are the screen pixels, the host's own
subscriber and sequence counters, and, for Korean input, the text each client
reports after real key events through a real input method.

Red evidence (`pytest -q -p no:cacheprovider`, run in a resource-limited job
wrapper): S01 collection error for the missing `procscope` module; S03 collection
errors for the missing report, latency, hostile and input-method modules; the
memory-composition assertion failed with an attribute error before its code
existed; S04 collection error for the missing decision module; the review-driven
latency and hostile assertions failed before their fixes; the Korean test failed
first with Latin letters (input method not engaged), then with the lost spaces.
**S02 has no red-before-code run**: the clients were drafted before the tests. Red
was observed afterwards by mutation: hidden views that keep their resources and a
host replay that skips a record both turned the tests red for every candidate.
Every other test was also turned red by at least one mutation (descendants
omitted, overlap allowed, content age ignored or unbounded, no sample minimum,
stale generation accepted, claims unchecked, unverified accepted). The
overflow-and-resync path and the peer-user check have no test.

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
  fixed and every experiment was re-run; the input line added afterwards for the
  Korean test changed the clients again, so every experiment was re-run once more
  from `46f271d`. A second review round was not run.
- A run's receipt is written before the first experiment and records binary
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
- The private input-method session first failed silently: a socket path over the
  Unix limit and a bus without a listen address; both were found by inspecting the
  daemon, and the bus is configured with no service directories so nothing else is
  activated on it.

## Measurement runs

| Run | Commit | Candidates | Experiments |
|---|---|---|---|
| final | `46f271d` | A, A2, B, B2, B3, C, C2, D | all seven timing and memory experiments, the glyph-variety run (A, A2) and Korean input (A, A2, B, C2) |

The run started from a clean tree. Raw samples (100 per latency metric) stay
local until the review is done; the bundle's receipt lists the command, commit,
binary hashes and workload. p95 is nearest rank on a monotonic clock. A latency
timeout is counted at its cap (2000 ms for activation, 10000 ms for restart),
never dropped; there were none. Candidate asymmetries: A and A2 rebuild their
glyph cache and shaped buffer on every show (pessimistic for them) and are much
thinner than B, C and D; D batches updates on a 16 ms timer; A uses fixed font
files while the others use system fonts.

## Design choices (self-audited)

| Choice | Decision | Alternatives kept as settings or open |
|---|---|---|
| RSS definition | process-tree sum, each pid once, scopes disjoint; PSS beside it | PSS as the target basis is an owner decision |
| p95 method | nearest rank | other estimators change values by well under one sample step at n=100 |
| Hidden growth | RSS(100 sessions) minus RSS(10), paired per repetition, after every session was shown once then hidden | a client with a per-session UI element |
| Targets | never relaxed | revising PERF-01 is listed as an option below |
| Variants | env-only variants labelled; the stub variant is marked experiment-only | none |
| Input method test | real ibus-hangul in a private session; exact text and no shrinking of committed text | other input methods and Wayland are open |

## Decision

No candidate was selected automatically. Four candidates meet every measured
target in this scope (A, A2, C2, and the experiment-only B3); B, C and D as
configured here do not (D misses activation by a wide margin and its growth
result is erratic). The four candidates that were given an input line (A, A2, B,
C2) all composed Korean correctly through ibus on X11.

| Option | Evidence | Trade-off |
|---|---|---|
| 1. Owned client on winit + softbuffer + cosmic-text (A2) | 20.5 MiB RSS, activation p95 33.7 ms, restart p95 77.2 ms, glyph pages +3.8 MiB, Korean composition exact | lowest memory; selection, accessibility, incremental shaping (noisy-case CPU 68.7%), mid-composition view switches and other input methods are unbuilt or untested; a lost-key class of defect was found once already |
| 2. Qt 5 Widgets with its XCB GL integration off (C2) | 41.8 MiB RSS (8 MiB of margin, measured with accessibility off), restart p95 61.8 ms, Korean composition exact | built-in IME and accessibility; relies on a launch-time switch, Qt 5 support status and licensing unverified, the margin shrinks as features and accessibility are added |
| 3. GTK 3 | 91.2 MiB RSS as installed, PSS 49.2 MiB, Korean composition exact | misses PERF-01 unless the target basis changes; no supported switch avoids the Mesa/LLVM mapping |
| 4. WebKitGTK | 396.5 MiB RSS, activation p95 643 ms | misses memory and activation targets; input not tested |
| 5. Change PERF-01 to a PSS basis | B 49.2, C 60.1, C2 28.2 | weakens the product promise; an owner decision, not made here |

Recommendation: keep A2 as the primary candidate and C2 as the fallback into
M01-P02. P02's host and control surface do not depend on the renderer, so the
choice does not need to be locked now. Lock it at the end of M01-P03, when the
installed agent path is exercised with Korean input on a real display (other
input methods, Wayland, view switches mid-composition, accessibility); if the
owned client cannot reach acceptable behaviour there, the Qt route remains
measured and ready. This recommendation depends on things this phase did not
verify: a real compositor and GPU, Wayland, input methods other than ibus,
accessibility cost for any candidate, and the cost of the remaining owned-client
features.

## Open items for the owner

- Inspection of the local bundle (G3) and the decision above.
- Whether "at most five tests" is per step or in total.
- Whether PERF-01 stays on an RSS basis.
- Whether a second review round of the fixes is wanted.
- Publishing the branch and opening the phase pull request (the development
  session has no repository credentials).
