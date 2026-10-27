# 00005 s08_ten_minutes_of_traffic

Version 1.0.2

**Question.** The stop line of the phase is a leak of 10 MiB in 10 minutes with the views hidden. Do the host and the window stay within it while three Claude sessions (the
simulator) use all five tools of the tool connection all the time, the person answers every card, and the window only shows the list of sessions?

**Workload.** `measure.py`. A private host with three sessions, each with a first turn (a session has its process, and so its credential, only from its first turn). Every 4 s one
cycle, the caller and the target rotating over the three sessions:

1. a word of the person to the caller (it starts the count of the chain of agents again);
2. `sessions`;
3. `send` of a text of 200 bytes or of 6,000 bytes (every second cycle, so half of the texts go as a file), and the person's yes;
4. `history` of the target and the person's yes (the answer is over 4,096 bytes soon, so it is a file in the caller's own folder);
5. `read_handoff` of the newest file in that folder;
6. every eighth cycle `create`, the person's yes, and the purge of the session it made.

150 cycles in 600 s: 150 `send`, 150 `history`, 150 `sessions` and 18 `create` as the cycle is written; `read_handoff` is skipped in a cycle whose caller's folder has no file yet (`if files:`), so its number is at most 150. The script does not count the calls per tool; the figures are those of the cycles. The host (and the window, when asked) are read from `/proc` every 30 s:
`VmRSS`, `RssAnon`, `VmHWM`, threads, descriptors. When the run is over, the credentials of the three sessions are searched for in every file of the run folder except their config
files (which the host has taken away at the first `initialize`), and the first texts that were sent are searched for in the host's own log.

| Variant | What runs |
|---|---|
| `host` | the host program alone, no window |
| `window` | the product window under a virtual display, showing the session list (no session open), and its host |

**Run.** `PYTHONPATH=src:. LICO_BUILD_ROOT=<build root> python3 measure.py host|window [SECONDS]` prints one JSON line per sample and, last, a summary line. Needs the repository's
venv and the test helpers. No model turn is spent (the simulator is the program).

**Results** (release build; the machine is shared with other sessions, so the load average is the other sessions').

| | `host` variant | `window` variant |
|---|---|---|
| started / load average at start and end | 2026-10-27 16:29:37, 4.99 and 7.00 | 2026-10-27 16:40:58, 5.77 and 8.87 |
| tree | `2ba1cfc`, clean | `2ba1cfc` plus the change that is the commit `6bef892` (the buttons no longer stand on the end of a card's text), which was in the working tree and not yet committed when the run was made; the `ui.rs` of that commit is byte for byte the copy of the working-tree file that I saved after the run; I kept no hash of the binary that ran. Only `gui/ui.rs` differs from `2ba1cfc`, so the host program is that of `2ba1cfc` |
| host RSS first to last | 6.00 to 6.82 MiB (**+0.82**, +0.19 after the first minute) | 5.93 to 5.93 MiB (**0.00**) |
| host threads, descriptors, first to last | 12 to 12, 17 to 17 | 16 to 14, 21 to 19 |
| host peak (`VmHWM`) | 13.31 MiB | 12.94 MiB |
| window RSS first to last | - | 19.67 to 19.67 MiB (**0.00**), peak 19.67 |
| window threads, descriptors | - | 4 to 4, 11 to 11 |
| credential found in a file | none (the whole run folder except the config files) | none (the window's own folder only, `w.directory`: not the `log/` and the program folder of the run's `directory`, which only the host variant searches) |
| text that was sent found in the host's log | none | not looked for: the product window starts its host with its standard error discarded (`captures.py`), so there is no host log to search |

Both runs are far below the stop line (10 MiB in 10 minutes). The host's growth is a step in the first minute (the first `status`, the first files and records) and then flat: the
samples after the first minute differ by 0.19 MiB at the most in the host variant and not at all in the window variant. The window does not grow while 150 cards come and go in its
list. The descriptors and the threads end where they began, so nothing is left open by the `create` and purge of 18 sessions.

**Finding: the transient peak of the host is older than the listener, and the first `status` is what makes it.** The peak of 12 to 13 MiB (`VmHWM`) against a steady 6 MiB resident is not the tool connection's. `status_peak.py` (in this folder) reads `VmRSS` and `VmHWM` of a host at five points: after the start, after three sessions are made, after their first turns, after the first `status` and after a second one. Three runs on each of two builds (the build from before the listener, S02, and the tip; the host program of the tip is that of `5b1cc01`, run on 2026-10-27 at about 17:21, load average 4 to 6 from other sessions); KiB:

| point | before the listener (3 runs) | the tip (3 runs) |
|---|---|---|
| after the start | 5,376 resident, peak 5,376 (all three) | 5,376 to 5,568, peak equal to it |
| after three sessions made | 5,760 to 5,952, peak equal | 5,760 to 5,952, peak equal |
| after their first turns | 5,760 to 6,144, peak equal | 5,760 to 5,952, peak equal |
| after the first `status` (the reply is 1,026 characters) | 5,764 to 6,264 resident, **peak 12,480 to 13,056** | 5,856 to 6,148 resident, **peak 12,864 to 13,056** |
| after a second `status` | 5,956 to 6,264, the peak the same | 6,048 to 6,148, the peak the same |

In 6 of 6 runs the peak stands at the size of the resident memory until the first `status` and is 12 to 13 MiB right after it, with the resident memory moving by at most a few hundred KiB: a transient of about 7 MiB that one call makes and gives back. A second call does not raise it again. It is on the build from before the listener as well, so it is not new in P03, and at the same points the ranges of the two builds overlap (after the first `status` the lowest peaks are 12,480 and 12,864 KiB, 0.4 MiB apart, and the three runs of the build from before the listener alone spread 12,480 to 13,056 KiB). (The first version of this paragraph said "before and after the call" for a peak of 12,288 KiB; a peak that is the same before and after a call was not made by it, and these five points are what shows that the call is where it appears.) Where inside `status` the 7 MiB goes (the 1,026-character reply is far too small for it) is not found here. The stop line is about growth and the runs above are flat, so nothing is changed; the figure is here so that nobody reads the 13 MiB of `VmHWM` as the cost of the tool connection.

**Not measured.** More than three sessions in the traffic (the memory with 10 sessions before and after the listener is `../20261026/00002_s02_listener_memory`); a window with a
session open and its history drawn (the window idle figures of the phase report are those of the first view); a traffic of the real program; more than ten minutes.
