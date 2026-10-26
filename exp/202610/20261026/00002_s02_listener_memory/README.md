# 00002 s02_listener_memory

Version 1.0.0

**Question.** Does the tool listener (P03 step S02) keep a host with 10 Claude sessions within 2 MiB of resident memory of the same host without it
(the stop line of the plan), and what does a burst of connections add?

**Workload.** `measure.py`, not part of the suite: a private host, 10 Claude sessions (the simulator), one turn each; idle 3 s; then
VmRSS, VmHWM, threads and open descriptors of the host from `/proc`. `before` is a host built from the commit before S02 (`1ab1cdf`); `after` is the host
with the listener, which in addition runs one `tools` turn in every session (initialize, initialized, list, call over HTTP) and holds 32 connections open at
once (the listener's cap) with a head that never ends, then closes them. 3 runs of each, one host per run.

**Observed result (2026-10-26, one machine, the simulator as the program).** Host RSS in MiB, three runs:

| | before | after |
|---|---|---|
| idle, 10 sessions | 6.19, 6.00, 6.38 | 6.19, 6.00, 6.19 |
| after 10 tool turns | | 6.19, 6.19, 6.19 |
| 32 connections held | | 6.19, 6.38, 6.56 (threads 58) |
| 3 s after they close | | 6.19, 6.56, 6.56 (threads 26) |

Threads 25 before, 26 after (the accept thread); descriptors 37 and 38. The held connections each have a thread of 256 KiB stack, of which only the pages used count:
the host grew by 0.2 to 0.4 MiB while 32 were held, and its thread count went back at once when they closed. The stop line (+2 MiB) holds with room. The size of the
`tools/list` result is 218 bytes for the one tool there is (the limit is 2,048 for all five).

**Not measured.** A real program in the sessions (each real program is a process of its own, outside the host); more than 10 sessions; a long run.
