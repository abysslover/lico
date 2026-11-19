# Lico control protocol v1 (M01-P02 S01)

Version: 0.1.8

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
| `status` | oracle | no | per-session id, generation, last sequence, subscribers, bytes (the host adds `oldest` retained sequence and `state`; a running session also `memory`: `private_kb` = `RssAnon` + `RssShmem`, `rss_kb` = `VmRSS`, both summed over the agent and all its descendants, and `processes`, absent until the first look is done, which a `status` call starts when the last is 3 s old; a process that vanished or cannot be read counts for nothing; a program detached with `setsid` is not a descendant; the reply carries `host_memory` for the host itself) The reply also has `memory_limits` (`warn_mb`, `limit_mb`: the memory guard's orange and red levels in MB, 400 and 500 unless the owner's `memory.json` says otherwise; M19-P01, requested by the owner in the chat, 2026-11-17; wording approved by the lead, to be confirmed by the owner) so that a window colours by the host's numbers, and `stall_s` (seconds of no activity after which a WORKING session is called stalled: 600 unless `memory.json` says otherwise (`stall_min` in minutes, or `stall_s` in seconds, which wins), 0 switches it off; Slice D) and `machine_memory` (`available_kb` = `MemAvailable`, `total_kb` = `MemTotal` of the host's machine, so that a window can show the machine's line and warn before a new session; M19-P01). |
| `snapshot` | oracle | no | latest screen lines and last sequence of one session |
| `subscribe` | oracle | yes | start the live stream; `after` is the client cursor; `snapshot: true` asks for the latest screen first |
| `unsubscribe` | oracle | yes | stop the stream, keep the cursor |
| `session.create` | host | yes | start a synthetic agent session (`kind` `synthetic`, optional `rate`, `history`, `payload`), a provider session (`kind` `codex` or `claude`: one provider process and one thread), or a local session (`kind` `local`: `endpoint` `ADDRESS:PORT` with a loopback address, `api` `openai` or `ollama`, `model`, optional `context_tokens` and `name`; there is no provider process and no account: the host asks the model server on this machine itself, keeps the conversation in an owner-only record of its own and runs the host's tools for the model; an endpoint that is not a loopback address, a missing `model` or an unknown `api` answers `bad_request`) |
| `session.purge` | host | yes | delete a session: its process is ended, its log segments and its workspace are removed and the session is forgotten; nothing of any other session is touched and nothing of this one can be written again (local socket and `lico session purge` only, not on HTTP) A local session has no process: close ends its work and keeps its record, resume reads the record again, purge deletes the record. |
| `session.close` | host | yes | stop the provider process of a resumable session and keep the session: it becomes `disconnected` with its history, its generation unchanged, and `session.resume` brings it back; closing a disconnected session answers ok again, an ended session answers `session_exited`, a session that cannot be resumed (the program has not yet told its own session id) answers `no_conversation_yet` A local session has no process: close ends its work and keeps its record, resume reads the record again, purge deletes the record. Optional `by` (`person`, the default, or `guard`: the memory guard closed it, M19-P01, requested by the owner in the chat, 2026-11-17; wording approved by the lead, to be confirmed by the owner): who closed the session; `status` shows it as `closed_by` until the session is resumed, and only a session the guard closed is ever resumed by the guard. The running session's `memory` also has `max_rss_kb`, the VmRSS of the largest single process of its tree (what the guard's colour follows). |
| `session.resume` | host | yes | bring a disconnected provider session back with a new provider process that resumes the same thread; the execution generation rises by one, so every older target is stale; a session whose provider has not yet told its own session id has no conversation to resume and answers `bad_request` (the same cause makes `session.close` answer `no_conversation_yet`: one word each for now) A local session has no process: close ends its work and keeps its record, resume reads the record again, purge deletes the record. |
| `account.create` | host | yes | add an account of a provider (`codex` or `claude`) with an editable label (at most 40 printable characters); a stable opaque id (`a` and twelve hex digits) is returned, a private home is created for it (mode 0700, never read by Lico), and a provider's first account becomes its default (local socket only) |
| `account.list` | host | no | the accounts with provider, label, whether each is its provider's default and its credential generation |
| `history` | host | no | up to 200 older records of a session (`before` is a sequence number, `limit` at most 200), oldest first, read from the durable log on demand; the reply names the oldest sequence still kept |
| `account.login.start` | host | yes | start the provider's own sign-in for an account in its private directory (`method` `browser`, or `device` for Codex's device-code flow); one login per account at a time (`busy`), at most 5 minutes; the process gets only the account's directories, a path and a locale, never a display (local socket only) |
| `account.login.status` | host | no | the state of the account's last login (`waiting`, `browser`, `code`, `succeeded`, `failed`, `cancelled`, `timed_out`, `none`), the login address once the provider printed one, whether a pasted code is expected; with `check` also `signed_in` from Codex's `login status` (`null` for Claude Code: no status command is known for the pinned version) |
| `account.login.input` | host | no | the code a person pasted, written to the login process's standard input only when it asked for one (`no_code_expected` otherwise); never stored or logged |
| `account.login.cancel` | host | yes | end the login process |
| `host.stop` | host | yes | end this host (the explicit way to replace one of another build): refused with `busy` while sessions or a sign-in exist unless `force` is true; with `build` (a build id as `status` reports it) it ends only a host of that build and answers `build_changed` otherwise, so a window that saw an old host cannot end the new one another window started; a second stop while one is under way answers `stopping`; `status` carries `build` (a hash of the running executable) and `logins_running` so a window can tell whether it and the host are the same build |
| `account.default` | host | yes | make an account its provider's default; this affects only sessions created afterwards |
| `account.credential` | host | yes | attach a credential of the credential store to an account (its `ref` from `lico-cred`, which must belong to this account and its provider; otherwise `bad_request`); the account's credential generation rises; a Claude session of that account gets its key only through a helper that reads the keyring |
| `account.affected` | host | no | the sessions bound to an account with their states, named before anything is removed |
| `account.remove` | host | yes | remove an account; `sessions` must be chosen: `detach` leaves its sessions running (histories and work stay, the account's home is kept for them) and `end` purges them first; the reply states the separate outcomes: the local record (`metadata_removed`), the key (`credential`: `removed` or `none`), the home (`kept`, `removed`, `none`) and `remote_revocation` (always `not_performed`: revoking a login is the provider's own logout); `credential_store_unavailable` when a key exists and the credential store cannot be reached (nothing is removed then); a default that pointed at the account is cleared, never handed to another account |
| `session.rebind` | host | yes | move a session whose provider process is gone to another account as a new thread under a new generation; refused with `busy` while the session is running, even when idle (a provider holds its account's login); `unknown_account` for an account that does not exist or belongs to another provider |
| `session.list` | host | no | id, generation, state (`running`, `exited` or `disconnected`) and last sequence of every session |
| `input` | host | yes | send text to a session's agent: needs the target generation and a client request id; the reply and later `input` events give the state `accepted`, `submitted` (written to the agent) and `observed` (the agent acted on it) separately. Optional member `attachment` (M02-P04 S05a): the NAME of a pasted picture, exactly 32 lowercase hex digits and `.png`, handed out by `picture.slot`; the text stays required and non-empty. Valid on `input` only, and NOT on the HTTP route (a body with `attachment` is `bad_request` and no file is read). See "Pasted picture" below |
| `transfer.send` | host | yes | one session's text to another (S03a): `to`, `kind` (`text` a message, `lines` a selection), `text` (the limits of `input`) and optionally `from`, the sending session; the host writes the header `[from session 'NAME' (ID7) \| HH:MM \| KIND \| SIZE]` (`[from the person …]` with no `from`) and delivers it with the text as one input, up to 4096 bytes of text; longer text is stored as a file in the receiver's own handoff folder and the input names the path, the first lines and how to read it; a line of the text is given one leading space when its first character, after the white space and the characters that draw nothing in front of it, is an ASCII `[` or an opening bracket or initial quote of Unicode category Ps or Pi (the letters after it do not matter). `kind` `conversation` (S05) is the whole conversation of the Claude session `from`, which is then required, with no `text`: the host finds the program's own record of that session (under the home it gave the program, by the id the program told it), turns it into a text (every word cleaned like an agent's, indented, with a head that says what was left out) and stores it as a file in the receiver's folder whatever its size; the header names the sender as `conversation`. Only a person's op: the tool connection has no way to ask for one. The reply says `delivery` (`inline` or `file`), `size`, the receiver's state before delivery (`idle`, `busy`, `waiting_for_approval`) and `hops`, never a path. Errors: `bad_request` (also a session sending to itself), `unknown_session`, `session_disconnected`, `session_exited`, `busy`, `too_large` (a conversation over 32 MiB is refused whole), `no_conversation_yet` (the program has not told its session id), `record_not_found`, `record_ambiguous` (more than one file has the id), `record_format` (a message written by a program version the reader was not measured on: Claude Code 2.1.310 and 2.1.311 are read, one message of any other version refuses the whole record), and `chain_depth` for a chain of sessions that each forwarded the one before (more than 4 steps without a person; used by the tool of S03b, never by this op as a person's) |
| `input.status` | host | no | the state of an earlier request id, so a client that lost a reply can ask instead of sending again |
| `approval.answer` | host | yes | answer a pending approval request (`allow` or `deny`) of a session |
| `view.attach`, `view.detach` | host | yes | register a view of a session (the latest screen now, live records after it); views vanish with their connection and never stop a session |
| `clipboard.write` | host | yes | write a text or a PNG picture to the CLIPBOARD selection through a window (M02-P04 S04b; the host holds no X connection and cannot own a selection): `kind` `text` with `text` (1 to 65536 bytes, any characters), or `image` with `path` (absolute; the WINDOW opens the file, never the host), optional `timeout_ms` (1 to 600000, default 3000). The host forwards the request to the newest connection that attached a view of kind `gui` and answers when that window confirms with `clipboard.done`; reply: `kind`, `bytes`, and `width` and `height` for a picture. Errors: `no_window` (no live window, at once), `timeout` (no confirmation in time; the request is forgotten), `too_large`, `bad_image`, `write_failed`, `x_error` (the reasons a window gives). A person's op: the tool connection has no way to ask for one |
| `clipboard.done` | host | yes | the window's confirmation (or failure) of a `clipboard.write` request: `req` (the id of the pushed `clip.write` event), `ok` (boolean), and with `ok` true `kind`, `bytes`, `width`, `height`; with `ok` false `error` (one of the reasons above; any other word is taken as `x_error`). Heard only from the connection that was asked and only while the request waits: an unknown id, a wrong connection or a late confirmation answers `unknown_request` and changes nothing. Sent after `get_selection_owner(CLIPBOARD)` says the window owns the selection |
| `picture.slot` | host | yes | the place for a pasted picture (M02-P04 S05a): no member but the common ones. The host removes the files of the pasted folder older than 600 seconds, answers `busy` when 8 files are there, and otherwise makes an EMPTY file `<32 lowercase hex>.png` (mode 0600, `O_CREAT \| O_EXCL \| O_NOFOLLOW`) and answers `name` and the absolute `path`. For the person's own window; see "Pasted picture" below. Errors: `busy`, `slot_failed` (the folder or the file could not be made) |
| `resources` | host | no | host process memory (RSS and PSS), threads, sessions, subscribers, views, log bytes and the memory of each agent |
| `browser.open` | host | yes | open a browser tab (M17): `url` (http or https), optional `local` (true: a tab for this machine's own services, an explicit act, off by default), optional `search` (true, from the window since M22-P05b: words are searched for as for `browser.go`; without it the address is strict and a wrong one is `bad_request`, with it the refusal says why: `address_*` or `search_setting`); answers `tab` (`b1`, `b2`, ...), `phase`, `local`, `unpinned`, or for an engine that is not installed `phase` `absent` with `release`, `download`, `archive_sha256`, `engine_sha256`, `directory`, `put`; errors `bad_request`, `tab_limit` (eight tabs), `engine_unsafe`, `engine_not_pinned`, `engine_setting`, `engine_unreadable`, `engine_fence`, `engine_failed` With no engine installed the host installs the pinned one (M17-P10, requested by the owner in the chat, 2026-11-17; wording approved by the lead, to be confirmed by the owner): the answer has `phase` `installing` with `size_mb`, `source` and `sha256_start`, `browser.list` and `browser.watch` show the same, then `starting` when the engine is in place, or `absent` with a `notice` (the plain reason; a hash that is not the pinned one begins `SECURITY:`) and the manual fields when the install failed; `{"auto_install": false}` in the owner-only `browser.json` turns it off (the tab is then `absent` at once). |
| `browser.list` | host | no | the open tabs (`tab`, `url`, `title`, `phase`, `loading`, `notice`, `viewport`, `viewport_capped`, `has_frame`, `frame_seq`, `shown`, `local`, `unpinned`, `no_font`, `dropped_frames`), the two engines (`local`, `running`, `unpinned`, `reported`, `guard_accepted`, `guard_refused`) and `max_tabs` |
| `browser.close` | host | yes | close a tab (`tab`); the last close of an engine ends it; `engine_stopped`; `unknown_tab` |
| `browser.shot` | host | yes | the newest frame of a tab (`tab`) written again as a PNG to a NEW owner-only file at an absolute `path` (never over a file); `no_frame`, `exists`, `bad_request`, `write_failed` |
| `browser.show`, `browser.resize`, `browser.hide` | host | yes | the window shows a tab at `width` x `height` (capped at 1920 x 1200: `viewport`, `viewport_capped`), resizes it, or hides it (its screencast stops, its newest compressed frame is kept) |
| `browser.go`, `browser.nav` | host | yes | `go`: a new `address` for a tab, with an optional `search` (true, from the window since M22-P05b: text that is not an address is searched for with the owner's `browser_search` setting, else Google; a tab for this machine's own services never searches; a wrong setting is `search_setting`) (a refusal of the address is `address_empty`, `address_too_long`, `address_scheme`, `address_credentials` or `address_invalid`; on a tab whose engine ended or was not installed it starts the engine again); `nav`: `action` `back`, `forward` or `reload` |
| `browser.watch` | host | no | the window's one blocking call for a shown tab: `tab`, `seq` (the change number it has), optional `frame_seq` and `frame`; it waits with no timeout until the tab changed, then answers `seq` and the tab's row, and, when the frame is newer than `frame_seq`, `frame_seq` and `png` (base64); or `gone`, `hidden` or `superseded` (at most one watch waits for a tab; a close, a hide and a stop release it) |
| `browser.input` | host | yes | the person's input for the page: `kind` `move`, `down`, `up` (`x`, `y`, `button`, `clicks`), `wheel` (`x`, `y`, `delta`), `key_down`, `key_up` (`key`, `code`, `text` of at most 8 characters) or `text` (committed text, at most 16 KiB); `sent` is false when the tab takes none (not shown, starting, absent or failed, or the address row has the focus) |
| `browser.reopen`, `browser.forget` | host | yes | the remembered pages (M22-P05c; no fields but the common ones): `reopen` opens the newest remembered page that no tab (not local) shows, as `browser.open` does (the same answer and `url`), `no_page` when there is none, an address can come only from the list; `forget` empties the list and answers `forgotten` (how many it held). A page is remembered (address, title, time; at most 100; owner-only file `browser_pages.json` of the state directory) when it has loaded in a tab that is not local |
| `terminal.open` | host | yes | open a terminal (M23-P02): the person's own shell on a pty, run as the person (socket-only: `POST /v1` refuses it with `bad_request`); optional `cwd` (absolute, checked as a session's folder is: `folder_*` errors; none = the home folder), `shell` (an absolute path of an executable file, else `$SHELL`, else `/bin/sh`; one that was asked for and is not there is `shell_missing`), `cols` (1 to 500) and `rows` (1 to 200), default 80 x 24; answers `tab` (`t1`, `t2`, ...; never used twice), `pid`, `cwd`, `shell`, `cols`, `rows`; errors `bad_request`, `tab_limit` (eight terminals), `shell_missing`, `pty_failed`, `spawn_failed`, and the folder errors. The shell has the host's environment without the `LICO_*` names (except `LICO_SOCKET`) and with `TERM=xterm-256color`, `COLORTERM=truecolor`; its process group is ended with the host. |
| `terminal.list` | host | no | every terminal: `terminals` of `tab`, `cwd`, `shell`, `pid`, `title`, `cols`, `rows`, `running`, `exit` (the code, or 128 plus the signal; null while it runs), `started` (seconds since the epoch), and `max_terminals`; a shell that ended stays listed, with its screen, until it is closed |
| `terminal.close` | host | yes | close a terminal (`tab`): its process group gets `SIGHUP` and, if it is still there a second later, `SIGKILL` (a program that left the group, or one that outlived its shell and ignores the hang-up, is not found); every watch of it is released; `closed`; `unknown_tab` |
| `terminal.input` | host | yes | what the person types or pastes (`tab`), as exactly one of `text` (UTF-8) or `data` (base64), at most 64 KiB; `written` (bytes); `terminal_ended` when the shell has ended, `busy` when the pty took nothing for two seconds, `unknown_tab` |
| `terminal.resize` | host | yes | the size of the pty and of the emulator (`tab`, `cols`, `rows`); the program gets its window-size signal; nothing is announced when the size is the one it has; `unknown_tab`, `pty_failed` |
| `terminal.watch` | host | no | the window's one blocking call for a terminal: `tab`, optional `seq` (the change number it has; 0 or none is answered at once) and `scroll` (rows above the bottom); held on a condition variable (no timer) until the number is above `seq`; many watches of one terminal may wait together; answers `seq`, `cols`, `rows`, `title`, `cursor` (`[row, col]`, null while hidden or scrolled away), `modes` (`app_cursor`, `app_keypad`, `bracketed_paste`, `alt_screen`, `mouse` `off`/`press`/`drag`/`any`, `mouse_sgr`, `focus_events`, `cursor_visible`), `scroll`, `top`, `total`, `lines` (each a list of runs `[text, fg, bg, attrs]`; a colour is -1 for the default, 0 to 255 indexed, 256 + (r << 16 \| g << 8 \| b) true colour; `attrs` bits 1 bold, 2 dim, 4 italic, 8 underline, 16 blink, 32 inverse, 64 hidden, 128 strike; trailing blank cells of a line are not sent), `wrapped`, `bells`, `running`, `exit`; a close or the host's stop releases every watch with `unknown_tab` |
| `terminal.text` | host | no | the text between two places of the screen as the emulator reads it now (`tab`, `from`, `to`: `[line, col]` with `line` absolute, 0 the oldest line of the scrollback, either order, both ends included; a wrapped line is joined, trailing blanks are cut); `text`; `unknown_tab` |

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

