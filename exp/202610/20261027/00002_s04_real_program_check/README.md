# 00002 s04_real_program_check

Version 1.0.1

**Question.** With the real `claude` program, does the way the tool connection answers `send` and `create` hold up: the call answers `pending`, the model ends its
turn, the person's decision comes later as an input, and the session answers that input? And two things the mocks cannot say: what the hand-over of a real
conversation stores, and what happens when a session's conversation is gone from the program.

**Workload.** `s04_check.py`, opt-in, never part of the suite. Each run starts a private host (own socket and state folder; no other socket is touched) with the
program as the Claude adapter's program, sessions on `--model haiku` in mode `default`, short prompts, no tool but Lico's. The script says yes or no to the host's own
cards through the owner operation `approval.answer`; a question of the program's own is denied and counted (none came). The sign-in is the one of the home the script
runs under; the script never reads it, and the program's environment is PATH, LANG and HOME. Budget: 16 model turns, counted from the records (`[turn completed]` and
`[turn failed]` of the sessions of the program that the check made). The 5 h and 7 d figures come from the status-line file named by `--status-files`, before and after.

**What ran** (2026-10-27, one machine; `claude` 2.1.310 for the first five rows, which is the version Lico pins, 2.1.311, the one on PATH, for the gone rows):

| Row | Tries | Model turns | What the script did |
|---|---|---|---|
| send | 3 | 9 (3 each: caller twice, receiver once) | session A is asked to `send` "Reply with the single word pong." to session B; the script answers yes after A's turn ended |
| long | 1 | 3 | as send, but A is also asked for about 600 words after the call, and the script says yes while A writes them |
| create | 1 | 3 | A is asked to `create` a session with the first prompt "Reply with the single word ok."; the script says yes after A's turn ended |
| hand-over | 1 | 0 | after send try 1, the conversation of A is handed to a session of the Codex simulator and the head of the stored file is read |
| gone | 1 | 1 | one real turn, the program's record file of that conversation moved away, the session closed, Enter pressed in the product window with a text in the box |
| gone0 | 1 | 0 | the same without a turn: the host stopped, the registry given a conversation id the program has no record of, the host started again |

Total 16 model turns. Allowance (whole percent, the figure follows the status line, so it can lag): 5 h 1% before and after; 7 d 17% before, 17% after the
first run, 18% before the last row.

