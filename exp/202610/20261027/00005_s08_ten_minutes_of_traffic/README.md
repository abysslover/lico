# 00005 s08_ten_minutes_of_traffic

Version 1.0.0

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

150 cycles in 600 s: about 150 `send`, 150 `history`, 150 `read_handoff`, 150 `sessions` and 18 `create`. The host (and the window, when asked) are read from `/proc` every 30 s:
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
| tree | `2ba1cfc`, clean | `2ba1cfc` plus the uncommitted change to the card bar (the fix of the buttons that covered the end of a card's text) |
| host RSS first to last | 6.00 to 6.82 MiB (**+0.82**, +0.19 after the first minute) | 5.93 to 5.93 MiB (**0.00**) |
| host threads, descriptors, first to last | 12 to 12, 17 to 17 | 16 to 14, 21 to 19 |
| host peak (`VmHWM`) | 13.31 MiB | 12.94 MiB |
| window RSS first to last | - | 19.67 to 19.67 MiB (**0.00**), peak 19.67 |
| window threads, descriptors | - | 4 to 4, 11 to 11 |
| credential found in a file of the run folder | none | none |
| text that was sent found in the host's log | none | none |

Both runs are far below the stop line (10 MiB in 10 minutes). The host's growth is a step in the first minute (the first `status`, the first files and records) and then flat: the
samples after the first minute differ by 0.19 MiB at the most in the host variant and not at all in the window variant. The window does not grow while 150 cards come and go in its
list. The descriptors and the threads end where they began, so nothing is left open by the `create` and purge of 18 sessions.

**Finding: the peak of the host is the `status` operation, not the tool connection.** The peak of 13 MiB against a steady 6 MiB comes from one operation of the host that the window
and the command line call, `status`. A script that only starts three sessions, gives each a first turn and asks for the status once (the reply is 1,026 characters) moves the host's
`VmHWM` by about 7 MiB and its resident size by 4 to 8 KiB: on the build from before the listener (S02) 5,568 KiB resident and a peak of 12,288 KiB before and after (5,576 KiB resident
after the call); on the tip 6,144 KiB resident and a peak of 13,248 KiB (6,148 KiB after the call). So the transient is older than the listener, comes and goes with one call, and the
peak stays in `VmHWM`. The two builds differ by about 0.6 MiB resident and 1 MiB of peak (all that P03 added, not the listener alone; the memory with 10 sessions before and after the listener is in the folder named below). It is not new in P03 and the stop line is about growth,
so nothing is changed; the figure is here so that nobody reads the 13 MiB of `VmHWM` as the cost of the tool connection.

**Not measured.** More than three sessions in the traffic (the memory with 10 sessions before and after the listener is `../20261026/00002_s02_listener_memory`); a window with a
session open and its history drawn (the window idle figures of the phase report are those of the first view); a traffic of the real program; more than ten minutes.
