# lico-convo 0.1.0 — specification

Approved by the owner on 2026-11-17 (M16-P05, shown on the screen before anything was built; questions Q1 to Q4 answered yes). `tests/` states each line as a test. Marks: **SEEN IN CODE**, **KNOWN FROM DOCUMENTATION**, **PROPOSED**.

## In plain words

1. **What the record file holds.** One file per local session, owner-only (directory `0700`, file `0600`, the same as `registry.rs`, `handoff.rs`, `creds.rs`: SEEN IN CODE). It holds the conversation as the model must get it back: what the person typed, what the model answered (text and the tool calls it made), and the results of those calls (each cut at 8 KiB). It does **not** hold: any key, any raw server response or header, any reasoning text (the reader drops it before it gets here, and the record has no place for it), the system prompt, the tool definitions, the server address or the model name.
2. **A torn last line.** A crash can leave a half-written last line. On the next open it is cut off (the number of bytes is reported, the earlier lines are untouched) and nothing is lost but that one unfinished message. A damaged line in the middle is never guessed at: the file is refused and named by line number.
3. **The oldest whole turns are dropped, and where the person is told.** When the conversation no longer fits the window, whole turns (a message of the person and everything after it up to the next one) are left out of what is sent, oldest first, never a call without its result. They stay in the file. The person is told in the session's own log line ("N oldest turns are no longer sent to the model") each time the number grows. A single newest turn that alone does not fit is refused with the figures, not cut.

## Specification

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
