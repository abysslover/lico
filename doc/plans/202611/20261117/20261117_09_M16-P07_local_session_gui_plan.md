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

## Parts

- **Display-free** `gui/local_rows.rs` (pure functions, unit tests, red first): servers from a discovery reply (openai and ollama kinds, loopback address only, model ids, whether a key is wanted), the choice (server, model) and how it moves, the request `session.create` makes, the words for an empty list, a server that wants a key, a server with no model, a refusal code of the host.
- **Form** (`newsession.rs`): the provider row cycles `claude`, `codex`, `local`; for `local` the rows are Provider, Server, Model, Name, Start (no account, folder, worktree, effort, permission); filled in from `last`; the existing refusal words cover the new codes.
- **Painter**: the rows are drawn by the form's existing row drawing from the same row list; only labels and value texts are added; painter tests are Rust tests of the rows and texts the painter is given (the existing `newsession_tests.rs` and `ui_tests.rs` style). Pictures cannot be red first: the painter has no test of its own that fails before it is written, so the proof is the row texts; no screenshot test is added because that would be new Python.
- **Session row and tabs**: the places that print a provider (`row.rs`, `sidebar_*`, `transcript.rs`, `tabs.rs`, `picture*.rs`) are audited one by one (about 135 lines name Claude or Codex); each is listed as "unaffected" or changed so that a `local` session is labelled `Local` with its model and never takes the Codex branch by default.
- **Host**: `start_driver_session` remembers the last-used choice (`last_of`), and the recent file accepts `local`; nothing else.

## Steps (tests first)

- [ ] 1. The question file, the lead's answer, this note updated with the approval.
- [ ] 2. `local_rows.rs` with its tests red then green.
- [ ] 3. Host: `last` for local (test first) and the recent file.
- [ ] 4. The form for `local` (tests of the rows and of the request first).
- [ ] 5. The audit of the provider places and the row label (tests of the texts first).
- [ ] 6. + New chat row (tests first).
- [ ] 7. Verification: the sum of what changed (gui tests, host tests, the Python window tests that start the window for the form and the sidebar, idle RSS at scale 1 and 2, the graph: the new module is a leaf of `gui` with no edge into a hub), report.

## Estimate and the stop rule

About 1,100 lines; **5 to 9 USD** at this milestone's measured rate (1.2 to 1.4 USD per 1,000 lines in P03 to P05, 6.7 in P06). If the phase has used about 13 USD (1.5 times the top of the range) and is not near done, I stop at a green commit and write a `question` file with the figures and the cause.

## Acceptance Criteria

The form makes a local session from a discovered server and model; the row says `Local`; the notes show; nothing secret is shown or stored; no new edge into a hub from below (checked on the graph); the idle window stays under 50 MiB (measured); every commit builds with `--locked` (`Cargo.lock` in the same commit as a dependency change).
