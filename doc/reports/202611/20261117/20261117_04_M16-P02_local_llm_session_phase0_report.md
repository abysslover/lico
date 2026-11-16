# M16-P02 Report — Phase 0 for the Local LLM Session

- Status: Phase 0 (read-only: no code, no moves); waits for the owner's pick
- Version: 1.0.0
- Date: 2026-11-17
- Branch: `phase/M16-P02-local-llm-session-phase0` (on `phase/M16-P01-retire-prototypes`, base `main` at `6712fe5`)
- Plan: [M16-P02](../../../plans/202611/20261117/20261117_03_M16-P02_local_llm_session_phase0_plan.md)
- Marks: every figure is **MEASURED** (read or counted in this tree today), **COMPUTED** (from measured figures) or **ASSUMED** (my estimate or my knowledge of a public API, to be checked)

## 0. Owner page (one screen)

**What you asked for.** A Local LLM server that discovery found becomes its own kind of session next to Claude and Codex. Lico runs the tool loop itself; Claude and Codex keep their sign-in and are not routed to local models; servers on this machine only.

**What exists (MEASURED).** Discovery (`llm`, 1,345 lines, 7 files) finds servers and lists their model ids but only reads (`GET`). A session today is a provider program that Lico supervises (`claude.rs` 598 lines, `codex.rs` 758 lines); there is no model client, no HTTP `POST`, no conversation store and no tool loop in Lico: the programs of Claude and Codex own all three. Lico's tools (10 in the list, 11 with clipboard writing on; 4,849 bytes with writing off) are the ones an agent calls through a loopback tool connection; three need no approval (`sessions`, `read_handoff`, `rename`), the others ask the person first.

**What it takes (COMPUTED/ASSUMED).** Five new components (`llm-wire`, `llm-http`, `llm-fake`, `convo`, `toolloop`), one thin host adapter, a small driver port so that the new kind does not enlarge the 48-module cycle, and GUI rows. About **7,500 lines including tests** in five build phases and a release. Order: wire, then client and fake server, then conversation and loop, then the host adapter with CLI/HTTP parity, then the GUI.

**Cost (ASSUMED rate, see section 6).** About **17 to 42 USD** for everything; a headless slice (no GUI) about **15 to 36 USD**; a chat-only slice (no tools) about **9 to 23 USD**. The range is wide on purpose: it is an estimate of new code, and this milestone's two phases so far cost less than I had planned. The credit left at your last count was 25.2 USD (150 limit, 124.8 used); P01 (1.9 USD) and this report (1.2 USD) cost 3.1 USD by this session's counter (MEASURED), so **about 22 USD is left**. The chat-only slice fits; the headless slice fits only at its low end; the full build needs a higher limit.

**Three things you should decide first.**
1. **Tools.** Lico's own tools are about sessions, messages, screen and clipboard. They contain **no file read or edit and no shell**; a local session with only these cannot work on a project's files (Claude Code has its own file and shell tools; a local model would have none). I recommend this milestone gives local sessions the existing tools only (three free, eight asking), and that file and shell tools for local sessions become their own later milestone with their own approvals.
2. **Servers without tool calls.** Chat only, said plainly to the person; no text-protocol fallback (fragile, and a weaker model that obeys text is easier to inject).
3. **Money.** Raise the limit, or begin with the chat-only slice and look at it before the rest.

**Safe default (recommended).** Loopback only; the key (if any) only in the keyring; the model's output is data; only the three no-approval tools run unasked, everything else shows its card as today; hard limits on rounds, time and sizes; no file or shell access.

