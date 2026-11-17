# M19-P01: the memory guard, release 0.6.0

- Version: 1.0.0
- Date: 2026-11-17
- Branch: `phase/M19-P01-memory-guard`, stacked on the M17-P10 tip `1c56353`; release commit `95f39e3`
- Source: lead mails 157 to 159 (the worker built slices A to D up to `8681425`, then its budget ended at ledger 235.82 of 235; the lead finished and released). **Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.**
- Labels: MEASURED (a run of this phase), READ (from the code), COMPUTED.

## 1. What the person sees

- Each session row shows the memory of the whole process tree (RSS). The figure is orange when the largest single process of the tree holds 400 MB, red from 500 MB (both settable); the line under the list takes the worst colour (a working session that shows nothing for 10 minutes also turns it orange, and the hover hint says when it last did something).
- A click on a red session asks once (per red episode) "End it and continue?"; Y does `session.close` then `session.resume` (the conversation comes back; tool jobs that were running are lost), N opens it as usual.
- With `"auto_close": true` in the owner-only `memory.json` (OFF by default) a red session is closed and resumed by the guard at an idle moment only (no turn, no approval pending, no typing for 30 s), never twice in ten minutes, one resume at a time 20 s apart; afterwards the session shows "[memory N MB: closed and resumed automatically]". A session the person closed is never revived.
- The main agent (the session named `main_agent` in `memory.json`; ON unless `"warn_main": false`) gets one fixed warning for each red episode of a session and of the machine, only at an idle moment; the automatic close waits for it (at most 5 minutes until the warning was tried, then 2 minutes).
- When the machine runs low (MemAvailable under 20 percent: orange, under 10: red) the line starts with "free N% ...", a session started then is followed by a plain warning, and with the auto-close on a red machine closes the heaviest idle session that is not the main agent, one every two minutes.

## 2. How it is built (READ)

- New pure component `lico-memguard` 0.1.0: levels, machine level, episodes, `ClosedBy`, `ResumeQueue` (20 s gap), `LoopGuard`, `Idle`, the warning texts, `stalled(working, quiet_secs, stall_secs)`, `quiet_text`. No thread, file, clock or dependency.
- `guard.rs` (host) is a leaf that names no hub; the hub calls are in `lifecycle::guard_tick`, called from the `status` arm of `ipc.rs`: no timer, nothing acts while no client asks for `status`. The window uses the host's limits and stall time from `status.memory_limits` and the machine figures from `status.machine_memory`.
- Protocol: `session.close` takes `by`; `status` carries `closed_by`, `memory.max_rss_kb`, `memory_limits {warn_mb, limit_mb, stall_s}`, `machine_memory {available_kb, total_kb}`.
- Spec rows MEM-01 to MEM-07 in the product spec; guide section "Memory guard"; CHANGELOG 0.6.0.

## 3. Review of the lead's changes (worker report 143, read-only) and what was done

| Finding | Done |
|---|---|
| 1 a close with no warning and no grace when the main agent could not be warned | fixed: the close waits until the warning has been tried, at most 5 minutes (`WARN_WAIT_MS`); tests |
| 2 the result of delivering the warning was ignored | fixed: a failed delivery re-opens the episode (`warn_failed`), tried again after 30 s, the grace runs from the first try; test |
| 3 the note and `clear` after a failed resume | fixed: only on `Ok`; a failed resume keeps the session closed and marked as closed by the guard |
| 4 `else` took every non-claude provider | fixed: `claude`, `codex`, and nothing for the others |
| 7 the machine line over 39 columns | fixed: the host figure is dropped first; a test at the longest case |
| 8 the machine pace set before the close happened | fixed: it starts in `closed_ok` |
| 9, 10 restart gap, wording of the spec | written into MEM-03 and MEM-04 (known gaps); the flash of the new-session warning may be overwritten right after creation: NOT fixed |
| 5, 6 deadlock, overflow | no finding |

## 4. Tests (MEASURED)

