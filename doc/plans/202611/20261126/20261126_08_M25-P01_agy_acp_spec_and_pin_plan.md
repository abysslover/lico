# M25-P01 Plan — The Spec And The Pin Of `agy acp` (Python Only)

- Status: done (2026-11-26)
- Version: 1.0.0
- Branch: `phase/M25-P01-agy-acp-spec-and-pin` from `main` (`2f41f28`, `lico 0.8.2`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P01
- The owner's words: the standing yes of 2026-11-25 (advance approval of the prefix, the commits, the merge and the push through level C)
- This phase changes **no host code**: it is the data the Rust adapter of P02 is built and tested against.

## What this phase makes

1. **`src/host/adapters/agy/capabilities.json`**: what `agy acp` offers, pinned to `1.5.17`, as read with a private empty home and a minimal environment (no sign-in, no turn): the `initialize` answer, the shape of the `session/new` answer, the error codes seen, the flags that matter; how every ACP message is handled (mapped, refused, ignored), what is unsupported and why, and what is **unverified** (everything after `session/new`).
2. **`src/lico_protocol/agy_mapping.py`**: the reference mapping from ACP messages to host records and approvals (the same record vocabulary as the Claude and Codex references: `[session started]`, `[agent] ` lines, `$ command`, `[tool]`, `> result`, `[turn completed]`, `[turn failed]`, `[error]`), including the streaming rule (chunks are fragments: a line is emitted when it is complete), the replay rule (what `session/load` replays is not copied), the choice of the option for an answer (never wider than the person's answer) and the sign-in-needed words.
3. **`tests/fixtures/agy_acp/`**: three recorded-by-hand sequences with their expected records, approvals and answers: a turn with a permission request, a turn that fails signed out, a resume with replay. They say that their shapes are from the ACP documentation.
4. **`src/host/adapters/agy/simulator.py`**: an ACP server for tests (a test double of `agy acp`), with modes chosen by the text of the prompt (`fixture`, `say:J`, `authfail`, `crash`, `stall`, `hostile`, otherwise echo), logging every line it receives.
5. **`tests/test_agy_adapter.py`** (listed in `tests/testmap.toml`, with a `[[sources]]` rule for the adapter folder and the fixtures): the contract test against the installed `agy` (skipped only when `agy` is not installed), the fixtures against the reference, the simulator against the reference, and the hostile-input bounds of the reference.
6. The README of the adapters gets a third section.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| A1 | the record says `1.5.17`, is pinned, lists unverified items, and equals what the installed `agy acp` answers (`initialize` keys and values, the keys of the `session/new` answer, the modes) | `tests/test_agy_adapter.py` against the installed program |
| A2 | each fixture maps to exactly its expected records, approvals and answers | the same file |
| A3 | the simulator, driven by a client written from the reference, reproduces the fixtures (the same lines, the permission outcome received as the answer says) | the same file |
| A4 | hostile input (an oversize line, control characters, a forged `[agent]` lead, a repeated request id, an unknown method) never produces a record that looks like the host's own and never an approval that is not a permission request | the same file |
| A5 | no middle dot, no local path, no secret in any new file | the guards of the commit |

## Steps

1. [x] The capability record from the probe (script in the scratchpad, values copied, not the tier)
2. [x] The reference mapping and the three fixtures
3. [x] The simulator
4. [x] The tests and the testmap
5. [x] The README, the plan's results, the milestone row
6. [x] The commit (the hook); the push of the branch is part of the release, not of this phase

## Personal data and security

- The probe runs with an empty home in the scratchpad; nothing of `~/.gemini` is read; `_meta.antigravity.tier` is not recorded.
- The reference treats every provider line as untrusted display text (`sanitize`, `agent_record`): the same rule as the other two references.

## Not decided here

The names of the option kinds that `agy` really sends (the ACP documentation says `allow_once`, `allow_always`, `reject_once`, `reject_always`); the real shape of a tool call's `rawInput` for a command; whether `session/resume` or `session/load` is used (P04).

## Results (2026-11-26)

- **Found with the installed `agy 1.5.17`** (a private empty home, a minimal environment, no sign-in, no turn): `session/new` **works signed out** and answers a `sessionId`, `models {currentModelId, availableModels[]}`, `modes {currentModeId "default", ids default / accept-edits / plan}`, `configOptions`, then two notifications (`available_commands_update`, `usage_update`); `session/list` answers `{"sessions": []}`; the error codes are `-32002` (load of an unknown session), `-32603` (resume of an unknown session), `-32602` (prompt to an unknown session), `-32601` (unknown method); `agy --version` prints the bare version; `agy acp --help` lists `--disable-telemetry`, `--add-dir`, `--sandbox`, `--process-per-session`, `--login`, `--dangerously-skip-permissions`. The default mode asks before edits and commands, which is what D5 wants. The account tier that the answer's `_meta` names is not recorded.
- **The tests** (`tests/test_agy_adapter.py`, 8 cases: six test functions, one of them run for each of the three fixtures): the contract test reads the record against the installed program (skipped only when `agy` is not installed), the three fixtures map to their records, approvals and answers, the hostile input is bounded one case at a time, every stop reason of the record ends the turn the way it says, the simulator driven by a client of the reference says what the fixture says (allowed and denied), and the simulator keeps a conversation across processes, replays for a load, not for a resume, and refuses what it does not know. 8 cases in 12 s; the budget of the owner (about 100 per turn) is far away.
- **The mutants**: 24 edits of the reference and the simulator (the answer widened to an always option, the check of an allow option, a repeated request id, a boolean id, a record not cleaned or not bounded, a forged lead not tagged, a replay copied, a response answered twice, the stop reason of a cancel, the option kinds and their count, the bound of tool calls, of pending approvals and of a line that never ends, the `[tool error]` of a failed call, the command of an approval, the sign-in code, the refusal of `fs/*`, the clip of a prompt, the codes and the replay of the simulator): six survived the first round, the tests were strengthened (a hostile command, a second answer to one request, the options one by one, the prompt clip, the stop reasons read from the record, the cut of a line that never ends), and **all 24 are killed**.
- **Found on the way**: the `session/load` of the simulator and of `agy` answer an error for an id that was never made; `session/list` is empty until a turn has been made in `agy`'s own store (the Chats of M23-P04b read the files, not this list); the permission request, its option kinds and the stop reasons stay unverified until the owner's real turn.
- **Nothing of the host changed**: no Rust, no build; the selection of tests for this change is the new file and three guards.
