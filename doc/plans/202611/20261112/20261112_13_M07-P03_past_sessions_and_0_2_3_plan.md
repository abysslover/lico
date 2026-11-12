# M07-P03 Plan — Full-pagination `session.past` discovery, one-click `claude`/`codex` resume, and `lico 0.2.3` release

- Status: in progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Branch: `phase/M07-P03-past-sessions-and-0-2-3`
- Approval: G1 and G2 approved by the owner on 2026-11-12

## Goal

Deliver full-pagination discovery (`session.past`, zero top-N truncation) and
one-click resume (`resume_id` on `session.create` + focus/reconnect of open
sessions) for all past `claude` and `codex` conversations associated with a
working directory (`cwd`) or `Global` scope across local and `[ssh: <host>]`
accounts, expose it identically across IPC, CLI (`lico session past` and
`lico session create --resume`), loopback HTTP (`GET /v1/sessions/past`), and the
left sidebar's `[ Past ]` tab in `lico gui`, bump the patch version `0.2.2` ->
`0.2.3`, run all release gates (`suite run --cold`, `idle_memory`, `hidden_growth`,
two-clone `suite acceptance`), and atomically install `~/bin/lico` `0.2.3`.

## Steps

1. **S01 — Plan & governance wiring (done in this commit)**:
   - Record `20261112_13_M07-P03_past_sessions_and_0_2_3_plan.md` and link it in
     the `M07` milestone and `doc/index_4.md`.
2. **S02 — `session.past` & `resume_id` across host, IPC, CLI, HTTP, and protocol**:
   - Add `src/host/hostd/src/past.rs` (`<= 650` lines; registered in `unified.rs`
     and `tests/testmap.toml` as `past = ["host"]`) implementing:
     - Safe `O_RDONLY | O_NOFOLLOW | O_NONBLOCK` + regular-file `fstat` scanning
       of local managed account homes (`<accounts_dir>/<id>/claude/projects/*/*.jsonl`
       and `<accounts_dir>/<id>/codex/sessions/**/*.jsonl`, or unbound host
       provider home only when `!accounts.any_for(provider)` so account isolation
       is preserved), SSH remote account homes over bounded `ssh_remote::run_ssh_bounded`,
       and registered `shared.sessions`.
     - Exact matching on `cwd` (canonical path or `worktree_repo`) or `global: true`
       (`scratch == true` or `cwd` inside `workspace_root` / remote `.local/state/lico/workspaces`),
       optional case-insensitive `query` filtering, deduplication linking
       `open_session` and `open_state`, deterministic sorting `(updated_at DESC, provider_session ASC)`,
       and exact `total` / `offset` / `limit` / `has_more` pagination with zero
       top-N cap.
     - `seed_resumed_transcript` reading recent user/assistant turns from the
       provider `.jsonl` record (local or SSH), sanitizing lines, seeding the
       `Session` log, and deriving `auto_title` when unnamed.
   - Wire `"session.past"` and `"resume_id"` across `core.rs` (`OPS` on line 25,
     `+0` lines), `admin.rs` (`GET /v1/sessions/past`), `ipc.rs`, `http.rs`
     (before `["v1", "sessions", id]`), `cli.rs` (`lico session past` and
     `--resume`), `folders.rs`, `ssh_remote.rs`, `claude.rs`, `codex.rs`, and
     `src/lico_protocol/v1.py`.
3. **S03 — GUI `[ Past ]` tab pagination, filtering, and one-click resume + `0.2.3` bump**:
   - Add `src/host/hostd/src/gui/sidebar_past.rs` (`<= 450` lines) wired into
     `gui/mod.rs`, `sidebar.rs`, `sidebar_render.rs`, `controls.rs`, and
     `panel.rs` (`panel.rs <= 800` lines):
     - Non-blocking `AsyncJob::SidebarPast` loading of `session.past` for the
       current `[ Past ]` scope (`Global` vs `(ssh_host, cwd)`), synced on tab
       switch, session switch, scope click, filter edits, `< / >` buttons, and
       mouse wheel across all `total` sessions (`Page X/Y (N)`).
     - One-click activation on any past session row: focuses (and reconnects if
       disconnected) an already-open `lico` session, or starts a new session tab
       with `resume_id` (`--resume` / `thread/resume`) and pre-seeded transcript.
   - Bump `0.2.2` -> `0.2.3` in `Cargo.toml`, `Cargo.lock`, tests, and docs.
4. **S04 — Full verification, memory & acceptance gates, inspection `00016`, and `~/bin/lico` install**:
   - Add `tests/test_past_sessions_pagination_and_resume.py` (`>= 35` seeded past
     sessions verifying full multi-page pagination with zero truncation across
     socket/CLI/HTTP/GUI, query filter, Global + folder + SSH scopes, open-tab
     focus, and external Claude/Codex 1-click resume with seeded transcript).
   - Run Rust unit tests under `umask 022` and `umask 077`, full `suite run --cold`,
     `idle_memory` (scales 1 & 2 `<= 50.0 MiB`), `hidden_growth` (`<= +10.0 MiB`),
     visual inspection bundle `00016`, report `20261112_13`, two-clone
     `suite acceptance`, and atomic install of `~/bin/lico` `0.2.3`, then stop at
     G3/G4.
