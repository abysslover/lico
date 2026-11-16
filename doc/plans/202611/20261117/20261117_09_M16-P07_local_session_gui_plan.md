# M16-P07 Plan — The Window for the Local Session

- Status: proposed; waits for the lead's answer to question 122 (the owner is away: approvals are given by the lead under the owner's delegation, and recorded here as such)
- Version: 1.0.0
- Branch: `phase/M16-P07-local-session-gui` (stacked on the P06 tip `60b6919`)
- Milestone: [M16](../../../milestones/202611/20261117/20261117_01_M16_retire_prototypes_and_local_llm_session_milestone.md)

## Objective

A person can make, see and use a local session in the window, with the display-free part first and a thin painter: **New session** and **+ New chat** choose a server and a model from the discovery, the session row says what it is, and the plain notes the host already writes ("the session answers in text only", "N oldest turns are no longer sent to the model") show in the transcript. No new host operation, no new Python, no secret in the window.

## What is new against the approved text (for the lead to decide)

1. **The key stays on the command line in P07** (`lico local-key set ADDRESS:PORT`); the window shows the sentence that says so when the server wants a key. Report 119 (Q8) allowed "in the window or on standard input". A masked entry in the window would put a secret in the window's memory and needs a masked text row in the painter; I recommend leaving it out of this milestone.
2. **`session.options` (reply, not a new operation) gets three keys in its `last` object** for a local session: `endpoint`, `api`, `context_tokens` (the model is already there): the host already remembers "what the last session used" in its owner-only recent file; that file's `kind` check accepts `local` too. I found no protocol line for `session.options` to edit; if the lead names one I will edit it in a `doc(M16-P07)` commit first.
3. **Servers come from a scan the form runs itself** (`llm.discover`, on demand, read-only, the person presses Enter on the row), kept in the window's memory with its age like the dialog does; no result is persisted. There is no "Use as session" button in the dialog (less code, one path).
4. **+ New chat** offers the last-used local choice as one more row ("Local: MODEL at ADDRESS") that makes a new local session at once (no `Resume`: a local session is resumed from its tab); its `Options...` opens the full form.

## Approval, and what the lead added (mail 141)

