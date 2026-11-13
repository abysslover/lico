# M10 — Chats Tab, Full `past` -> `chats` Rename, "+ New chat" with Resume, and Resume Parity (`lico 0.2.6`)

- Status: done (accepted at G3, merged and published on `main` on 2026-11-14)
- Version: 1.0.1
- Approved: 2026-11-13 (G1 for M10 and G2 for M10-P01 to M10-P04 approved by the owner on 2026-11-13; G3 and G4, with publication, approved on 2026-11-14)
- Project: Lico
- Updated: 2026-11-14
- Milestone branch: `milestone/M10-chats-tab-rename-and-resume` (created at the closing commit, together with `milestone/M11-native-provider-home-chats`: M11 is stacked on M10 and both were accepted together)
- Implementation: `phase/M10-P01-tab-colors-and-chats-rename` .. `phase/M10-P04-verification-and-0-2-6` (`lico 0.2.6`; later phases are stacked on the previous phase tip)
- Next gate: closed (accepted at G3, merged and published on `main` at G4 on 2026-11-14)

## Goal

Make the left sidebar easier to read and the way back into earlier conversations one click, for people and for agents, without losing the single-executable and `< 50 MiB` idle GUI RSS rules:
(1) the three sidebar tabs (Directories, Chats, Calls) get clearly different colors;
(2) the word "Past" becomes "Chats" everywhere, completely: label, wire op `session.past` -> `session.chats`, HTTP `/v1/sessions/past` -> `/v1/sessions/chats`, CLI `lico session past` -> `lico chat list`, capabilities lists, code names, tests and living documents. No alias and no compatibility layer remain, so the old word cannot get in the way of later work;
(3) a "+ New chat" button above the `[..]` parent row of the Directories tab opens a small modal: choose an account, then `Resume` (default) or `New`. `Resume` shows `Last Session : <provider> / <YYYY-MM-DD HH:MM> / <title> (<N> turns)` and continues the latest chat of that directory for that account, falling back to `New` when there is none;
(4) the same rule is available to agents and scripts: `lico session create --resume-latest` and `mcp__lico__create` with `resume: true`, plus `lico chat latest`.

## Done when

1. The three tabs have distinct own colors (Directories blue, Chats amber, Calls teal), inactive tabs keep a faint tint of their own color, and label contrast is computed and `>= 4.5:1`.
2. No retired name remains in `src/`, `tests/`, README, CONTRIBUTING or the living docs (a test scans for it); a request with `session.past` gets the ordinary `unknown_op`, and the CLI and GUI explain a host/binary build mismatch in one clear sentence.
3. `session.chats` items carry `turns` (bounded scan, `null` when unknown, no latency regression); one host rule `latest_chat` serves the GUI preview, `lico chat latest` and `resume_latest` creation, for the three provider resume paths (running tab, disconnected tab, closed or external transcript) and for ssh accounts on the existing fake-ssh fixtures.
4. The "+ New chat" modal works by mouse and keyboard on local and ssh directories.
5. Cold full suite `<= 180.0 s` and `<= 2,048.0 MiB`, idle RSS `< 50.0 MiB`, 10->100 hidden growth `<= +10.0 MiB`, two-clone `suite acceptance` identical, `lico 0.2.6` installed to `~/bin/lico`. Numbers are recorded only as measured.

## Out of scope

- Any alias or fallback for the old names; changing dated reports, milestone and plan records of M07 to M09 (they are history and keep the old word).
- Real accounts or billable provider turns (all end-to-end runs use mocked providers).
- Push, pull request, merge or publication (owner G4).

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M10-P01 | Tab colors and the full rename, with old/new build mismatch handling | [Plan](../../plans/202611/20261113/20261113_09_M10-P01_tab_colors_and_chats_rename_plan.md) | [Report](../../reports/202611/20261113/20261113_09_M10-P01_tab_colors_and_chats_rename_report.md) | accepted & merged |
| M10-P02 | Resume core: `turns`, one `latest_chat` rule, `--resume-latest`, MCP `resume` | [Plan](../../plans/202611/20261113/20261113_10_M10-P02_resume_core_plan.md) | [Report](../../reports/202611/20261113/20261113_10_M10-P02_resume_core_report.md) | accepted & merged |
| M10-P03 | GUI "+ New chat" row and modal | [Plan](../../plans/202611/20261113/20261113_11_M10-P03_gui_new_chat_plan.md) | [Report](../../reports/202611/20261113/20261113_11_M10-P03_gui_new_chat_report.md) | accepted & merged |
| M10-P04 | End-to-end verification, measurements, and `lico 0.2.6` | [Plan](../../plans/202611/20261113/20261113_12_M10-P04_verification_and_0_2_6_plan.md) | [Report](../../reports/202611/20261113/20261113_12_M10-P04_verification_and_0_2_6_report.md) | accepted & merged |

## Gate record

- G3 and G4 (with publication) for this milestone were given by the owner in the chat on 2026-11-14, in one instruction that covered M10 and M11 together, because M11 is stacked on M10: the chain moved to `main` by fast-forward to the M11-P03 tip `38a9974`, and this closing commit then recorded the acceptance. The owner named no individual tips; the chain tips are `b943496` (M10-P04) and `38a9974` (M11-P03).
- What the owner had observed before giving it: `+ New chat` used on a real project folder (this is how the M11 finding came about: chats written before Lico was installed were not listed under the managed accounts), and, on a real open terminal, a project folder's open Claude Code session listed as running elsewhere (M11). Nothing beyond that was recorded as observed; the measurements in the phase reports were not independently reproduced at the gate.
- Follow-up asked by the owner, not blocking this gate: the running-elsewhere row should show the chat's title and how long ago it was written, and the repeated `Native` text should be reduced; an ssh host's own default home as `Native` is a possible extension. Both are for a later minor release.
