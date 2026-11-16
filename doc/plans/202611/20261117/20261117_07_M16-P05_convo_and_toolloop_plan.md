# M16-P05 Plan — `convo` and `toolloop`

- Status: approved by the owner on 2026-11-17 (the two interface specifications below, shown on the screen, are G2 of this phase; the four decisions Q1 to Q4 were answered yes, as recommended); in progress
- Version: 1.0.0
- Branch: `phase/M16-P05-convo-and-toolloop` (stacked on the P04 tip `0e6f024`)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)
- Marks: **SEEN IN CODE**, **KNOWN FROM DOCUMENTATION**, **PROPOSED**

## A. What the owner can check in plain words

1. **What the record file holds.** One file per local session, owner-only (directory `0700`, file `0600`, the same as `registry.rs`, `handoff.rs`, `creds.rs`: SEEN IN CODE). It holds the conversation as the model must get it back: what the person typed, what the model answered (text and the tool calls it made), and the results of those calls (each cut at 8 KiB). It does **not** hold: any key, any raw server response or header, any reasoning text (the reader drops it before it gets here, and the record has no place for it), the system prompt, the tool definitions, the server address or the model name.
2. **A torn last line.** A crash can leave a half-written last line. On the next open it is cut off (the number of bytes is reported, the earlier lines are untouched) and nothing is lost but that one unfinished message. A damaged line in the middle is never guessed at: the file is refused and named by line number.
3. **The oldest whole turns are dropped, and where the person is told.** When the conversation no longer fits the window, whole turns (a message of the person and everything after it up to the next one) are left out of what is sent, oldest first, never a call without its result. They stay in the file. The person is told in the session's own log line ("N oldest turns are no longer sent to the model") each time the number grows. A single newest turn that alone does not fit is refused with the figures, not cut.
4. **The loop's limits** (the numbers of report 119): 8 rounds for one message of the person, 4 tool calls in one answer, 16 in one message, one tool result at most 8 KiB, the arguments of a call at most 8 KiB, an answer at most 256 KiB, one turn at most 10 minutes. Reaching one ends the turn with a plain word; the model cannot change them.
5. **Which tools run unasked.** The loop hands a call to the host and does nothing else with it. Today's host runs **three** tools at once (`sessions`, `read_handoff`, `rename`: SEEN IN CODE, no approval card); every other tool leaves an approval card for the person, so the loop **ends the turn at that waiting tool** and goes on only when the person's decision arrives as a message from the host. The loop never answers a card.
6. **What the loop can NOT do.** No file and no shell (it has no way to reach either: it has no tool for them and no access to anything outside its ports); no new tool (a name that was not offered is refused with a word and never reaches the host); no network except through its one model port; no choosing its own limits; no running a tool whose request was not first written to the record.
7. **Said first, as the lead asked: a call that arrives with a cut or failed reply is not run.** If the stream breaks, the answer is shown as far as it came, not kept, and no tool call of it runs.

## B. `lico-convo` (component `components/convo`, crate `lico-convo`)

