# lico-cdp-fake 0.3.0 — specification

Written with the P02 plan (M17-P02); approved by the lead under the owner's delegation (the owner away), mail 145. Marks: **KNOWN FROM DOCUMENTATION** (the shape of CDP messages: `{id, method, params, sessionId}` calls, `{id, result}` or `{id, error: {code, message}}` replies, `{method, params, sessionId}` events; the error code `-32601` for an unknown method), **PROPOSED** (the screencast frame event in the Chrome shape: the engine may differ; re-fitted after the measurement of M17-P03).

## Purpose

A scripted fake of a CDP endpoint for tests: it listens on `127.0.0.1` with an ephemeral port, answers plain HTTP requests by path (discovery), upgrades to a WebSocket (optionally only with a bearer token), answers calls and sends events as its script says, includes scripted failures, and records every head and every call. It depends on `lico-ws` (with the feature `server`) and `serde_json`.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Fake::start(Script) -> io::Result<Fake>`, `addr()`, `calls() -> Vec<Call>`, `heads() -> Vec<Seen>`, `stop(self)` | listens on `127.0.0.1:0`; `stop` (and drop) ends the accept loop and every connection |
| `Call { method, params, session_id }` | one call that arrived, in order, answered by a rule or not |
| `Seen { method, path }`, `header(name)`, `authorization()` | one head that arrived (a refused one too); `Debug` shows the names of the headers and hides the values of `Authorization`, `Proxy-Authorization` and `Cookie` |
| `Script::new().token(&str)`, `.http(path, status, body)`, `.socket_path(path)`, `.on(method, Answer)`, `.times(n, method, Answer)`, `.idle(Duration)`, `.max_message(n)` | `socket_path` (0.3.0): the upgrade is accepted at that whole path only (no query) and a 404 anywhere else, as an engine that makes its socket path at each start; rules in order: the first rule for the method that is not used up answers; no rule is the error `-32601` with the method in its message; the last two are for tests of the fake's own limits |
| `Answer::result(Value)`, `::error(code, message)`, `::silent()`, `::close()`, `.event(method, params)`, `.event_in(session, method, params)`, `.after(ms)` | a reply, a CDP error, no reply, the end of the connection without a close frame; events are sent after the reply in the order given; `after` delays the reply |
| `screencast_frame(frame_id, data_b64, width, height) -> Value` | the params of a `Page.screencastFrame` event: `data`, `sessionId` (the number to acknowledge) and `metadata` (**PROPOSED** shape) |

A reply carries the `sessionId` of the call it answers (none if the call had none).

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MAX_MESSAGE` | 4 MiB (= `lico_ws::DEFAULT_MAX_MESSAGE` of `0.2.0`) | the most a client may send in one message; a screencast acknowledgement or a navigation is far smaller; a larger one ends the connection |
| `CONNECTION_IDLE` | 5 s | a connection that says nothing for that long is ended: a test that forgot to close cannot keep a thread for ever |
| the head | `lico_ws::MAX_HEAD` (16 KiB) | as `lico-ws` |

## Invariants

1. Loopback only: `Fake::start` binds `127.0.0.1` with port 0; `Fake::start_at(addr, script)` (0.2.0) binds the given address, which must be a loopback one (`InvalidInput` otherwise) and a port in use is an error.
2. The token is checked first: with a required token, a head without `Authorization: Bearer TOKEN` (plain or upgrade) is a 401 and nothing else is done; the token appears in no `Debug` of this crate.
3. Recorded: every head with a complete request line is recorded, and every call (a text message that is a JSON object with a numeric `id` and a string `method`), answered by a rule or not. A message that is not one ends the connection with the close code 1007 and is not a call; a binary message ends it with 1003.
4. Stoppable: `stop` returns within about 2 s whatever the connections do (a `silent` or an idle one included): every connection is shut down.
5. Honest ends: `silent` never replies and never closes by itself; `close` ends the stream without a close frame; a plain answer ends the connection after the body.
6. Independent: it shares no code with `lico-cdp` (the client); both read the protocol separately.

## Versioning and budget

`0.3.0` (M20-P01): `Script::socket_path(path)`, for the fake engine in Moli's shape (its socket path is `/devtools/browser/<id>`, made at each start and told by the discovery page), so that a host that assumes a path fails its system test; `0.2.0` (M17-P06, mail 150): `start_at` and `Script::observe(f)`, a function that runs for every call before its reply, and `Script::on_fn(method, f)`, a rule whose answer is built from the call, both used by the fake engine program; `0.1.1`: the cap follows `lico-ws 0.2.0` (4 MiB); `0.1.0` while the shapes have not met the engine; semantic after. Budget: the default 256 MiB and 30 s.
