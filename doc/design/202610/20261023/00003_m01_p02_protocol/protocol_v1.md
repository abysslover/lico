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
| `session.create` | host | yes | start a synthetic agent session (`kind` `synthetic`, optional `rate`, `history`, `payload`) or a provider session (`kind` `codex`: one provider process and one thread) |
| `session.purge` | host | yes | delete a session: its process is ended, its log segments and its workspace are removed and the session is forgotten; nothing of any other session is touched and nothing of this one can be written again (local socket and `lico session purge` only, not on HTTP) |
| `session.resume` | host | yes | bring a disconnected provider session back with a new provider process that resumes the same thread; the execution generation rises by one, so every older target is stale |
| `account.create` | host | yes | add an account of a provider (`codex` or `claude`) with an editable label (at most 40 printable characters); a stable opaque id (`a` and twelve hex digits) is returned, a private home is created for it (mode 0700, never read by Lico), and a provider's first account becomes its default (local socket only) |
| `account.list` | host | no | the accounts with provider, label, whether each is its provider's default and its credential generation |
| `account.default` | host | yes | make an account its provider's default; this affects only sessions created afterwards |
| `account.credential` | host | yes | attach a credential of the credential store to an account (its `ref` from `lico-cred`, which must belong to this account and its provider; otherwise `bad_request`); the account's credential generation rises; a Claude session of that account gets its key only through a helper that reads the keyring |
| `account.affected` | host | no | the sessions bound to an account with their states, named before anything is removed |
| `account.remove` | host | yes | remove an account; `sessions` must be chosen: `detach` leaves its sessions running (histories and work stay, the account's home is kept for them) and `end` purges them first; the reply states the separate outcomes: the local record (`metadata_removed`), the key (`credential`: `removed` or `none`), the home (`kept`, `removed`, `none`) and `remote_revocation` (always `not_performed`: revoking a login is the provider's own logout); `credential_store_unavailable` when a key exists and the credential store cannot be reached (nothing is removed then); a default that pointed at the account is cleared, never handed to another account |
| `session.rebind` | host | yes | move a session whose provider process is gone to another account as a new thread under a new generation; refused with `busy` while the session is running, even when idle (a provider holds its account's login); `unknown_account` for an account that does not exist or belongs to another provider |
| `session.list` | host | no | id, generation, state (`running`, `exited` or `disconnected`) and last sequence of every session |
| `input` | host | yes | send text to a session's agent: needs the target generation and a client request id; the reply and later `input` events give the state `accepted`, `submitted` (written to the agent) and `observed` (the agent acted on it) separately |
| `input.status` | host | no | the state of an earlier request id, so a client that lost a reply can ask instead of sending again |
| `approval.answer` | host | yes | answer a pending approval request (`allow` or `deny`) of a session |
| `view.attach`, `view.detach` | host | yes | register a view of a session (the latest screen now, live records after it); views vanish with their connection and never stop a session |
| `resources` | host | no | host process memory (RSS and PSS), threads, sessions, subscribers, views, log bytes and the memory of each agent |

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

## Input integrity

A request id is at most 64 characters of letters, digits, dash and underscore, so
it can be passed to an agent without any chance of injecting a line. The text is
non-empty, at most 512 bytes and has no control characters. The host dispatches a
request id to the agent **at most once within the last 65536 inputs of a session** (a window of 64-bit hashes; an older id is forgotten, answers `unknown_request` and would be dispatched again): a repeat of the same id (a client that
lost the reply and reconnected) is answered with the current state and
`duplicate: true` and is not sent again. A target of the wrong generation, an
unknown session or an exited session is refused (`stale_generation`,
`unknown_session`, `session_exited`) before anything is written.

## Loopback HTTP (optional)

The listener is off unless started with a port and a token file; it binds
127.0.0.1 only. The control credential is generated at start, written to the token
file (mode 0600) and never printed or placed on a command line. Requests need
`Authorization: Bearer <token>` (compared in constant time), a `Host` of exactly
`127.0.0.1:<port>` or `localhost:<port>`, and no `Origin` or exactly the same
origin; Host and Origin are checked before the credential so that rebinding and
cross-site requests are refused outright. Heads are limited to 8 KiB, bodies to
4 KiB, reads time out after 5 s, at most 32 connections are served at once and
requests are throttled to 200 per second (burst 50, then 429).

| Route | Operation |
|---|---|
| `GET /v1/status`, `GET /v1/resources` | `status`, `resources` |
| `GET /v1/sessions`, `POST /v1/sessions` | `session.list`, `session.create` |
| `GET /v1/sessions/{id}`, `GET /v1/sessions/{id}/snapshot` | one session, `snapshot` |
| `GET /v1/sessions/{id}/events?after=N` | one bounded page of records after the cursor, or a gap with a snapshot |
| `POST /v1/sessions/{id}/input` | `input` |
| `POST /v1/approvals/{n}/response` | `approval.answer` (the body names the session) |
| `POST /v1/views`, `DELETE /v1/views/{id}` | `view.attach`, `view.detach` (a view without a connection expires after 10 minutes) |

Anything else is 404, a known path with another method is 405. Errors map to
statuses: `bad_request` 400, unknown targets 404, `stale_generation` and
`session_exited` 409, `too_large` 413, `busy` 429 or 503; a missing or wrong
credential is 401 and a wrong Host or Origin 403.

## Bounds

Per connection: a request line is at most 4096 bytes; unread replies are cut off at
four times the subscriber queue (the peer is disconnected); a write that stalls for
30 s ends the connection; at most 64 views. Per host: at most 256 local connections
(`--max-connections`) and 256 sessions (`--max-sessions`) and the descriptor limit is
raised to its hard maximum; an agent's output line is limited to 16 KiB; pending
approvals are limited to 64 per session; the agent input channel holds 1024 lines
(a full channel answers `busy`). HTTP: 32 connections, one 10 s deadline for a whole
request, 200 authenticated requests per second (burst 50) and a separate budget of 20
failed credentials per second. The log directory is only cleared if it holds nothing
but session segments, and the socket directory must be owned by the user with mode
0700 or the host refuses to start. The log is written but never fsynced: it survives
a killed GUI or host process, not a power cut, and a write error ends that session.

## Provider sessions (M01-P03)

A `codex` session is backed by the provider's app-server (or its simulator) over standard
input and output. An `input` becomes one `turn/start`, or a `turn/steer` of the turn that is
running; a steer the provider refuses (the turn had just ended) becomes a turn of its own, so
an input is applied once and never twice. The input states are `accepted` (queued), `submitted`
(written to the provider), `observed` (the provider answered) and `failed` (the provider refused
it, shown as a record). An input whose answer never came stays `submitted` after a disconnect and
is never sent again. A provider that goes away makes the session `disconnected` (never idle);
`input` then answers `session_disconnected` until `session.resume`. Provider approval requests
(commands and file changes) become host approvals bound to exactly one provider request and
answered at most once; every other provider request (tokens, tools, attestation, elicitation,
permission grants) is answered with an error. Provider output is sanitized into one-line records.

## Isolation of real output (M01-P03)

A provider process gets an explicit environment: only `PATH`, `HOME`, `LANG` and, when the host has
it, `CODEX_HOME` (the provider finds its own login state through these; Lico never reads it). Nothing
else of the host's environment reaches it, and synthetic agents get only `PATH` and `LANG`. Each session
has a private workspace directory named by its session id under the host's workspace root (mode 0700),
which is the provider's working directory. The durable log of a provider session holds conversation
content: it is sensitive, owner-only (0700 directory, 0600 files), retained by record count and removed
by `session.purge`; it is never uploaded and the host has no telemetry.


## Accounts and failures (M01-P04)

A provider failure that concerns the account (authentication failed, organization not allowed, account on hold, billing, rate limit) is
recorded as `[account issue] CATEGORY`, shown on the session (`account_issue` in `status`) and against the account (`issue`,
`issue_sessions` in `account.list`), and cleared by the next successful turn. Nothing is retried under another account or another
authentication mode, a restart keeps the binding, and a session created while the provider has accounts but no default is refused
(`unknown_account`) instead of running unbound. Usage is `null` (unknown), never zero.

## Restart recovery (M01-P05)

The host keeps a registry of its provider sessions (`registry/sessions.json`, owner-only, replaced atomically within a second of a change
and at once on create and purge): id, provider, generation, account binding, credential generation, provider session id and the time of
the last state change. After a restart every registered session returns as `disconnected` (`state_at` keeps the time of its last known
state), with its bindings and history, and `session.resume` brings it back under a new generation; a provider process is never assumed
to have survived, and synthetic sessions are not registered. A registry that cannot be parsed, or that names an id, provider or
account that cannot be right, stops the host with a message; it is never repaired by guessing. Each log frame is checked on start (its
sequence number must follow the one before): at the first frame that is cut short or out of order the rest of the segment and every
later segment are set aside in owner-only `.damaged` files (never read again), the session shows one `[log recovered: ...]` record,
and the sequence continues from the last good record. Logs are synced to disk about once a second; there is no checksum, so damage
inside a frame that keeps its length is not detected.
