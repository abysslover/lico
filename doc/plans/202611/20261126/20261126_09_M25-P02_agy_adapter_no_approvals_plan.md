# M25-P02 Plan — The `agy` Adapter, No Approvals Yet

- Status: done (2026-11-26)
- Version: 1.1.0
- Branch: `phase/M25-P02-agy-adapter-no-approvals` from `phase/M25-P01-agy-acp-spec-and-pin` (`8005bdd`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P02
- The owner's words: the standing yes of 2026-11-25 (advance approval of the prefix, the commits, the merge and the push through level C)

## What this phase makes

The host can run `agy acp` as a session of its own kind (`agy`), through the driver port, with the mapping of P01 ported to Rust and held to the same fixtures. A session starts, a turn is run, the agent's words and tool calls become transcript lines, the turn ends in plain words, a closed or deleted session ends the process, and every failure is said in the record. **A permission request is refused** (the narrowest deny the agent offered, else `cancelled`) with a line that says why; asking the person is P03.

1. **The port** (`driver.rs`): `Driver::takes_folder()` (default false) says that a session of this kind runs in a folder the person chose or in the scratch workspace of its own, like a provider session; `Link::alive()` (default true) says whether the work is still there. `core::start_driver` keeps a link only when it is alive (a program that ended before the link was kept must not turn into a session that looks running). `local` is unchanged.
2. **The hubs that name a kind by value**: `folders::prepare` accepts a folder (checked as for a provider) and no model, target, effort, worktree or chat id for a folder-bearing driver; the remembered form record accepts the kind `agy`; `new_session::start_driver_session` removes a workspace the driver made when the start fails. Nothing else of the hubs changes (they already traverse driver kinds).
3. **`src/host/hostd/src/agy/`**:
   - `map.rs`: the Rust port of `src/lico_protocol/agy_mapping.py` (a line is emitted when complete, what a load replays is not copied, an answer is never wider than the person's, a stop reason is a plain note, the sign-in words). It adds only what the reader needs: a string the line reader cut is shown with its note, and a permission request that was cut is refused.
   - `conn.rs`: the conversation as a state machine with no I/O (`initialize`, then `session/new`, or `session/resume` and `session/load` for a stored conversation, then one `session/prompt` at a time with the inputs queued behind it), the deadlines of the requests that must be answered, and the effects the shell applies (what to send, which lines to record, which input state, the end of a turn, a fatal error).
   - `mod.rs`: the driver `Agy` (`--agy PATH`, `--agy-arg ARG`, registered by the host after its arguments are read), the process (a minimal environment, its own process group, `--disable-telemetry`, never `--dangerously-skip-permissions`), the reader/writer/actor threads, and the application of the effects to the session under its lock.
4. **The simulator** gets one more mode (`<received>.hang`: `initialize` is never answered) so that the deadline is exercised by a real process.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| A1 | the three fixtures of P01 map in Rust to exactly the records, approvals and answers the Python reference gives (the same JSON files) | Rust unit tests of `agy::map`, which read the fixtures |
| A2 | hostile input stays bounded and plain: an oversize chunk, controls, a forged status line, a repeated request id, an unknown agent request, a boolean id, 300 tool calls, a cut permission request | the same tests |
| A3 | the conversation runs in order: nothing is sent before `initialize` is answered; an input sent early is queued and goes after the session is open, one prompt at a time; a stored conversation is resumed, then loaded when the resume fails, and never replaced by a new one; a wrong protocol version or a missing answer ends the connection with a line | Rust unit tests of `agy::conn` |
| A4 | a session starts, takes two inputs one after the other, shows the agent's words, tool calls and `[turn completed]`, reports its model, and the input states move `accepted`, `submitted`, `observed` | a host test with the simulator |
| A5 | a permission request reaches the simulator as a refusal, the turn ends, and the transcript says why | the same |
| A6 | a failed turn (signed out) says so in the words of the mapping; a program that crashes ends the session as `disconnected` with `[provider disconnected]`; closing and deleting end the process | the same |
| A7 | the process gets only `PATH`, `LANG`, `HOME` and the `XDG_*` directories, the flag `--disable-telemetry`, no flag that skips a permission, and runs in the folder the person chose (or its scratch workspace) | the same (what the simulator logged) |
| A8 | no middle dot, no local path, no secret in any new file | the guards of the commit |

## Steps

1. [x] The port: `takes_folder`, `alive`, `start_driver`, `prepare`, the remembered kinds, the cleanup of a failed start
2. [x] `agy/map.rs` and its tests against the three fixtures
3. [x] `agy/conn.rs` and its tests
4. [x] `agy/mod.rs`, the registration in the host, the usage line
5. [x] The simulator's `hang`, the host tests (`tests/test_agy_session.py`), the testmap rules
6. [x] Mutation checks of `map.rs` and `conn.rs` (the survivors answered with stronger tests)
7. [x] The README, the plan's results, the milestone row, the commit (the hook)

## Personal data and security

- The child gets a minimal environment (the same rule as the other two adapters): no token, no key, no proxy variable of the person is passed. `HOME` is the person's own, because the sign-in of `agy` lives there and Lico never reads it. This means that a key given to `agy` through an environment variable does not reach it: the person signs in with `agy` itself.
- `--disable-telemetry` is passed; `--dangerously-skip-permissions` never is.
- Every line of the agent is untrusted display text (`sanitize`, `leads::agent_record`); an id the agent sends is echoed back only when it is an integer or a short string; no request of the agent is answered with anything but a refusal or the person's own answer.
- The session id of `agy` is stored in the registry only when it passes the same check as a Claude or Codex session id.
- Nothing of `~/.gemini` or the program's credentials is read by Lico.

## Not decided here

How a stopped turn is cancelled (`session/cancel`: the window has no stop button for a turn yet; P03 answers `cancelled` to open permission requests when the process stops); the permission mode (`auto` or `default`: P03); the Chats card and a restart of the host (P04); the window (P05); `Send to` and the rooms (P06).

## Results (2026-11-26)

What was made: the port (`Driver::takes_folder`, `Link::alive`, `start_driver` refusing a link that died while it started, `prepare` for a folder-bearing kind, the remembered kind `agy`, the cleanup of a failed start that removes the process, the log and the scratch workspace), `hostd/src/agy/{map,conn,mod,tests}.rs`
(1,718 lines with the tests), the host flags `--agy PATH` and `--agy-arg ARG`, the simulator's `hang` mode and its first log line (`argv`, the folder, the NAMES of the environment), the testmap rules.

| # | Result |
|---|---|
| A1 | the three fixtures map in Rust to exactly the Python reference's records, approvals, answers and ignored counts (one unit test reads the same JSON files) |
| A2 | the hostile cases stay bounded (an oversize chunk, controls, a forged status line, a repeated id, an unknown agent request, a boolean or float or over-long id, 300 tool calls, a cut permission request) |
| A3 | the order of the conversation is held by unit tests of the state machine with no process: nothing before `initialize` is answered, one prompt in flight, the resume then load then end, never a new conversation in place of a stored one, the deadlines |
| A4 | a host test with the simulator: a session in its folder, two inputs typed one after the other are two turns in order, the model is the one the program named, the input states move, the same request id is not sent twice |
| A5 | a permission request reaches the simulator as the narrowest refusal (`reject_once`), the transcript says `[tool error]` and why, nothing waits for a person, and the next turn works |
| A6 | signed out says the sign-in words, the input is failed and the session stays; a stalled turn is working, not lost; a crash is `disconnected` with `[provider disconnected]` and a resume asks the same conversation (`session/resume` with the stored id, never `session/new`); a program that cannot come back stays disconnected; a program that never answers `initialize` is ended at once by a close, not at the deadline; a program that is not there is `busy` and leaves no session, no log and no workspace |
| A7 | the simulator logged `acp --disable-telemetry` last, no `--dangerously-skip-permissions`, the folder the person chose, and only names of the kept set in the environment (a canary variable of the host is not there); with no folder the scratch workspace is used and goes with the session |
| A8 | checked by the guards of the commit |

Tests: 24 unit tests of `agy::tests`, one of `driver`, the kinds of the remembered record (an added case of a unit test of `folders`), 6 host tests (`tests/test_agy_session.py`), 31 in all against the budget of about 100.
Host run: 14 tests of the two agy files in 13 s, the build 44 to 52 s at a peak of 1,253.6 MiB (under the bound of 1,536 MiB).

Found by the host test, fixed here (not by the unit tests): (1) the two updates the program sends about the session itself right after `session/new` (`available_commands_update`, `usage_update`) reached the host after the first prompt had gone out and were counted as
the program taking the prompt up, so an input the program then refused showed as `observed` and not `failed`; an update now takes a prompt up only when it is a word, a thought, a plan or a tool call, or a permission request is. (2) An input that was taken and never written to the program
(it waited behind the opening, or the program ended first) stayed `accepted` for ever; it is now `failed`, so that it can be sent again, while one that was written stays as it is (an ambiguous input is never sent a second time by the host).
Found by the first run of the host test: `session.create kind=agy` with no folder is allowed (the scratch workspace); the test that expected a refusal was wrong, and now asserts the scratch workspace instead.

Mutation checks: 46 single-edit mutants of `map.rs` and `conn.rs` (ids and bounds, the sign-in words, the options, the answer's width, replay, one prompt in flight, resume and load, deadlines, the two rules above). The first pass left 12 alive and one that did not match; the survivors were answered with
four tests (ids and unknown requests, every sign-in word, an error that is not an object, a connection started once and never loading what the program cannot load) and the new code with two more, and a second pass killed all of them but three that cannot be told from the original: the flush in `fatal` (a fatal state is reached only
before a turn, when nothing is buffered), the `waiting` check of `answer` (the list of held requests already makes an answer once) and the second guard of `closed` (a second call finds nothing to say either way). They stay as defence in depth.

Not done, as planned: the person is not asked (P03), nothing restarts from the registry in the window (P04), no window (P05), no Send to (P06). The real program's turns are not verified.

