# Lico roadmap: from the measured foundation to a usable Linux workbench

> Status: proposed (2026-10-25). It approves nothing: M02 needs its own milestone approval (G1) and every phase needs its own plan approval (G2).
> Source of the goal: [README](../../../../README.md) and the [product specification](../../20261023/00001_product_spec/product_spec.md) sections 1, 2, 7, 8 and 11.

## Goal

A small native workbench for persistent command-line agents (Claude Code and Codex first). The host owns execution; the window can be hidden,
closed and reopened without stopping work. Linux first, Windows later behind explicit platform interfaces. Secrets only in the OS keyring,
several accounts per provider, observable resource use. Budgets stay proposed targets until measured.

## Where the work stands

| Milestone | Scope | State |
|---|---|---|
| M01 Measured Linux foundation | P01 to P06 built; P07 (first view and key input) in progress | G3 and G4 pending for every phase |
| M02 Usable Linux workbench | proposed below | not started |
| M03 Windows backend | later; own credential, windowing and measured acceptance | not planned in detail |

M01 "Done when" items still open: item 3 needs an installed official provider path exercised with real sign-in evidence (the account owner's first
real login), and item 5 needs a clean public clone to produce the reviewed Linux package plus the account owner's end-to-end use.

## Phases

### M01 (finish)

| ID | Outcome | Evidence |
|---|---|---|
| M01-P07 | The window shows its first view and every panel, and the documented keys act (fixes a blank window after Ctrl+N) | red-then-green capture test on a virtual display, three captures |
| M01-P08 | Acceptance pack: clean public clone rebuilds the package; one index maps every M01 "Done when" item to its evidence; owner end-to-end checklist | clone-and-build log, evidence index; no new feature |

### M02 Usable Linux workbench (proposed)

| ID | Outcome (specification section) | Depends on |
|---|---|---|
| M02-P01 | Discoverable mouse controls: a classic menu bar (Session, Account, View, Help) and a status bar; every shortcut is also a menu entry (s8) | M01-P08 |
| M02-P02 | Workspace and session navigator: grouped by workspace, stable order, real work/approval/error state, approvals visible while a session is hidden (s2, s8) | M02-P01 |
| M02-P03 | Conversation view quality: selection, copy and paste, resize, wide and combining characters, Korean IME, two display scales (s8) | M01-P08 |
| M02-P04 | Control-surface parity: CLI and local REST with owner-only access, event-gap recovery and an API inventory (s7) | M01-P08 |
| M02-P05 | Hidden-session metadata, optional notifications, resource panel, accessibility baseline, native Wayland test labelled apart from X11 (s4, s8) | M02-P02 |
| M02-P06 | Release hygiene: public-clone build, package, documentation, independent review of the whole milestone (s10, s11) | all of M02 |

Each phase gets its own plan (why, scope, steps, tests, budget) and waits at `draft` until approved. A phase is sized so that its branch is
reviewable in one sitting and its outcome can be inspected on screen.

## How the work is divided

- Lanes with disjoint files: window (`src/host/hostd/src/gui/`), host and control surface (`core`, `ipc`, REST, resources), verification
  (capture harness, visual tests, mutation checks), documents. A phase edits files of one lane; a shared file goes through the lead of the phase.
- Phases of one lane run in sequence. M02-P03 (window) and M02-P04 (host) may run side by side after M01-P08 because they touch different files.
- Verification is independent of construction: the session that wrote a change does not judge it. Capture evidence comes from a virtual display
  with a private runtime directory, never from the person's own session or host.
- Test-first: a failing test is recorded before the change, then the passing run. Mock results and installed-provider results stay separate.

## Gates

G1 milestone approval, G2 phase-plan approval, G3 inspection acceptance and G4 merge approval stay with the project owner. This document grants none of them.
