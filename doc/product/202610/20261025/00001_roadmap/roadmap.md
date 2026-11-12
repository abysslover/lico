# Lico roadmap: from the measured foundation to the basics of agentic coding

> Status: updated 2026-11-12 (the state of M01, M02 and M03 in the table below; the plan itself is unchanged since 2026-10-26). The proposal of 2026-10-25 for M02 ("Usable Linux workbench") is replaced by the M02 "Agentic-coding basics" plan that the owner approved at G1 on 2026-10-26 (G2 included). This document grants no approval itself: every phase writes its plan note as its first step, and G3 inspection and G4 merge stay with the owner.
> Source of the goal: [README](../../../../../README.md) and the [product specification](../../20261023/00001_product_spec/product_spec.md) sections 1, 2, 7, 8 and 11. Detail of M02: the [planning set](../../20261026/00001_agentic_basics_plan/planning.md) and the [M02 milestone](../../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md).

## Goal

A small native workbench for persistent command-line agents (Claude Code and Codex first). The host owns execution; the window can be hidden,
closed and reopened without stopping work. Linux first, Windows later behind explicit platform interfaces. Secrets only in the OS keyring,
several accounts per provider, observable resource use. Budgets stay proposed targets until measured.

## Where the work stands

| Milestone | Scope | State |
|---|---|---|
| M01 Measured Linux foundation | All phases P01 to P08 (including P07a) accepted at G3, merged and published on `main` (`lico 0.1.0`) | Done (2026-11-12): `milestone/M01-measured-linux-foundation` |
| M02 Agentic-coding basics | Sessions in a real folder, sending between sessions, Lico tools for the agent, screen capture and clipboard, computer use, git worktrees, notifications, search, and memory pass; P01 to P07 below | Done (2026-11-11): all seven phases P01 to P07 accepted at G3, merged and published on `main` at G4 (`milestone/M02-agentic-coding-basics`) |
| M03 Linux administrator GUI completeness and control-surface parity | Account rename, in-window session rebind and permission-mode toggle, workspace grouping and text size settings, prompt editor selection/undo/reflow, and CLI/HTTP/capabilities parity (`M03-P01` to `M03-P04`; Windows backend stays in a later milestone) | Done (2026-11-12): all four phases M03-P01 to M03-P04 accepted at G3, merged and published on `main` at G4 (`milestone/M03-admin-gui-and-control-parity`, `lico 0.1.0`) |
| M04 Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer | Direct sidebar switching from panels, derived session titles, active reverse-chronological sorting, clickable toolbar, default-hidden internal control markers + user/agent colors, `7d` weekly quota telemetry, and on-demand (`F5`) working-directory file viewer with `+ New here` and `.html` browser launch (`M04-P01` to `M04-P04`, `lico 0.2.0`) | Done (2026-11-12): all four phases M04-P01 to M04-P04 accepted at G3, merged and published on `main` at G4 (`milestone/M04-workspace-workbench-and-conversation-ux`, `lico 0.2.0`) |
| M05 Working-directory restore across restarts, paged directory navigation, and unified file opening | Automatic working-directory & workspace-view restore across restarts, top-anchored panel layout + `Page X / Y` / `< Prev` / `Next >` / wheel navigation (`<= 10,000` entries), and unified file open (direct text preview vs system default `xdg-open`, removing separate `V` shortcut; `M05-P01` to `M05-P03`, `lico 0.2.1`) | Done (2026-11-12): all three phases M05-P01 to M05-P03 accepted at G3, merged and published on `main` at G4 (`milestone/M05-workspace-continuity-and-file-navigation`, `lico 0.2.1`) |
| M06 Hidden-file toggle and SSH remote accounts, directory browsing, and sessions | Default-off dotfile toggle (`Hidden: off/on`) in `Files`, `~/.ssh/config` host discovery, `[ssh: <host>]` remote accounts with remote `Sign in`, and SSH remote directory browsing (`Host` combo-box + non-blocking `ssh_unreachable`) & persistent `claude`/`codex` sessions (`M06-P01` to `M06-P03`, `lico 0.2.2`) | Done (2026-11-12): all three phases M06-P01 to M06-P03 accepted at G3, merged and published on `main` at G4 (`milestone/M06-hidden-files-and-ssh-remote-sessions`, `lico 0.2.2`) |
| M07 Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume | Maximized launch, `Global` general sessions, right-pane session tab bar, live `Model(Effort)` / `Tok in/out` / `5h·Wk` / `cwd` telemetry strip, left `[ Directories \| Past ]` sidebar with highlighted active `cwd`, and full-pagination `session.past` discovery & one-click `claude`/`codex` resume (`M07-P01` to `M07-P03`, `lico 0.2.3`) | In progress (2026-11-12): G1 and G2 approved 2026-11-12 |

All five M01 "Done when" items are evidenced in the [M01-P08 acceptance pack report](../../../../reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md) (live session continuity accepted at G3 on 2026-11-11 and two-clone reproducible `lico 0.1.0` build verified via `python3 -m lico_suite acceptance`).

## Phases

### M01 (finish)

| ID | Outcome | Evidence |
|---|---|---|
| M01-P07 | The window shows its first view and every panel, and the documented keys act (fixes a blank window after Ctrl+N) | red-then-green capture test on a virtual display, three captures; accepted at G3 and merged on 2026-11-11 |
| M01-P07a | Comparison of lightweight Rust UI toolkits against the hand-drawn client on the unchanged 50 MiB budget (approved in chat on 2026-10-25) | comparison report accepted at G3 and merged on 2026-11-11 |
| M01-P08 | Acceptance pack: clean public clone rebuilds `lico 0.1.0` identically; cold 449-test suite cut from `179.3 s` to `92.9 s`; one index maps every M01 "Done when" item to its evidence; updated owner end-to-end guide | two-clone receipt, `suite acceptance` pass, cold ledger, [report](../../../../reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md); accepted at G3 and merged on 2026-11-12 |

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
its outcome can be inspected on screen. G3 is asked every two phases (end of M02-P02 together with M02-P01, then M02-P04, M02-P06 and M02-P07); since 2026-10-27 G4 is asked for each finished phase on its own.

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

G1 milestone approval, G2 phase-plan approval, G3 inspection acceptance and G4 merge approval stay with the project owner. On 2026-11-11 the owner accepted G3 and approved G4 merge and publication through M02-P07 (`milestone/M02-agentic-coding-basics`, with M01-P01 to M01-P07a also accepted). Phase branches are kept, and a milestone gets a branch `milestone/Mxx-<name>` at its closing commit (CONTRIBUTING.md). Publication (push) needs the owner's explicit yes for the exact list. This document grants none of them.
