# M16-P04 Plan — `llm-http` and `llm-fake`

- Status: approved with P03 on 2026-11-17 (no separate owner stop for P04); implemented
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

- [x] 1. `llm-fake` first: the server, the scripts, the failures, its own contract tests.
- [x] 2. `llm-http` red against the fake, then green.
- [x] 3. The cross test (fake, client, decoder), manifests, receipts and budgets, `check` and `verify`.
- [x] 4. The measured cost of P03 and P04 against the estimate, the proposal about discovery, the sketch of P05, mail 120.

## Acceptance Criteria

Each component inside 256 MiB and 30 s with a receipt; the cross test passes; no real service or socket (only the fake's ephemeral loopback port); `llm-fake` independent of `llm-wire`; no version bump, no spec edit.

## Result (2026-11-17, as measured)

| Component | Source lines | Test lines | Tests | Own run (`cargo xtask test NAME`) | Mutations caught |
|---|---:|---:|---:|---|---|
| `lico-llm-wire 0.1.0` (P03) | 782 | 599 | 31 | 1.12 s, peak 14.8 MiB | 19 of 20 (one equivalent) |
| `lico-llm-fake 0.1.0` | 509 | 294 | 13 | 0.61 s, peak 6.9 MiB | 14 of 14 |
| `lico-llm-http 0.1.0` | 451 | 620 | 25 (15 contract + 10 cross) | 2.23 s, peak 3.2 MiB | 23 of 25 (one equivalent, one untestable on loopback) |

All inside 256 MiB and 30 s with receipts; `cargo xtask check` and `verify` pass; the receipt of `llm-http` lists `lico-llm-fake (dev)` and `lico-llm-wire (dev)` as inputs (a tools improvement made in this phase: a dev-dependency on another component is hashed into the receipt, test first, `6a944e9`; the tools' tests are 13). The program does not depend on any of them yet (graph unchanged: 167 modules, 62 nodes, cycle 48; no edge into the hubs).

Deviations and honest notes: (1) the fake's server was written before its tests (the client's tests were red first against a stub); the fake's tests were therefore checked by 14 mutations, all caught. (2) `HttpError` has one variant more than planned (`BadRequest`, a refused header or path). (3) A `lico-llm-http` mutation of the declared-length check is equivalent (the byte count refuses the same body a moment later) and the connect-timeout mapping cannot be reached on loopback; both are named in the report. (4) The request bytes are overwritten after the send, which no test can see (best effort, as the specification says).

**Discovery on this client (a proposal only).** Discovery's probe (`llm/http.rs`, 211 lines) has a rule this client does not: it sends nothing until it knows the server does not speak first (SSH, SMTP), then a `GET` with `HTTP/1.0`, 150 ms quiet wait, 64 sockets, 1.5 s each. Moving it would remove a second HTTP implementation, but needs a listen-first mode in `llm-http` and touches M12 and M13 behaviour with many tests. Recommendation: not in this milestone; if wanted, one small phase after the adapter exists: add `first_contact` to `llm-http`, replace `llm/http.rs`, keep every discovery test as the proof.

**Sketch of P05 (`convo`, `toolloop`)** (to be planned with its own specification at its own stop): `convo` (std, `serde_json`, `lico-llm-wire` types): the conversation as an append-only JSONL file (owner-only), `load`, `append`, `fit(window)` that drops whole oldest turns and reports how many; `toolloop` (depends on the wire types only; ports `ModelPort`, `ToolPort`, `Clock`): `run_turn` with the limits of report 119 (8 rounds, 4 calls an answer, 16 a message, 8 KiB results, a turn of 10 min), a waiting tool ends the turn, cancel, a refused tool name; tested with scripted ports and, once, through `llm-http` and the fake. Estimate, revised by what P03 and P04 measured: about 2,150 lines.

