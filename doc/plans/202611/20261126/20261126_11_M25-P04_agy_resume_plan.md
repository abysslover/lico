# M25-P04 Plan — An `agy` Conversation Comes Back As A Session

- Status: done (2026-11-26)
- Version: 1.1.0
- Branch: `phase/M25-P04-agy-resume` from `phase/M25-P03-agy-approvals` (`88f5686`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P04
- The owner's words: the standing yes of 2026-11-25 (advance approval of the prefix, the commits, the merge and the push through level C)

## What this phase makes

P02 and P03 start a new conversation. A person also has conversations that `agy` itself already keeps (the Chats cards read them from its files) and sessions of the host that were running when the host stopped. This phase lets both come back as a session, and never as a different conversation than the one asked for.

1. **`session.create kind=agy` takes `resume_id`**: the conversation's id (the same text the host stores for a Claude Code or a Codex session, `valid_provider_session`: letters, digits, `-`, `_`, at most 64 characters). A bad id is `bad_request`; the kinds with no conversation of their own (`local`) still refuse it, and `cwd` keeps meaning "the folder the program works in" for a kind that takes a folder.
2. **The session remembers the id**: `s.codex_thread` (the stored provider conversation, persisted in the registry as `provider_session`) is set from `resume_id` at creation, so a first start already goes by `session/resume` (then `session/load` when the program cannot resume it, P02) and never `session/new` in its place.
3. **One session holds a conversation at a time** (as for the other providers): a second `session.create` with an id that a session of the host already holds answers with that session (`resumed: true`, `already_open: true`); when the holder is disconnected it is resumed in place (a new generation), when it is running nothing is started. The check is by kind and id (`new_session::holder_of`), and the live-chat check against other processes (`chat_running_elsewhere`) is not made for a driver kind: whether a terminal tab of the window holds the same conversation is the window's to see (`agy_plan`, P05).
4. **A host that was killed** leaves the session `disconnected` with the stored id (the registry already keeps `provider_session`); `session.resume` then asks the program for that conversation (`session/resume`), never for a new one, writes `[provider reconnected]`, and does not copy what a resume says again (a resume replays nothing; a load replays and the mapping does not copy it, P01).
5. **What is not changed**: `folders.rs` (792 lines) is as P03 left it; the checks of a driver kind are in `new_session::prepare_driver_kind`.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| C1 | `session.create kind=agy resume_id=<id>` starts the program, asks `session/resume` with that id and never `session/new`; the session's first prompt carries that id | host test (`tests/test_agy_resume.py`) |
| C2 | a second create of the same id answers with the holder (`already_open`, `resumed`), starts nothing; a disconnected holder is resumed in place under the next generation and no second session appears | same |
| C3 | a bad id (`../x`, a space, 100 characters, empty) is refused and starts nothing; `local` with a resume id is refused; an id the program does not know ends in `[error] agy could not bring this conversation back; start a new session`, the session is disconnected, and the program was asked `session/resume` then `session/load` and never `session/new` | same |
| C4 | a host killed with SIGKILL leaves the session disconnected with its history and its mode; `session.resume` asks for the stored id, the same conversation answers the next prompt, nothing the history already has is written twice | same |
| C5 | no middle dot, no local path, no secret in any new file | the guards of the commit |

## Steps

1. [x] `new_session.rs`: `prepare_driver_kind` (resume id for a folder-bearing kind), `holder_of`, the reopen, the stored id, the mode
2. [x] The host tests (`tests/test_agy_resume.py`, listed in `tests/testmap.toml`)
3. [x] Mutation checks of the new logic
4. [x] The README, the plan's results, the milestone row, `PLAN.md`, the index row, the commit (the hook)

## Personal data and security

- The id is the program's conversation id, validated with the same function as every other provider id before it reaches the registry, a command line or a log; it is never a path. It is stored in the host's registry file only (mode 0600 as the rest of it).
- The folder the person names is checked as for a provider (`check_folder`: it exists, it is a directory, it is under the allowed roots).
- Nothing is ever asked of the program in place of the stored conversation: an id the program does not know ends in plain words, not in a new empty conversation that would look like the old one.

## Not decided here

How the window shows a conversation that a terminal tab of the window already holds (P05); the stop of a turn (`session/cancel`); the model of a resumed session (the program says it on `session/new` only; after a resume the header has none until the program says one).

## Results (2026-11-26)

What was made: in `new_session.rs` the checks of a driver kind take `resume_id` for a kind that takes a folder (`agy`) and still refuse it for one that does not (`local`), a bad id is `bad_request` (the request's own check already refuses most of them; the provider id check adds the leading `-`), `holder_of` finds the session of a kind that holds a conversation and is running or disconnected, `session_create` answers with it (`resumed`, `already_open`) through the same `reopen` as a latest chat, the session keeps the id in `codex_thread` (the registry's `provider_session`) and the mode the person chose is what the form remembers. No other source file changed: the adapter of P02 already resumed a stored id, and the registry already kept it.

| # | Result |
|---|---|
| C1 | the simulator was asked `session/resume` with the given id and nothing else; the first prompt carried that id and the folder was the one named |
| C2 | the second create of the same id answered with the same session (`resumed`, `already_open`) and started nothing; another id made another session; after the program crashed, the create of the held id resumed that session in place under the next generation, no session was added, and the program was asked `session/resume` for that id |
| C3 | `../x`, `a b`, 100 characters, the empty text and `-` before a good id were refused and started nothing; a complete `local` request with a resume id is `option_unavailable`; an id the program does not know ended in `[error] agy could not bring this conversation back; start a new session`, the session is disconnected, the program was asked `session/resume` then `session/load` and never `session/new`, and no prompt was sent |
| C4 | a host killed with SIGKILL came back with the session `disconnected`, the same generation, the mode `default` and its history; `session.resume` asked the program for the stored id (`session/resume`, never `session/new`), wrote `[provider reconnected]`, the next prompt went to that id, and `echo: remember` stands once |
| C5 | checked by the guards of the commit |

Tests: 3 host tests (`tests/test_agy_resume.py`), one line of the P02 folder test changed (a good id is no longer refused: it now refuses `not a valid id`, and says where a good one is tried): 3 new tests, against the budget of about 100 (39 for M25 so far). No unit test: the logic needs the host's registry and a program, and the host tests have both. Host run of the three agy files: 19 tests (the third is `test_agy_adapter.py`), all green.

Mutation checks: 12 single-edit mutants of `new_session.rs` (the resume id refused for a folder kind, taken by `local`, a bad id accepted, the holder found by any kind or by any stored id, a disconnected or a running holder not found, each of the two flags of the reply missing, the id not stored, the remembered mode fixed, the dedupe skipped). 11 killed. One survivor, the holder found without looking at the kind: only a session of another provider that holds the very same conversation id would tell it apart, and that needs an id collision between two programs (the ids are random UUIDs); the check stays in the code because the two kinds' ids are different things.

How the first round went (a lesson, not a defect of the code): the first mutation run reported its mutants killed by `test_agy_session.py` because a test of P02 still said that a resume id is refused, which made that file red without any mutant. A mutation run starts only after the same tests are green on the unmutated source; the run was stopped, the test was changed, and the mutants were run again. That second round left five survivors, four of which the tests did not reach (the second conversation, the leading `-`, a complete `local` request, the form's mode); they were answered with stronger tests and killed.

Not done, as planned: the window (P05: the card of the Chats tab still opens a terminal and the tile still has no agy session), Send to (P06), no stop of a turn, the real program's `session/resume` is not verified (the owner's check, gate G3).
