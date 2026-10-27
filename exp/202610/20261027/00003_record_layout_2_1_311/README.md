# 00003 record_layout_2_1_311

Version 1.0.0

**Question.** The conversation reader of Lico (`provider_record`, hand-over of a whole conversation) read only records written by Claude Code 2.1.310 and refused every other
version. The program on the owner's machine is now 2.1.311. Does the record of 2.1.311 have, for everything the reader reads, the layout of 2.1.310?

**Workload.** `layout_check.py`, opt-in, never part of the suite. `run` starts a private host (own socket and state folder), one session on the program given by `--claude`,
`--model haiku`, mode `default`: turn 1 asks for one call of Lico's `sessions` tool and then the word `done`, turn 2 asks for the word `ok`. That is 2 model turns. The record
the program wrote is copied out under its version. `table` reads the records and prints, for each record type, each content block type, each key of a record, of a message
and of a block, the shapes of `content` and the values of `version`, one of the words **same / new / gone** of the candidate against the reference; the rows the reader reads
are marked `read`. Only keys, types, counts and the version are printed, never a word of a conversation. `scrub` makes a fixture out of a record (see below).

**Tested.** Reference: the 10 records of 2.1.310 that the check `00002_s04_real_program_check` left (62 messages with a `version`). Candidate: the record of 2.1.311 made by `run`
(9 messages). Allowance (whole percent, from the status line, so it can lag): 5 h 3% before and after, 7 d 18% before and after.

**Observed result.** 171 rows: same 162, new 6, gone 3.

| | What | Read by the reader? |
|---|---|---|
| new | record type `tools-used` (keys `type`, `sessionId`, `tools`) | the reader reads the type name only: it counts the record as `tools-used` in "Not shown" and nothing else |
| new | key `turnAttributionKey` of a `user` record | no |
| new / gone | the value of `version` (`2.1.311` where there was `2.1.310`) | yes: this is the pin |
| gone | key `is_error` of a `tool_result` block | yes, only when the result is an error: one result in the 10 records of 2.1.310 (the `bad_request` answer to a call with wrong argument names) has it, the 2 turns of 2.1.311 made none |
| gone | `content` of a `tool_result` as a plain text (`str`) | yes (both shapes are read): the same one result in 2.1.310; 2.1.311 answered with a list every time in this sample |

Everything else the reader reads is the same: the record types `user`, `assistant`, `system` (`subtype`), `attachment`, `api-request`, `api-request-blob`, `api-request-shape`,
`atis-latch`, `last-prompt`, `queue-operation`; the block types `text`, `thinking`, `tool_use`, `tool_result` and, inside a result, `text` and `tool_reference`; the keys
`message`, `message.content`, `timestamp`, `isSidechain`, `version`, `type`, and `name` and `input` of a tool use; `content` as a list in a message; the time written as ISO.
The two `gone` rows that are not the version are one result, and a gap of the sample, not a change: an error result did not occur in 2 turns of 2.1.311. The reader handles the
shape and the unit tests cover it, with the real 2.1.310 record (`tests/fixtures/claude_record/`) and with records made by hand. What 2.1.311 writes for a compaction, an image or a long text was not measured.

**Consequence.** The reader accepts a list of versions, `[2.1.310, 2.1.311]` (`READ_VERSIONS` in `provider_record/mod.rs`), and the head of a hand-over names the version(s) of the
messages it read. A message of any other version still refuses the whole read. `version.cli` of `capabilities.json` is not moved: it is the version the adapter was measured
on. The scrubbed records are `tests/fixtures/claude_record/` (README there). When a third version has to be added: run this check against it first, and put it in the list
only when every row the reader reads is `same` or explained.

    PYTHONPATH=src python3 layout_check.py run --lico PATH_TO_LICO --claude PROGRAM --out DIR [--status-files GLOB]
    python3 layout_check.py table --reference A.jsonl B.jsonl ... --candidate C.jsonl [--md]
    python3 layout_check.py scrub --in RECORD.jsonl --out FIXTURE.jsonl
