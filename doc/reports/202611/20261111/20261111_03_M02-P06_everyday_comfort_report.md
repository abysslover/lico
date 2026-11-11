# M02-P06 report - everyday comfort: git worktree sessions, attention marks, desktop notifications, and conversation search

- Version: 0.1.0
- Status: in_review (inspection verdict G3 pending; G3 falls here at the end of M02-P06 and covers M02-P05 and M02-P06 together)
- Date: 2026-11-11
- Plan: [M02-P06](../../../plans/202611/20261111/20261111_02_M02-P06_everyday_comfort_plan.md) (version 1.1.0; Deviations D1 to D5 record every as-built refinement)
- Milestone: [M02](../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md)
- Branch: `phase/M02-P06-everyday-comfort`, plan and implementation commits at `7ef7c6c` (before this closing report and status commit) cut at `main` `9700012`; nothing is pushed. Inspection bundle `private/manual_inspection/202611/20261111/00003_M02-P06_everyday_comfort/` (5 window captures, warm `--cold` suite log, `umask 022`/`077` Rust logs, 6 clean-tree `idle_memory` logs, and 10-session search/host/window measurements).
- Tested tree: `7ef7c6c` (`lico 0.0.0 (commit 7ef7c6c, build de1142372c2f3364)`). This closing commit touches `doc/`, `PLAN.md` and `README.md` only.
- Gates: G1 and G2 approved 2026-10-26. G3 pending (covers M02-P05 and M02-P06 together). G4 local merge to `main` authorized by the owner on 2026-11-11; nothing is published.
- Changes in 0.1.0: first version.

## What was built

People running several sessions side by side now have git worktree isolation (`WT-01`, `WT-02`), background attention marks and optional `notify-send` desktop notifications (`NT-01`, `NT-02`), and zero-resident-index conversation search inside the open session or across every session (`SR-01`, `SR-02`), all inside the single `lico` executable with full parity between `lico gui` (for the administrator) and the `lico` CLI (for agents and scripts).

- **S01 `7ef7c6c`: git worktree sessions and two-step dirty removal (`worktree.rs`, `folders.rs`, `lifecycle.rs`, `core.rs`, `cli.rs`, `gui/newsession.rs`, `gui/confirm.rs`, `WT-01`, `WT-02`, `SF-07`, D2).** Added socket op `folder.git` and `worktree_branch` on `session.create` (`lico session create --cwd REPO --worktree BRANCH`, or `[x] start in a new git worktree` + `Branch` on the `Ctrl+N` screen when the folder is a git repository). Worktrees are created under `<workspace_root>/worktrees/<session_id>` with strict branch-name validation (rejecting leading `-`, `..`, `@{`, `.lock`, trailing `/`, control/whitespace/hidden characters, and existing refs), while `recent_folders.json` remembers the parent repository root (`worktree_repo`). Session rows show `(wt)`. Closing or deleting a session with an existing worktree asks whether to **Keep** (`K` / `--keep-worktree`, socket default) or **Remove** (`R` / `--remove-worktree`); if `git status --porcelain --untracked-files=normal` reports uncommitted or untracked work (`worktree_dirty`), `lico gui` opens a second confirmation (`D` remove anyway / `K` keep / `N` cancel; CLI `--force-dirty`). Removing a worktree never deletes the git branch and never touches the parent repository's working tree (`SF-07`).
- **S02 `7ef7c6c`: attention marks, window-title hints, and privacy-preserving desktop notifications (`notify.rs`, `clipboard/settings.rs`, `claude.rs`, `codex.rs`, `pending.rs`, `gui/attn.rs`, `gui/menu.rs`, `NT-01`, `NT-02`, `SF-02`, `SF-03`, D1, D3).** `Session::finished_seq` records completed provider turns and never fires on `>> ` input echo lines. In `lico gui`, background sessions with an unseen completed turn display `•`, sessions waiting on an approval display `!`, viewing a session clears its unread mark, and the X11 window title appends pure-ASCII ` [approval]` or ` [finished]` after `Lico <version>` (preserving `^Lico( [0-9]|$)` discovery, D1). When no GUI view is attached to that session, `notify.rs` invokes `notify-send --app-name=Lico` (when `DBUS_SESSION_BUS_ADDRESS` is present, or `LICO_NOTIFY_BIN` in tests, with bounded background reap and zero orphan processes) carrying only the sanitized session label (`'<label>' finished` or `'<label>' needs approval` - never message text, commands, or sign-in codes). Toggleable live via `View > Desktop notifications: on/off` and `lico settings [--notifications on|off]` in `0600` `settings.json`.
- **S03 `7ef7c6c`: on-demand single-session and cross-session conversation search (`search.rs`, `gui/search.rs`, `gui/ui.rs`, `cli.rs`, `SR-01`, `SR-02`, D4).** Socket op `session.search` and CLI `lico search <QUERY> [--session ID] [--limit N]` stream `.seg` files one segment at a time outside the session lock, matching case-insensitively across ASCII and UTF-8 (including Korean) with zero resident index in `lico-host` or `lico gui`. In `lico gui`, `Ctrl+F` (`View > Find in conversation`) searches the open conversation without disturbing the per-session input draft, highlights matched lines in amber, and steps forward/backward with `Enter` / `Shift+Enter`; `Ctrl+Shift+F` (`View > Search all sessions`, or `Tab` inside the search bar) searches across all non-purged sessions, switches to the target session, loads older history around the matched `seq` when outside the tail snapshot, and scrolls directly to that line.
- **S04 `7ef7c6c`: file-size split and window-harness settle cleanup (`core/tests.rs`, `gui/update.rs`, `src/lico_measure/captures.py`, `TST-01`, `TST-07`, D5).** Extracted `core/tests.rs` (`core.rs` shrank from `1,087` to `988` lines), `gui/attn.rs`, and `gui/update.rs` (`gui/mod.rs` shrank from `794` to `744` lines; `gui/ui.rs` is `1,172` against `1,185`), and tightened non-IME `ProductWindow` key/type settle pauses so the growing Cold suite runs in `172.3 s`.

