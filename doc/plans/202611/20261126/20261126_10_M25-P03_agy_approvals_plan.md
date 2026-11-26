# M25-P03 Plan — Approvals For An `agy` Session

- Status: done (2026-11-26)
- Version: 1.1.0
- Branch: `phase/M25-P03-agy-approvals` from `phase/M25-P02-agy-adapter-no-approvals` (`1132a71`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P03
- The owner's words: the standing yes of 2026-11-25 (advance approval of the prefix, the commits, the merge and the push through level C)

## What this phase makes

In P02 a permission request of `agy` was refused to the program at once. Now the person decides, the way they do for a Claude Code or a Codex session, and the program is still never started with the flag that skips its permissions.

1. **The port** (`driver.rs`): `Link::answer(approval_id, allow)` (default: `unknown_approval`, so `local` is unchanged). `core::approval_answer` gives a session of a driver kind to its link after the provider branches.
2. **The mode of the session decides who answers** (the amendment of decision D5, below): `session.create` and `session.mode` already carry `auto` or `default`; a driver session now keeps the one it was created with (it was always `auto`).
   - `default`: the request becomes an approval of the session (`ev: approval`, the prompt is the command or the title of the tool call, clipped to 200 characters, a number that is never reused), the desktop notice of a pending approval fires as for the other providers, and the program waits for the person.
   - `auto`: the host answers at once with the one-shot yes, and says so in the transcript as `[auto-allowed] <prompt>` before it goes on, so what was allowed is always readable afterwards.
   - In both, the answer is exactly the option of kind `allow_once` (yes) or `reject_once` (no) of that one request, and `cancelled` when the program offered no such option; never an `always` option. A request with no `allow_once` option, one the line reader had to cut, one over the 8 pending (`MAX_PENDING`) or a repeated id is denied or ignored as in P02.
3. **A turn that ends takes its unanswered requests with it**: when the prompt is answered (end, cancel or error) the held requests are forgotten, the card of the session is withdrawn, and an answer arriving later is refused (`unknown_approval`) and sends nothing. The same when the program ends (the exit path of P02 already dropped the cards; the number table is now cleared too).
4. `new_session::start_driver_session` passes the person's mode to the session (it set `auto` for every driver kind).

## The amendment of D5

D5 said: "`--dangerously-skip-permissions` is never passed; every permission request is shown to the person (the mode of the session: default asks)". The host's own meaning of the mode is the owner's: `auto` is "permissions skipped, the default of a new session", `default` is "the provider asks". Taking D5 literally would make the mode switch of the window do nothing for `agy`, and would ask for every call of a session the person put in `auto`. The reading now is: the program is never given the flag that skips its permissions and never an answer wider than one request, a request is shown to the person in `default`, and in `auto` the host (not the program) says yes once per request and writes it down. The window (P05) starts an `agy` session in `default` until the owner has seen a real turn (gate G3); that is a default of the window and the owner's to change.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| B1 | in `default` a permission request is an approval of the session with its prompt, the viewers are told, nothing is sent to the program before the person answers, and an answer reaches it as the one `allow_once` or `reject_once` option of that request id | host test with the simulator |
| B2 | a forged number, a stale generation and a second answer to the same approval are refused and send nothing; the number of a later approval is greater | the same |
| B3 | in `auto` nobody is asked: the host's answer is the one-shot yes, the line `[auto-allowed] <prompt>` comes before the program's next words, and the switch to `default` (the program is not restarted) makes the next request ask; the switch is refused while one waits | the same |
| B4 | a repeated request id is one approval, a request with a boolean id and a request for a file are never approvals (the file read is answered with an error) | the same (hostile mode) |
| B5 | a request the turn left unanswered goes with the turn: the card is withdrawn, the late answer is refused and nothing is sent to the program; a program that ends takes its approvals with it | unit test of `agy::conn`, and the host test |
| B6 | the answer of a driver session that asks nothing (`local`) is still refused as an unknown approval | unit test of `driver` |
| B7 | no middle dot, no local path, no secret in any new file | the guards of the commit |

## Steps

1. [x] `Link::answer`, the core branch, the mode of a driver session
2. [x] The adapter: the number table, the two modes, the end of a turn, the notice
3. [x] The unit tests and the host tests (the P02 refusal test is replaced by three: default, auto, hostile and the end of a turn)
4. [x] Mutation checks of the new logic
5. [x] The README, the plan's results, the milestone row, the carried note of the P02 commit, the commit (the hook)

## Personal data and security

- The prompt shown to the person is the program's own text, cleaned by the same function as every other line (`sanitize`, 200 characters); a command that does not fit is clipped with an ellipsis, so the person is never asked to approve text that is cut silently: a request the reader had to cut is denied (P02).
- An approval number is never reused within a session and is not the program's request id: the program's id is kept only in the adapter, and a number answers one request once.
- `keep` ("remember this answer") of the window's approval card is ignored for `agy`: no answer is wider than one request.
- `[auto-allowed]` makes the host's own decision visible; the transcript is the record of what was allowed.

## Not decided here

How a person stops a turn that waits for them (`session/cancel`: the window has no stop button for a turn yet; closing the session ends the process, and with it the request); `Send to` and the rooms of an `agy` session (P06); the window's start in `default` (P05).

## Carried from P02 (the commit)

The commit of P02 was refused by the hook once before it passed, for two reasons: the size ratchet (`folders.rs` 816 lines over the limit of 800; it is now 792, because the checks of a driver kind moved from `folders::prepare` to `new_session::prepare_driver_kind`, so `folders.rs` has less than before and no allow-list entry was asked for), and a build peak of 2,083.1 MiB over the bound of 1,536 MiB with a cold run of 184.2 s over 180 s. The run by hand afterwards: 650 of 650 tests in 191.5 s at a peak of 1,087.7 MiB (the build was warm), the hook then accepted it. Neither bound was raised.

## Results (2026-11-26)

What was made: `Link::answer` (default `unknown_approval`), the `driver` branch of `core::approval_answer`, the mode of a driver session (kept from `session.create`, it was always `auto`), and in `agy/mod.rs` the table of the approvals put to the person (`Asked`: the session's number to the program's request), the two modes in `Actor::apply`, the notice of a pending approval, `Msg::Answer`, and the forgetting of the requests a turn leaves (`Conn::end_turn`, `Mapper::forget_pending`, the session's cards and the table). `mod.rs` is 353 lines, the whole of `agy/` 1,812 (the tests included).

| # | Result |
|---|---|
| B1 | in `default` the fixture's request is an approval with the prompt `run: ls -la`, the viewers got the event, the simulator received nothing until the answer and then exactly one result on id 100, `selected` with the option `allow` (the `allow_once` one); a no on the next turn is `reject`, and the program says the command was not run |
| B2 | a number that was never asked, a stale generation and a second answer are refused (`unknown_approval`, `stale_generation`, `unknown_approval`) and the simulator got nothing; the second approval's number is greater |
| B3 | in `auto` the line `[auto-allowed] run: ls -la` stands before the program's next words and the simulator got the same `allow`; `session.mode` to `default` keeps the generation (no restart), the next request asks, and the switch back is refused with `busy` while it waits |
| B4 | the hostile turn (20 requests on one id, one with a boolean id, a request for a file, 300 tool calls) made exactly one approval, and only the file read was answered, with the error `-32601` |
| B5 | the turn of the hostile case ended with that approval unanswered: the card is gone, its late answer is `unknown_approval`, nothing was sent; a program killed while an approval waited leaves a disconnected session with no card and a refused answer; the unit test shows the conversation forgets the request and frees its id |
| B6 | `Idle` (the test link of `driver`) answers `unknown_approval`; `local` has no other link |
| B7 | checked by the guards of the commit |

Tests: 2 unit tests of `agy::tests` (the end of a turn; `Handle::answer` once, for a known number, `busy`, dead), one assertion added to a unit test of `driver`, 3 host tests that replace the refusal test of P02 (`tests/test_agy_session.py`, now 8 tests): 5 new tests, against the budget of about 100 (36 for M25 so far).
Host run: the two agy files, 16 tests in 12 s.

Found while writing the host test (not a defect): the fixture repeats the tool call id `c1` on the second turn, and the mapping follows a tool call id once per session (a repeated id is not drawn again), so the second turn shows no second `$ ls -la` and no second `[tool error]`; the test says so and checks the program's own words instead.

Mutation checks: 14 single-edit mutants. Of `conn.rs` (the forgetting of a turn's requests), `mod.rs` and `core.rs` (the mode test, the line of an auto-allowed request, its yes, the table insert, the withdrawal of the cards, the answer's direction, the number counter, the mode of the session, the four branches of `Handle::answer`): 13 killed. One survivor, the clearing of the table at the end of a turn: the number it would leave is never reached, because `core::approval_answer` looks the number up in the session's own list first and that list is cleared in the same lock; it stays so that the table cannot grow over a long session.

Not done, as planned: no stop of a turn that waits (`session/cancel`), no restart from the registry in the window (P04), no window (P05), no Send to (P06). The real program's permission requests are not verified: the options it offers (`allow_once`, `reject_once` and the ids) are the documentation's, on the record's unverified list.
