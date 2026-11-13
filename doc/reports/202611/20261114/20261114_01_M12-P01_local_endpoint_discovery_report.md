# M12-P01 Report — Discovery Engine for This Machine

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M12-P01-local-endpoint-discovery` (from `main` at `ece5706`; implementation commit `2dfaab5`)
- Plan: [M12-P01](../../plans/202611/20261114/20261114_01_M12-P01_local_endpoint_discovery_plan.md)

## What changed

- **Engine** (`llm/`): `net.rs` reads `/proc/net/tcp` and `tcp6` (LISTEN, IPv4 and the four IPv6 words, loopback, wildcard, mapped and interface binds) and maps socket inodes to the user's own processes through `/proc/<pid>/fd` (a process whose fd folder cannot be read is skipped, which is how the default scope stays the user's own; only the process name `comm` is read); `http.rs` is the minimal client (connect 300 ms, listen 150 ms without sending, then `GET <path> HTTP/1.0` with `Host`, `Accept`, `User-Agent`, `Connection: close`; no body, no credential, no cookie, no redirect; at most 64 KiB of body); `classify.rs` turns an answer into `openai`, `ollama`, `http-other`, `not-http` or `unknown`, with model ids (at most 50, cleaned, a tolerant scan for a list cut at the bound) and the auth state (`key_needed` only for a 401 or 403 with an OpenAI-style error body; no key is ever tried); `mod.rs` runs it (64 sockets at most, 8 at a time, 1.5 s a request, 8 s in all, the machine's own address only, servers first in the answer). `--llm-proc-root DIR` (tests only) replaces `/proc`.
- **Wire**: op `llm.discover` (`ssh_host?` is validated and answers `option_unavailable` until P02, `all_users?`), `POST /v1/llm/discover`, capabilities, protocol schemas, `lico llm discover [--ssh-host HOST] [--all-users]`.
- **Tool** `llm_endpoints` (no argument for now): the call leaves a card, the look is made on the person's yes, the agent is told in a message whose model names are cut to `[A-Za-z0-9._:/@+-]` and 60 characters (a server cannot start a line of its own with a model name); a no looks at nothing. The tool list stays within its byte budget (`test_agent_tools` asserts `<= 5120`; it passed).
- **Window**: `Session > Find Local LLM servers...` opens a centred dialog (`gui/llm_find.rs`): `Enter` or a click scans (never by itself), `A` toggles all users' ports, Up/Down scroll, `Esc` closes; the rows are `address:port kind bind auth models (process)`. `AsyncJob` and `AsyncDone` moved out of `gui/panel.rs` into `gui/jobs.rs` first, as a pure move (`1ae6844`, suite green), because `panel.rs` was at its size limit.
- Docs: `doc/manual_end_to_end.md`.

## Red first (run before the engine existed)

`tests/test_llm_discover.py` was run against the unchanged host (`private/runs/m12_p01_red1.log`): `6 failed`, with `unknown_op` for `llm.discover` and the CLI printing no JSON; then the engine was written and the six went green. The Rust unit tests, the tool test and the window test were written together with their code and were not run red on their own (a Rust test lives in the file of its code; the tool and window tests came after their code): the same shortfall as in M11, smaller, and I say so.

## Evidence (as measured)

| Check | Result |
|---|---|
| Rust unit tests | `llm::net` 4 (IPv4 and IPv6 tables, probe address, owners with an unreadable fd folder), `llm::classify` 6, `llm::http` 4 (request text, silence to a server that speaks first, refused, closed, never answering, bound and 64 KiB cut), `llm::tool` 1, `gui::llm_find` 2; all `gui::` tests `205 passed` |
| `tests/test_llm_discover.py` (fake servers on ephemeral loopback ports, a process table the test wrote) | 6 passed, repeated 3 times, `4.2` to `6.0 s`: every class and auth state; the two paths only, HTTP/1.0, no body, no `Authorization`, no cookie, recorded by each server; a redirect target never requested; a speak-first banner server receives nothing; the oversize list is cut (`models_truncated`, 50 ids); a foreign socket is not looked at by default and is with `all_users`; 70 listeners: 64 probed, 7 skipped, inside 8.5 s; a silent server is `unknown` in under 4 s; socket, CLI and HTTP give the same normalised answer; an empty process table says `supported: false, proc_unreadable` |
| `tests/test_llm_tool.py` | 2 passed, repeated 3 times: card first, nothing looked at before the yes, safe words in the message, a no looks at nothing, a second call is the same card |
| `tests/test_llm_window.py` (private Xvfb) | 1 passed, repeated 5 times: the menu entry, the dialog idle with no connection for a second, `Enter` scans, servers listed, a foreign socket untouched, `A` adds all users, `Esc` closes |
| Cold full suite | `526` tests in `108` files, `526 passed`, `114.4 s`, tree peak `1,903.2 MiB` |

## Honest notes

- An earlier changed-suite run on the staged tree had three failures that I caused and fixed (the new Rust module was not in `tests/testmap.toml`, and two lists of tool names in `test_control_parity` and the selection test), and its tree peak was `2,051.9 MiB`, `3.9 MiB` over the `2,048 MiB` bound; the cold run after the fixes peaked at `1,903.2 MiB`. M11's cold peaks were `1,645` to `1,776 MiB`. The spread between runs is large and the margin is thin; P03 measures three cold runs and reports each.
- A first version of the fixtures made the bounds test take `21.7 s` (closing 70 fake servers waited for the poll interval of each); a short poll interval fixed it.
- A real server that answers both `/v1/models` and `/api/tags` is reported as `ollama` with both `apis`; a plain OpenAI server therefore receives both questions (the second gets a 404). That is a choice of mine for classification.
- Not exercised: real servers (by rule). What real Ollama, llama.cpp, vLLM and LM Studio answer is known to me from their documentation, not from this machine; the owner's check at G3 against a real server is the real test.

## Next

P02: the same discovery on an ssh host (a bounded remote script, the existing fake-ssh fixtures plus failure hosts).
