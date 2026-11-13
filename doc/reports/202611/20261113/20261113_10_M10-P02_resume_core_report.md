# M10-P02 Report — Resume Core: `turns`, One `latest_chat` Rule, `--resume-latest`, MCP `resume`

- Status: implemented, awaiting owner inspection (G3)
- Version: 1.0.0
- Branch: `phase/M10-P02-resume-core` (stacked on the P01 tip; implementation commit `3a15c3a`)
- Plan: [M10-P02](../../plans/202611/20261113/20261113_10_M10-P02_resume_core_plan.md)

## What changed

- **`turns`** (`chats/turns.rs`): visible user and assistant text messages (the title scan's own extractors, so tool calls, tool results, side chains and Codex context text do not count), a streaming count bounded to 8 MiB and 100,000 lines per transcript, `null` beyond the bound or when unknown, a 512-entry cache keyed by (file, modification time, size), computed only for the items of the returned page. Remote (ssh) chats report `turns: null`: counting inside the remote script would read every matched remote file on each listing; the GUI shows `?` for it.
- **One rule** (`chats/latest.rs`, `chats/mod.rs`): the list is built once by `collect` (same scope, merge with open tabs, sort); `session.chats`, `lico chat latest` and creation with `resume_latest` all read it. "Latest" is the first item of the asked provider and account among chats that have a saved transcript (new request field `recorded`, also `--recorded` and `?recorded=1`): a tab that has said nothing yet is in the list but is never the chat to continue. This was found by a red test: without it the freshly opened blank tab shadowed the real chat.
- **Creation** (`new_session.rs`, moved out of `core.rs`, which has a size ratchet): `session.create {resume_latest: true}`. Latest chat open in a running tab: that tab is shown again (`already_open: true`, no new process); in a disconnected tab: resumed in place with a new generation; otherwise a new session continues it (`--resume` / `thread/resume`). No chat: a new session and `resumed: false` with a one-sentence `note`. Refused: with `resume_id`, without a folder (`bad_request`), with a worktree or a synthetic session (`option_unavailable`), a non-boolean (`bad_request`). The request is also available over HTTP POST since it uses the same validation.
- **CLI**: `lico chat latest [--cwd D] [--ssh-host H] [--account ID]`, `lico chat list --recorded`, `lico session create ... --resume-latest`. Default off.
- **MCP** `create` gets optional `resume` (boolean in the schema, the strings "true" and "false" also accepted), default off: an agent silently inheriting an older context would be surprising. The card says `continues the latest chat here if one exists`; the result tells the caller which chat was continued (title, turns), that the tab was already open (no new session; with a first prompt this is refused, because a prompt may only go to a session the caller made), or that there was none. `resume` without a folder (scope global, or a caller without one) is refused at the call.
- Docs: `doc/manual_end_to_end.md`. Protocol schemas in `lico_protocol/v1.py` (`turns`, `recorded`, `resume_latest`, the new reply fields).

## Evidence (as measured)

| Check | Result |
|---|---|
| Rust unit tests | `chats::turns` 3 (Claude and Codex counting, the line bound), `chats::latest` 1 (the pick rule) |
| `tests/test_chats_resume_latest.py` | 6 passed: turns (3 counted, 9 MiB transcript `null`, Codex context excluded); one rule across list, CLI and creation for Claude and Codex, blank-tab case, no-chat fallback; the three resume paths (new tab on a closed transcript, running tab reused with no second process, disconnected tab resumed with a higher generation); refusals; remote account on the fake-ssh rig; the MCP tool end to end with the card text and the three outcomes |
| `session.chats` latency, 2,500 Claude transcripts for one folder, median of 8 warm calls | `limit 20`: 23.4 ms (P01 tip) vs 24.2 ms (P02); `limit 100`: 23.7 ms vs 24.4 ms. Cold first call: 39.8 vs 27.1 ms and 24.6 vs 25.2 ms (disk-cache noise). About +0.8 ms, +3 %: no regression beyond noise. One machine, one run, private scratch fixture |
| Full changed suite on the staged tree | 470 tests in 89 files, 470 passed, 107.7 s, tree peak 1,750.1 MiB |

## Honest notes

- The red run of `test_chats_resume_latest.py` failed for the right reasons (`turns` missing, unknown field refused), then went green. A first full run failed once on `tests/test_size_ratchet.py`: `core.rs` had grown from 988 to 1,051 lines; the creation code moved to `new_session.rs` and the allowlist for `core.rs` was lowered to its new size, 941.
- Ssh `turns` stay `null` by decision (above); the resume itself works over ssh on the fake-ssh rig.
- The bound is a decision of mine, not measured against real huge transcripts: 8 MiB or 100,000 lines. Over it the answer is `null`, never a wrong number.

## Next

P03: the "+ New chat" row and modal in the Directories tab, using `session.chats {recorded: true, limit: 1}` for the `Last Session` line and `session.create {resume_latest: true}` to confirm.
