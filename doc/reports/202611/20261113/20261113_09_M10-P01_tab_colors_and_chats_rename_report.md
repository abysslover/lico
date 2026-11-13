# M10-P01 Report — Tab Colors and the Full `past` -> `chats` Rename

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M10-P01-tab-colors-and-chats-rename` (implementation commit `597cf66`)
- Plan: [M10-P01](../../plans/202611/20261113/20261113_09_M10-P01_tab_colors_and_chats_rename_plan.md)

## What changed

- **Tab colors** (`gui/sidebar_tabs.rs`, used by `gui/sidebar_render.rs`): Directories blue, Chats amber, Calls teal, each with its own bar, active fill, and a faint own-hue tint when inactive. Labels come from one list (`Directories`, `Chats`, `Calls`) that the UI dump also exposes (`sidebar.tab_labels`).
- **Rename, no alias**: label; wire op `session.past` -> `session.chats` (`core.rs`, `ipc.rs`, `lico_protocol/v1.py`); HTTP `GET /v1/sessions/past` -> `/v1/sessions/chats` (`http.rs`, `admin.rs` capabilities); CLI `lico session past` -> `lico chat [list]`; module `past/` -> `chats/`, `gui/sidebar_past.rs` -> `gui/sidebar_chats.rs` (moved with `git mv`), types `PastItem`/`PastEntry`/`PastSidebarState`/`SidebarPast`/`PastResume`/`SidebarPastEntry` -> `ChatItem`/`ChatEntry`/`ChatsSidebarState`/`SidebarChats`/`ChatResume`/`SidebarChatEntry`, UI dump keys `past_*` -> `chats_*`, tab value `"past"` -> `"chats"`, user strings in `gui/confirm.rs` and the sidebar, `tests/testmap.toml`, the renamed test file `tests/test_chats_pagination_and_resume.py`, and `doc/manual_end_to_end.md`.
- **Build mismatch**: the CLI turns `unknown_op` into one stderr line (`this host (build X) does not know OP; this lico is build Y; run lico host stop and retry`), keeps the host's JSON reply on standard output, exits 2. The sidebar shows a sentence naming the same remedy instead of the bare code. An already installed old binary cannot be changed.
- Left as written (history): dated reports, milestone and plan records of M07 to M09, older CHANGELOG entries, `doc/index_*.md` rows. The new CHANGELOG entry comes with P04.

## Evidence (as measured)

| Check | Result |
|---|---|
| Tab palette unit tests (`gui::sidebar_tabs`) | 5 passed: three hues (blue 207, amber 38, teal 177 degrees), inactive tint within 20 degrees of the bar hue, contrast of every label on its fill `>= 4.5` |
| `tests/test_chats_rename.py` | 4 passed: no retired name in `src`, `tests`, README, CONTRIBUTING, manual, design docs; host answers `session.past` with `unknown_op` and lists only `session.chats`; CLI mismatch sentence against a fake host of another build; `lico session past` is a usage error, `lico chat` and `lico chat list` work |
| Full changed suite on the staged tree | 496 tests in 97 files, 496 passed, 109.2 s, tree peak 2,012.1 MiB (`<= 2,048`) |
| Screenshots (private Xvfb, mocked providers, no billable turn) | three tab states in `private/manual_inspection/202611/20261113/00005_M10-P01_tab_colors/` (local only) |

## Honest notes

- The Python tests of this phase were written before the rename but were not run red on their own; the Rust palette tests were run red (compile failure) first.
- The first full run used the system Python without `pytest` (0 tests ran); it was repeated with the repository's `private/venv`. That second run had 3 failures: a CLI call name left in the pagination test, the new test file missing from `tests/testmap.toml`, and `test_call_rooms_and_mentions` (an index error on the Call Room jump card), which passed when run again alone and in the final full run; I did not find its cause and treat it as a load-dependent flake to watch. That second run also reported a peak of 2,224.8 MiB, above the 2,048 MiB bound; the final run measured 2,012.1 MiB. The two peaks differ by 212 MiB for the same code, so the margin is thin; P04 measures the cold run.
- The mismatch sentence is tested for the CLI against a fake host and for the sidebar as a unit; the real old-binary against new-host pair was not exercised.

## Next

P02 (resume core) on a branch stacked on this tip. M10 is approved to run P01 to P04 in one stretch; G3 and G4 stay with the owner.
