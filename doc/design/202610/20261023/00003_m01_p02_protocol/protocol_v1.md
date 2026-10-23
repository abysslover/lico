# Lico control protocol v1 (M01-P02 S01)

Status: proposed with M01-P02; the Python reference in `src/lico_host` is the
oracle and `src/lico_protocol/v1.py` holds the machine-readable schemas.

## Transport and framing

One JSON object per line (UTF-8, `\n`) over an owner-only Unix socket in a
directory with mode 0700 (socket mode 0600; the host also checks the peer user).
A request line is at most 4096 bytes: a longer one gets `too_large` and the
connection is closed. A reply is queued on the same connection as events, so a
client reads lines until it finds the reply (it has `ok`); events have `ev` and
no `ok`.

## Messages

Every message carries `v` = 1. A request has `op` and may have `id` (a string of
at most 64 characters, echoed in its reply). Unknown fields are rejected in
requests (`bad_request`) and a reply that carries one fails conformance. `ok` is
the integer 1 or 0 so the prototype clients keep working.

Errors: `bad_request`, `unknown_op`, `unknown_session`, `stale_generation`,
`version_unsupported`, `too_large`. An error never reveals whether a guessed
session id exists beyond `unknown_session`, and a rejected request has no effect.

## Operation inventory (the complete route list)

| Operation | State | Mutates | Meaning |
|---|---|---|---|
| `hello` | oracle | no | protocol version, limits, retain window, operation list |
| `status` | oracle | no | per-session id, generation, last sequence, subscribers, bytes (the host adds `oldest` retained sequence and `state`) |
| `snapshot` | oracle | no | latest screen lines and last sequence of one session |
| `subscribe` | oracle | yes | start the live stream; `after` is the client cursor; `snapshot: true` asks for the latest screen first |
| `unsubscribe` | oracle | yes | stop the stream, keep the cursor |
| `session.create` | host | yes | start a synthetic agent session (`kind` `synthetic`, optional `rate`, `history`, `payload`) |
| `session.list` | host | no | id, generation, state (`running` or `exited`) and last sequence of every session |
| `input`, `approval.answer`, `view.attach`, `view.detach`, `resources` | planned S03 | | input with request id and generation, approvals, views, resource report |

Anything not in this table is refused as `unknown_op`.

## Sessions, generations and cursors

A session id is `s` plus 16 random hex digits and is never derived from an
index. The generation starts at 1 and a mutation that names another generation
is refused as `stale_generation`. The sequence of a session starts at 0 and
rises by one per record; the host keeps the most recent `retain` records for
replay.

`subscribe` rules: with a cursor inside the retained window and no snapshot
request, the reply has `gap: false` and `from: cursor + 1`, and records after the
cursor follow with no gap and no duplicate. A cursor older than the window or
beyond the last sequence is a gap: the reply carries a snapshot and `from` is the
snapshot sequence plus one. With `snapshot: true` the reply always carries the
latest screen and `missed` says how many records the client had not seen (null
when the cursor is unusable), and live records follow it. A stalled subscriber
gets an `overflow` event and must resubscribe with a snapshot; the host never
waits for it.