## Tests and evidence

- **TST-10, five lines.**
  - **Whole suite from scratch, `--cold`, warm Cargo cache, at `7ef7c6c`: 448 tests in 75 files on 3 workers, 448 passed, 0 skipped, 172.3 s against the 180 s line (exit 0).** Build skipped (binary matched `7ef7c6c`); `cargo test --bin lico` took 15.0 s inside the run. Machine load average: 11.93 at start, 9.28 at end.
  - **Whole Rust unit suite under `umask 022` and `umask 077` at `7ef7c6c`:** 560 passed, 0 failed, 1 ignored (`property_driver`, run by its parent test) under `umask 022` (13.48 s) and 560 passed, 0 failed, 1 ignored under `umask 077` (13.71 s).
  - **Usual run (`--changed`):** 0.5 s for documentation changes (`test_no_hidden_characters.py`); 2.5 s for `tests/test_everyday_comfort.py` mapped from `worktree`, `notify`, `search`, `gui/attn.rs`, and `gui/search.rs` in `tests/testmap.toml`.
  - **Peak memory** (all workers together, bound 2,048 MiB): 1,525.1 MiB in the warm `--cold` run at `7ef7c6c`.
  - **Number of tests and what grew** at `7ef7c6c` (vs `main` `9700012`): Python pytest items 446 to 448 (+2 end-to-end product tests in `tests/test_everyday_comfort.py` across 75 files); Rust unit tests 552 to 560 (+8 across `worktree`, `notify`, `search`, `clipboard::settings`, `gui::attn`, and `gui::search`). Diff `main..7ef7c6c`: 41 files changed, +2,446 / -392. Big files (`TST-07`): `core.rs` shrank 1,087 to 988 (allowlist 1,087), `gui/ui.rs` 1,172 (allowlist 1,185), `gui/mod.rs` shrank 794 to 744 (under 800); new Rust modules are 112 to 486 lines.

## Measurements

- **MB-04 summary line:** Idle window RSS (`--text --realistic-text --ui --menu --accounts 10`, 3 runs of 3 launches, clean tree `7ef7c6c`): **scale 1: 23.98, 24.03, 24.00 MiB** (PSS median 16.88, 16.84, 16.79 MiB; `RssAnon` 3.94, 3.94, 3.75 MiB; limit 50.0 MiB, met); **scale 2: 35.07, 35.20, 35.11 MiB** (PSS median 22.46, 22.45, 22.43 MiB; `RssAnon` 3.94, 4.12, 3.94 MiB; limit 50.0 MiB, met). **Idle host RSS (10 Claude sessions, 1,210 sequenced records):** **7.24 MiB** (`7,416 KiB`, PSS 5.81 MiB, `RssAnon` 1.05 MiB, 26 threads, 38 fds) before search and **7.24 MiB** (`7,416 KiB`, **`+0 KiB` RSS delta, `+0 KiB` `RssAnon` delta, `+0` fds, `+0` children**) after 20 cross-session `session.search` queries (**0.28 ms median**, 0.47 ms max), proving zero resident search index (`SR-02`). **Feature memory in `lico gui`:** opening `Ctrl+F` search adds **+0.19 MiB** (`20,796` to `20,988 KiB`, `RssAnon` `2.43` to `2.62 MiB`); git worktree creation and `notify-send` use short-lived child commands with `0` standing processes.

## Captures

Saved in `private/manual_inspection/202611/20261111/00003_M02-P06_everyday_comfort/`:
1. `p06_1_new_session_worktree.png`: `Ctrl+N` new-session screen on a git repository with `[x] start in a new git worktree` and `Branch` `feat/p06-demo`.
2. `p06_2_worktree_dirty_confirm.png`: second confirmation panel (`Uncommitted changes in worktree`) when closing a dirty worktree session (`D` remove anyway / `K` keep / `N` cancel; git branch preserved).
3. `p06_3_attention_marks.png`: unread dot `•` on background session `bg-review`, window title `[finished]` suffix, and `View > Desktop notifications: on`.
4. `p06_4_search_conversation.png`: `Ctrl+F` in-conversation search bar highlighting matches with `Enter` / `Shift+Enter` navigation.
5. `p06_5_search_all_sessions.png`: `Ctrl+Shift+F` cross-session search jumping directly to `DEPLOY_CHECK_OK` in `bg-review`.

