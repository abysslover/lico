# Host language decision (M01-P02 S01)

Measured at commit `b3b12ee` on the shared host, one virtual run per cell, three
repetitions, synthetic records of 1 KiB, history 500 records per session, retain
window 2048. Every implementation passes the same protocol conformance suite
(`tests/test_protocol_conformance.py`). Values are resident memory of the host
process (RSS) in MiB; nothing here is a target.

| Implementation | Binary | 10 sessions, idle | 100 sessions, idle | 100 sessions, 5 producing, 10 streaming | streaming CPU |
|---|---|---|---|---|---|
| Python reference oracle | n/a | 27.4 | 77.4 | 84.5 | 3.6% |
| Rust, standard library only | 373 KB | 7.2 | 54.0 | 61.1 | 4.3% |
| Rust with serde_json 1.0.154 | 397 KB | 7.1 | 53.4 | 60.7 | 3.5% |
| C (POSIX threads) | 26 KB | 6.5 | 50.8 | 57.5 | 4.0% |

## What the numbers say

- The language floor is small in all three native cases. The C core is smaller by
  about 350 KB of binary and about 0.7 MiB (10 sessions) to 3 MiB (100 sessions) of
  resident memory; serde_json adds 24 KB and under 1 MiB over the standard-library
  Rust core.
- At 100 sessions about 49 MiB of every native figure is the retained record
  window itself (500 records of 1 KiB for each of 100 sessions). Host memory is set
  by how much output is kept in memory, not by the language: keeping the default
  window of 2048 records would hold about 200 MiB at 100 sessions.
- The Python oracle costs 20 to 27 MiB more than the native cores, the cost of its
  interpreter and objects.
- Streaming CPU is about 4% of one core for every implementation at this load.

## Decision

The host is written in **Rust with serde_json**. The difference to C is not
material against memory safety for a process that parses authenticated input and
will parse provider protocols later; serde_json costs 24 KB and replaces a
hand-written parser that only handles flat messages. The standard-library-only
core stays in the tree as the measured baseline. No size or memory budget has been
set by the owner; none is invented, and the owner may still set one.

## Memory design that follows

- The host keeps only a **bounded recent window** per session in memory (a few
  dozen records beyond the snapshot lines, configurable) and serves older replay
  from the durable log on disk. The window, the subscriber queues and the snapshot
  are all bounded.
- The host's RSS at 10 and at 100 sessions, with and without producing sessions,
  is reported again with the real host, so a host that grows with retained history
  is caught.

Limits of this comparison: payloads are generated from the sequence number, the
cores keep no durable log and run no agent processes, threads are used instead of
an event loop, and the measurement shares the machine with other work.
