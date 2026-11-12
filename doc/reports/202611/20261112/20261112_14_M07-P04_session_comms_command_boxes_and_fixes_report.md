# M07-P04 Report — Cross-provider inter-session communication (`Claude` <-> `Codex`), boxed command execution cards, Claude `get_usage` quota, symlink navigation, and `/exit` tab dismissal

- Status: done (owner G3 accepted and G4 merged on 2026-11-13)
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M07 — Two-pane tabbed workbench, session telemetry bar, Global general sessions, and paginated past-session resume (`lico 0.2.3`)](../../../milestones/202611/20261112/20261112_04_M07_two_pane_workbench_and_past_sessions_milestone.md)
- Plan: [M07-P04 plan](../../../plans/202611/20261112/20261112_14_M07-P04_session_comms_command_boxes_and_fixes_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M07-P04-session-comms-command-boxes-and-fixes`
- Inspection bundle: `202611/20261112/00017_M07-P04_session_comms_command_boxes_and_fixes/`

## Summary

`M07-P04` resolves six live owner inspection items on `lico 0.2.3` while keeping
`lico` a single self-contained executable well inside its `< 50.0 MiB` idle GUI
RSS budget:

1. Local `/exit` and `/quit` interception, clean tab dismissal (`tab_closed`),
   blank 0-turn tab purge, and slash-command `auto_title` protection + `.seg`
   self-healing.
2. Hangul (`한/영`) / non-Latin IME-safe `Ctrl+` and `Alt+` keyboard shortcuts via
   `physical_key` fallback plus a conditional toolbar `Copy` action (preserving
   explicit-copy X11 `CLIPBOARD` and `PRIMARY` invariants).
3. Per-invocation bordered command/tool execution cards (`CMD_BG`, left accent
   rail, mint `$` badge, slate-teal command stdout, coral-amber `[exit != 0]` /
   `[tool error]`) and teal `>> [from session ...]` peer banners with exact
   `scroll_oracle` incremental-blit parity.
4. Bidirectional `claude` <-> `codex` loopback MCP inter-session communication
   (`sessions`, `history`, `send`, `read_handoff`) with human approval cards and
   a top-toolbar `Send -> Session (N)` button (`Ctrl+Shift+S`).
5. Live `claude` `5h` and `Wk` quota telemetry via `stream-json`
   `control_request` `get_usage` on spawn and turn completion (`0..100`
   `utilization` + ISO-8601 `resets_at`).
6. Safe symlink directory navigation and symlink regular-file preview across
   both local (`fsview.rs`) and SSH remote (`ssh_remote.rs`) workspaces while
   refusing symlinks into Lico's protected private state roots.

## Delivered changes

1. **Local `/exit` (`/quit`) tab dismissal & slash-command `auto_title` protection (`src/host/hostd/src/{session.rs,registry.rs,core.rs,lifecycle.rs,ipc.rs}`, `src/host/hostd/src/gui/{panel.rs,ui.rs,sidebar_past.rs,ui_tests.rs}`, `src/lico_protocol/v1.py`)**:
   - Intercepted `/exit` and `/quit` locally in `gui/panel.rs` (`send_line`) before auto-reconnecting or dispatching prompt text so neither provider receives `/exit` as a user turn.
   - Added `hide_tab: bool` on `session.close` and persisted `tab_closed` in `sessions.json` (`status` exposes `tab_closed`). Dismissing a 0-turn blank tab (`!Session::has_conversation()`) deletes it cleanly; dismissing a conversation-bearing tab removes it from the horizontal tab bar (`ui::open_tabs`), switches to an adjacent open tab, and preserves the conversation in `[ Past ]` (reopening on click, `session.resume`, `rebind`, or `input`).
   - Updated `derive_auto_title` (and `past/seed.rs`) to skip `/...` slash commands and added startup self-healing in `core.rs` (`load_saved_sessions`) to recover polluted `"/exit"` / `"/quit"` `auto_title` entries from `.seg` prompt history.
2. **IME-safe shortcuts & conditional toolbar `Copy` (`src/host/hostd/src/gui/{events.rs,panel.rs,toolbar.rs,ui.rs,clip.rs,clip_tests.rs}`, `src/host/hostd/src/gui/editor/app.rs`)**:
   - Added `physical_key` (`KeyCode::KeyA..=KeyZ`) fallback in `gui/events.rs` (`key_letter`, `key_letter_event`) across all `gui/*` panels and `editor/app.rs` so `Ctrl+C`, `Ctrl+Shift+C`, `Ctrl+V`, `Ctrl+X`, `Ctrl+Z`, `Ctrl+Shift+S`, `Ctrl+N`, `Ctrl+B`, `Ctrl+F`, `Ctrl+R`, and `Ctrl+Q` fire reliably when Hangul (`한/영`) IME produces Hangul syllables on `logical_key`.
   - Exposed a conditional `Copy` button in the top toolbar whenever transcript lines or prompt text are selected, keeping mouse-drag selection strictly visual without overwriting X11 `CLIPBOARD` or `PRIMARY`.
3. **Bordered command/tool execution cards & peer-session banner (`src/host/hostd/src/gui/{transcript.rs,scroll_oracle.rs,controls.rs}`)**:
   - Added stateful line classification (`LineKind`: `Prompt`, `PeerPrompt`, `Prose`, `CmdLead`, `CmdOut`, `CmdExitOk`, `CmdExitErr`, `ToolCard`, `Event`, `Error`) and card-boundary pairing (`CardEdge`: `Single`, `Top`, `Mid`, `Bottom`) so command and tool runs render inside recessed dark cards (`CMD_BG`) with a 1 px border (`CMD_BORDER` / `CMD_ERR_BORDER`), 3 px left accent rail, mint `$` prompt badge, slate-teal command stdout (`CMD_OUT_INK`), and coral-amber `[exit N]` (`N != 0`) / `[tool error]`.
   - Styled incoming inter-session headers (`>> [from session ...]`) with a distinct teal banner (`PEER_BG` + `PEER_RAIL` + `PEER_INK`) and verified pixel-for-pixel incremental `Shift` scroll parity across scales `1.0`, `1.25`, and `2.0` (`scroll_oracle`).
4. **Bidirectional `Claude` <-> `Codex` MCP communication & toolbar discoverability (`src/host/hostd/src/{calls.rs,codex.rs,transfer.rs,tools.rs}`, `src/host/hostd/src/gui/{toolbar.rs,sendto.rs,ui.rs}`, `src/lico_protocol/codex_mapping.py`, `src/host/adapters/codex/simulator.py`)**:
   - Lifted the `claude`-only target check in `calls::send` so approved agent `mcp__lico__send` calls deliver across both `claude` and `codex` sessions while keeping human approval cards, `20/min` rate limits, and `MAX_HOPS = 4` loop protection intact.
   - Wired Lico's loopback MCP server into local `codex app-server` launches via `-c mcp_servers.lico.url=...`, `-c mcp_servers.lico.bearer_token_env_var="LICO_MCP_TOKEN"`, and `-c 'shell_environment_policy.exclude=["LICO_MCP_TOKEN"]'` (so subprocesses spawned by Codex never inherit the MCP bearer token), mapped Codex `mcpToolCall` items to `[tool] ...` / `[tool error]`, and updated `transfer::notice` so Codex handoffs reference `read_handoff`.
   - Added `Send -> Session` (displaying live peer count `Send -> Session (N)` when multiple sessions are active) to the top toolbar alongside a `Ctrl+Shift+S` status hint.
5. **Live `Claude` `5h` & `Wk` quota via `get_usage` (`src/host/hostd/src/{claude.rs,quota.rs}`, `src/host/adapters/claude/simulator.py`)**:
   - Sent `{"type": "control_request", "request_id": "lico-usage-<n>", "request": {"subtype": "get_usage", "skip_behaviors": true}}` on `claude -p --input-format stream-json` spawn and after each `result` event (handling `control_response` before `s.heard()` so quota polling never perturbs idle silence tracking).
   - Parsed `get_usage`'s `0..100` `utilization` values (`claude_usage_pct_remaining`) and pure-Rust ISO-8601 `resets_at` timestamps (`parse_iso8601_ts`), populating `5h` and `Wk` remaining percentages and reset countdowns during normal `allowed` operation.
6. **Safe symlink directory & file following (`src/host/hostd/src/{fsview.rs,ssh_remote.rs,folders.rs}`, `src/host/hostd/src/gui/{sidebar.rs,files_ui.rs}`)**:
   - Updated local (`fsview::folder_list`) and SSH (`ssh_remote::folder_list`) directory listing to inspect symlink targets (`std::fs::metadata` / `os.stat(..., follow_symlinks=True)`), classifying symlinks to directories as `"dir"` and symlinks to regular files as `"file"` (with target size and external-open detection) while leaving broken or special-device symlinks as `"link"`.
   - Updated local (`fsview::folder_read`) and SSH (`ssh_remote::folder_read`) file readers to canonicalize symlink targets, reject links whose parent or resolved target enters Lico's private state roots (`folder_protected`), verify regular-file `fstat` (`file_not_regular`), and read resolved targets with `O_NOFOLLOW | O_NONBLOCK`.

## Measured verification (`lico 0.2.3`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `596 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `483` tests in `93` files, `0` failed, **`100.0 s`**, 4-worker harness tree peak **`1,724.9 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.64 MiB` (`24.48..24.64 MiB`), median PSS `17.15 MiB`, anon `3.94 MiB`, host `7.61 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.70 MiB` (`35.61..35.70 MiB`), median PSS `22.70 MiB`, anon `3.94 MiB`, host `7.67 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.36 MiB` (`24.66 MiB` -> `24.98 MiB`; host `8.70` -> `20.52 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.3` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`core.rs` / `ui.rs` / `transfer.rs` / `panel.rs` / `session.rs` / `codex.rs` / `ssh_remote.rs` / `claude.rs` / `calls.rs` / `fsview.rs` / `scroll_oracle.rs` / `transcript.rs` / `quota.rs`) | `979` / `1,107` / `792` / `791` / `779` / `724` / `691` / `580` / `511` / `510` / `495` / `469` / `428` lines | `<= 988` / `<= 1,120` / `<= 800` | Met |

## Gate status

All four phases of Milestone `M07` (`M07-P01` through `M07-P04`, `lico 0.2.3`)
are implemented, measured, and installed to `~/bin/lico` for live testing,
awaiting owner G3 inspection acceptance and G4 merge/publication approval.
