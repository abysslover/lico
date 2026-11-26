# M25 — `agy` Is A Lico Session (Level C), Release 0.9.0

- Status: in_progress (2026-11-26); P01 done (the spec and the pin), P02 next
- Version: 1.0.0
- Project: Lico
- Updated: 2026-11-26
- Follows: [M24](../20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md) (release `0.8.2`, PR #13, `main` `2f41f28`); level C of [M23-P04](../../../plans/202611/20261119/20261119_10_M23-P04_agent_in_terminal_plan.md)
- Renumbering: the module-split milestone that the owner's test-budget rule of 2026-11-26 calls "the next milestone after level C" is **M26** (this one took the number M25)

## The owner's words (the source)

- (2026-11-19, chosen levels) A and B for `agy` now, C later; level C is "agy is a Lico session: fourth choice of the wizard, its conversation is a session tab with Lico's transcript, approvals, Send to and the rooms; the terminal is not involved".
- (2026-11-24, the order) "P08, P09, UI 단순화 -> 레벨 C로 가면 됨"; and about the version: "그 다음에 레벨 C를 수행하면 그게 minor 올라갈 시점이긴 함." So level C is `0.9.0`.
- (2026-11-25, the standing yes) "지금까지 실행해보면 내가 굳이 merge, push 승인 등을 안해도 퀄리티가 좋았어. 그래서 다음 세션 부터는 별도의 문제가 없다면 agy까지 쭈욱 지정된 버전 번호로 진행해서 그 결과 나올 때 확인해줘. [...] 내가 기대하는 것은 위 기능이 모두 완성되고 agy가 native하게 인식되는 시점의 merge, push 상태로 버전 번호 minor 올라간 것이 될 때 내가 대화를 치는 것이 기대치임."
- (2026-11-26) at most about 100 unit tests per development turn; after `0.8.2` and level C a milestone splits the modules so finished modules are not retested (M26).

## What was found (2026-11-26)

1. **`agy acp` is a standard Agent Client Protocol server** (JSON lines over standard input and output, JSON-RPC 2.0). Read with a private empty home and a minimal environment, no sign-in, no turn: `initialize` answers `protocolVersion 1`, `agentCapabilities.loadSession true`, `promptCapabilities {image true, audio false, embeddedContext true}`, `sessionCapabilities {resume, close, load, list, additionalDirectories}`, `agentInfo {"name":"Antigravity ACP Server","version":"1.5.17"}`, four `authMethods` (`oauth-personal`, `oauth-business`, `gemini-api-key`, `agent-platform`).
2. **`session/new` works without a sign-in**: it answers a `sessionId` (a UUID), `models.availableModels[]`, then the notifications `available_commands_update` and `usage_update {used, size}`. The sign-in is therefore met at `session/prompt`, which this milestone does not run without the owner (see "Not checked here").
3. **Error shapes** (observed): an unknown session on `session/load` is `-32002 Session not found: ID`; on `session/resume` it is `-32603 Failed to read session ID; retry, or load it with session/load`; an unknown method is `-32601`; a prompt to an unknown session is `-32602 Session not found`. `session/list` answers `{"sessions": []}`.
4. **`agy acp` flags that matter**: `--disable-telemetry`, `--add-dir` (repeatable), `--sandbox`, `--process-per-session`, `--dangerously-skip-permissions` (never used), `--app-data-dir`/`--gemini-dir`/`--log-file` (paths), `--login` (the interactive Google sign-in, then exit).
5. **The host already has a port for a session that is not a provider program**: `driver.rs` (`Driver` and `Link`, M16-P06), made for `local`, which every hub already traverses (`session.create`, `session.resume`, `input`, `session.purge`, the registry, a restart of the host, `lifecycle`). An `agy` kind registered there touches no hub: the provider names `claude` and `codex` stay as they are. What the port lacks for a provider that asks the person is **one method**: `Link::answer(number, allow)`, because `core::approval_answer` routes an answer to Claude or Codex by name and to `crate::pending` for the local model's tools.
6. **What an adapter of this host owes** (the pattern of `claude.rs` and `codex.rs`): a capability record pinned to an exact version with a test that fails when the installed provider differs, a reference mapping in Python (`src/lico_protocol/`) and fixtures that the Rust adapter must reproduce, a simulator that stands in for the provider in tests, the minimal environment, bounded lines and queues, text cleaned of escapes, and a list of what is **unverified**.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The transport is **`agy acp`** (ACP), not `agy -p --output-format stream-json` | ACP is bidirectional (permission requests, cancel), has resume and a published shape; the print mode is one shot per process |
| D2 | `agy` is a **driver kind** (`driver.rs`), registered from the root like `local`; the port gets `Link::answer` | no hub names `agy`; the nine modules that route by provider name stay untouched |
| D3 | **Lico does not sign in for the person** and never reads `~/.gemini` credentials: the person signs in with `agy` itself (the Terminal tile); a signed-out session says so in plain words | the rule of M23: a provider's login state belongs to the provider |
| D4 | Lico offers the agent **no `fs/*` and no `terminal/*`** client capabilities | `agy` does its own file and shell work, as in a terminal; only its permission requests reach the person |
| D5 | **`--dangerously-skip-permissions` is never passed**; every permission request is shown to the person (the mode of the session: default asks) | the owner's rule for every provider |
| D6 | **`--disable-telemetry` is passed** | a Lico session does not send what the person did not choose to send; the owner's call |
| D7 | The `agy` tile starts a **session at once** (one touch, like Claude); the terminal path stays: the Terminal tile and typing `agy` | M24's one-touch rule; levels A and B stay |
| D8 | A Chats card of an `agy` conversation **resumes as a session** (`session/resume`, or `session/load` with the replay not copied) | the Chats of M23-P04b already read the conversation |
| D9 | **Pinned to `agy 1.5.17`**; a test fails when the installed `agy acp` answers differently | the pattern of the other two adapters |
| D10 | **No model picker, no effort, no mode switch in `0.9.0`**: `agy`'s own default; the model it names is shown in the header | scope; the owner's UI philosophy (almost no text) |
| D11 | **No real turn without the owner**: shapes after `session/new` come from the ACP documentation and are listed as unverified | the rule of the first two adapters; the owner's hands-on check (G3) is the real turn |

## Proposed phases (each: plan, few tests, mutation checks, docs, the hook, `~/bin/lico` rebuilt; the prefix is the owner's standing yes)

| Phase | What | The person can inspect |
|---|---|---|
| M25-P01 | **(done 2026-11-26, [plan](../../../plans/202611/20261126/20261126_08_M25-P01_agy_acp_spec_and_pin_plan.md))** **The spec and the pin (Python only, no host code)**: `src/host/adapters/agy/capabilities.json` (what `agy acp` offers, pinned to `1.5.17`, what is mapped, refused, unsupported, **unverified**), `src/lico_protocol/agy_mapping.py` (the reference: ACP messages to host records and approvals), `tests/fixtures/agy_acp/` (a turn with a permission request, a turn with a refusal, a resume with replay), `src/host/adapters/agy/simulator.py` (an ACP server for tests), the contract test against the installed `agy`, the README | the capability record and the fixtures, readable |
| M25-P02 | **The adapter, no approvals yet**: the driver kind `agy` (`src/host/hostd/src/agy/`): spawn, `initialize`, `session/new`, `session/prompt`, the chunks as transcript lines, the end of a turn, stop and purge, every failure in plain words; a permission request is refused until P03 | `lico` host with the simulator: a session, a turn, the lines |
| M25-P03 | **Approvals**: `Link::answer`; `session/request_permission` becomes an approval (the prompt text from the tool call), allow and deny map to the offered option kinds, a stopped turn answers `cancelled`; the same bounds as the other adapters (pending count, size, a repeated request id) | an approval in the window and the answer reaching the simulator |
| M25-P04 | **Resume**: the conversation id kept in the registry (`provider_session`), `session/resume` or `session/load` with replay not copied, a restart of the host, a Chats card of an `agy` conversation opens a session | close the window, reopen: the conversation continues |
| M25-P05 | **The window**: the `agy` tile starts a session (no account step), the tab and header show `agy` and the model it names, the sign-in-needed line, the picture rules of M24 | the tile, the tab |
| M25-P06 | **Send to, mentions, rooms, handoff**: an `agy` session is a peer like the others (`peer_turn`, `Send to`, `@agy`, the Call Room), proven with the simulator | a room of Claude, Codex and `agy` |
| M25-P07 | **The security note and the guide**: the environment of the child, the bounds, what `agy` can do as the person (it runs as the person: the note says so), the guide `doc/manual_end_to_end.md`, the CHANGELOG draft | the note |
| M25-P08 | **Release `0.9.0`**: version pins first, the whole cold suite, `cargo xtask check|graph|verify|system`, the clean-clone acceptance, the report, the scans, one atomic push, the pull request, `main` by fast-forward | the release |

## Not checked here (the owner's hands-on check, with G3)

A real turn: the sign-in, a model's answer, a real permission request, a real resume of a real conversation. Every shape after `session/new` is from the ACP documentation, in the capability record's **unverified** list, and the adapter treats anything it does not know as text to show or to ignore, never as a command.

## Out of scope

A model picker, effort, mode switch; the `fs/*` and `terminal/*` client capabilities; images in a prompt (`promptCapabilities.image` is offered by `agy`, a later phase); `session/list` as a source for Chats (Chats read the files, M23-P04b); the sign-in flow inside Lico (`--login`); the module split (M26).

## When done

(to be written at the release)
