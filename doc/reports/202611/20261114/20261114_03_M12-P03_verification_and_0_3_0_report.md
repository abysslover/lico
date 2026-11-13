# M12-P03 Report — End-to-End Verification, Measurements, and `lico 0.3.0`

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M12-P03-verification-and-0-3-0` (stacked on the P02 tip; code commit `e57f790`)
- Plan: [M12-P03](../../plans/202611/20261114/20261114_03_M12-P03_verification_and_0_3_0_plan.md)

## What changed

- `lico` is `0.3.0` (`Cargo.toml`, `Cargo.lock`, `tests/test_single_executable.py`, `tests/test_control_parity.py`; the two version tests were run red against `0.2.7` first, in the same run in which the end-to-end test failed on a mistake of mine in its fixture setup, which I fixed).
- `tests/test_llm_end_to_end.py` (private Xvfb, fake servers, a fake ssh, a process table the test wrote; no real service or host): the Session menu opens the dialog and nothing is looked at for a second; Enter lists this machine's servers (`openai` with its models, loopback and the process name; `ollama`; a server that wants a key; a speak-first banner and an HTML page as the other ports) while a socket of another user is not touched; the command gives the same endpoints; `A` makes all users' ports an explicit step; `Right` chooses `gpu-box` and nothing is called until Enter; its server says `loopback`; the command with `--ssh-host` says `remote_script`; `offline-box` fails with a sentence; Esc closes. Over the whole run every fake server was sent only `GET /v1/models` and `GET /api/tags` as HTTP/1.0, with no body, credential or cookie, the speak-first server was sent nothing, and the ssh calls are exactly `gpu-box` and `offline-box`. Repeated 4 times, 4 green.
- Inspection bundle (local only): `private/manual_inspection/202611/20261114/00001_M12-P03_local_llm_discovery/`: `1_session_menu.png`, `2_dialog_before_a_scan.png`, `3_this_machine_found.png`, `4_ssh_host_loopback_server.png`, `5_unreachable_host_sentence.png`.
- Docs: CHANGELOG `0.3.0`, PLAN, roadmap, README, manual, `doc/index_4.md`, milestone and plan statuses.

## Measured verification (`lico 0.3.0`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `656 passed; 0 failed` (`1` ignored, both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`), three runs | `533` tests in `110` files, `533 passed` each: `120.0 s` / peak `1,980.5 MiB`; `116.2 s` / `1,964.8 MiB`; `116.4 s` / `1,998.7 MiB` | `<= 180.0 s`, `<= 2,048.0 MiB` | Met, the memory margin is thin (see below) |
| Idle window RSS, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on, 3 launches) | `max = 25.11 MiB` (`24.76..25.11`), median PSS `16.93 MiB`, host `7.81 MiB` | `< 50 MiB` | Met |
| Idle window RSS, scale 2 | `max = 36.23 MiB` (`35.82..36.23`), median PSS `22.30 MiB`, host `7.95 MiB` | `< 50 MiB` | Met |
| Hidden background growth (`hidden_growth`, 10 vs 100 sessions, 3 repetitions) | `max = +0.41 MiB` (deltas `+0.41`, `+0.20`, `+0.39`; GUI median `25.11` -> `25.51 MiB`; host `9.19` -> `20.86 MiB`) | `<= +10 MiB` | Met |
| Two clean clones build identical binaries (`suite acceptance`) | `1 passed` in `92.16 s` | identical | Met |
| A local scan with 8 fake ports (a 30-id list, an Ollama, a keyed, a speak-first, two closers, an HTML page, a list of 1,500 ids cut at 64 KiB), 30 runs | median `160 ms`, worst `161 ms` (the `150 ms` quiet wait of the first contact is most of it); the host's resident size rose from `5.81` to `7.50 MiB` once and stayed there (its peak `VmHWM` is the same `7.50 MiB`); the answer is `4,529` bytes | bounded, nothing in the background | Met |
| Idle cost of the feature | the idle runs above include the new code and no scan: host `7.81` / `7.95 MiB` against `7.61` / `7.70 MiB` at M10 and `7.92` / `7.93 MiB` at M11; the dialog is drawn only while open | `< 50 MiB` | Met |
| Tool list size (`tools/list`, both ends of the clipboard-write switch) | `4,849` bytes with writing off, `5,103` with writing on | `<= 5,120` | Met with `17` bytes to spare |

The measurements ran on the committed clean tree (`e57f790`) in the heavy lane, one after the other, on a shared host with a load average of about `3` to `7`.

## Memory margin and the flaky tests

- The three cold runs peaked at `1,980.5`, `1,964.8` and `1,998.7 MiB`: all under `2,048 MiB`, but only `49` to `83 MiB` under it. M11's three runs peaked at `1,645` to `1,776 MiB`; M10's at `1,732` to `1,976 MiB` over three runs of differing outcome. The peak is of the whole test-runner tree (four workers and the compiler at the start) and moves with the number of tests that run at the same moment; it grew by about `200 MiB` since M11, with 16 new test files, and I do not know how much of that is these tests and how much is the load of the host. If the next milestone adds more tests this bound will be crossed; a decision then (a smaller worker count, a split of the largest GUI tests, or a higher bound) is the owner's. I did not change the bound.
- The tool list is `17` bytes under its budget: the next tool will not fit without shortening others.
- Flaky tests: in P02 one changed-suite run had two failures of the kind the lead asked about (`tests/test_first_view.py`, which took `15.6 s` against its `15 s` budget and takes about `15.5 s` alone, and `tests/test_picture_paste.py`); they passed alone and in the cold run after it. In M12 the cold and changed-suite full runs were: P01 two (one with failures I had caused, then clean), P02 two (one with those two flakes, then clean), P03 three clean. So M12 had two flaky failures in seven full runs, against M10's four in about eight and M11's none in six. The cause of the flakes is still unknown; `test_first_view` sits at its budget by itself.

## Proposed spec requirements (not added; the spec is edited only with the owner's yes)

The product specification says nothing about Local LLM servers. I propose three requirements, to be placed after the provider and authentication rules (section 5):

- **LLM-01** Discovery of Local LLM endpoints is on demand and read-only: it looks only at sockets that listen on the machine itself (this machine, or an ssh host the person named), with GET requests that have short timeouts, a bounded response, no body, no redirect and no credential, and never in the background; nothing it finds is persisted.
- **LLM-02** Discovery reads no other application's configuration or credential file (AUTH-08); it uses the list of listening ports and the HTTP answers only. A server that wants a key is reported as such and no key is tried.
- **LLM-03** A discovered endpoint is not a provider or an account until an explicit later flow makes it one; a key for it lives only in the OS keyring (AUTH-01, AUTH-02).

## Honest notes

- The end-to-end test failed first on a mistake in its own fixture setup (it wrote the same process-table links twice) and then on a loop that counted the speak-first server's empty receipts as requests; both were fixed before it went green.
- The remote script ran under this machine's Python and shell (the fake ssh is a local shell); a real host with a very old Python 3 or an unusual shell was not exercised.
- A real Ollama, llama.cpp, vLLM or LM Studio was not exercised (by rule): their answers are known from their documentation. The owner's check at G3 against a real server is the real test.
- Install: see the mailbox report (the installed file is built by `src/host/hostd/build-release.sh` from the clean closing commit; the `0.2.7` binary is kept at `private/release_0_2_7_backup/lico`).

## Not done / left to the owner

G3 acceptance and G4 merge; the branch `milestone/M12-local-llm-endpoint-discovery` is created by the lead at the closing commit; the decision about the spec requirements above; a decision on the memory margin if more tests are added.