Items 1 to 5 above are approved by the lead under the owner's delegation (the owner away). Additions: (1) the key stays on the command line and standard input; **"a masked key entry in the window"** is named in the P08 report's list of what is left over, for the owner to decide later. (2) No document of the protocol or the spec names `session.options`, so there is nothing to edit first; **the `last` object of its reply gains `endpoint`, `api` and `context_tokens`** for a local session, and the host's owner-only recent file keeps no key and no raw response (only the address, the format, the model and the window the person chose). (3) The scan never starts by itself when the form opens: only Enter, Space or a click on the Server row starts it (LLM-01: on demand, read-only, kept in the window's memory only). (4) No Resume for a local session in + New chat. (5) At the end of P07, with the existing window rig only (no new Python file), one screenshot per new view (the New session form with the Local row, the server and model choice, a local session's row with its note, a waiting card), kept under `private/scratch/` in the dated layout, for looking, not as a test; the paths go into the P07 and P08 reports, and a view the rig cannot show without new Python is named and skipped.

## Parts

- **Display-free** `gui/local_rows.rs` (pure functions, unit tests, red first): servers from a discovery reply (openai and ollama kinds, loopback address only, model ids, whether a key is wanted), the choice (server, model) and how it moves, the request `session.create` makes, the words for an empty list, a server that wants a key, a server with no model, a refusal code of the host.
- **Form** (`newsession.rs`): the provider row cycles `claude`, `codex`, `local`; for `local` the rows are Provider, Server, Model, Name, Start (no account, folder, worktree, effort, permission); filled in from `last`; the existing refusal words cover the new codes.
- **Painter**: the rows are drawn by the form's existing row drawing from the same row list; only labels and value texts are added; painter tests are Rust tests of the rows and texts the painter is given (the existing `newsession_tests.rs` and `ui_tests.rs` style). Pictures cannot be red first: the painter has no test of its own that fails before it is written, so the proof is the row texts; no screenshot test is added because that would be new Python.
- **Session row and tabs**: the places that print a provider (`row.rs`, `sidebar_*`, `transcript.rs`, `tabs.rs`, `picture*.rs`) are audited one by one (about 135 lines name Claude or Codex); each is listed as "unaffected" or changed so that a `local` session is labelled `Local` with its model and never takes the Codex branch by default.
- **Host**: `start_driver_session` remembers the last-used choice (`last_of`), and the recent file accepts `local`; nothing else.

## Steps (tests first)

- [x] 1. The question file, the lead's answer, this note updated with the approval.
- [x] 2. `local_rows.rs` with its tests red then green.
- [x] 3. Host: `last` for local (test first) and the recent file.
- [x] 4. The form for `local` (tests of the rows and of the request first).
- [x] 5. The audit of the provider places and the row label (tests of the texts first).
- [x] 6. + New chat row (tests first).
- [x] 7. Verification: the sum of what changed (gui tests, host tests, the Python window tests that start the window for the form and the sidebar, idle RSS at scale 1 and 2, the graph: the new module is a leaf of `gui` with no edge into a hub), report.

## Estimate and the stop rule

About 1,100 lines; **5 to 9 USD** at this milestone's measured rate (1.2 to 1.4 USD per 1,000 lines in P03 to P05, 6.7 in P06). If the phase has used about 13 USD (1.5 times the top of the range) and is not near done, I stop at a green commit and write a `question` file with the figures and the cause.

## Acceptance Criteria

The form makes a local session from a discovered server and model; the row says `Local`; the notes show; nothing secret is shown or stored; no new edge into a hub from below (checked on the graph); the idle window stays under 50 MiB (measured); every commit builds with `--locked` (`Cargo.lock` in the same commit as a dependency change).

## Result (2026-11-17, as measured)

Approved by the lead under the owner's delegation (the owner away); nothing here is accepted: G3 and G4 stay with the owner.

| Check | Result |
|---|---|
| What was built | `gui/local_rows.rs` (display-free rows, 244 lines with 7 tests); the Local provider in the new-session form (`newsession.rs` and `newsession_local.rs`: Provider, Server, Model, Name, Start; the look only on Enter, Space or a click on the Server row, run off the window's thread as `AsyncJob::LocalScan`); the last local choice in `session.options` (`last` gains `endpoint`, `api`, `context_tokens`; the owner-only recent file keeps no key and no raw response); the `+ New chat` row "Local: MODEL at ADDRESS" (a fresh session, no Resume, `Options...` opens the form on it); the labels (header "Local \| MODEL \| state", list heading "Local / this machine", tab `L`, footer "Local (no files)", no account or permission mode to change) |
| New tests | host units 701 passed and 5 ignored under `umask 022` and `077` (from 673 at the end of P06; 28 new: `local_rows` 7, the form 10, + New chat 4, labels 5, host `last` 1, status model 1); tools 17; Python files that start the window (new session screen, + New chat, tabs and telemetry, card window, left sidebar, keys, admin controls, session lifecycle, selection, text rules, size ratchet): 60 passed |
| Red first | every test above was written and seen red before its code (compile errors or assertion failures); the painter was not changed: the rows are strings the existing painter draws, so no pixel test exists and none was added (no new Python) |
| Found by the screenshots, fixed with a test first | a form that opens on Local had its focus on a folder row it does not show; the header said "model not reported" (the adapter now sets the session's model); the tab said `G` for a session with no folder; a notice about the claude program stayed on the Local form |
| Size ratchet | P06 had left `core.rs` (966, held at 941), `folders.rs` (815) and `session.rs` (806) over the ratchet and I had not run `test_size_ratchet` then; fixed here with no allowlist change: the spawner block moved unchanged to a new leaf `procs.rs`, `start_driver_session` moved to `new_session.rs`, the Local part of the form to `newsession_local.rs` (core 899, folders 800, session 800, newsession 748) |
| Module graph | 178 modules (3 new: `local_rows`, `procs`, `newsession_local`), **the cycle is still 48**; `procs` imports nothing of the crate; no new edge from a leaf into a hub |
| System tests (`cargo xtask system`) | all four pass: `0.1 s / 18.4 MiB`, `0.2 s / 18.8`, `0.4 s / 18.6`, `0.2 s / 18.2` |
| Idle window RSS (3 launches, `lico_measure run --only idle_memory`) | scale 1 `25.31 MiB` (`25.18..25.31`), scale 2 `36.41 MiB` (`36.29..36.41`), host `8.07` and `8.18 MiB`; P06 had `25.51`, `36.65`, `8.21`, `8.19`; limit `< 50 MiB` |
| Cost | about 6.3 USD (estimate 5 to 9; the stop rule was 13), about 970 lines added and 140 removed in `src`, 10 commits |

### Which parts could not be red first, and why

The painter: it draws the strings the form and the modal give it; its output is pixels, and no pixel test exists for any row of any form. The proof is the texts (the rows, the lines and the dump the existing window tests read) and the screenshots below, which are for looking and are not a test.

### The audit of the places that print a provider

Changed (each with a test): `row.rs` header, `attn.rs` list heading, `tabs.rs` caption, `admin_ui.rs` (rebind and mode reasons), `telemetry.rs` (footer and the Files flash), `newchat.rs`, `newsession.rs`. Unaffected, read and left: `picture.rs` (only `claude` takes pictures: a local session is refused with the existing words), `sendto.rs` (`has_record` is claude only), `sidebar_chats.rs` (the chats list comes from the host's chat sources, which hold no local session), `approval_modal.rs`, `menu.rs`, `mention_popup.rs`, `call_view.rs`, `sidebar_calls.rs`, `native_prompt.rs` (display of the provider word, or about accounts of claude and codex).

### Screenshots (for looking, not a test; the existing window rig on a private display, a private home, fixed-reply servers of `tests/llm_fixtures.py`, driven by an inline script that is not saved in the repository)

`private/scratch/202611/20261117/m16_p07_screens/`: `1_new_session_form_local_row.png` (the form on Local before any look), `2_server_and_model_choice.png` (after the look: three servers found, one chosen, its second model chosen), `3_local_session_row_and_answer.png` (the session: tab `L`, header, an answer, footer), `4_waiting_card.png` (a tool call that needs a card: the approval card and `[waiting for the person]`). **Not shown:** the notes "the session answers in text only" and "N oldest turns are no longer sent to the model": the fixture answers every request with the same reply, so it cannot refuse the tools once and accept the retry, nor fill a window; the Rust tests of the adapter prove those two lines.

### What is left over (for the owner)

A masked key entry in the window (the key is stored with `lico local-key set ADDRESS:PORT`, on standard input); the two notes above have no picture.
