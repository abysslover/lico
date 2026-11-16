# M16-P04 Plan — `llm-http` and `llm-fake`

- Status: approved with P03 on 2026-11-17 (no separate owner stop for P04); waits for P03
- Version: 1.0.0
- Branch: `phase/M16-P04-llm-http-and-fake` (stacked on the P03 tip)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)

## Specification

The specification of the two components (no separate owner stop, by the lead's mail 137):
- **`llm-fake`** (component, std and `serde_json`, independent of `llm-wire` on purpose): `Fake::start(Script) -> Fake { addr, requests(), stop() }` on `127.0.0.1:0`; scripted behaviours per (method, path): a whole reply, an OpenAI-style SSE stream, an Ollama-style NDJSON stream (with tool calls in the shapes of the documentation), a call chain, and failures: server gone (accept then close), model missing (404), key needed (401), busy (429), slow first byte, silence mid-stream, truncated stream, an oversize body, a redirect. It records what it was sent (method, path, headers, body, with the `Authorization` value kept apart from logs). Its contract tests: replay as written, ephemeral loopback port only, stops cleanly, never listens on another address.
- **`llm-http`** (component, std only): `send(request, limits, cancel, on_body) -> Result<Head, HttpError>`; a loopback `SocketAddr` only (`NotLoopback` before any connect), methods `Get` and `Post`, HTTP/1.1 with `Connection: close`, a response head of at most 16 KiB, a body read as plain, with a length, or chunked, each piece handed to `on_body` as it arrives (it may stop the read), `Cancel` (a shared flag; a read timeout of 250 ms between checks, so a cancel ends the call within about 1 s), `Limits { connect, first_byte, idle, total, max_body }` with the defaults of report 119 (2 s, 120 s, 60 s, 10 min), no redirect followed (a 3xx is returned as it is), no TLS, no proxy, no name lookup; the caller's `Authorization` value is never put in an error text or a `Debug` output and the request bytes are wiped after the send (best effort); `HttpError { NotLoopback, Connect, Timeout(phase), Cancelled, TooLarge, BadResponse, Io }`; never a panic. Its tests run against `llm-fake`: every failure above, a cancel in mid-stream, a chunked stream, limits.
- **The cross test** (in `llm-fake/tests`, dev-dependencies on `llm-wire` and `llm-http`): the fake's streams through the client into the decoder give the events the script says: the two independent readings of the documentation check each other. Order: fake first, then the client red against it, then green, then the cross test, then manifests, receipts, budgets.
- **One HTTP client in the product**: `llm-http` does not copy the discovery probe. The report says whether discovery (`llm/http.rs`, 211 lines, GET only, a first-contact rule that sends nothing) should later move onto it; a proposal only.
- At the end: `done` mail 120 with the measured cost of P03 and P04 against my estimate (3 to 8 USD each), the figures, and the plan sketch for P05 (`convo`, `toolloop`), and a STOP.


## Steps

- [ ] 1. `llm-fake` first: the server, the scripts, the failures, its own contract tests.
- [ ] 2. `llm-http` red against the fake, then green.
- [ ] 3. The cross test (fake, client, decoder), manifests, receipts and budgets, `check` and `verify`.
- [ ] 4. The measured cost of P03 and P04 against the estimate, the proposal about discovery, the sketch of P05, mail 120.

## Acceptance Criteria

Each component inside 256 MiB and 30 s with a receipt; the cross test passes; no real service or socket (only the fake's ephemeral loopback port); `llm-fake` independent of `llm-wire`; no version bump, no spec edit.
