# Claude Code conversation records (scrubbed)

Version 1.0.0

Two records of the real program, read by the conversation reader (`provider_record`) in its unit tests. They are what the program wrote on 2026-10-27 in a private host
(`exp/202610/20261027/00003_record_layout_2_1_311/`), one session each, `--model haiku`, with Lico's tool connection and no other tool.

| File | Program | What the session did |
|---|---|---|
| `claude_2.1.310_send.jsonl` | 2.1.310 | asked to `send` a text to another session; the first call had other argument names and got an error result (`is_error`, a plain text), the second was answered `pending`; the person's decision came later as an input and was answered |
| `claude_2.1.311_sessions.jsonl` | 2.1.311 | asked to call the tool `sessions` once, then to answer with one word; a second turn asked for one word |

**What was changed** (`layout_check.py scrub`): ids of every kind are made up in the order they appear; the folder and the branch are made up; the text of thinking and its
signature are replaced; every record that is not a message (`attachment`, `api-request`, `queue-operation` and the rest) keeps its keys and the kind of each value, with every
text replaced by `x`; a text over 300 characters is replaced; the folder is `/work/proj`. The script then searches the result for what must not be in a tracked file and
stops when it finds any. What stays as the program wrote it: the keys, the types, the `version`, the times, the counts of the usage block, the model name, and the text of the
short prompts and results.

The records are for the layout. They say nothing about what the program writes for a tool it does not know, a long text, an image, a compaction or an error result: those
are in the unit tests as records made by hand.
