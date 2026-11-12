# M07-P04 Plan — Cross-Provider Inter-Session Communication (`Claude` <-> `Codex`), Boxed Command Execution Cards, Claude `get_usage` Quota, Symlink Navigation, and `/exit` Tab Dismissal

- Milestone: `M07` (`lico 0.2.3`)
- Branch: `phase/M07-P04-session-comms-command-boxes-and-fixes`
- Gate: G2 approved 2026-11-12 (live owner G3 inspection feedback on `M07-P03`)

## Goal

Resolve all six usability and protocol gaps discovered during live inspection of `lico 0.2.3` while keeping `lico` single-executable and `< 50.0 MiB` idle GUI RSS:

1. **Local `/exit` (`/quit`) interception, clean tab dismissal (`×`), and slash-command `auto_title` protection**:
   - Intercept `/exit` and `/quit` locally in `lico gui` before reconnecting or dispatching prompt input so they dismiss the active session tab instead of triggering an LLM farewell turn.
   - Support `hide_tab: bool` on `session.close` (`tab_closed` state persisted in `sessions.json` and exposed in `status`), removing dismissed tabs from the top tab bar while keeping their conversations resumable from `[ Past ]` (and cleanly purging brand-new 0-turn tabs).
   - Skip `/...` slash commands in `derive_auto_title()` and heal legacy `/exit`-polluted `auto_title` entries from `.seg` prompt history on host load.
2. **IME-safe keyboard shortcuts (`physical_key` fallback) + conditional toolbar `Copy` (preserving explicit-copy X11 selection invariants)**:
   - Add physical-key (`KeyCode::KeyA..=KeyZ`) fallback when matching `Ctrl+<letter>` and `Alt+<letter>` shortcuts so `Ctrl+C`, `Ctrl+Shift+C`, `Ctrl+V`, `Ctrl+X`, `Ctrl+Z`, `Ctrl+Shift+S`, `Ctrl+N`, `Ctrl+B`, `Ctrl+F`, `Ctrl+R`, and `Ctrl+Q` work identically even when Hangul (`한/영`) or another non-Latin IME layout is active.
   - Preserve X11 explicit-copy invariants (dragging selects lines without overwriting `CLIPBOARD` or `PRIMARY`; `Ctrl+C`, `Ctrl+Shift+C`, and the conditional toolbar `Copy` button copy UTF-8 text to `CLIPBOARD`).
3. **Bordered command/tool execution cards and `[exit N]` styling**:
   - Classify Codex and Claude command/tool output lines distinctly from agent prose and render per-invocation command/tool blocks inside bordered dark cards (`CMD_BG`, left accent rail, mint `$` badge, slate-teal command stdout, and coral-amber `[exit != 0]` / `[tool error]`).
   - Paint incoming inter-session headers (`>> [from session ...]`) with a distinct teal peer-message banner, keeping incremental scroll `Shift` 100% pixel-identical to full repaint (`scroll_oracle`).
4. **Bidirectional `Claude` <-> `Codex` inter-session communication and clear GUI discoverability**:
   - Lift the `Claude Code sessions only` target restriction in `calls::send` so approved agent `send` calls work across both `claude` and `codex` provider sessions (preserving human approval cards, `20/min` rate limits, and `MAX_HOPS = 4` chain depth).
   - Connect local `codex app-server` sessions to `lico`'s loopback MCP tool server (`-c mcp_servers.lico.url=...`, `bearer_token_env_var="LICO_MCP_TOKEN"`, `shell_environment_policy.exclude=["LICO_MCP_TOKEN"]`), and map Codex `mcpToolCall` notifications into `[tool] ...` / `[tool error]` records.
   - Surface a dedicated **`Send -> Session`** (`Ctrl+Shift+S`) button in the top action toolbar (showing peer count when multiple sessions run) and bottom shortcut hints.
5. **Live `Claude` `5h` and `Wk` quota telemetry via `control_request` `get_usage`**:
   - Query `{"type": "control_request", "request": {"subtype": "get_usage", "skip_behaviors": true}}` on `claude -p --input-format stream-json` session spawn and after each turn `result` (alongside passive `rate_limit_event` handling) so normal `status: "allowed"` Claude sessions report `5h` (`short_remaining_pct`, `short_resets_at`) and `Wk` (`weekly_remaining_pct`, `weekly_resets_at`) just like Codex.
   - Parse Claude `get_usage` `0..100` `utilization` values and ISO-8601 `resets_at` timestamps in `quota.rs` without fabricating percentages when `rate_limits` is `null`.
6. **Follow safe symlink directories and symlink files in `[ Directories ]` and `Files`**:
   - Classify symlinks to directories as `"dir"` and symlinks to regular files as `"file"` (with target size and HTML/external detection) across both local (`fsview.rs`) and SSH remote (`ssh_remote.rs`) `folder.list`, keeping broken/special symlinks as `"link"`.
   - Follow symlinks to regular files in local and SSH `folder.read` after canonicalizing and verifying that neither the symlink nor its resolved target enters Lico's protected private state roots (`folder_protected`), opening resolved files with `O_NOFOLLOW | O_NONBLOCK` + `fstat`.

## Acceptance Criteria

1. Typing `/exit` or `/quit` in `lico gui` never reaches `claude` or `codex` as a prompt, never sets `auto_title` to `/exit`, dismisses the tab (`tab_closed: true`, or purges a 0-turn tab), and keeps conversation-bearing sessions resumable in `[ Past ]`.
2. `Ctrl+C` / `Ctrl+Shift+C` / `Ctrl+V` work under Hangul IME (`physical_key` fallback), toolbar `Copy` appears when lines/text are selected, and mouse drag-selection never clobbers `CLIPBOARD` or `PRIMARY`.
3. Command/tool invocations (`$ ...`, stdout, `[exit 1]`, `[tool] ...`, `[patch] ...`) render inside per-invocation bordered cards with distinct colors, incoming `>> [from session ...]` headers render with a peer banner, and `scroll_oracle` passes 100% pixel-for-pixel across scales `1.0`, `1.25`, and `2.0`.
4. Bidirectional `claude` <-> `codex` MCP `sessions` / `send` / `history` / `read_handoff` flows succeed with owner approval, and `Send -> Session (N)` is visible in the top toolbar.
5. `claude` sessions populate `5h` and `Wk` remaining percentages and reset countdowns in `status`, `account.list`, session headers, and the bottom telemetry strip immediately via `get_usage` without requiring an `>= 80%` rate-limit warning.
6. Clicking a symlink directory in `[ Directories ]` or `Files` enters it and clicking a symlink file previews (or externally opens) its target across local and SSH workspaces, while symlinks targeting Lico's private state directories are refused with `folder_protected`.
7. Cold test suite (`<= 180.0 s`), `idle_memory` (`<= 50.0 MiB`), `hidden_growth` (`<= +10.0 MiB`), and two-clone `suite acceptance` all pass, and the local `lico` binary (`0.2.3`) is updated.
