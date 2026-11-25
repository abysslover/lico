# M24-P11a: the time of each line (the transcript view, first part)

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P11a-time-of-each-line` from `phase/M24-P10c-directories-menu-compress-and-extract` (`e2581e6`); code `aaf3573`; not pushed (P07 to P11 go out with the release `0.8.2`, under the owner's advance yes of 2026-11-25)
- Source: the owner's words of 2026-11-24 (quoted in the [plan](../../../plans/202611/20261125/20261125_08_M24-P11a_time_of_each_line_plan.md)): "...당시 년월일시분초를 남기는 것이 자원을 많이 안 쓰면 표시를 해주면 좋겠고..." and "이게 0.8.2의 내용"
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees (READ; MEASURED in the window test)

Nothing yet, on purpose: this phase gives the next one (P11b, the date and time beside each block) something true to draw. What changed under the window:

| | Before | Now |
|---|---|---|
| When a line was written | not kept anywhere (the log's records are `[length][sequence][bytes]`, the protocol's lines `[sequence, text]`) | the host notes the **second** of every record it writes, as **runs** (a run begins only when the second changes: a hundred lines in one second are one run, 16 bytes) |
| Where it is kept | | an owner-only file beside the log folder, `<log folder>.times/<session>.times` (folder `0700`, file `0600`); **the log folder holds nothing new**, so a build that does not know the file still starts |
| How the window gets it | | a new read-only request `stamps` (`session`, `from`, `until`); the window asks about once a second, one request at a time, newest lines first, at most 2,000 lines a request, only for what its view holds and does not know; a complete view asks nothing |
| An old session | | its old lines have no time (the log never had them) and are shown as nothing; its new lines are timed |
| When it goes wrong | | a time **never ends a session**: a missing, torn, linked or unwritable file means no time, said once on the host's error output |

## 2. How it is built (READ)

- `stamps/mod.rs` (a leaf module, std only): `Stamps` (the runs: `note`, `at`, `between`, `drop_before`, `drop_from`, `merge`) and `Book` (a `Stamps` with its file: `new` reads it tolerantly and repairs it, `note` appends, `drop_before` trims and compacts when the file holds more than twice what is kept and 256 more, `drop_from` forgets the runs of records a torn log tail took away, `purge`, and `sweep` for the start of the host). The file is opened without following links; an id that is not letters, digits, `-` and `_` (or is longer than 128) keeps its runs in memory only; a file with more than 262,144 runs gives its newest.
- `session.rs` (kept at its 800 lines by joining lines): `Log` holds a `Book`; the one place that writes a record notes it, the prune trims, the purge removes, the recovery drops the runs past the last record the log still holds. `core.rs` answers `stamps`; `ipc.rs` checks it (all three fields required, the numbers `0` or more, an extra field refused); `host.rs` sweeps at start; the reference schema `v1.py` and the protocol note `0.1.14` have the request and the reply.
- `gui/times.rs`: `Times` (the runs, the range `lo` to `hi` they answer for, the request awaited) with `next` (what to ask, or nothing), `on_reply`, `at`, `json`; hooked on joined lines of `ui.rs` (the tick, the state dump) and an arm of the socket reply chain in `mod.rs`.
- The module graph is unchanged (the cycle stays at 50: `stamps` is a leaf). The size ratchet holds (`session.rs` `800`, `mod.rs` `797`, `ui.rs` `1114`, `core.rs` `909` of `941`).

## 3. Checks (MEASURED)

- **Tests first**: the `14` unit tests of `stamps` were run on a stub and all failed, then green; later tests were added for what the mutants showed; the host tests (`tests/test_stamps.py`, `8`) failed on the old host (the request is unknown) and the window test (`tests/test_stamps_window.py`) on the old window.
- **The tests now**: `43` unit tests (`stamps`, 26, and `gui::times`, 17), `8` host tests with the real host and the simulated provider (the runs of a session and their range, a restart and a crash keep them, a torn file is repaired, a log without the file answers no time, a purge and the start remove files, a bad request is refused, the runs of what the log dropped go with it, the runs of records a cut log lost are forgotten and the next records are timed again) and `1` window test (the view's lines are covered, new ones are followed, nothing is asked while the view is complete).
- **Mutation checks**: **unit** 103 mutants in the first run (82 killed, 21 survived). The survivors showed code that did nothing (a limit-of-zero guard, two bound checks, a one-record read allowance, a guard for a purged book, a returned flag) which was removed or joined, and real test gaps (a link at the work file, a rewrite that cannot finish, an open that fails, a rewrite's count of the file, a view next to the known range, a wait for another session, the counters across a restart, the file made right after a huge number) which were closed with tests; the rerun on the changed code was 76 mutants, 74 killed at once, one more killed by a new assertion, and one equivalent written down (the work file made with `create` instead of `create_new`: the stale one is removed first, so only a race between the two calls could tell). **host** 23 in the first run, 22 killed; the survivor (a restart that forgets three records too many) was closed by a log cut right after the first record of a run, and the three file-mode mutants moved to the host tests (the unit tests run under a umask of `0077` that hides a wider mode), 5 of 5 killed in the rerun. **window** 8 mutants, 7 killed; the survivor (the window always passing the same tick) is written down: the retry timing is pinned by the unit tests of `next`, and the window test cannot make the real host lose an answer.
- **Other tests**: the tests that read the protocol (conformance, the control parity, the reference mapping), the host's core and recovery tests, the size ratchet and the hostile-input test pass.
- **The commit hook**: the hook's run refused once on the build's peak of 1701.5 MiB over the bound of 1536 MiB (632 tests passed, nothing saved); the bound stays; the same command run by hand with the build cached passed (632 tests in 130 files, 177 s, peak 1019 MiB) and the commit then passed from the receipts.

## 4. What is not decided (the owner's)

- Where in the block the time stands, its size and how it looks in a narrow window are P11b's.
- A session made before this version has no times for its old lines; nothing can recover them.
- A session on another machine (ssh) answers for itself: an older host that does not know `stamps` makes the window ask again no sooner than every ten seconds (each try is one refused line).
- The transcript's blocks, the whole-block select and fold (P11c), and the file under the pointer (P11d) come next; the release is `0.8.2`.