## Clipboard write (M02-P04 S04b)

`lico clipboard write (--text STRING | --text-file FILE | --image FILE) [--timeout-ms N]` is a client of the host. It sends `clipboard.write`;
the host pushes `{"ev":"clip.write","req":ID,"kind":"text"|"image","text":...|"path":...}` to the newest window, which takes the CLIPBOARD
selection (a text as UTF8_STRING, STRING and TEXT; a picture as `image/png`, served by the incremental protocol above 256 KiB or what one X
request can carry), checks that the server says it owns the selection, and answers `clipboard.done`. Only CLIPBOARD is written, never PRIMARY.
A second write replaces the first; when another client takes the selection the window lets its content go. A window that is not the newest is
never asked. The command exits 0 on success, 1 for a usage error and 2 for any other failure, and prints one JSON line; a host that cannot be
reached ends it as the other commands end (exit 3 and 4).

The setting `clipboard_write` (SF-06) governs the agent tool `clipboard_write` of M02-P04 S07a (listed and callable only while the setting is on, with the
person's approval per call): off unless the file `settings.json` in the host's state folder (the parent of the handoff folder) is a regular file of the user,
at most 4096 bytes, not writable by group or others, holding a JSON object whose `clipboard_write` is the boolean `true`. Anything else, a missing file included,
is off. It is read at each use, reported in `status` under `"settings": {"clipboard_write": bool}`, and toggled over the local socket by `{"op": "settings.set", "clipboard_write": bool}` (used by the window's View menu; never exposed on HTTP or the tool connection).

## Pasted picture (M02-P04 S05a)

The window cannot know the host's state folder (the host may run with another `--state-dir`), so the host hands out the place. The flow of a window:

1. `{"op":"picture.slot"}` gives `{"ok":1,"v":1,"name":"<32 hex>.png","path":"/ABSOLUTE/STATE/pasted/<32 hex>.png"}`. The file exists, is empty and is mode 0600, in the folder
   `pasted` (mode 0700) beside the handoff folder of the state folder; it is never in a repository or a workspace. `busy` (8 files are there) is the only refusal to expect.
2. The window opens that path for writing WITHOUT creating it and writes the PNG (at most 3 MiB, a side of 1 to 8000 pixels).
3. `input` with the usual members and `attachment` set to the NAME (never a path). Order of the host's checks: shape, session, generation, a repeated request id (answers its
   state and does not look at the file), the disconnected and exited states, THEN the kind of the session, THEN the file. Only a Claude session takes a picture.
   The host builds the path itself, opens it without following a link, judges the opened file (a regular file of the user, 8 bytes to 3 MiB), reads it and checks the PNG header.
4. Claude gets one user message whose content is the image block (`source.type` `base64`, `media_type` `image/png`) FIRST and the text block after it. The transcript shows
   `[picture N KiB] ` in front of the text (N rounded to the nearest KiB, at least 1); the program gets the text unchanged.

The host removes the named file after EVERY answer to an `input` that names one: sent, refused, a repeat, an unknown session, a stale generation. It also removes the files of the
folder at host start and, at a slot call, those older than 10 minutes. What it does not remove: a file the window made and never named in an `input` (until the next slot call
after 10 minutes, or the next start), and the file of an `input` that was refused by its SHAPE or by the HTTP route (nothing was looked at).

Errors of `input` with an attachment (the window shows a sentence for each): `attachment_gone` (no such file, a symbolic link, not a regular file, not the user's, or it changed
while it was read), `attachment_too_large` (over 3 MiB), `bad_image` (under 8 bytes, not a PNG, a side of 0 or over 8000), `attachment_unsupported` (the session is not a Claude
session; no guess is made at the shape of an image item for Codex). A name of a bad shape is the ordinary `bad_request`. The reply of a slot is for the person's own window: the
sentence "the reply never says where the file is" of the agent tools stays true, because an agent has no way to ask for one.