## Security

- **Worktree safety and notification privacy (`WT-01`, `WT-02`, `NT-02`, `SF-02`, `SF-03`, `SF-07`):** branch names are validated before invoking `git worktree add -b` (argument separator and ref rules enforced; `Command` with explicit argv, never a shell). Worktrees live only inside `<workspace_root>/worktrees/<session_id>`; automated `session.close` / `session.purge` calls default `worktree` to `"keep"`, dirty removal requires explicit `force_dirty = true` (`D` in the second GUI prompt), the branch ref is never deleted, and the parent repository's working tree is never modified. Desktop notifications pass `--app-name=Lico` and only the sanitized session label (never transcript text, approval prompt commands, or login codes), require non-empty `DBUS_SESSION_BUS_ADDRESS` by default so headless `Xvfb` sessions never autolaunch orphan `dbus-daemon` processes, and respect `notifications: false` in `0600` `settings.json`.

## Try it yourself (for G3 - M02-P05 and M02-P06)

G3 falls at the end of M02-P06 and covers **M02-P05** (computer use on a private virtual display and gated real-screen control) and **M02-P06** (git worktree sessions, attention marks, desktop notifications, and conversation search) together. Run on an X11 display (`echo $DISPLAY`) using a private socket and temporary git repository so your own host and repositories stay untouched:

```bash
T=$(mktemp -d)
git -C "$T" init -b main demo-repo && echo "# demo" > "$T/demo-repo/README.md"
git -C "$T/demo-repo" -c user.name=demo -c user.email=demo@local add README.md
git -C "$T/demo-repo" -c user.name=demo -c user.email=demo@local commit -m "init"
lico host --socket "$T/host.sock" --state-dir "$T/state" --sessions 0 --producing 0 &
export LICO_SOCKET="$T/host.sock"
lico gui --socket "$T/host.sock"
```

1. **M02-P06 - start a git worktree session and check two-step dirty close (`WT-01`, `WT-02`):**
   - Press `Ctrl+N`, type `$T/demo-repo` as the folder, press `Tab`: Lico detects the git repository and enables `[ ] start in a new git worktree`. Press `Space` to check it, type `feat/try-wt` as `Branch` and `wt-worker` as `Name`, and press `Enter` (ensure an account is signed in via `Ctrl+A` first if needed).
   - Verify `(wt)` on the session row. Create an untracked file inside the worktree (`touch "$T"/state/workspaces/worktrees/*/draft.txt`), press `Ctrl+Shift+W`, press `R` (`Remove worktree`), and verify the second warning `Uncommitted changes in worktree` (`K` keeps the worktree; `D` removes the worktree while `git -C "$T/demo-repo" branch` still shows `feat/try-wt`).
2. **M02-P06 - attention marks, desktop notifications, and conversation search (`NT-01`, `NT-02`, `SR-01`, `SR-02`):**
   - Start a second session (`bg-session`), send it a prompt, and immediately switch back to the first session: when `bg-session` finishes or asks for approval, its row shows `•` or `!`, the window title shows `[finished]` or `[approval]`, and `notify-send` posts a desktop notification naming only `'bg-session'` (toggleable in `View > Desktop notifications: on/off` or `lico settings --notifications off`). Clicking `bg-session` clears `•`.
   - Press `Ctrl+F` to search inside the open conversation (`Enter` / `Shift+Enter` steps between highlighted matches; `Esc` closes without losing your input draft). Press `Ctrl+Shift+F` (or `Tab` in the search bar) to search across all sessions and jump directly to the matched line, or run `lico search "keyword"` on the CLI.
3. **M02-P05 - isolated virtual display and 15-minute real-screen gate (`CU-01` to `CU-06`):**
   - Open `Session > Virtual display: start` (or run `lico display --session <ID>` and `lico display run --session <ID> -- xclock`): `:N` appears on the session's status/menu without touching your real screen. Ask the Claude session to use `computer` (`action: "look"` or `click`) on its virtual display - zero approval prompts are needed for `"virtual"`.
   - Ask the Claude session to call `computer` with `target: "real"`, `action: "look"`: an approval card appears (`Shift+S` absent). Press `Y` after the 800 ms dwell: the crimson top bar `!! REAL SCREEN: ... MM:SS left [ Stop now ]` appears. Click `[ Stop now ]` or press `Ctrl+Alt+Esc` (or `lico real-screen stop`) to revoke immediately.
4. **Clean up:** `lico --socket "$T/host.sock" host stop --force && rm -rf "$T"`.
