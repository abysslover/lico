# Lico

a lightweight workbench for agentic workflows

Lico is a proposed desktop workbench for persistent command-line agents. The
session host owns execution and output independently of the interface, so
closing or hiding a view should let work continue. Hidden views are intended to
release presentation resources and return from a snapshot plus later events.

Linux comes first; Windows follows through explicit platform interfaces. The
renderer, implementation language and integration strategy are selected from
measurements recorded in the first milestone's reports and accepted by the owner
on 2026-11-11.

## Current status

This repository holds the [product requirements](doc/product/202610/20261023/00001_product_spec/product_spec.md), the milestones,
the phase plans and reports, and the source of the finished phases. Implementation is written on reviewed phase branches. Since
2026-10-27 each finished phase reaches `main` on the owner's merge approval (G4); the phase branches are
kept, and a branch `milestone/Mxx-<name>` marks a milestone once it is achieved (see [CONTRIBUTING.md](CONTRIBUTING.md)). On
2026-11-11 and 2026-11-12 the owner accepted G3 and approved G4 merge and publication through M04-P04 (`milestone/M01-measured-linux-foundation`, `milestone/M02-agentic-coding-basics`, `milestone/M03-admin-gui-and-control-parity`, and `milestone/M04-workspace-workbench-and-conversation-ux`, `lico 0.2.0`). Resource budgets are targets unless a report marks a number
as measured.

The first milestone, the [measured Linux foundation](doc/milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md), is complete (`milestone/M01-measured-linux-foundation`, accepted and published on 2026-11-12).
The second milestone, [M02 Agentic-coding basics](doc/milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md), is complete (`milestone/M02-agentic-coding-basics`, accepted and published on 2026-11-11):
sessions that run in a real folder or git worktree, sending between sessions, Lico tools that an agent can call, screen capture and clipboard, computer use,
attention marks, notifications, conversation search, and per-session memory in the window, verified against the limit of an idle window under 50 MiB (`24.01..24.18` / `35.14..35.23 MiB`).
The third milestone, [M03 Linux administrator GUI completeness and control-surface parity](doc/milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md), is complete (`milestone/M03-admin-gui-and-control-parity`, accepted and published on 2026-11-12 at `lico 0.1.0`, idle window `23.96..24.21` / `35.08..35.38 MiB`, 10->100 session hidden growth `+0.37 MiB`): in-window account rename, session rebind and live permission-mode toggle, workspace-grouped session navigation and `12..=20` px font sizing, multi-line prompt mouse/keyboard selection, cut/copy, undo/redo and resize reflow, and `capabilities` + loopback HTTP parity (`POST /v1` and REST routes).
The fourth milestone, [M04 Workspace workbench, clean conversation UX, weekly quota visibility, and directory file viewer](doc/milestones/202611/20261112/20261112_01_M04_workspace_workbench_and_conversation_ux_milestone.md), is complete (`milestone/M04-workspace-workbench-and-conversation-ux`, accepted and published on 2026-11-12 at `lico 0.2.0`, idle window `24.18..24.42` / `35.29..35.50 MiB`, 10->100 session hidden growth `+0.38 MiB`): direct left-sidebar session switching while panels are open, derived session titles, active-first reverse-chronological sorting, clickable action toolbar, clean conversation view hiding internal control markers by default with user/agent color differentiation, provider weekly (`7d`) quota remaining display, and an on-demand (`F5`-refreshed) working-directory file viewer with `+ New here` and external `.html` browser launch.
The fifth milestone, [M05 Working-directory restore across restarts, paged directory navigation, and unified file opening](doc/milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md) (`M05-P01` to `M05-P03`, `lico 0.2.1` patch candidate, awaiting owner G3/G4), adds automatic working-directory and workspace-view restoration across `lico` restarts, top-anchored panel rendering with `Page X / Y` / `< Prev` / `Next >` / mouse-wheel navigation up to `10,000` directory entries, and unified file activation opening text-family files directly in-window while delegating web, document, image, media, archive, and binary files to the OS default application (`xdg-open`).
Its [planning set](doc/product/202610/20261026/00001_agentic_basics_plan/planning.md) gives the M02 scope and order, and the
[roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md) shows where each milestone stands.

The program is one executable, `lico` (`0.2.0` on `main`); its build is described in [src/host/README.md](src/host/README.md) and end-to-end verification is in [doc/manual_end_to_end.md](doc/manual_end_to_end.md). Two clean clones build byte-for-byte identical release binaries (`python3 -m lico_measure.suite acceptance`, [M01-P08 report](doc/reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md), [M03-P04 report](doc/reports/202611/20261111/20261111_09_M03-P04_control_parity_and_review_report.md), and [M04-P04 report](doc/reports/202611/20261112/20261112_04_M04-P04_workspace_file_viewer_report.md)). The build works from a public clone with
documented, publicly available dependencies.
Provider tools are independently installed unless redistribution is permitted.

## Project map

- [Documentation](doc/README.md)
- [Current status and approval gates](PLAN.md)
- [Contributing](CONTRIBUTING.md)
- [Experiments](exp/README.md)
- [Source](src/README.md) and [tests](tests/README.md)

## Authentication and accounts

The intended design uses official provider authentication only. Application-owned
secrets belong in the operating system credential store, with no plaintext
fallback. Accounts for the same provider must have separate credentials,
refresh state and session bindings. A default-account change must not retarget
running work, and an error must not silently switch accounts or billing modes.
The phases that implement them (M01-P04 and M01-P06) were accepted by the owner on 2026-11-11.

## License

[MIT](LICENSE). Provider names identify integrations and do not imply endorsement.
