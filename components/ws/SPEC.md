# lico-ws 0.3.0 — specification

Written with the P02 plan (M17-P02); approved by the lead under the owner's delegation (the owner away), mail 145. Marks: **KNOWN FROM DOCUMENTATION** (the shapes of RFC 6455 and RFC 4648, SHA-1 of FIPS 180), **PROPOSED** (a value chosen here that the first measurement of the engine may change).

## Purpose

A WebSocket connection between two ends on this machine: the client role (always), and the server role (only behind the cargo feature `server`, which only `lico-cdp-fake` and the tests of this crate enable, so that a build of the program has the client role only). Loopback addresses only, no TLS, no proxy, no name lookup, no extensions, no subprotocols, no thread. Std only.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Limits { connect, handshake, idle, max_message }`, `Default` | the deadlines and the cap of one message |
| `Request { addr, path, headers }` | what `connect` is given; `Debug` shows the names of the headers and never a value |
| `connect(&Request, &Limits) -> Result<Conn, Error>` | connect, upgrade (`Host` from `addr`, never an `Origin`), check the answer; the bytes that follow the answer are kept |
| `Conn::send_text`, `send_binary`, `send_ping`, `close(code)`, `recv() -> Result<Message, Error>`, `set_read_timeout(Option<Duration>)`, `canceller() -> Canceller` | `recv` answers pings, drops pongs, joins fragments, answers a close frame once; `Canceller::cancel()` from another thread ends a blocking `recv` with `Cancelled` and the connection is dead |
| `Message { Text(String), Binary(Vec<u8>), Close { code, reason } }` | what `recv` gives |
| `Error { NotLoopback, Connect, Refused(status), Handshake(&'static str), Timeout, Cancelled, Protocol(&'static str), TooBig, Closed, Io(kind) }` | `Debug` and `Display` never show a header value |
| `accept_key(key)`, `encode_frame(fin, op, payload, mask)`, `Op`, `Role { Client, Server }`, `Incoming`, `Decoder::new(role, max_message)`, `feed(&[u8]) -> Vec<Result<Incoming, Error>>` | the pure parts, tested with no socket |
| feature `server`: `read_request(&mut TcpStream, &Limits) -> Result<Head, Error>`, `Head { method, path, headers }` with `header(name)` and `is_upgrade()`, `accept(TcpStream, &Head, &Limits) -> Result<Conn, Error>` | the server role: a plain request (not an upgrade) is a `Head` too, for the fake's discovery answers |

## The limits (every one is a named public constant, with its reason)

| Constant | Value | Reason |
|---|---|---|
| `MAX_HEAD` | 16 KiB | a real upgrade request or answer is under 1 KiB; the room is for a long path and a token; a peer that never ends a head is stopped |
| `MAX_HEADERS` | 64 | a real head has about ten lines |
| `DEFAULT_MAX_MESSAGE` | 4 MiB (MEASURED, M17-P03: the largest screencast frame was 69,612 bytes; changed from the proposed 8 MiB in `0.2.0`) | the cap of one message and of one frame (a frame cannot be larger than the message it belongs to): a 1920x1080 frame is 8.3 MB as raw pixels (COMPUTED) and a compressed one is a fraction of it (ASSUMED: under 2 MB for a busy page), so 4 MiB leaves room and keeps a hostile peer from making the host buffer more |
| `HARD_MAX_MESSAGE` | 64 MiB | the most a caller can ask for: a bug elsewhere cannot make this crate buffer gigabytes (a larger `Limits.max_message` is cut to it) |
| `MAX_CONTROL_PAYLOAD` | 125 | the RFC (section 5.5): ping, pong and close frames carry at most 125 bytes |
| `DEFAULT_CONNECT` | 3 s | a loopback connect is immediate; 3 s is for a machine under load |
| `DEFAULT_HANDSHAKE` | 3 s | the upgrade answer of a local program comes at once; 3 s is for a program that is starting |
| `DEFAULT_IDLE` | 30 s | the longest a read or a write waits for the peer by default; a caller that polls sets a shorter read timeout |

## Invariants

1. Loopback only: an address that is not a loopback address is `NotLoopback` before any connection is tried; a path or a header that is not one clean line (a control character, a space in the path, an empty name) is `Handshake` before any connection is tried; headers the upgrade sets itself (`Host`, `Upgrade`, `Connection`, `Origin`, `Sec-WebSocket-*`, `Content-Length`) cannot be given.
2. The answer is checked: status 101 (any other status is `Refused(status)`), `Upgrade: websocket`, `Connection: Upgrade`, the exact accept key; an extension or a subprotocol in the answer is `Handshake` (none was asked for).
3. The rules of the RFC that a peer can break are `Protocol`: reserved bits, an unknown opcode, a fragmented control frame, a control frame over 125 bytes, a continuation with nothing to continue, a new message while one is open, a length with the top bit set, text that is not UTF-8 (checked on the whole message, so a character split between fragments is fine), a bad close code or reason, a masked frame from a server, an unmasked frame from a client.
4. The cap is checked from the length in the header: a frame over the cap is `TooBig` before its payload is buffered; the parts of a fragmented message are counted together. After an error the decoder and the connection stay failed (`Closed`).
5. Client frames are masked with a fresh non-zero mask each; the mask and the key are not secrets (no proxy is in the way), they come from a counter, the clock and the process's random hash keys.
6. A header value the caller sends (a token) appears in no `Debug`, `Display` or error of this crate; the `Debug` of a `Head` (server role) hides the values of `Authorization`, `Proxy-Authorization` and `Cookie`.
7. `recv` never hangs past its timeout (`set_read_timeout`, else `idle`) and ends at once for a `Canceller`; `Drop` shuts the socket down (also when a `Canceller` still holds a duplicate).
8. The server role is only compiled with the feature `server`; the product's build of the crate has none of it. No thread is ever started.

## Versioning and budget

`0.2.0`: the default cap was lowered to 4 MiB from the figure of the first measurement (a changed default is a changed behaviour); semantic after. Budget: the default 256 MiB and 30 s.

## The writer half (0.3.0, M17-P08, mail 152)

`Conn::writer() -> Writer` (cheap to clone, any thread): `send_text` and `send_binary` from another thread while the owner of the `Conn` blocks in `recv`, so that a command reaches the peer at once and nothing polls. One frame is written whole before the next begins, whichever half writes (a pong or a close of the reader included: one lock for the socket's writes); after a close frame or a cancel every half refuses to send (`Error::Closed`). The server role stays behind the `server` feature; the writer half has the client's masking like `Conn`.
