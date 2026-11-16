# lico-cdp 0.1.1 — specification

Written with the P02 plan (M17-P02); approved by the lead under the owner's delegation (the owner away), mail 145. Marks: **KNOWN FROM DOCUMENTATION** (the shape of CDP messages and the names and parameters of the commands, from Chrome's public protocol reference), **PROPOSED** (a choice made here that the first measurement of the engine may change; a command the engine lacks is found by that measurement, not guessed here).

## Purpose

A synchronous CDP client over one WebSocket on this machine: a call is sent with the next id and waits for the reply with that id; events that arrive meanwhile are kept in a bounded queue; deadlines and cancellation; and pure builders of the parameters of the commands a browser tab needs. No thread, no async: the host adapter of a later phase owns the one reader. It depends on `lico-ws` (client role only) and `serde_json`. It does not discover endpoints (no HTTP): the caller gives the address and the path.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Client::connect(addr, path, token: Option<&str>, &Limits) -> Result<Client, Error>` | `Limits` and `Canceller` are those of `lico-ws`; the token goes into one `Authorization: Bearer` header and nowhere else |
| `call(&mut self, method, params: Value, session: Option<&str>, timeout) -> Result<Value, Error>` | the `result` (`{}` when the reply has none); `params` is an object or `Null` (sent as `{}`), anything else is `Protocol` and nothing is sent; `session` is the CDP `sessionId` |
| `next_event(&mut self, timeout) -> Result<Option<Event>, Error>` | the oldest queued event, else what arrives within `timeout`; a timeout is `Ok(None)` |
| `dropped() -> u64`, `stray() -> u64`, `canceller() -> Canceller` | events dropped by a full queue; replies nobody waited for |
| `Event { method, params, session_id }` | one event |
| `Error { Remote { code, message }, Timeout, Cancelled, Closed, TooBig, Protocol(&'static str), Ws(lico_ws::Error) }` | a CDP error reply is `Remote` and the connection goes on; the others about the connection are described below |
| `method::*` | the names: `NAVIGATE`, `SET_VIEWPORT`, `START_SCREENCAST`, `SCREENCAST_ACK`, `SCREENCAST_FRAME` (the event), `CAPTURE_SCREENSHOT`, `MOUSE`, `KEY`, `INSERT_TEXT`, `CREATE_TARGET`, `ATTACH` |
| `ImageFormat { Png, Jpeg }`, `MouseKind { Pressed, Released, Moved, Wheel }`, `Button { None, Left, Middle, Right }`, `KeyKind { Down, Up, Char }` | the words of the builders, as types so that a typo cannot compile |
| `cmd::navigate(url)`, `set_viewport(w, h, scale)`, `start_screencast(format, quality, max_w, max_h, every_nth)`, `screencast_ack(frame_id)`, `capture_screenshot(format)`, `mouse(kind, x, y, button, clicks, delta)`, `key(kind, key, code, text)`, `insert_text(text)`, `create_target(url)`, `attach(target_id)` | pure builders of the `params`; `delta` is the vertical wheel movement; an empty `text` is left out; `attach` asks for a flat session |

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MAX_QUEUED_EVENTS` | 64 | events kept between two reads: a tab sends a frame every few tens of milliseconds while it changes, and a caller that reads each time it wakes has a few at most; 64 is room for a slow moment; when full the OLDEST is dropped (a newer frame replaces an older one) and counted |
| `DEFAULT_CALL_TIMEOUT` | 30 s | for a caller with no better figure: the engine's own deadlines are 30 s for a navigation and 60 s for a command (its documentation) |
| the size of a message | `Limits.max_message` of `lico-ws` (default 4 MiB since `lico-ws 0.2.0`, from the engine's measured frames: the largest was 69,612 bytes) | a larger message is `TooBig` and ends the connection |

## Invariants

1. Ids start at 1 and rise by one; every reply is matched by its id: a reply with an id nobody waits for (a late reply of a call that timed out, or an id never sent) is ignored and counted in `stray()`, and never handed to another call.
2. A message that is not JSON, not an object, has an id that is not a number, or is neither a reply (`id`) nor an event (`method`), or is a binary message, is `Protocol` and ends the connection; so does `TooBig`, a close frame and any error of the socket: from then on every call and `next_event` is `Closed`. A timeout and a CDP error do not end it.
3. Events never wait for a call: they are kept as they arrive, in order, up to `MAX_QUEUED_EVENTS`.
4. `call` returns within its timeout and ends at once for a `Canceller` (the connection is dead afterwards).
5. The token appears in no `Debug`, `Display` or error of this crate; a token that is not one clean line is refused by `lico-ws` before any connection.
6. Only a loopback address can be connected to (`Ws(NotLoopback)` before any attempt); no thread is started; the crate does not read any file or any environment variable.
7. It shares no code with `lico-cdp-fake` (the fake is a dev-dependency: an input of the tests); the two read the protocol separately.

## Versioning and budget

`0.1.1`: the default cap follows `lico-ws 0.2.0`; `0.1.0` while the commands have not met the engine; semantic after. Budget: the default 256 MiB and 30 s.