| Check | Result |
|---|---|
| `lico-memguard` | 9 contract tests; mutants 12 of 12 killed (worker, step 1) |
| host unit tests `guard::` and `memrow` | 18 passed (including the hold, retry, machine close, note and width cases) |
| window tests `tests/test_memory_guard_window.py` | 5 passed, 62 s; pictures in `private/scratch/202611/20261117/m19_p01_screens/` (1_orange, 2_red_question, 3_red_after_cancel, 4_after_end_and_resume, 5_auto_closed_and_resumed, 6_stalled); the stall test takes 6 s now (the stall time is in seconds: it took 63 s and pushed one whole run to 181.6 s against 180) |
| whole suite, cold, three runs (build skipped: the sources hash to the built binary) | 572 of 572 passed each: **154.7 s, 158.5 s, 163.7 s**; peaks **1,758.6, 1,847.6, 1,955.1 MiB** of 2,048 (worst margin **92.9 MiB**: thin, as at M17-P10, 94.7); a run with its own build is longer (139 s build + 55 s test build on a cold target) |
| `cargo xtask check`, `verify`, `graph` | green; graph: 201 modules, the biggest cycle still **48** |
| `cargo xtask test` (all 15 components), `system`, `accept` | all passed; system tests include `browser_engine_installs_itself` |
| Two clean clones | identical, `8887004198a168d335413a24d988ca4b1ddb6fd6b71010b7f56e6eb3b91ce0a8` |

Two flakes found on the way, neither caused by the guard: (a) a unit test that called `total()` coloured the frame the scroll oracle compared (the colour state was one global for all test threads; it is per thread now); (b) `tests/test_everyday_comfort.py` took the number of a session from the tab order, while the list sorts by the last activity in whole seconds (it failed once on a loaded machine; it takes the number the list shows now). A third, `cargo xtask test lico-sandbox` failing once in 0.85 s inside the suite, could not be repeated in 12 runs alone and under load; xtask now prints which tests failed (a test for that), so the next one names itself.

## 5. The measurement of eight sessions (MEASURED, once each)

Eight simulator sessions, each holding 100 MiB in its agent, a host and a window on a private display, 0.5.1 (`~/bin/lico` before) against 0.6.0 (the acceptance artifact), 64 GB machine:

| | 0.5.1 | 0.6.0 |
|---|---|---|
| binary | 10,722,312 bytes | **10,820,264 bytes (+97,952, +0.91 %)** |
| host RSS, 8 sessions idle / holding | 7.88 / 8.25 MiB | 8.25 / 8.25 MiB |
| window RSS, 8 sessions idle / holding | 23.18 / 23.18 MiB | 22.93 / 22.93 MiB |
| one minute later (host, window) | 8.25, 23.18 | 8.25, 22.93 (no growth) |
| `status` call with 8 sessions, median / p95 / max | 0.29 / 0.38 / 0.61 ms | 0.45 / 0.49 / 3.06 ms |
| sum of the sessions' RSS (what the guard sees) | 1,125.7 MiB | 1,122.0 MiB |

The +0.16 ms per `status` is the guard's step (it reads `memory.json` and `/proc/meminfo` each time). The sum of RSS is 800 MiB of held pages plus eight interpreters' resident files, counted in each. The drop of MemAvailable for the same load was 1,024.9 MiB (0.5.1 run) and 1,284.7 MiB (0.6.0 run): page-cache noise, so no claim is made on it; the `machine_memory.available_kb` the host sent was equal to the kB read from `/proc/meminfo` at the same moment (33,111,884).

The machine's real memory was NOT driven low (the oomd lesson of 2026-10-10): the red-machine behaviour is tested by unit tests with a machine figure passed in, not by exhausting memory.

## 6. Installed

`~/bin/lico` is 0.6.0 (`lico 0.6.0 (commit 95f39e3, build ca5a20e979e8a747)`, mode 0700, replaced by rename; nothing was killed: running hosts keep the old binary until the next start). The 0.5.1 binary is at `private/release_0_5_1_backup/lico` (different from the 0.5.0 backup).

## 7. Known limits and what is open

- The guard's tables live in the host's memory: a host restart between its close and its resume leaves the session closed (MEM-03).
- `main_agent` is only set by file; the host does not record which session created which, so there is no default.
- The stall colour is on the line under the list, not in the tab caption; the host's older `stalled` flag and the new rule are not unified.
- The whole-suite margin is thin (92.9 MiB of 2,048): a test diet is the owner's decision.
- Not done by the owner yet: the real-home first-run check of the engine auto-install (0.5.1), now on 0.6.0 as well.
- Push, PR, merge to `main` (G3, G4) for M16, M17, M18-P01 and M19 stay with the owner.
