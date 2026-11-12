# M07-P03 Report — Full-pagination `session.past` discovery, one-click `claude`/`codex` resume, and `lico 0.2.3` release

- Status: done (owner G3 accepted and G4 merged on 2026-11-13)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Plan: [M07-P03 plan](../../../plans/202611/20261112/20261112_13_M07-P03_past_sessions_and_0_2_3_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M07-P03-past-sessions-and-0-2-3`
- Inspection bundle: `202611/20261112/00016_M07-P03_past_sessions_and_0_2_3/`

## Summary

`M07-P03` completes Milestone `M07` (`lico 0.2.3` patch release) by adding
full-pagination past-session discovery (`session.past`, zero top-N truncation)
and one-click session resume (`resume_id` on `session.create` plus in-place
focus/reconnect of open or closed sessions) across `claude`, `codex`, and
registered `lico` sessions in working-directory (`cwd`), `Global`, and
`[ssh: <host>]` scopes. Every capability is exposed identically across Unix
socket IPC, CLI (`lico session past`, `lico session create --resume`), loopback
HTTP (`GET /v1/sessions/past`, `POST /v1`), and the left sidebar's `[ Past ]`
tab in `lico gui`.

## Delivered changes

1. **S02 — Full-pagination `session.past` & `resume_id` transcript seeding across host, IPC, CLI, HTTP, and protocol (`src/host/hostd/src/past/{mod.rs,seed.rs}`, `src/host/hostd/src/{unified.rs,accounts.rs,claude.rs,codex.rs,core.rs,admin.rs,ssh_remote.rs,folders.rs,ipc.rs,http.rs,cli.rs}`, `src/lico_protocol/v1.py`, `tests/testmap.toml`)**:
   - Added `past/mod.rs` (`662` lines) and `past/seed.rs` (`380` lines) scanning local managed account homes (`<accounts_dir>/<id>/claude/projects/*/*.jsonl` and `<accounts_dir>/<id>/codex/sessions/**/*.jsonl`, falling back to unbound host provider homes only when `!accounts.any_for(provider)` so account isolation holds), SSH remote account homes over bounded `ssh_remote::run_ssh_bounded` (`4 s` hard wall-clock bound), and registered `shared.sessions` via `O_RDONLY | O_NOFOLLOW | O_NONBLOCK` + regular-file `fstat`.
   - Matched `cwd` (canonical folder or `worktree_repo`) or `global: true` (`cwd` inside `workspace_root` / remote `.local/state/lico/workspaces`), supported case-insensitive `query` filtering, deduplicated entries while attaching `open_session` and `open_state` (`"running"` / `"disconnected"` / `"exited"`), sorted deterministically by `(updated_at DESC, provider_session ASC)`, and paginated via `offset`, `limit`, `total`, and `has_more` with zero top-N truncation.
   - Added `resume_id` on `session.create` (`claude --resume <id>` and Codex `thread/resume` with `threadId`), pre-seeding recent sanitized user (`>> ...`) and assistant turns plus `auto_title` via `seed_resumed_transcript`.
2. **S03 — GUI `[ Past ]` tab multi-page navigation, filtering, scope sync, and 1-click resume + `0.2.3` bump (`src/host/hostd/src/gui/{sidebar_past.rs,mod.rs,controls.rs,panel.rs,ui.rs,sidebar.rs,sidebar_render.rs}`, `Cargo.toml`, `Cargo.lock`, `tests/{test_control_parity.py,test_single_executable.py}`, `doc/manual_end_to_end.md`)**:
   - Added `gui/sidebar_past.rs` (`628` lines) rendering 2-line past-session cards (`C·` Claude and `X·` Codex badges, derived title, `[open]` / `[closed]` / `[ended]` status pills, and `provider · account · #short_id · relative_age` subtitle), `< Page X/Y (Total) >` + mouse-wheel pagination across all pages, `Filter past sessions...` search box, `Global (general)` / active working-directory / `Host: local v` (`ssh:<alias>`) scope controls, and 1-click activation (focuses `running` tab, calls `resume_session` on `disconnected` tab, or opens a new session tab via `AsyncJob::PastResume` `session.create` with `resume_id` and pre-seeded transcript).
   - Bumped `lico` version from `0.2.2` to `0.2.3`.
3. **S04 — Integration verification, release gates, and manual inspection (`tests/test_past_sessions_pagination_and_resume.py`)**:
   - Added `tests/test_past_sessions_pagination_and_resume.py` (`520` lines) verifying 35+ seeded past sessions across Unix socket, CLI, HTTP `GET /v1/sessions/past`, HTTP `POST /v1`, and `lico gui` `[ Past ]` multi-page traversal with zero truncation, query filtering, `Global` and SSH `gpu-box` scopes, `[closed]` in-place reconnect, `[open]` tab focus, and 1-click external Claude (`--resume`) and Codex (`thread/resume`) resume with pre-seeded conversation history.

## Measured verification (`lico 0.2.3`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `589 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `479` tests in `92` files, `0` failed, **`102.1 s`**, 4-worker harness tree peak **`1,664.3 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.82 MiB` (`24.68..24.82 MiB`), median PSS `17.41 MiB`, anon `3.94 MiB`, host `7.51 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.98 MiB` (`35.77..35.98 MiB`), median PSS `23.01 MiB`, anon `3.94 MiB`, host `7.65 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.43 MiB` (`24.96 MiB` -> `25.21 MiB`; host `8.83` -> `20.43 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.3` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`core.rs` / `ui.rs` / `panel.rs` / `past/mod.rs` / `past/seed.rs` / `sidebar_past.rs` / `sidebar.rs` / `sidebar_render.rs`) | `984` / `1,092` / `797` / `662` / `380` / `628` / `770` / `541` lines | `<= 988` / `<= 1,120` / `<= 800` | Met |

## Gate status

All three phases of Milestone `M07` (`M07-P01` through `M07-P03`, `lico 0.2.3`)
are implemented, measured, and installed to `~/bin/lico` for live testing,
awaiting owner G3 inspection acceptance and G4 merge/publication approval.