**What the model saw.** The answer of `send` and `create` while the person has not decided (the program's tool result, JSON): `{"approval_id":1,"message":"Nothing was
sent yet. The person has been asked to approve this send to session 'receiver-1' (s8c0676). Lico will tell you here when they decide; you can end your turn. A restart of
Lico drops the request.","status":"pending"}`; for `create`: `"Nothing was made yet. The person has been asked to approve the creation of a new session 'ok-check' in a
scratch folder. ..."` with the same two sentences after it. The decision comes as a user input of two lines: `[from lico | 14:18 | approval 1]`, then `approved and done: 32 B
delivered to session 'receiver-1' (s8c0676) (inline); the receiver was idle`; for `create`: `approved and done: created session s... 'ok-check - made by caller-c' in a
scratch folder, mode: asks for approval; the first prompt (49 B) was delivered (inline). It is working on it; send to it with its id to talk to it.` A call with the wrong
argument names got `bad_request: send needs `to` (a session id from the sessions tool) and `text` (not empty, and not only characters that draw nothing)`.

**Observed result.**

- *send, 3 of 3:* A's turn ended after `pending` with a short sentence and no further call of Lico's tools; the receiver got the text and answered `pong`; the decision
  reached A as an input after its turn had ended, and A's second turn answered it in one or two sentences. In tries 2 and 3 (and in long and create) the model called
  `ToolSearch` before the Lico tool (the tool's schema is loaded on demand); in try 1 it did not, called `send` with `session_id` and `message`, got the `bad_request`
  above, and called again with `to` and `text`. The script printed `did not hold` for try 1 because its criterion counted that second call as polling; the criterion is
  corrected in this file (`polled()` looks only at calls after the `pending` answer) and, on the program's own record of try 1, there is no call after `pending`. Try 1
  was not run again, the budget was spent. The same first run counted the Codex-simulator receiver of the hand-over as a model turn (it printed 16 where 15 had been
  spent), which skipped the gone row; the count is corrected too and the gone rows were run on their own.
- *long, 1 of 1:* A's essay was whole, 568 words in one text block of the first turn; the decision was queued by the program while it wrote and handled after the turn, as a
  separate turn that answered it.
- *create, 1 of 1:* the card said `model: haiku, effort: the program's default`; the new session started on `claude-haiku-5-5`, read its first prompt and answered `ok`; the
  decision reached A and A's second turn answered it. The host audit lines of the run (stderr of the host) are `request ... state=asked`, `call ... result=created`,
  `request ... state=done`.
- *hand-over, send try 1:* reply `{ok:1, kind:conversation, delivery:file, size:2333}`; the stored file has 48 lines. Its first 12 lines (session ids shortened):

```
Conversation record, read by Lico from the program's own record of the session (Claude Code 2.1.310).
Messages shown: 10 (2 user, 6 assistant, 2 tool results), from 2026-10-27 05:18:43 UTC to 2026-10-27 05:18:51 UTC.
Compaction: none found in the record.
Not shown: api-request 4, api-request-blob 5, api-request-shape 1, atis-latch 4, attachment 24, block thinking 2, last-prompt 3, queue-operation 4, system 2.
Lines at the left edge were written by Lico; the text of the conversation is indented by two spaces.

## user | 2026-10-27 05:18:43 UTC
  Send the message 'Reply with the single word pong.' to the session s8c0676... with the tool mcp__lico__send. Use no other tool.

## assistant | 2026-10-27 05:18:46 UTC

## assistant | 2026-10-27 05:18:46 UTC
```

  The two `thinking` blocks are counted in the `Not shown` line (`block thinking 2`); the messages that held only a thinking block are shown as an assistant heading
  with no text under it and are counted in "Messages shown". The same hand-over made from the 2.1.311 record in the gone row answered `record_format`: the reader is
  pinned to 2.1.310 and refuses another version (true when this was run; `00003_record_layout_2_1_311` measured the layout and the reader now reads 2.1.311 too).
- *gone (2.1.311, one real turn):* the program does not swallow the text. Enter in the window reconnected the session (`[provider reconnected]`), the host echoed
  `>> second question`, and the program answered `[turn failed] error_during_execution`, `[error] No conversation found with session ID: <id>`, then the process ended
  (`[provider disconnected]`). The row went `running` at 0.26 s and `disconnected` at 0.46 s after Enter and stayed `disconnected` (10 s later still; the generation is the
  new one). The window shows those lines in the transcript pane, the editor box is empty (the text went with the input and is not given back, there is no editor note),
  and the hint is the Disconnected hint ("Disconnected: Enter reconnects and sends, Ctrl+R reconnects ..."). *gone0* gave the same history, timing and window, with
  the header `claude | no account | model not reported | disconnected | mode: asks for approval`; a screenshot of it was taken and is not kept here.

**Not measured.** More than three tries of send, a second model, a model that polls (none did), the program's other versions, a long wait before the decision, a restart
of the host between `pending` and the decision, the program's behaviour with a window not in front.

**Left behind.** The program wrote its own record for each of the 11 sessions of the real program into its per-folder record store under the sign-in's home. The one of the
gone row was moved into the run's temporary folder by the script; the other 10 stay, the check does not delete them. The temporary folders of the host are removed by the script.

    PYTHONPATH=src python3 s04_check.py --lico PATH_TO_LICO --out RESULT.json [--rows send,long,create,gone,gone0] [--tries 3] [--status-files GLOB] [--shots DIR]
    (--sim: the protocol simulator in place of the program, no model call: a dry run of the script itself)
