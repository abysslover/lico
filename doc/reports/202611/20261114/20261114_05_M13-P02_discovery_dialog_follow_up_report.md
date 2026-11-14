# M13-P02 Report — Discovery Dialog Follow-Up: Cache, Speed, Fold, Readable Rows

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M13-P02-discovery-dialog-follow-up` (stacked on `phase/M13-P01-test-split-and-flaky` at `e8a857c`; commits `d638389`, `a129945`)
- Plan: [M13-P02](../../plans/202611/20261114/20261114_05_M13-P02_discovery_dialog_follow_up_plan.md)

## What changed

- **Speed.** Where the time went (measured on a fixture of 46 fake sockets, 26 of them quiet HTTP servers, 8 that speak first, 6 that close, 6 that nobody serves; nothing real is probed): the engine waited the 150 ms quiet period once per round of 8 sockets, so 26 quiet sockets cost four rounds, `608` ms. Now every selected socket (at most 64) gets its first, silent contact at once in its own short thread, so the quiet period is waited for once; the two questions are then asked 8 at a time and only of the sockets that stayed quiet (`llm/mod.rs`; `http.rs` is unchanged). The remote Python script has the same two stages (`llm/remote_script.rs`). Every bound is unchanged: 64 sockets, connect 300 ms, quiet 150 ms, 1.5 s a request, 8 s in all (20 s for an ssh host), 64 KiB, GET only, a server that speaks first is sent nothing, no redirect, no credential. The quiet wait was not shortened.
- **Cache.** The window keeps the last result of each (host, all users) pair in its memory with its time (`Kept`, at most 8, the oldest is dropped, nothing on disk, nothing for the CLI, the HTTP route or the tool). Opening the dialog, choosing another host (Left, Right) or toggling `A` shows what is kept for that choice, with `found N s/min/h ago` and a note that it is kept from the earlier look, and scans nothing; Enter, `S` or the `Refresh` button (the `Scan` button once a result is shown) scans again. The window JSON carries `cached`, `age_s`, `found`, `scan_label`.
- **Noise.** The list shows the servers that list models and those that ask for a key; servers that answered but list no model sit behind one line, `N more servers answered but list no model - click or press F to show`. A row owned by a process named `ssh` says `via ssh` and is folded by the model rule only, never by its name (the test has an ssh-owned server with a model that stays in the list and one without that is folded). The summary line stays.
- **Readable rows.** A row is two lines (address, kind, bind, key state and owner; then the models as whole ids with `+N` for the rest). Up and Down select a row (the list follows; opening the fold selects the first folded server), the selected row is shown whole under the list (head, every model id the reply holds, wrapped, at most 6 lines; beyond that it says how many models there are and that `lico llm discover` prints every id). The ssh host name is wrapped on its own lines under the title, never cut. One plain line says what the folded and key-less rows mean: `No model listed: the server answered, but it lists no model, so nothing can be run on it yet (for example nothing is pulled, or a key is needed).`
- New file `gui/llm_rows.rs` (pure functions: items, fold, two-line rows, detail, age words, scrolling by lines); `llm_find.rs` rewritten around it (`521` lines); two actions `LlmMore` and `LlmPick(i)`; `sidebar.rs` has the cache field (`778 -> 779` lines).

## Evidence (as measured)

| Check | Before | After |
|---|---|---|
| Local engine, 46 fake sockets, 30 runs (`private/measure/discover_speed.py`) | median `608 ms`, worst `616` (engine `607`/`615`) | median `160 ms`, worst `164` (engine `159`/`163`); same classification (`openai 5, ollama 1, not-http 14, http-other 20, unknown 6`) |
| Remote script, 48 quiet sockets, 30 runs (`private/measure/remote_speed.py`) | `464 ms` (3 runs in the red test) | median `172 ms`, worst `193` |
| `tests/test_llm_discover_speed.py` (2 tests, bounds `< 450 ms` local and `< 300 ms` remote, far over the measured values) | red: median `615 ms` and `464 ms` | green |
| `tests/test_llm_dialog_follow_up.py` (3 window tests: cache and Refresh, fold and `via ssh`, long ssh rows with an 80-character alias and 84-character model ids) | red: missing `cached`, `items`, `place_lines` | green; old `tests/test_llm_window.py` and the discovery, ssh and tool tests green |
| Rust unit tests (`gui::llm_rows` 4, `gui::llm_find` 3) | | `7 passed` |
| Cold full suite on the tip | | `538` tests in `112` files, `538 passed`, `117.8 s`, tree peak `1,427.9 MiB`; Rust `663 passed, 1 ignored` |

At the default width the dialog is `84` columns. Screenshots (private, not in the repository): `private/shots/m13_p02/` (idle, scanned and folded, fold open with a selection, reopened from memory, an ssh host with long rows).

## Red first

The two timing tests and the three window tests were written and run before any code (`private/runs/m13_p02_speed_red.log`, `m13_p02_dialog_red.log`). The Rust unit tests were written together with the code (a Rust test lives in the file of its code) and were not run red; the window tests above cover the same behaviour from outside.

## Honest notes

- The owner's `1,061 ms` on the real machine was not reproduced or re-measured here (tests never probe a real machine); the fixture has the same socket count and a plausible mix, and its time fell by a factor of 3.8. The real gain depends on how many of the 46 are quiet.
- Stage 1 holds up to 64 connections open for the quiet period (it was 8) and uses one short thread per socket instead of eight workers; the host's idle memory is not touched (nothing runs until asked), and P04 measures it again.
- A small behaviour fix that came with the cache: an answer that arrives after the person has chosen another host or toggled `A` is kept for its own pair but no longer shown as if it belonged to the new choice (before, it could be).
- The detail area is capped at 6 lines; a server with very many long model ids is not whole on screen, which the last line says, with where the full list is.
- The remote script was changed, so the parity test (`tests/test_llm_discover_ssh.py`) was run: it passes.

## Next

P03: the chat list (a chat running elsewhere shows its title and when it was last written; `Native` is shown once).
