# lico-llm-http 0.2.0 — specification

Draft written with the P04 plan (M16-P04); no separate owner stop (lead's mail 137). Marks: **PROPOSED**, **KNOWN FROM DOCUMENTATION** (HTTP/1.1 message framing, RFC 9112), **SEEN IN CODE** (the discovery probe's rules, which this client does not copy).

## Purpose

A small HTTP/1.1 client for a Local LLM server on this machine: one request, the reply body handed on as it arrives. It is the one HTTP client of the product for model servers. It does nothing else: no TLS, no proxy, no name lookup, no redirects, no cookies, no keep-alive, no retries.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `send(&Request, &Limits, &Cancel, on_body: &mut dyn FnMut(&[u8]) -> Flow) -> Result<Head, HttpError>` | sends the request and calls `on_body` with each piece of the body, in order, as it arrives; returns the head when the body is complete (or when `on_body` said `Stop`) |
| `send_with_head(&Request, &Limits, &Cancel, on_head: &mut dyn FnMut(&Head) -> Flow, on_body) -> Result<Head, HttpError>` (0.2.0, M16-P06) | like `send`, and `on_head` is called once with the status and headers before any piece of the body; `Stop` there ends the call and returns the head (a caller that reads an error body differently from a stream knows which it has) |
| `Request { addr: SocketAddr, method, path, headers, body }`, `Method { Get, Post }` | a loopback address (no host name), a path that starts with `/`, extra headers, a body; `Debug` shows no header value of `Authorization`, `Proxy-Authorization`, `Cookie` or a name containing `key` or `token` |
| `Limits { connect, first_byte, idle, total, max_body }`, `Default` | 2 s, 120 s, 60 s, 600 s, 8 MiB |
| `Cancel` (`new`, `clone`, `cancel`, `is_cancelled`) | a shared flag; the call ends with `Cancelled` within about 1 s |
| `Head { status, headers }`, `header(name)` | the status and the headers (names in lower case) |
| `Flow { Continue, Stop }` | what `on_body` answers |
| `HttpError { NotLoopback, BadRequest(String), Connect(ErrorKind), Timeout(Phase), Cancelled, TooLarge, BadResponse(String), Io(ErrorKind) }`, `Phase { Connect, FirstByte, Idle, Total }` | what can go wrong; `BadRequest` is an addition to the planned list (a refused header or path) |
| `MAX_HEAD_BYTES` = 16 KiB | the largest reply head |

## Behaviour

- **Loopback only** (PROPOSED): an address that is not loopback (`127.0.0.0/8`, `::1`) is `NotLoopback` before any socket is made; no host name is ever resolved.
- **Request** (KNOWN FROM DOCUMENTATION): `METHOD path HTTP/1.1`, `Host: 127.0.0.1:PORT`, `User-Agent`, `Accept`, `Connection: close`, the caller's headers, `Content-Length` when there is a body. The caller may not set `Host`, `Content-Length`, `Connection`, `Transfer-Encoding` (a `BadRequest`); a header name with anything but token characters, a value or path with a control character, and a path without a leading `/` or with a space are `BadRequest`: nothing is sent.
- **Reply** (KNOWN FROM DOCUMENTATION): a head of at most `MAX_HEAD_BYTES`; the body is chunked (extensions and trailers read and dropped), or `Content-Length` bytes, or until the connection closes; statuses 1xx, 204 and 304 have no body; two different `Content-Length` values, a negative or huge one, a bad chunk size, a missing chunk end and a body shorter than promised are `BadResponse`. A redirect is a status like any other and is never followed.
- **Deadlines**: `connect` for the connect; `first_byte` from the end of the request to the first reply byte; `idle` between reads after that; `total` for the whole call. The phase that ran out is in `Timeout`.
- **Cancel**: checked at least every 250 ms while waiting and before the connect.
- **Bytes**: more than `max_body` body bytes is `TooLarge`; the callback is never called with an empty piece.
- **Key hygiene** (PROPOSED): a header value is never in an error or a `Debug` output; the bytes of the request are overwritten after the send (best effort, not a tested contract).
- **Cut replies**: when the connection ends before the body does, the pieces that arrived have already been handed on and the call ends with `BadResponse`.

## Invariants

1. Nothing but loopback is ever connected to. 2. A refused request sends no byte. 3. Memory is bounded: the head (16 KiB), one read buffer and the caller's piece; a body is never held. 4. No panic and no hang on any server behaviour: every wait is bounded by a deadline or the cancel. 5. Only `std`: no dependency, no `unsafe`, no environment, no files, no process.

## Tests

`tests/contract.rs` against `lico-llm-fake` and raw sockets (every line above); `tests/cross.rs`: the fake, this client and `lico-llm-wire` together (both formats, any chunking, failures, cut and cancelled replies, the Ollama facts). `lico-llm-fake` and `lico-llm-wire` are dev-dependencies: inputs of the tests, listed in the receipt as `(dev)`, not dependencies of the crate.

## Versioning and budget

`0.2.0` (`send_with_head` added in M16-P06, found needed by the host adapter: the status must be known before the body is read) while the formats have not met a real server. Budget: the default 256 MiB and 30 s.
