# lico-proxy 0.3.0 — specification

Written with the P05 plan (M17-P05); approved by the lead under the owner's delegation (the owner away), mail 148. Marks: **KNOWN FROM DOCUMENTATION** (HTTP/1.1 message syntax and semantics, RFC 9110 and 9112; `CONNECT`, RFC 9110 section 9.3.6; the special-purpose address registries of IANA, RFC 6890 and its updates; Basic authentication, RFC 7617), **PROPOSED** (the limits; whether the engine takes proxy credentials and honours the proxy for every fetch is the second measurement of M17-P05).

## Purpose

The fetch guard of a browser engine. A small HTTP and `CONNECT` proxy on a loopback port that resolves names itself, refuses every address that is not public (loopback only when the person opted a tab into `localhost`), connects to the address it checked, fixes ONE destination per connection and ends every connection after it. The engine is given it with `--proxy` and a fence that allows TCP to its port only, so the engine can reach nothing else and does no lookup of its own. Std only. **Threads exist only between `start` and `stop`**: one acceptor and at most one or two per connection (each of 256 KiB of stack).

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Proxy::start(Config, Arc<dyn Resolver>, Arc<dyn Connector>) -> io::Result<Proxy>`, `addr()`, `stats() -> Stats`, `stop(self)` | binds `127.0.0.1:0` (no other address is ever named); `stop` (and drop) shuts down every connection and returns within about 2 s |
| `Config { token, max_connections, limits, policy, observe, deny_port }`, `Default`; `Limits { connect, resolve, idle, total, max_bytes, max_body }`, `Default` | `Debug` of a `Config` hides the token |
| `Policy { allow_loopback, only_loopback }`, `Policy::allows(Class)`, `Class`, `classify(IpAddr) -> Class` | the address table below; only `Public` is allowed, `Loopback` as well with `allow_loopback`, and with `only_loopback` (0.2.0) ONLY `Loopback`: not even public (the guard of a tab that may reach this machine's own services must never load a public page; a redirect to a public host is refused with 403). A name with any answer that is not allowed is refused, in every mode. `Config.deny_port` (0.2.0) is a function of a port: a loopback destination (every answer loopback) whose port it denies is refused with 403 in every mode, so that a page can never reach the host's own listening ports or the engine's; a public destination on the same number is not affected |
| `Host { Ip, Name }`, `HostError`, `check_host(&str)`, `MAX_HOST` | the host rules below, applied before any lookup |
| `trait Resolver { resolve(host, timeout) -> io::Result<Vec<IpAddr>> }`, `trait Connector { connect(ip, port, timeout) -> io::Result<TcpStream> }`, `StdResolver`, `StdConnector`, `with_deadline(timeout, f)` | the two doors to the world; the std ones are thin (a lookup behind a deadline, a connect with a timeout) and cannot be tested without a real lookup or connection, which no test may make; every test uses fakes |
| `Event { kind: Connect \| Http \| Refused(status), host, port, authenticated, why }`, `Why { None, Other, NotPublic, LoopbackOnly, Port, Denied }`, `Config.observe` | (0.3.0: `why` says the reason of a 403 so that the person is told the truth: `NotPublic` an address that is not public, `LoopbackOnly` a public or private address at a loopback-only guard, `Port` a public destination on a port that is not 80, 443, 8080 or 8443, `Denied` a port of this machine that no page may reach; `Other` any other refusal; `None` for a connection that was let through) | what the observer is told: the destination host and port, never a path, a query or a header |
| `Stats { accepted, refused, active, bytes }` | counters |

## What it does with a request

`CONNECT host:port`: the destination is fixed by that line; after `200 Connection Established` the bytes go both ways untouched (TLS stays end to end). Plain `http://`: the request line must be absolute form (`GET http://host[:port]/path HTTP/1.1`); it is rewritten to origin form with the `Host` of the destination, the end-to-end headers are kept in order, `Connection`, `Proxy-Connection`, `Keep-Alive`, `TE`, `Trailer`, `Upgrade`, `Proxy-Authorization`, `Proxy-Authenticate` and `Expect` are dropped, `Connection: close` is added; the body, only by `Content-Length`, is sent whole; the response head is rewritten to `Connection: close` (its `Connection`, `Keep-Alive`, `Proxy-Connection` and `Proxy-Authenticate` removed) and its body is relayed as it comes until the origin closes. **No keep-alive, no pipelining, no `Upgrade` (a plain `ws://` through this proxy does not work; `wss://` is a `CONNECT`), no chunked request bodies.**

