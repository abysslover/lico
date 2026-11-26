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

Since M25-P05 the window uses it: the `agy` picture of New session starts `session.create kind=agy mode=default` at once in the folder the left lists (no account step, no model), and a card of an
`agy` conversation in Chats resumes it by `resume_id` in the same mode (a conversation whose folder is gone is brought back in a workspace of the host's own). `session.chats` lists a session of kind
`agy` as a card with no account and no label, and a conversation that a session holds is one card (the program's own file gives the title and the folder, the session gives `open_session`, `open_state`
and the later time). The tab begins with `A`, and the strip, the header and the list heading say `agy` with no account word. The terminal path of levels A and B for a conversation is gone; the Terminal
picture, typing `agy` in a terminal and the look for the program stay.

Since M25-P06 an `agy` session is a peer like the others: Send to, an `@name` of the person and a Call Room work between it and a Claude, a Codex or another `agy` session (`calls::takes_part` is the one
question the host asks; the Call Room files the session as `agy`). The program has no tool of Lico's (its tool servers are empty), so after an inline `@name` the host tells it that a paragraph of its answer
that begins with `@name` at the start of the line is sent to that session (`calls::mention_hint`, `mention::dispatch_assistant_mentions`), where a program with tools is told to call `mcp__lico__send`; what
an `agy` session says in answer to a room's turn is the reply, recorded in the room and returned to the asker. Handing over a whole `agy` conversation is not supported, as for Codex. `tests/test_agy_peers.py`
tries it with the simulator; what the real program does with the hint is the owner's check (gate G3).