Section 7 lists the ten questions with my recommendations; none of them is a spec edit (the LLM-01..03 text still waits for your yes, and one point, the session kind value `local`, touches the protocol's kind list).

## 1. What exists and what the session kind builds on

### 1.1 Discovery (M12, M13)

- `llm/` 1,345 lines in 7 files (MEASURED): `mod.rs` 204 (the run: `llm.discover`, bounds 64 sockets, 8 at a time, 1.5 s a request, 8 s a run), `classify.rs` 178 (pure: what an answer says), `http.rs` 211 (the probe client), `net.rs` 216 (listening sockets from `/proc`), `remote.rs` and `remote_script.rs` (ssh hosts, out of scope here), `tool.rs` 136 (the tool `llm_endpoints`).
- A result row carries (MEASURED, `llm/mod.rs:74`): `port`, `bind` (loopback, all interfaces or a specific address), `address`, `kind` (`openai`, `ollama` or other), `apis`, `auth` (whether it wants a key; a key is never tried), `models` (at most 50 ids, `models_total`, `models_truncated`), `process`, `probe_ms`, `detail`. The window's dialog (`gui/llm_find.rs` 521 lines, `gui/llm_rows.rs` 263 lines, pure functions of the reply) keeps the last result of each choice in memory only, with its age.
- The client is `GET HTTP/1.0`, no body, no credential, bounded in time and bytes (MEASURED, `llm/http.rs` head). **No HTTP client crate is in the program** (MEASURED: `Cargo.toml` has `winit`, `softbuffer`, `cosmic-text`, `unicode-linebreak`, `x11rb`, `serde_json`, `lico-png`). So `POST`, chunked bodies and streaming are new code.

### 1.2 How a session kind is added today

- A provider is a **string** (`"claude"`, `"codex"`, plus `"synthetic"` in the protocol), not a type or a trait (MEASURED). An adapter is a module with the same shape: a `Link` struct held in the `Session` (`s.claude`, `s.codex`), and the functions `start`, `start_with_resume`, `resume`, `send_input`, `answer`, `note` (MEASURED: `claude.rs` 38 to 172, `codex.rs` 36 to 441). It writes text lines into the session's log with `s.append(bytes)` (`record`, `record_text`, `record_agent`), which the window shows.
- Dispatch is by `if provider == ...` at **12 call lines in 4 modules** (`core.rs` 7, `folders.rs` 2, `calls.rs` 2, `admin.rs` 1) and **50 comparisons** of `provider` in non-test code (MEASURED by grep). The value is also fixed in three more places: the protocol check `("session.create", "kind") => synthetic | codex | claude` (`ipc.rs:141`), the registry (`registry.rs:73` refuses any other provider and `session.rs:587` maps everything not `claude` to `codex`).
- The adapter of Claude uses **17 modules** (`calls`, `core`, `folders`, `handoff`, `leads`, `lines`, `notify`, `pasted`, `quota`, `registry`, `session`, `tokens`, `tools`, `transfer`, `codex`, `broker`, `ssh_remote`); the hubs have these fan-ins in the module graph (COMPUTED by `cargo xtask graph`, 2026-11-17, text-based): `core` 41, `session` 35, `folders` 22, `transfer` 15, `codex` 17, `handoff` 7. The cycle is still 48 modules (167 modules, 62 nodes).
- Records: what the window shows is the host's text log (`.seg` files, `session.rs` 800 lines); the full conversation of a **Claude** session is the program's own file, read by `provider_record` (470 lines) only when a person asks; resume restarts the program with its own session id; the registry brings every session back `disconnected` with its history (`registry.rs` 201). The chats list (`chats/` 725 lines) discovers Claude and Codex chats from their homes. **For a local session there is no program and no file of its own: Lico itself must keep the conversation**, because the model has no memory between requests.
- New session and + New chat (`gui/newsession.rs` 674 lines, `gui/newchat.rs` 517 lines): the form asks provider, account, folder, name, model, effort, mode and sends `session.create` with `kind`. About 135 lines naming the two providers in 24 `gui` files (MEASURED by count). A local session has no account: its row is a **server and a model**.
- Parity: all of it goes through one dispatch (`ipc.rs::dispatch_request`) shared by the local IPC, the HTTP listener and the CLI, so a new `kind` and its arguments are parity by construction.

### 1.3 The tool connection

- The tools reach a Claude or Codex session through a JSON-RPC server on the loopback with a key per provider process (`tools.rs` 702 lines, `mcp.rs` 437 lines). The table `TOOLS` in `mcp.rs` lists 10 tools; `clipboard_write` is added when its switch is on. The list result is **4,849 bytes with writing off and 5,103 with it on** against the budget 5,120 (MEASURED in M15-P04).
- Behind it `tools.rs::run(shared, caller, name, arguments)` (private now) maps a name to the implementation, with `caller` the session id; nothing in it needs a network connection, so a local session can call it **in process**, with its own id as caller.
- Approval (MEASURED by reading): **no card** for `sessions` (names and states of other sessions), `read_handoff` (the caller's own inbox, read-only) and `rename` (its own name); **a card on the agent's row** for `send`, `create`, `history`, `capture`, `clipboard_read`, `clipboard_write`, `computer`, `llm_endpoints` (`pending.rs` 790 lines). The decision happens when the person answers the card; the agent is told by a notice from the host later, not by the call. For a local session that means the loop must end its turn on a waiting tool and take the notice as the next input, as the other sessions do.

## 2. The model side

(The wire details below are **ASSUMED** from the public documentation of the two APIs as I know it; no server was contacted. The first build phase checks them against the documentation and against the fake server's recorded vectors.)

- **OpenAI-compatible** `POST /v1/chat/completions`: body `model`, `messages` (`system`, `user`, `assistant` with `tool_calls`, `tool` with `tool_call_id`), `stream: true`, `tools: [{type: "function", function: {name, description, parameters}}]`. The stream is server-sent events (`data: {...}` lines, `data: [DONE]`); text arrives in `choices[0].delta.content`; tool calls arrive as deltas by `index` with the name once and the `arguments` as a string in pieces that must be joined and then parsed (it may not be valid JSON); `finish_reason` is `tool_calls` or `stop`; usage only if asked for.
- **Ollama** `POST /api/chat`: body `model`, `messages`, `tools`, `stream`; the stream is one JSON object per line; `message.content`, `message.tool_calls[].function{name, arguments (an object)}` (no call id: Lico makes one), `done`, `done_reason`, `prompt_eval_count`, `eval_count`. Models may emit reasoning in a separate field.
- **A server without tool calls** (Ollama reports capabilities per model; OpenAI-compatible servers mostly just fail or ignore `tools`): **recommendation: chat only**, said to the person on the session row ("this server cannot call tools"); on a refusal of the `tools` field the first request is repeated once without it and the session is marked chat only. No text protocol (parsing "tool: x" out of prose): fragile, and it makes the weakest models the most exposed.
- **Cancellation**: the person's stop or Esc closes the connection from Lico's side and ends the turn within about 1 s (ASSUMED target; the reader polls a cancel flag between reads with a 250 ms read timeout). A half-built tool call is dropped, never run.
- **Timeouts** (ASSUMED values, all in one place): connect 2 s (discovery uses 0.3 s for a probe; a loaded machine needs more), first byte 120 s (a model may load first), silence between stream pieces 60 s, a whole turn 10 min, a whole conversation step bounded by the loop limits below.
- **Context and truncation**: the window size is not reliably reported (Ollama has it per model; others vary): a per-session `context_tokens`, default 8,192 (ASSUMED), changeable. Tokens are estimated from bytes (ASSUMED: about 3.5 bytes per token) and checked against the server's own usage counts when it reports them. When the conversation does not fit, the oldest **complete** turns go first (a tool call and its result are never separated), the system prompt stays, and the person is told ("the oldest 6 turns are no longer sent"). A single message larger than the window is refused with the figures.
- **Errors and what the person sees** (one line each on the session row and in the log): server gone ("no server at 127.0.0.1:PORT; the session is disconnected, the next message tries again"); model missing (HTTP 404 or Ollama "model not found": "the server has no model NAME; choose another"); key needed (401, 403: "the server wants a key; store one with Session > Local LLM key"); malformed or cut stream ("the answer was cut; nothing was run"); server error text is shown cut and cleaned, as data. No retry loops: one automatic retry of a refused connection, nothing else.

## 3. The Lico tool loop

- One round: model answer, then its tool calls (at most 4 per answer), each checked against the allowed set, run in process with the session as caller, results back as `tool` messages, next request. A tool that needs approval answers "waiting for the person" at once; **the turn ends there** and the person's decision arrives as a host notice that starts the next turn. So an approval never blocks a thread or the loop.
- **Allowed tools**: the table `TOOLS` is the only source (names and argument rules are checked by the same code Claude's calls pass); the definitions sent to the model are generated from it (4,849 bytes, about 1,400 tokens ASSUMED, sent with every request; a chat-only session sends none). Default set: the three no-approval tools; the eight others are offered too and each shows its card as today (the first question in section 7 is whether to offer them at all).
- **Loop limits** (ASSUMED values): 8 rounds per user message, 4 tool calls per answer, 16 per user message, one tool result at most 8 KiB (cut with a note), arguments at most 8 KiB, a turn at most 10 min; reaching one ends the turn with a plain line, never a silent loop.
- **What the loop must not be able to**: run anything outside `TOOLS`; touch files or run a shell (none exists in `TOOLS`); answer its own approval card (only the person's answer does); create sessions without a card; change its own limits, endpoint, model or key; reach any address that is not loopback; call another session's tools with another caller id; persist or log a key; follow a redirect; loop without a limit.

## 4. Security

- **Loopback only**: connect only to `127.0.0.1` or `::1`, taken from the discovery row's own address, never a name (no DNS), no redirect, no TLS, no proxy. A server bound to all interfaces is still reached through loopback. Anything else is refused by the client component itself (a contract test).
- **Key**: only in the OS keyring through the existing client (`keyring.rs` 138 lines over `dbus.rs`: `set`, `get`, `delete` by attribute pairs), attributes (proposed) `service=lico`, `kind=local-llm`, `endpoint=ADDRESS:PORT`. It is read per request, sent only as `Authorization: Bearer`, wiped after use, never on a command line, in a file, in a log or in a record; entered in the window (masked) or from standard input in the CLI. Metadata file `creds` (105 lines, masked labels only) already exists for the reference.
- **Logged**: connection events and status codes and sizes (no content) in the host's diagnostic output; **recorded**: the conversation text in the session's own record as for any session (that is the product), owner-only (`0600` in an owner-only directory). **Not logged or kept**: keys, raw discovery responses (AUTH-08 stays), request bodies in diagnostics.
- **Prompt injection**: a weaker model with tools is the exposure. What it reads (a message from another session, a clipboard text, a screen text, a model id) is data; the tools that can act (`send`, `create`, `computer`, `clipboard_write`) each need the person's card, which shows what will happen. The injection route that stays open is a card the person allows without reading; the card text is built by the host, not by the model, as today. Smallest safe default: only `sessions`, `read_handoff`, `rename` run unasked.
- Model ids and server text are cut to a safe alphabet and length before the model or the window sees them, as `llm_endpoints` already does.

## 5. Component design

Rules (component rules 1.0.1): downward-only explicit dependencies, `SPEC.md`, contract tests, `RECEIPT.json`, 256 MiB and 30 s per run; the few system tests named in one list at 512 MiB and 60 s, in Rust; no new Python; no new edge into the hubs.

| Component | Layer and dependencies | Public surface (proposed) | Contract tests (no real server) | Lines, with tests (ASSUMED) |
|---|---|---|---|---|
| `llm-wire` | pure, std and `serde_json` only | neutral `Message`, `ToolDef`, `Event` (`Text`, `ToolCall`, `Done`, `Error`); `encode(api, request)`; `Decoder::new(api).feed(bytes) -> events` for SSE and NDJSON; call-id making for Ollama | golden vectors of both APIs; byte-by-byte feeding; cut and malformed JSON; arguments in pieces; oversize cap; no panic on any input | 900 + 700 |
| `llm-http` | std only | `post_stream(addr, path, headers, body, limits, cancel, on_chunk)`; refuses a non-loopback address; chunked and plain bodies; deadlines; byte caps | against `llm-fake`: slow, hung, closed mid-stream, 401/404/500, huge, redirect refused, cancel within 1 s | 500 + 450 |
| `llm-fake` | std only (a test-support crate) | a scripted OpenAI-style and Ollama-style server on an ephemeral loopback port | its own: scripts replay as written, port is loopback and ephemeral | 450 + 150 |
| `convo` | std and `serde_json` | append-only JSONL conversation (`0600`), `load`, `append`, `fit(window)` that drops whole oldest turns and reports it | atomicity on a cut write, no split call/result pair, size caps, permission bits | 450 + 350 |
| `toolloop` | depends on `llm-wire` types and `convo` only; talks through ports it defines (`ModelPort`, `ToolPort`, `Clock`) | `run_turn(state, input, ports, limits) -> Outcome` (`Text`, `Waiting`, `LimitReached`, `Failed`) | scripted ports: every limit, a waiting tool, cancel, a refused tool name, results cut, no network at all | 700 + 650 |
| thin host adapter `local` (inside the program) | uses the driver port, `tools::run`, `keyring`, `session` | the `Link` shape plus a small `Driver` registration | the system tests below | 500 |
| driver port (a small low-level module) | depends on nothing; the hubs call it, the adapter registers itself at start | `trait Driver { start, resume, send_input, answer }`, `for_kind(kind)` | existing session tests unchanged; one new for the third kind | 150 + edits at the 12 call lines |
| display-free GUI part | pure functions in a `gui` module, as `llm_rows.rs` | rows of the server and model choice, wording of every error above, state of the choice | unit tests as `llm_rows` has | 400 + 300 |
| painter changes | the `gui` painter | the form row, the session row line, the key dialog | screenshots by the existing window tests | 400 |

- **The cycle**: the adapter must use `core`, `session`, `tools`, as `claude.rs` does; if the hubs call it by name it joins the 48-module cycle. With the **driver port** the hubs call the port, the adapter registers itself in `unified.rs`, and the cycle stays at 48 (stop point: the port step is its own small phase with the existing tests as its proof; it also shows what the second provider adapter would cost).
- **Verification under the budget**: each component run alone with `cargo xtask test NAME` (the pure ones are expected in well under 5 s and 100 MiB; `llm-http` with the fake server a few seconds, ASSUMED from `lico-png`: 4.2 s and 83 to 95 MiB measured for 25 tests). **System tests** (the short named list, Rust, a fake server and the real host on a private home and socket, no display): (1) a message through a local session produces streamed lines in the log; (2) a tool call that needs a card waits, the person allows, the result comes back and the answer ends; (3) stop cancels a running stream; (4) the server vanishes and the session says so and recovers on the next input. Four tests, each expected inside 512 MiB and 60 s (ASSUMED).
- **Build order and stop points**: P03 `llm-wire` (stop: the owner sees the vectors and the spec); P04 `llm-http` and `llm-fake` (stop); P05 `convo` and `toolloop` (stop: a loop that runs entirely against scripted ports); P06 the driver port, the adapter, CLI and HTTP parity, the four system tests (stop: a headless local session works end to end against the fake server, **the first moment a real local server may be tried, by you, with your allowance**); P07 the GUI; P08 release `0.4.0` (the whole suite, idle and hidden numbers, acceptance, install).

## 6. The estimate

Rate (ASSUMED, from what was measured): M15 cost about 21.7 USD in all (COMPUTED from the session counters), of which P03 (about 1,400 lines moved or split and about 600 new, with the selection work and a mutation check) about 3.2 USD and P04 (a release, no new code) about 2.2 USD; in this milestone P01 (a deletion and a guard test) cost 1.9 USD and this report 1.2 USD (both MEASURED by this session's counter). Those are small figures because they are moves, deletions and reading. New code with tests, the hook's runs and one or two corrections costs more per line, and a test that must be rerun many times costs more than any of this, so I take **2 to 5 USD per 1,000 new lines** (the low end is M15-P03's rate with a margin, the high end is a bad run of failures).

| Phase | Content | New lines with tests (ASSUMED) | Cost (ASSUMED, USD) |
|---|---|---:|---:|
| P03 | `llm-wire` | 1,600 | 3 to 8 |
| P04 | `llm-http`, `llm-fake` | 1,550 | 3 to 8 |
| P05 | `convo`, `toolloop` | 2,150 | 4 to 11 |
| P06 | driver port, adapter, CLI and HTTP parity, four system tests | 1,100 | 3 to 6 |
| P07 | GUI (display-free part and painter) | 1,100 | 2 to 6 |
| P08 | release `0.4.0` (no new code; M15-P04 measured 2.2) | 0 | 2 to 3 |
| **Total** | | **7,500** | **17 to 42** |

- **Headless slice** (P03 to P06, then P08): about 6,400 lines, **15 to 36 USD**. **Chat-only slice** (no tools: wire, client, fake, `convo`, the port and a small adapter, no loop, no GUI): about 4,650 lines, **9 to 23 USD** (COMPUTED from the rate).
- **Against the limit**: 25.2 USD left at your last count; P01 (1.9) and this report (1.2), MEASURED, leave **about 22 USD**. The chat-only slice fits; the headless slice fits only if its low end holds; the full build needs a higher limit.
- If the limit comes near during a phase I stop at a green commit and say so.

## 7. Open questions for the owner, with recommendations

1. **Money**: raise the 150 USD limit, or begin with the chat-only slice. Recommendation: begin with P03 and P04 (about 6 to 16 USD) and look at what they cost before deciding on the rest; the limit decision can wait until then.
2. **Default tool set / file tools**: Lico's tools have no file or shell access. Recommendation: this milestone offers the existing tools only (three unasked, the others asking); a later milestone designs file and shell tools for local sessions with approvals. Is that what you meant by "the same operations"?
3. **Servers without tool calls**: chat only, said plainly (recommended), or refuse to start a session on them.
4. **Model selection and where it is remembered**: chosen in New session and + New chat as server plus model from the last discovery result (kept in memory by the dialog today), remembered as the form's last-used choice (host state, owner-only: address, port, model name; never a raw response); changeable per session.
5. **Reasoning text**: not shown and not kept by default (recommended); a later toggle. A model that thinks aloud must not spend the context window on it: the field is dropped before the next request.
6. **Record format**: its own small JSONL of the conversation (needed to rebuild the model's context), next to the host's display lines; not shared with Claude's or Codex's formats, which belong to their programs. The chats list reads it as a third source.
7. **CLI and HTTP parity**: yes, by construction (`session.create` with `kind: "local"`, `endpoint`, `model`, `context_tokens`, and a key operation reading the key from standard input). One point is not mine to decide: the protocol check lists the kinds `synthetic`, `codex`, `claude` (`ipc.rs:141`); if the protocol document names the kinds, adding `local` is a spec edit and waits for your yes; the proposed LLM-01..03 text is where it would go.
8. **The key**: a key per server address and port in the keyring, asked once, in the window or on standard input; never on a command line.
9. **Context default**: 8,192 tokens per session with a visible "oldest turns dropped" note; or ask the server when it can tell (Ollama) and fall back to 8,192.
10. **Version and order**: `0.4.0` at the release phase (a new kind of session); P01 and P02 of this milestone bump nothing; the maximized first start waits after this milestone as you said.

## 8. Honest limits of this report

- The wire formats are from my knowledge of the public documentation, not from a server; the first build phase must check them.
- The line counts and costs are estimates with ranges; the graph figures are module-level and text-based (no method or trait-object coupling).
- Nothing was run against any model, nothing billable, no real home, socket or service was touched; only documents changed in this phase.
