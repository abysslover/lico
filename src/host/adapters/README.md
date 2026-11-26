# Provider adapters (M01-P03)

Provider paths are versioned data plus a mapping, never a guess. `codex/capabilities.json`
records what the installed Codex app-server offers (pinned to its exact version, with the hash
of the protocol schema the installed binary generates), how each client request, server request
and notification is handled, what is unsupported and why, and what is **unverified**.
`python -m lico_measure providers codex` regenerates it; a test fails when the installed
provider no longer matches the record.

Everything about an installed provider is read with a private empty home and a minimal
environment: no sign-in, no credential and no model turn. A provider's login state belongs to
the provider and is never read or copied by Lico.

`src/lico_protocol/codex_mapping.py` is the reference mapping from provider messages to host
records and approvals (one UTF-8 line per record, escape sequences and control characters
removed, an approval answer bound to exactly one provider request). The fixtures under
`tests/fixtures/codex_app_server/` are recorded sequences validated against the provider's own
schema; the owned host's adapter is tested against the same fixtures.

What the reference does with a line of text from the agent (what it says, what a command prints) is in `tests/fixtures/provider_text.json`: the line, and the
record it becomes (cleaned of escapes, controls and characters that draw nothing; `[agent] ` in front when it begins like a record of the host). The same
file is read by the reference's test, by a unit test of `hostd/src/leads.rs` and by `tests/test_forged_leads.py`, which runs both adapters of the built program.

## Claude Code (second adapter)

`claude/capabilities.json` pins Claude Code 2.1.310 (the version the owner reported; the conversation reader of the host
also reads the records of 2.1.311, see `exp/202610/20261027/00003_record_layout_2_1_311/`) and lists what is
mapped, refused, unsupported and **unverified**. The message shapes come from the provider's
documentation, not from a recording; `tests/fixtures/claude_stream/` says so, and the permission
request subtype it uses (`can_use_tool`) is an assumption the documentation does not confirm.
`claude/simulator.py` stands in for `claude -p --input-format stream-json` in tests. The adapter
(`hostd/src/claude.rs`) must produce the records `lico_protocol/claude_mapping.py` produces.

## `agy` (third adapter, M25)

`agy/capabilities.json` pins `agy 1.5.17` and speaks the Agent Client Protocol (`agy acp`, JSON-RPC 2.0, one object per line). What is observed with the installed program (a private
empty home, no sign-in, no turn) is the `initialize` answer, the keys of the `session/new` answer (it works signed out) and the error codes; every shape of a turn is from the ACP
documentation and on the record's **unverified** list. `src/lico_protocol/agy_mapping.py` is the reference mapping (a line is emitted when it is complete, what `session/load`
replays is not copied, an answer is never wider than the person's, a stop reason is a plain note), `tests/fixtures/agy_acp/` holds three hand-made sequences with their records, and
`agy/simulator.py` stands in for `agy acp` in tests. The adapter (`hostd/src/agy/`, a driver kind of `driver.rs`) must produce the records the reference produces.

The adapter exists since M25-P02: `map.rs` is the Rust port of the reference (the unit tests read the same three fixtures), `conn.rs` is the order of the protocol with no input or
output of its own, `mod.rs` is the process and the session. The host takes `--agy PATH` (default `agy`) and `--agy-arg ARG` (repeatable, put before the fixed `acp --disable-telemetry`;
the tests pass the interpreter and the simulator this way); `--dangerously-skip-permissions` is never passed, and the child gets only `PATH`, `LANG`, `HOME` and the `XDG_*`
directories. The simulator logs how it was started (`argv`, the folder, the NAMES of the environment, never a value) as the first line of its `--received` file, and
`<received>.hang` makes it never answer `initialize`. `tests/test_agy_session.py` tries the session with it; the real program's turns are not verified (the owner's hands-on check).

Since M25-P03 a permission request of the program is an approval of the session: in the mode `default` the person is asked (`ev: approval`, `approval.answer`), in the mode `auto` the host answers
the one-shot yes at once and writes `[auto-allowed] <prompt>` first. Either way the answer is the `allow_once` or `reject_once` option of that one request (`cancelled` when the program
offered none), never an `always` option, and a request the turn left unanswered is over with the turn. The mode is the session's own (`session.create`, `session.mode`); the program is never
started with the flag that skips its permissions. In the simulator the mode `fixture` waits for the host's answer to the request and the mode `hostile` never does.

Since M25-P04 `session.create kind=agy` takes `resume_id`, the id of a conversation of the program (the text every provider id is checked as): the session starts by `session/resume` with that id
(then `session/load` when the program cannot resume it) and never by `session/new`, and an id the program does not know ends as `[error] agy could not bring this conversation back; start a new
session`. One session of the host holds a conversation at a time: a second create of the same id answers with it (`resumed`, `already_open`), a disconnected holder is resumed in place. The id is
the registry's `provider_session`, so a host that was killed brings the session back disconnected and `session.resume` asks for the same conversation. `tests/test_agy_resume.py` tries it with the simulator, which keeps its conversations in `<received>.sessions`.

