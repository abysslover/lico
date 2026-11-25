# M24-P11a Plan — The Transcript View: The Time Of Each Line

- Status: in_progress (2026-11-25)
- Version: 1.0.0
- Branch: `phase/M24-P11a-time-of-each-line` from `phase/M24-P10c-directories-menu-compress-and-extract` (`e2581e6`; P07 to P10c are not pushed yet, they go out with the release 0.8.2)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row (content of the release 0.8.2)
- The owner's words (2026-11-24, the content of 0.8.2): "각 대화 블록의 너비를 풀로 쓰지 않고 오른쪽에 어느 정도 일정 간격을 남기고 거기에 당시 년월일시분초를 남기는 것이 자원을 많이 안 쓰면 표시를 해주면 좋겠고, 그게 내가 보낸 것인지, peer 끼리 보낸 메시지인지, 응답인지, 뭔가 command 실행인지에 따라서 배경에 보이는 색상이 달라지는 형태로 보이고 대화를 클릭하면 현재 한줄씩 클릭 비슷하게 되는데 해당 대화 전체가 통으로 선택되어 펼치기 닫히기 가능(표시 메모리 줄이기?), 다만 파일 등에 마우스를 올리면 밑줄 생기고 파일을 누르면 탭이 열러서 볼 수 있게" and "이게 0.8.2의 내용". The owner's standing yes (2026-11-25): the guard prefix `phase/M24-P11a-` is added without a question (done).
- No version bump (the release 0.8.2 carries it).

## The transcript view in five phases (the lead's reading; the owner may correct)

| Phase | What the person gets |
|---|---|
| **P11a (this one)** | the host remembers **when** each record of a session was written (to the second), and the window can ask for it; nothing new is drawn yet |
| P11b | each block of the conversation stops short of the right edge and the **date and time to the second** stand in the gap; the block's background tells who spoke: me, a peer, the agent's reply, a command or tool |
| P11c | a click on a block selects the **whole block** (not one line) and folds or unfolds it (a folded block holds one line of the display) |
| P11d | a file name under the pointer is underlined, and a click on it opens the file in a tab |
| P12 | the release 0.8.2 |

The order is by what depends on what: P11b needs the times; P11c and P11d need the block's lines to be known as one thing (P11b makes the block a unit of the layout).

## Why this phase is its own (what was found)

- The log of a session (`.seg` files: `[u32 length][u64 sequence][bytes]`) carries **no time**, and the protocol's lines (`rec`, `snapshot`, `history`) are `[sequence, text]`: the schema fixes their length at two. A time cannot be put into them without breaking every reader, and a log written by an older build has none.
- So the times are kept **beside** the log, and asked for on their own, so that nothing that reads a log or a line changes.

## What this phase does (decisions D1 to D7)

| # | Decision | Detail |
|---|---|---|
| D1 | The host notes the **time (unix seconds) of every record it writes**, as **runs**: a run is `(first sequence, seconds)` and a new run begins only when the second differs from the last run's, so a burst of a hundred lines in one second is one run | the cost is one comparison per record and 16 bytes per second of activity |
| D2 | The runs are kept in memory (all that the log still holds) and **appended to a file beside the log folder**: `<log folder>.times/<session>.times`, records of `[u64 sequence][u64 seconds]` little-endian, the folder `0700` and the file `0600`; the log folder itself holds nothing new, so a build that does not know the file still starts | a downgrade must not refuse to start on a file it does not know |
| D3 | Reading the file is **tolerant**: a torn last record is ignored, a record that does not go forward is ignored, a missing or unreadable file means "no time" for the records before the first run; the file is rewritten without what the log no longer holds (when the log drops an old segment and the file is twice what is kept); a write that fails is said once on the host's error output and the runs go on in memory; **a time never ends a session** | the times are a gift, not a record of truth |
| D4 | **Purge** (deleting a session) removes the file; the start of the host removes the files of sessions that are not registered | the same lifetime as the log |
| D5 | A new request of the protocol, **`stamps`**: `{"op":"stamps","session":ID,"from":N,"until":M}` answers `{"ok":1,"session":ID,"stamps":[[seq,secs],...],"from":N,"until":M}` with the run that covers `from` and every later run that begins before `until`, at most 2,048; read-only; the schema, the list of requests, the request check and the protocol note are brought along | one more optional reading, nothing existing changes |
| D6 | The window keeps, for the session it shows, the runs it was given and the **range of sequence numbers they cover** (`lo` to `hi`); once a second (the window's own tick) it asks for what it holds and does not know yet: the new lines after `hi`, the old lines before `lo` (after the history was read back); one request at a time; an answer that does not come (an older host) is asked again no sooner than ten seconds later; **`at(seq)` is the time of the run that covers `seq`, and nothing where the range does not reach or no run begins before it** | the live lines are timed by the host, the same on a second look |
| D7 | The window's state dump gets `times` (the range, the number of runs, the last run) so the tests can read it; nothing is drawn | P11b draws |

## The pieces

1. `stamps.rs` (a leaf module, std only, no window): `Stamps` (the runs: `note`, `at`, `between`, `drop_before`, `merge`) and `Book` (a `Stamps` with its file: `new` reads it, `note` appends, `drop_before` trims and compacts, `purge` removes, `sweep` removes the files of unregistered sessions); unit tests over a temporary folder: runs begin only on a new second, `at` at the edges, a torn tail and a backward record ignored, a file that cannot be written, a compaction that keeps what is kept, the file's modes.
2. `session.rs`: `Log` holds a `Book`; the one place that writes a record (`Log::append`) notes it, `Log::prune` trims, `Log::purge` removes; `Session` gets one reading accessor; the file stays at 800 lines (lines are joined, none is added). `host.rs` sweeps at start; `core.rs` answers `stamps` (the list of requests, the reply); `ipc.rs` checks the request; the reference schema `v1.py` and the protocol note get the request and the reply.
3. `gui/times.rs` (the window's part): `Times` (the runs, `lo`, `hi`, the pending request), `due(view)` and `ask(view)` as pure functions, `on_reply`, `at`; hooked on existing lines of `ui.rs` (its size is pinned), `mod.rs` (pinned at 800) and the socket reply chain.
4. Tests: unit (`stamps`, `times`), a real-host test (`tests/test_stamps.py`: a session's lines come back with times; a restart keeps them; a purge removes the file; an old log without the file answers no time; a bad request is refused), and one window test (the dump's `times` covers the lines of a session and grows with new ones).

## Steps

1. [ ] **Tests first, run red** (unit, host, window).
2. [ ] **The change.**
3. [ ] **Mutation checks**, one edit at a time on `stamps.rs`, `times.rs` and the lines added to `session.rs` and `core.rs`.
4. [ ] **Other tests** that read the protocol (the schema conformance, the list of requests) still pass.
5. [ ] **Words**: the guide, the protocol note.
6. [ ] `cargo xtask check` and `graph` (the cycle stays at 50: `stamps` is a leaf), the hook, `~/bin/lico` rebuilt, the report.

## Not decided here

- Where in the block the time stands, its size, and how it looks in a narrow window are P11b's.
- A session made before this phase has no times for its old lines (the log never had them): they show nothing, which is honest.
- Sessions on another machine (ssh) answer for themselves: an older host that does not know `stamps` makes the window stop asking (ten seconds between tries).
