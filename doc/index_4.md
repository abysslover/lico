# Documentation index, page 4

| Document | Current meaning |
|---|---|
| [M01-P05 plan](plans/202610/20261024/20261024_02_M01-P05_workflow_recovery_and_package_plan.md) | User-facing workflow, failure recovery, resource acceptance and a single self-contained executable; G2 approved |
| [Roadmap](product/202610/20261025/00001_roadmap/roadmap.md) | Where M01, M02, M03 and the later work stand; updated 2026-11-12, grants no approval |
| [M02 milestone](milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md) | Agentic-coding basics; G1 approved 2026-10-26 with the phase plans of G2 included; closed 2026-11-11 |
| [M02 planning set](product/202610/20261026/00001_agentic_basics_plan/planning.md) | Scope, order and decisions; its [requirements](product/202610/20261026/00001_agentic_basics_plan/requirements.md), [screens](product/202610/20261026/00001_agentic_basics_plan/screens.md), [test plan](product/202610/20261026/00001_agentic_basics_plan/test_plan.md) and [changes](product/202610/20261026/00001_agentic_basics_plan/changes.md) sit beside it |
| [M01-P08 plan](plans/202611/20261111/20261111_04_M01-P08_acceptance_pack_plan.md) | Clean public-clone build reproducibility, test-suite bottleneck optimization, and `0.1.0` version bump; done 2026-11-12 |
| [M01-P08 report](reports/202611/20261111/20261111_05_M01-P08_acceptance_pack_report.md) | Complete `M01` "Done when" evidence index, two-clone `sha256` match, `167.7 s -> 92.9 s` cold suite speedup, and `lico 0.1.0`; accepted at G3 and merged 2026-11-12 |
| [M03 milestone](milestones/202611/20261111/20261111_01_M03_admin_gui_and_control_parity_milestone.md) | Linux administrator GUI completeness and control-surface parity (`M03-P01` to `M03-P04`); accepted and closed 2026-11-12 |
| [M03-P01 plan](plans/202611/20261111/20261111_05_M03-P01_account_and_session_admin_plan.md) | In-window account rename, session rebind, and live permission-mode control; done 2026-11-12 |
| [M03-P01 report](reports/202611/20261111/20261111_06_M03-P01_account_and_session_admin_report.md) | `account.rename`, `session.mode`, and in-window `session.rebind` across GUI, CLI, IPC, and HTTP; accepted at G3 and merged 2026-11-12 |
| [M03-P02 plan](plans/202611/20261111/20261111_06_M03-P02_workspace_navigator_plan.md) | Persistent workspace-grouped session navigator and adjustable text size (`12..=20` px); done 2026-11-12 |
| [M03-P02 report](reports/202611/20261111/20261111_07_M03-P02_workspace_navigator_report.md) | Workspace grouping (rolling git worktrees under parent repos), `12..=20` px scaling, and `< 50 MiB` RSS at scales 1 & 2; accepted at G3 and merged 2026-11-12 |
| [M03-P03 plan](plans/202611/20261111/20261111_07_M03-P03_editor_and_reflow_plan.md) | Prompt editor modularization, mouse/keyboard selection, undo/redo, and resize reflow; done 2026-11-12 |
| [M03-P03 report](reports/202611/20261111/20261111_08_M03-P03_editor_and_reflow_report.md) | Split `gui/editor` into 4 modules (`ui.rs` down to `1,115` lines), mouse/keyboard selection, `Ctrl+C`/`Ctrl+X`, per-session undo/redo, and resize reflow; accepted at G3 and merged 2026-11-12 |
| [M03-P04 plan](plans/202611/20261111/20261111_08_M03-P04_control_parity_and_review_plan.md) | Control-surface `capabilities`, loopback HTTP parity (`POST /v1` + REST routes), full memory pass, and `0.1.0` release review; done 2026-11-12 |
| [M03-P04 report](reports/202611/20261111/20261111_09_M03-P04_control_parity_and_review_report.md) | `capabilities` across CLI/socket/HTTP, `POST /v1` + REST parity, `458/458` cold suite (`94.9 s`), `24.21`/`35.38 MiB` idle RSS, `+0.37 MiB` hidden growth, two-clone `0.1.0` match, and `~/bin/lico` install; accepted at G3 and merged 2026-11-12 |

Back to the [first index page](README.md), the [second](index_2.md) or the [third](index_3.md).
