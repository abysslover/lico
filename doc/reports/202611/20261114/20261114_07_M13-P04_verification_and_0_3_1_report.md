# M13-P04 Report — End-to-End Verification, Measurements, and `lico 0.3.1`

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M13-P04-verification-and-0-3-1` (stacked on `phase/M13-P03-chat-list-polish` at `b09bc19`; code commit `6b1b522`)
- Plan: [M13-P04](../../plans/202611/20261114/20261114_07_M13-P04_verification_and_0_3_1_plan.md)

## What changed

- `lico` is `0.3.1` (`Cargo.toml`, `Cargo.lock`, `tests/test_single_executable.py`, `tests/test_control_parity.py`).
- `tests/test_llm_end_to_end_m13.py` (private Xvfb, fake servers, a fake ssh, a process table the test wrote; no real service or host): nothing is looked at before Enter; five servers are found of which one that lists no model is folded behind a line and four are listed (one `via ssh`, one `key needed`); the command gives the same servers as the dialog; closing and opening again shows the kept result without a scan; a click opens the fold; `Refresh` scans again and starts with the fold closed; an ssh host with an 80-character name is chosen, called once, its long model ids are whole in the detail area and the name is whole in the wrapped lines; choosing it again shows it from memory (`_calls` is the host once); every fake server was sent only the two GETs as HTTP/1.0 with no body, credential or cookie, and the speak-first server was sent nothing. The chat list is covered by `tests/test_chat_list_polish.py` (P03).
- Docs: CHANGELOG `0.3.1`, PLAN, roadmap, README, manual, `doc/index_4.md`, milestone and plan statuses.

## Measured verification (`lico 0.3.1`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `666 passed; 0 failed` (`1` ignored, both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`), three runs | `540` tests in `114` files, `540 passed` each: `124.9 s` / peak `1,421.1 MiB`; `118.9 s` / `1,489.4 MiB`; `120.2 s` / `1,414.2 MiB` | `<= 180.0 s`, `<= 2,048.0 MiB` | Met; the peak margin is `559..634 MiB` (it was `49..83 MiB` at `0.3.0`) |
| Idle window RSS, scale 1 (`idle_memory`, 10 accounts, menu on, 3 launches) | `max = 25.64 MiB` (`25.45..25.64`), PSS `17.05..17.14`, host `7.96..8.15 MiB` | `< 50 MiB` | Met |
| Idle window RSS, scale 2 | `max = 36.51 MiB` (`36.27..36.51`), PSS `22.46..22.65`, host `8.01..8.13 MiB` | `< 50 MiB` | Met |
| Hidden background growth (`hidden_growth`, 10 vs 100 sessions, 3 repetitions) | `max = +0.32 MiB` (deltas `-0.13`, `+0.29`, `+0.32`; GUI `25.58..25.88` at 100 sessions; host `9.10..9.24` -> `20.82..21.14`) | `<= +10 MiB` | Met |
| Two clean clones build identical binaries (`suite acceptance`) | `1 passed` in `165.56 s` | identical | Met |
| Scan time of the local engine on 46 fake sockets, 30 runs | median `160 ms`, worst `164` (was `608` / `616`); remote script on 48 quiet sockets `172 ms` (was `464`) | bounded | Met (P02) |
| Tool list size (`tools/list`, both ends of the clipboard-write switch) | `4,849` bytes with writing off, `5,103` with writing on (unchanged from `0.3.0`) | `<= 5,120` | Met with `17` bytes to spare |

Idle numbers are about `0.4 to 0.5 MiB` above `0.3.0` (`24.76..25.11` / `35.82..36.23`) and the host about `0.2` above (`7.81` / `7.95`); I did not look for the cause (new dialog and card code, the larger window JSON), the budget is `< 50 MiB`. The cold time is `118.9..124.9 s` against `114.8..116.2 s` in P01 and `116.7..117.8 s` in the single P02 and P03 runs: the first of the three runs here was `124.9 s`, about `6` seconds slower than the other two; I do not know whether the host was busier. The measurements ran on the committed clean tree in the heavy lane, one after the other.

## Memory margin and the flaky tests (the lead's open question)

- The cold peak is `1,414.2..1,489.4 MiB`, `559..634 MiB` under the bound (P01: the Rust job's `build_id()` read the whole program file in every test that touched it, about `200 MiB` each; font systems and private X servers ran ten at a time). The thread count of the Rust tests is the default (24), as the owner asked.
- Flaky tests: both found in P01 and fixed on the test side (`test_first_view` sent 170 `xdotool` processes; `test_picture_paste` lost a race to the window's once-a-second status read and now repeats the attempt). In M13 the full runs were: P01 six cold runs (one of them with the experimental thread variable), P02 one, P03 one, P04 three: `11` full runs, all passed, no flaky failure (the commit hook's changed-suite runs were all clean too).

## Proposed spec requirements (unchanged from the M12-P03 report, not added)

LLM-01 to LLM-03 are still waiting for the owner's yes; M13 added nothing that needs another. One more point for the owner: the cache of the dialog is memory only and per window; if a later milestone shares results with the tool, that is a new decision (a stale answer must not reach an agent).

## Later direction (direction only, not approved)

A found Local LLM server as its own kind of session in `+ New chat` with a Lico tool loop (Claude and Codex keep their OAuth sign-in and are not routed to local models), and later a lico host on an ssh host installed with the person's approval. Nothing here is built toward it except that the discovery result already carries host, port, bind, address, kind, apis, auth state, model ids and process.

## Honest notes

- The end-to-end test was not run red: it tests behaviour that P02 and P03 built and tested; the version tests changed together with the version.
- The idle growth of about `0.5 MiB` and the slower first cold run are measured, not explained.
- Not exercised (by rule): a real Ollama, llama.cpp, vLLM or LM Studio, a real ssh host, the owner's real homes; the owner's own use of the dialog on the real machine is the real test of the scan time.
- Install: see the mailbox report (built by `src/host/hostd/build-release.sh` from the clean commit of this report; the `0.3.0` binary is kept at `private/release_0_3_0_backup/lico`).

## Not done / left to the owner

G3 acceptance and G4 merge of the four M13 branches; the `milestone/M13-*` branch (the lead creates it at the closing commit); the decision about the spec requirements; the choice between the `Native` tag rule and group headings in the chat list (headings need a page-size change).
