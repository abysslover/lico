# M22-P01 Plan — `session.chats` Scope `all`: Every Past Session In One List

- Status: planned (2026-11-18); the lead proceeds on the owner's words in the chat once the milestone's defaults are not changed
- Version: 1.0.0
- Branch: `phase/M22-P01-chats-of-all-sessions` (from `main` `103d17e`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)

## What changes, and what does not

The window's Chats tab (P02) needs ONE list of every past session, newest first. Today `session.chats` answers for one folder (`cwd`) or for Global (`global`, the workspace root). This phase adds a third scope, `all`, to the host, so that the window, the CLI and REST read the same list and cannot disagree (the rule of `chats::collect`, which `chat latest` and `resume_latest` already share).

Nothing the window shows changes in this phase. `cwd` and `global` answer as before (their tests stay green and unchanged); no name on the wire is removed.

## The scope `all`

- **Request:** `{"op": "session.chats", "all": true}` with the same `offset`, `limit`, `query`, `account`, `recorded`. `all` with `cwd` or `global` is `bad_request` (one scope at a time). REST: `GET /v1/sessions/chats?all=1`. CLI: `lico chat list --all` (the existing chats command; the usage text says it).
- **What is listed:** the sessions of the host's own list (open, closed, ended, disconnected) and the records the Lico accounts' provider homes hold, as today, with NO folder filter. The provider's own default home (the M11 native home) is listed only when the owner has turned it on, as in the other scopes: this machine's own `~/.claude` holds 854 records of work Lico never opened, and they are not "sessions opened in Lico".
- **Each item** has what the other scopes give (`provider`, `provider_session`, `account`, `title`, `updated_at`, `open_session`, `open_state`, `turns`, `live`, `recorded`) and its own `cwd` (the real folder) and `global`: a record under the workspace root is `global: true` with no `cwd` (an Anon chat); any other has `global: false` and its `cwd`. The reply's `scope` is `"all"` and its `cwd` is null.
- **Order:** `updated_at` descending, then `provider_session`, then `account` (the same tie-break as today), so a page boundary never repeats or loses an item.
- **Bounded cost:** a scan of a whole home must not read every record. For `all`, a home's files are listed by modification time first (metadata only), and only the newest `ALL_SCAN_MAX` (a constant, 400 to start, measured in step 4) of each home are opened. The total in the reply says how many matched among those opened, and the reply says `"truncated": true` when files were left unread, so a short list never looks whole.
- **ssh hosts:** `all` is local. A session that runs on an ssh host appears through the host's own list (open sessions), as it does in every scope; the past records of a remote home stay per host (`ssh_host` with `cwd` or `global`), because listing a whole remote home is another cost to measure. `all` with `ssh_host` is `bad_request` for now and the plan of P02 says so on the card.

## Steps (tests first; green-only commits)

- [ ] 1. Red tests in the host's chats tests and one system test: two folders, an Anon session and an open session in one request; the order; a page boundary at `limit` 2 with a tie in `updated_at`; `query` and `account` filters; `recorded`; `all` with `cwd`, `global` or `ssh_host` is `bad_request`; the native home is absent unless on; the reply of `cwd` and `global` is unchanged byte for byte (golden). Read each test fail for the right reason.
- [ ] 2. `chats::collect`: the scope as an enum (`Folder`, `Global`, `All`) instead of the boolean `is_global`, so a fourth scope cannot be a boolean again; `scan_*_home` take the cap; the item's `global` and `cwd` by the workspace-root rule above. `chat latest` and `resume_latest` keep their scope (a folder or Anon) and pass.
- [ ] 3. The wire: request validation (`ipc.rs`), REST (`http.rs`), the CLI flag and the usage line of `chat [list]`, the protocol's request and response schemas in `src/lico_protocol/v1.py` (the request gets `all`, `scope` gets `all`, `truncated` is added) and the manual's chats paragraph, the parity test between the CLI, REST and the socket (`tests/test_control_parity.py`).
- [ ] 4. A measurement, once: the real scale on this machine, in a private home with 1,000 synthetic records over 40 folders (no real session's text is read or kept): the time of one `all` page cold and warm, the memory of the host, against the suite's budget; `ALL_SCAN_MAX` is set from it and the figure goes in the report.
- [ ] 5. `suite run --changed --receipts`, `cargo xtask check|verify|graph`; commit. A review of the diff by the lead (the fence: no path outside the homes is read; `real_dir` and `open_regular_nofollow` still guard every open).

## What follows (written as plans when the phase before is done)

P02 window A (two left tabs, Anon, Chats of all sessions with the jump, Calls out of the sidebar); P03 window B (path row, one-line top bar, tab bar); P04 the browser tab; P05 the release `0.8.0`.

## Stop and ask

Stop only if: the reply of `cwd` or `global` has to change for `all` to fit (a golden test goes red); the cost of one cold `all` page does not fit a 2 s budget even with the cap; or a step needs a push or a publication.
