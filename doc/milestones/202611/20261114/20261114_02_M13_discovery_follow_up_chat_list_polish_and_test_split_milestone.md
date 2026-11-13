# M13 — Local LLM Discovery Follow-Up, Chat List Polish, and the Split of the Big GUI Tests (`lico 0.3.1`)

- Status: planned (G1 approved by the owner on 2026-11-14; plan approved in the chat the same day)
- Version: 1.0.0
- Approved: 2026-11-14 (G1 and G2 for M13-P01 to M13-P04: the owner's go-ahead and plan approval on 2026-11-14)
- Project: Lico
- Updated: 2026-11-14
- Base: `main` at `cc34ab4` (M12 accepted, merged and published); later phases are stacked on the previous phase tip
- Milestone branch: `milestone/M13-*` is created by the lead at the closing commit, not by the implementing session
- Implementation: `phase/M13-P01-test-split-and-flaky` .. `phase/M13-P04-verification-and-0-3-1` (`lico 0.3.1`)
- Next gate: G3 after P04 (owner)

## Goal

Follow up what the owner found when using the M12 dialog on the real machine and in the chat list, and bring the test suite's memory margin back.

1. The cold full suite peaked at `1,964.8` to `1,998.7 MiB` against its `2,048 MiB` bound (M11: `1,645.1` to `1,775.9 MiB`), and two tests failed once each. Measure which files carry the memory, split the heaviest GUI and host tests without losing coverage, and find the cause of the two flaky tests.
2. Local LLM discovery dialog: the last result is kept in the window with its time and a Refresh scans again; the scan is faster with the same bounds; the list is less noisy (servers that list no model are folded, never hidden by process name); long rows, in particular those of an ssh host, can be read; the dialog says what "no model listed" means.
3. Chat list: a chat that runs elsewhere shows its title and how long ago it was written; the word `Native` is not repeated on every row.

## Rules kept

Discovery stays read-only, bounded and on demand, never in the background: GET only, no credential, no redirect, at most 64 KiB read, AUTH-08 unchanged; raw responses are not persisted. Tests never probe a real service, host or socket (fake servers on ephemeral loopback ports, `--llm-proc-root`, the fake ssh, a private `--socket`, no real `~/.claude` or `~/.codex`). Idle RSS `< 50 MiB`, one executable, GUI for the human with CLI and tool parity, no billable turn in tests. No product behaviour change in P01.

## Done when

1. P01: per-file time and peak are measured with the ledger (commands in the report); the heaviest GUI and host files are split with the same tests (a by-name count proves nothing was lost); the cold tree peak is `<= 1,800 MiB` with time not worse than `120 s` by more than a few percent, or the measured best is reported with at least three alternatives and a recommendation; the cause of the two flaky tests is named and fixed where it is on the test side.
2. P02: the dialog shows a cached result with its age and scans again on Refresh or Enter; the scan is measured before and after on fake servers of the owner's count with every bound kept; servers that list no model are folded behind one line; an `ssh`-owned row is labelled `via ssh` and never hidden by name; rows are readable at the default width and no cut text is unreachable; one plain line says what "no model listed" means.
3. P03: a chat running elsewhere shows its title and when it was last written; `Native` is shown once as a group label; the running-elsewhere rule is unchanged.
4. P04: cold full suite `<= 180.0 s` and `<= 2,048.0 MiB` (each of three runs reported, with the margin reached), idle RSS `< 50.0 MiB`, hidden growth `<= +10.0 MiB`, two-clone `suite acceptance`, the tool-list byte budget stated, `lico 0.3.1` installed to `~/bin/lico` (the `0.3.0` binary kept as a backup). Numbers only as measured.

## Out of scope

- Using a found endpoint, a tunnel, a proxy, stored endpoints or keys, a remote install, a Native entry for ssh hosts; no spec edit (the proposed LLM-01 to LLM-03 wait for the owner's yes).
- Later direction (direction only, not approved): a found Local LLM server as its own kind of session in `+ New chat`, driven by a Lico tool loop (Claude and Codex keep their OAuth sign-in unchanged and are not routed to local models); and, later still, a lico host on an ssh host, installed with the person's approval. Nothing in M13 is built toward these except that the discovery result already carries the fields they would need.
- Push, pull request, merge or publication (owner G4).

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M13-P01 | Split the big GUI tests; reproduce the flaky ones | [Plan](../../plans/202611/20261114/20261114_04_M13-P01_test_split_and_flaky_plan.md) | pending | planned |
| M13-P02 | Discovery dialog follow-up: cache, speed, fold, readable rows | [Plan](../../plans/202611/20261114/20261114_05_M13-P02_discovery_dialog_follow_up_plan.md) | pending | planned |
| M13-P03 | Chat list polish | [Plan](../../plans/202611/20261114/20261114_06_M13-P03_chat_list_polish_plan.md) | pending | planned |
| M13-P04 | Verification and `lico 0.3.1` | [Plan](../../plans/202611/20261114/20261114_07_M13-P04_verification_and_0_3_1_plan.md) | pending | planned |
