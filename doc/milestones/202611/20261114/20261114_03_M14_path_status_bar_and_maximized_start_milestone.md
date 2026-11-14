# M14 — Full Path in a Bottom Status Bar, and a Maximized First Start (`lico 0.3.2`)

- Status: planned (G1 and G2 approved by the owner on 2026-11-14)
- Version: 1.0.0
- Approved: 2026-11-14 (G1 and G2 for M14-P01 to M14-P03: the owner's plan approval in the chat on 2026-11-14)
- Project: Lico
- Updated: 2026-11-14
- Base: `main` at `8857a4c` (M13 accepted, merged and pushed; the component rules are on `main`); later phases are stacked on the previous phase tip
- Milestone branch: `milestone/M14-*` is created by the lead at the closing commit, not by the implementing session
- Implementation: `phase/M14-P01-path-status-bar` .. `phase/M14-P03-verification-and-0-3-2` (`lico 0.3.2`)
- Next gate: G3 after P03 (owner)

## Goal

Two small things the owner asked for on 2026-11-14.

1. When a directory is shown the full path is hard to see: a long status bar at the bottom of the window shows the full path of what the person points at or has selected (a directory or file row of the Directories tab, a Files panel row, a chat card's folder), and the active session's working directory when nothing is pointed at; `host:path` for an ssh host; a path too long for the bar keeps its end behind a leading ellipsis and stays whole one click away (it is copied).
2. The window does not start maximized the first time (M07-P01 delivered a maximized launch). Reproduce it first, find the cause, fix the cause, and test the outcome where a test can.

## Rules kept

The component rules of 2026-11-14 bind this new code: no new upward edge into a hub (`core`, `session`, `folders`, `transfer`, `handoff`, `codex`); the bar belongs to the window layer. Idle RSS `< 50 MiB`: the bar redraws only when its text changes, with no timer, thread or polling. Tests never use the owner's socket, host, `~/.claude` or `~/.codex`; no billable turn; no new op, tool or CLI command for the path. No spec edit. This milestone is verified with the whole cold suite (the last milestone to be, before the component foundation).

## Done when

1. P01: the bar is the last row of the window at its full width; it shows what is pointed at, else what is selected, else the active session's working directory, as `host:path` for an ssh host; a long path (wide characters included) keeps its end with a leading `…` and a click copies the whole text; the telemetry strip, the hint line and the input box stay where they were relative to each other at the default size and at a narrow width; the page sizes of the chat list, the directory list and the Files panel fit above it; two idle seconds draw nothing; window tests red first; before and after screenshots.
2. P02: the cause of the unmaximized first start is found by reproduction (a real window manager in a private display if one can be had without touching the owner's session, and a model of the standard otherwise) and fixed; a test checks the OUTCOME (`_NET_WM_STATE` and size read from X), and the report says what it can and cannot show; the owner is told what to check on their own screen.
3. P03: cold full suite `<= 180.0 s` and `<= 2,048.0 MiB` (three runs, each reported, with the margin), idle RSS `< 50.0 MiB`, hidden growth `<= +10.0 MiB`, two-clone `suite acceptance`, the tool-list bytes stated, `lico 0.3.2` installed to `~/bin/lico` (the `0.3.1` binary kept at `private/release_0_3_1_backup/lico`). Numbers only as measured.

## Out of scope

- A change of the telemetry strip, of any other path display (they keep their short forms), a new op, tool or CLI command; any spec edit.
- The component foundation (Phase 0 report, workspace, Rust tools, the first component) that follows this milestone under the owner's order.
- Push, pull request, merge or publication (owner G4).

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M14-P01 | Bottom status bar with the full path | [Plan](../../plans/202611/20261114/20261114_08_M14-P01_path_status_bar_plan.md) | [Report](../../reports/202611/20261114/20261114_08_M14-P01_path_status_bar_report.md) | implemented |
| M14-P02 | Maximized first start | [Plan](../../plans/202611/20261114/20261114_09_M14-P02_maximized_first_start_plan.md) | pending | planned |
| M14-P03 | Verification and `lico 0.3.2` | [Plan](../../plans/202611/20261114/20261114_10_M14-P03_verification_and_0_3_2_plan.md) | pending | planned |