| Line | Content | Mark |
|---|---|---|
| Purpose | The conversation record of one local session on disk, and the choice of how much of it fits a context window. Nothing else: no network, no model, no tools. | PROPOSED |
| Dependencies | `lico-llm-wire` (the types `Message` and `ToolCall`), `serde_json =1.0.154` (the program's pin), `std`. Layer 1 (above `llm-wire`). | SEEN IN CODE (pin) / PROPOSED |
| Version | `0.1.0` | PROPOSED |
| Open | `Convo::open(path) -> Result<(Convo, Opened), Error>`; `Opened { loaded, repaired_bytes, created }`: creates the directory (`0700`) and the file (`0600`) when missing, reads every line, repairs a torn last line | PROPOSED |
| Read, append | `messages() -> &[Message]`; `append(&Message) -> Result<(), Error>`: one line, written in one `write_all` and synced; `System` messages are refused (the system prompt is built for each request, never stored) | PROPOSED |
| Line format | `{"v":1,"t":"user","text":..}`, `{"v":1,"t":"assistant","text":..,"calls":[{"id","name","arguments"}]}`, `{"v":1,"t":"tool","call_id","name","content"}`; no other key is ever written | PROPOSED |
| Errors | `Error { Io(kind), Corrupt { line }, NotOwnerOnly, NotARegularFile, SystemNotStored, TooLarge, Full }`: a symlink or a non-regular file, or a file readable by group or others, is refused; a line over 1 MiB is `TooLarge`; a file over 8 MiB is `Full` (the adapter says "start a new conversation"; what is sent is fitted separately) | PROPOSED |
| Fit | `estimate_tokens(&Message) -> u32` (bytes over 3.5, rounded up, plus 4 per message) and `fit(messages, budget_tokens) -> Fitted { start, dropped_turns, tokens, too_big }`: the newest turns whose estimates sum to at most `budget` (the caller subtracts the system prompt, the tool definitions and the room for the answer); whole turns only, oldest dropped first; the newest turn is always kept, and `too_big` says it alone is over | PROPOSED (report 119: the estimate and the default 8,192) |
| Limits | `MAX_LINE_BYTES` 1 MiB, `MAX_FILE_BYTES` 8 MiB: the whole conversation is held in memory, so this is the memory bound of one session | PROPOSED |
| Invariants | **C1** the file never holds more than the three line shapes; **C2** a torn last line costs at most itself; **C3** a call and its result are never separated by `fit`; **C4** the same input gives the same output; **C5** nothing else on disk is touched; **C6** never a panic on any file content | PROPOSED |
| Contract tests | round trip of every message kind (escapes, Korean, emoji, nested arguments); permissions of the directory and file; refusal of a symlink, a directory and a loose mode; torn line cut at every byte of a last line; corruption in the middle named by line; limits; `System` refused; only the whitelisted keys are in the file; `fit` at the exact budget boundary, whole turns, pairs kept, newest too big, empty; random file content never panics; the public names and limits pinned by tests that read the source | PROPOSED |
| Pattern, budget | A plain new component (it uses the file system through `std` only); default 256 MiB and 30 s | PROPOSED |

## C. `lico-toolloop` (component `components/toolloop`, crate `lico-toolloop`)

| Line | Content | Mark |
|---|---|---|
| Purpose | One turn of a local session: the person's message goes in, the model is asked, its tool calls are handed to the host one by one, their results go back to the model, until the model is done, a tool waits for the person, or a limit is reached. Pure logic over four ports; no IO of its own. | PROPOSED |
| Dependencies | `lico-llm-wire` (types only), `serde_json` (`Value` lengths); no `std::fs`, `net`, `process`, `env`, no clock, no thread. Layer 1. A test decides: the source may use none of them. | PROPOSED |
| Version | `0.1.0` | PROPOSED |
| Ports | `ModelPort::turn(&mut self, messages: &[Message], tools: &[ToolDef], sink: &mut dyn FnMut(Event) -> bool) -> Result<(), ModelError>` (the host's implementation does encode, HTTP, decode; `sink` returns `false` to stop the read); `ToolPort::call(&mut self, &ToolCall) -> ToolOutcome { Done(text), Waiting(text), Refused(text) }`; `RecordPort::append(&mut self, &Message) -> Result<(), String>`; `Env { cancelled() -> bool, now_ms() -> u64 }`. `ModelError { Failure(Failure), Gone, Timeout, Cancelled, TooBig { needed, window }, Other(String) }` | PROPOSED |
| Run | `run_turn(&mut Ports, &mut Vec<Message> history, input: Message, &Config, &mut dyn FnMut(Out)) -> TurnEnd`; `Config { tools: Vec<ToolDef>, limits: Limits }`; `Out { Text(String), Thinking, Tool { name, call_id, outcome: Done/Waiting/Refused } }`; `TurnEnd { Finished(StopReason), Waiting, Limit(Rounds / CallsPerTurn / Time / AnswerSize), Failed(Failed), Cancelled }`; `Failed { Model(Failure), Stream(StreamError), Port(ModelError), Record(String) }` | PROPOSED |
| Limits (default) | 8 rounds, 4 calls an answer, 16 calls a turn, result 8 KiB, arguments 8 KiB, answer 256 KiB, turn 600,000 ms | PROPOSED (report 119, accepted) |
| Behaviour | The input is appended to the history and to the record first. A round: ask the model with the whole history; stream `Text` and `Thinking` out; collect the calls; on `Done`: record the assistant message (text and calls) **before** any call runs, then for each call in order: a name that was not offered, a call with bad arguments (`BadCall`), arguments over 8 KiB or a call over the per-answer or per-turn limit gets a result that says why and is **not** handed to the host; the others go to `ToolPort::call`; every call gets exactly one result message (cut at 8 KiB with a note). If any call answered `Waiting`, the turn ends `Waiting` after the results of the whole answer are recorded: the person's decision comes back as an ordinary message from the host that starts the next turn. Otherwise the next round. | PROPOSED |
| Failures | A stream error, truncation or port error ends the turn `Failed`; the text that came stays shown, the answer is **not** recorded, no call of it runs. A failed `RecordPort::append` ends the turn before anything runs. | PROPOSED (the first is the lead's rule) |
| Cancel and time | `Env::cancelled()` is asked before each round and each call and the model read is stopped through `sink`; the clock is asked before each round and call | PROPOSED |
| Invariants | **L1** every call of an answer has exactly one result, in order; **L2** a tool is run only if it was offered, within the limits, after its request was recorded; **L3** after `Waiting`, `Failed`, `Limit` or `Cancelled` the model is not asked again in that turn; **L4** the history the model gets is the record plus nothing; **L5** a failed answer is never recorded; **L6** same ports, same input, same output; **L7** no panic | PROPOSED |
| Contract tests | scripted ports (no server): a plain answer; a tool round; parallel calls; a waiting tool (the model is not asked again, the other calls of the answer still get their results); every limit at its exact edge; an unknown tool and a bad call never reach `ToolPort`; results and arguments cut; a failed or truncated stream records nothing and runs nothing; a failed record runs nothing; cancel before, between and during; time; chat-only (no tools offered, any call refused); the history the model sees each round; **one end-to-end test** through the real client, the P04 fake and `lico-convo` (dev-dependencies, receipt inputs); the source is free of `std::fs`, `net`, `process`, `env`, `time`, `thread`, `unsafe`; names and limits pinned | PROPOSED |
| Pattern, budget | Ports and adapters (the host supplies the adapters in P06); default 256 MiB and 30 s | PROPOSED |

Decisions of the owner: Q1 the record refuses a file readable by group or others; Q2 a failed answer is not kept in the record; Q3 a message of the person that alone is over the window is refused with the figures; Q4 `convo` holds the whole conversation in memory with the 8 MiB file cap. All four: yes.

## Steps (small green commits; red first; the hook allows only green commits)
1. Docs commit: milestone/plan notes (P05, P06 sketch), the two SPEC.md. 2. `convo`: tests red against a stub (open/append, then fit), then green groups; mutation check; receipt. 3. `toolloop`: scripted-port tests red against a stub, then green groups (answer, calls, limits, waiting, failures, cancel and time), the end-to-end test; mutation check; receipt. 4. Link scan, `check`/`verify`, plan note result with measured lines, tests, runs and cost.


## Acceptance Criteria

`cargo xtask check` and `verify` pass; both components inside 256 MiB and 30 s with receipts; the contract tests pass; the mutation check shown; no version bump, no real server or home, no spec edit.