Refusals (the status; the origin is never contacted; the connection closes with `Connection: close`): `400` a bare line feed or carriage return, a NUL, a request line that is not three parts, a method that is not upper-case letters, a version other than HTTP/1.0 and 1.1, a target with a space, a control or a non-ASCII character, a folded header, a header without a colon or with a name that is not a token or a control character in its value, two `Content-Length` headers (equal or not), a `Content-Length` that is not digits (a sign included), `Transfer-Encoding` together with `Content-Length`, `https://` in absolute form, an origin-form or asterisk-form target, a user name or password in the address, a port that is not 1 to 65535, a `CONNECT` target without a port or with a path, a host the rules below refuse, **any bytes after the first request (a pipelined second request, whatever host it names, or bytes after a body, or bytes sent before a `CONNECT` is answered)**; `405` a method that is not `GET HEAD POST PUT DELETE OPTIONS PATCH CONNECT`; `407` the token is required and missing or wrong; `413` a `Content-Length` over `max_body`; `431` a head over `MAX_HEAD` or more than `MAX_HEADERS` headers; `501` any `Transfer-Encoding`; `403` a destination the policy refuses; `502` the name does not resolve or the connect failed or the origin's answer is not an HTTP/1.x head within `MAX_HEAD`, has `Transfer-Encoding` with `Content-Length`, is a 1xx or is empty; `503` no free connection; `504` the lookup or the origin was too slow; `408` a stalled head.

## The address table (one test per row)

IPv4: `0.0.0.0/8` Unspecified; `10/8` Private; `100.64/10` Shared; `127/8` Loopback; `169.254/16` LinkLocal; `172.16/12` Private; `192.0.0/24` Reserved; `192.0.2/24` Documentation; `192.88.99/24` Reserved; `192.168/16` Private; `198.18/15` Benchmark; `198.51.100/24` and `203.0.113/24` Documentation; `224/4` Multicast; `240/4` Reserved; `255.255.255.255` Broadcast; the rest Public. IPv6: `::` Unspecified; `::1` Loopback; IPv4-mapped `::ffff:0:0/96`, NAT64 `64:ff9b::/96` and 6to4 `2002::/16` take the class of the IPv4 address inside; `64:ff9b:1::/48` and the rest of `::/96` (IPv4-compatible) Reserved; `fc00::/7` and `fec0::/10` Private; `fe80::/10` LinkLocal; `ff00::/8` Multicast; `2001::/32` (Teredo), `2001:2::/48` and `100::/64` Reserved; `2001:db8::/32` Documentation; the rest of `2000::/3` Public; everything outside `2000::/3` Reserved.

## Host rules and the resolve-once rule

A host is checked before any lookup: empty, over `MAX_HOST` (253), non-ASCII, a name with a label that is empty, over 63 bytes, starts or ends with `-` or has a character other than `A-Za-z0-9_-`, and **a host whose every label is digits or a `0x` number but that is not a strict dotted quad or an IPv6 address** (`127.1`, `2130706433`, `0x7f.0.0.1`, `0177.0.0.1`: the system lookup would read them as addresses) are refused. An address literal needs no lookup. A name is resolved ONCE per connection by the `Resolver`; **every** answer is classified and the name is refused if ANY answer is not allowed (a name that resolves to a mix); the connect goes to the first answer, the address that was checked (a second lookup never happens, so a rebinding answer is never seen). Ports: a destination with any public address may use only `PUBLIC_PORTS`; a destination whose addresses are all loopback (the opt-in) may use any port.

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `DEFAULT_MAX_CONNECTIONS` | 16 | a page opens at most six connections to a host and a few hosts; each costs one or two threads of 256 KiB of stack (about 3 MB of the host's memory at the very most), only while an engine runs; the next gets a 503 |
| `MAX_HEAD` | 16 KiB | a real head is under 8 KiB |
| `MAX_HEADERS` | 100 | a real request has about twenty |
| `MAX_BODY` | 16 MiB | form posts are small; an upload is not what a tab is for |
| `DEFAULT_CONNECT` | 10 s | immediate on a good network; 10 s covers a slow one |
| `DEFAULT_RESOLVE` | 5 s | a lookup that takes longer is given up on |
| `DEFAULT_IDLE` | 60 s | a connection that says nothing for that long is ended |
| `DEFAULT_TOTAL` | 300 s | a page load is seconds, a stream a few minutes |
| `DEFAULT_MAX_BYTES` | 256 MiB | the bytes one connection may move in both directions together |
| `PUBLIC_PORTS` | 80, 443, 8080, 8443 | http, https and their usual alternatives |

## The token

If `Config.token` is set, every request needs `Proxy-Authorization: Basic` for the user `lico` and that token (a `407` with `Proxy-Authenticate: Basic realm="lico"` otherwise), compared in time that does not depend on where they differ. It appears in no `Debug`, no event and no answer, and the header is never forwarded. Whether the engine sends it is a measurement of M17-P05.

## Invariants

1. Loopback only for its own socket; one destination per connection; every connection ends after its request; nothing reaches an origin before the policy has allowed the destination, and no refusal reaches it at all.
2. After `stop` no listener, no connection and no thread of this crate remains (a connection that waits for a lookup that does not return ends when the lookup does or at its deadline); before `start` nothing exists.
3. The observer and the counters never see a path, a query, a header value or the token.
4. The Std resolver is never run by a test; every address in a test is a fake or a loopback socket of the test's own; the "public" addresses of the tests are never connected to (the fake connector sends them to a local origin).

## Versioning and budget

`0.1.0` while it has met one client (the engine of M17-P05); `0.3.0` (M17-P07, mail 152) adds `Event.why`; `0.2.0` (M17-P06, mail 151, approved by the lead under the owner's delegation) adds `only_loopback` and `deny_port`. Budget: the default 256 MiB and 30 s.
