# lico-llm-fake 0.1.0 — specification

Draft written with the P04 plan (M16-P04); no separate owner stop (lead's mail 137). The marks: **KNOWN FROM DOCUMENTATION** (a shape of the OpenAI-compatible or Ollama API from their public documentation), **PROPOSED**. The fake is the **contract** of the wire formats until the owner allows a check against a real loopback server.

## Purpose

A scripted fake of a Local LLM server for tests: it listens on `127.0.0.1` with an ephemeral port, answers what its script says in the shapes of the two APIs, includes scripted failures, and records what it was sent. Nothing else: it is not a model and has no behaviour of its own.

## API (all of it; `tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Fake::start(Script) -> io::Result<Fake>`, `addr()`, `requests() -> Vec<Seen>`, `stop(self)` | listens on `127.0.0.1:0`; `stop` (and drop) ends the accept thread and every connection and returns when the threads are done |
| `Seen { method, path, headers (names in lower case), body }`, `header(name)`, `authorization()` | what was received; `Debug` never shows the `Authorization` value |
| `Script::new().on(method, path, Behaviour)`, `.times(n, method, path, Behaviour)` | rules in order; the first that matches and is not used up answers; no rule is a 404 with `{"error":"no script for METHOD PATH"}` |
| `Behaviour { Answer(Reply), Close, Hang }` | `Close`: read the request, then end the connection without a word (a server that is gone); `Hang`: never answer, keep the connection |
| `Reply { status, headers, body: Body, first_byte_delay, gap, ending: Ending }` with `Reply::json`, `chunked`, `sse`, `ndjson` and `.header`, `.first_byte_after`, `.gap`, `.ending` | one answer |
| `Body { Whole, Chunked, UntilClose, Raw }` | a length, chunks, the end of the connection, or only the given bytes |
| `Ending { Complete, Cut(n), Silent(n) }` | `n` pieces (bytes for `Whole`) then the connection closes without the end, or stays open and silent |
| `Piece { Text, Reasoning, Call { id, name, args, split }, Finish, Usage, End }`, `openai_sse(&[Piece])`, `ollama_ndjson(&[Piece])`, `openai_error(msg, code)`, `ollama_error(msg)` | the bytes of a reply in each API's shape, one `Vec<u8>` per step |

## Shapes (KNOWN FROM DOCUMENTATION)

OpenAI: `data: {chunk}\n\n` events; `choices[0].delta` with `content`, `reasoning_content`, `tool_calls` by `index` (id and name first, then `arguments` in slices); `finish_reason`; a `usage` chunk with empty `choices`; `data: [DONE]`. Ollama: one JSON object per line; `message.content`, `message.thinking`, `message.tool_calls[].function{name, arguments (object)}`; a last line `done: true` with `done_reason` and the two counts. Errors: `{"error": {"message", "type", "param", "code"}}` and `{"error": "text"}`.

## Invariants

1. Loopback only: it binds `127.0.0.1` with port 0; the code names no other address.
2. Bounded: a request head of at most 64 KiB, a body of at most 8 MiB (a larger one is a 413), a request must arrive within 5 s.
3. Stoppable: `stop` returns within about 2 s whatever the connections do (a `Hang` or `Silent` one included).
4. Recorded: every request that reaches a connection with a complete head is recorded, answered by a rule or not.
5. Independent: it depends on `serde_json` only and shares no code with `lico-llm-wire`; the two read the same documentation separately.
6. Honest ends: `Cut` never sends the end of the body, `Silent` never closes by itself.

## Versioning and budget

`0.1.0` while the shapes have not met a real server; semantic after. Budget: the default 256 MiB and 30 s.
