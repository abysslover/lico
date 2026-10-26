# Lico roadmap: from the measured foundation to the basics of agentic coding

> Status: updated 2026-10-27 (the state of M01 and M02 in the table below; the plan itself is unchanged since 2026-10-26). The proposal of 2026-10-25 for M02 ("Usable Linux workbench") is replaced by the M02 "Agentic-coding basics" plan that the owner approved at G1 on 2026-10-26 (G2 included). This document grants no approval itself: every phase writes its plan note as its first step, and G3 inspection and G4 merge stay with the owner.
> Source of the goal: [README](../../../../../README.md) and the [product specification](../../20261023/00001_product_spec/product_spec.md) sections 1, 2, 7, 8 and 11. Detail of M02: the [planning set](../../20261026/00001_agentic_basics_plan/planning.md) and the [M02 milestone](../../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md).

## Goal

A small native workbench for persistent command-line agents (Claude Code and Codex first). The host owns execution; the window can be hidden,
closed and reopened without stopping work. Linux first, Windows later behind explicit platform interfaces. Secrets only in the OS keyring,
several accounts per provider, observable resource use. Budgets stay proposed targets until measured.

## Where the work stands

| Milestone | Scope | State |
|---|---|---|
| M01 Measured Linux foundation | P01 to P07 and P07a have plans and reports (all `in_review`, inspection pending); P08 (acceptance pack) is not started | G3 pending for every phase; G4 given for two moves of `main` (2026-10-25 and 2026-10-27) |
| M02 Agentic-coding basics | Sessions in a real folder, sending between sessions, Lico tools for the agent, screen capture and clipboard, computer use, memory per session, then a memory pass; P01 to P07 below | G1 approved 2026-10-26; P01 and P02 are reported (`in_review`, inspection pending); P03 in progress |
| M03 and later | Windows backend (own credential, windowing and measured acceptance) and the candidates pushed out of the earlier M02 proposal | not planned in detail |

M01 "Done when" items still open: item 3 needs an installed official provider path exercised with real sign-in evidence (the account owner's first
real login), and item 5 needs a clean public clone to produce the reviewed Linux package plus the account owner's end-to-end use.

## Phases

### M01 (finish)

| ID | Outcome | Evidence |
|---|---|---|
| M01-P07 | The window shows its first view and every panel, and the documented keys act (fixes a blank window after Ctrl+N) | red-then-green capture test on a virtual display, three captures; report written, inspection pending |
| M01-P07a | Comparison of lightweight Rust UI toolkits against the hand-drawn client on the unchanged 50 MiB budget (approved in chat on 2026-10-25) | comparison report on its phase branch, inspection pending |
| M01-P08 | Acceptance pack: clean public clone rebuilds the package; one index maps every M01 "Done when" item to its evidence; owner end-to-end checklist | clone-and-build log, evidence index; no new feature |

### M02 Agentic-coding basics (approved 2026-10-26)

The order is: define the features, build them with one measured memory line per phase, then reduce memory once with every feature on. The idle window stays under 50 MiB throughout.

| ID | Outcome (requirement groups) | Depends on |
|---|---|---|
| M02-P01 | Menu bar and shared controls, font and scroll speed, activity indicator, and the test cleanup (S05: the whole suite within 180 s, tests chosen by module dependency) (TST) | M01-P07 |
| M02-P02 | Sessions in a real folder: new-session screen, close, rename, delete; the memory of each session at the right of its row; input-box basics (WK, SX, MI, IN) | M02-P01 |
| M02-P03 | Sending text, selected lines or a whole conversation between sessions (long content goes by file path, nothing is cut); Lico tools the agent can call (SC, AC) | M02-P02 |
| M02-P04 | Screen capture, clipboard read and write, image paste, middle-button paste (XF) | M02-P03 for the agent tools; the CLI parts need only M02-P02 |
| M02-P05 | Computer use: a private virtual display for the agent, the real screen only by per-session, time-limited permission (CU) | M02-P04 and M02-P03 |
| M02-P06 | Git worktree sessions, attention marks and notifications, conversation search (WT, NT, SR) | M02-P02 |
| M02-P07 | Memory pass: re-measure the 50 MiB limit and the hidden growth with every feature on, reduce, review the whole milestone (MB) | all of M02 |

POC-0, a throwaway proof of concept with no product code, runs in parallel to the test cleanup and replaces the working assumptions of the planning set with measured values.

Each phase gets its own plan note (why, scope, steps, tests, budget) as its first step. A phase is sized so that its branch is reviewable in one sitting and
its outcome can be inspected on screen. G3 and G4 are asked every two phases (end of M02-P02 together with M02-P01, then M02-P04, M02-P06 and M02-P07).

### Candidates for the milestone after M02

Pushed out of the earlier M02 proposal and recorded in the [changes](../../20261026/00001_agentic_basics_plan/changes.md): grouping sessions by workspace; conversation view quality beyond
what M02-P01 did; control-surface parity (CLI and local REST inventory, event-gap recovery); an accessibility baseline and a native Wayland test labelled apart from X11;
release hygiene (public-clone build, package, documentation). The Windows backend keeps its own later milestone.

## How the work is divided

- Lanes with disjoint files: window (`src/host/hostd/src/gui/`), host and control surface (`core`, `ipc`, REST, resources), verification
  (capture harness, visual tests, mutation checks), documents. A phase edits files of one lane where it can; a shared file goes through the lead of the phase.
- Phases run in order, because each builds on the previous one (see the dependencies above). A new module is preferred to growing a file over 800 lines.
- Verification is independent of construction: the session that wrote a change does not judge it. Capture evidence comes from a virtual display
  with a private runtime directory, never from the person's own session or host.
- Test-first: a failing test is recorded before the change, then the passing run. Mock results and installed-provider results stay separate.
- Tests stay light: the whole suite within 180 s from scratch, normally only the tests a change can affect, and time and memory of tests in a ledger. See the [test plan](../../20261026/00001_agentic_basics_plan/test_plan.md).

## Gates

G1 milestone approval, G2 phase-plan approval, G3 inspection acceptance and G4 merge approval stay with the project owner. The owner approved G1 for M02, with the phase plans of G2 included, on 2026-10-26;
G3 is pending; G4 was given for two moves of `main` (2026-10-25 and 2026-10-27), each without G3. Publication (push) needs the owner's explicit yes for the exact list. This document grants none of them.
