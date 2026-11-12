# M06 — Hidden-file toggle in directory viewer and SSH remote host accounts, directory browsing, and sessions (`lico 0.2.2`)

- Status: in_progress
- Approved: 2026-11-12 (G1 and G2 approved by the owner for continuous execution; patch version bump `0.2.1` -> `0.2.2` on completion)
- Project: Lico
- Updated: 2026-11-12
- Implementation: In progress in `M06-P01` (`phase/M06-P01-hidden-files-and-ssh-discovery`)
- Next gate: G3 inspection acceptance and G4 merge approval after `M06-P03` (`lico 0.2.2`)

## Goal

Extend `lico` (`0.2.1` -> `0.2.2`) with two everyday workbench capabilities while
preserving Lico's single-executable design and `< 50.0 MiB` idle GUI RSS budget:
(1) a dotfile / hidden-file visibility toggle in the directory viewer that keeps
entries starting with `.` hidden by default (`Hidden: off`) and lets the
administrator reveal or hide them on demand, and (2) agentless SSH remote-host
support built on `~/.ssh/config` aliases where remote provider accounts
(`[ssh: <host>]`) authenticate and store their provider state on the remote
machine, connect directly from that remote machine to the LLM API provider, and
support remote directory browsing, UTF-8 text preview, and persistent session
execution from `lico gui`, CLI, IPC, and loopback HTTP.

## Done when

1. `folder.list` hides dot-prefixed entries (`name.starts_with('.')`) by default
   (`show_hidden = false`, always excluding `.` and `..`), exposes an explicit
   toggle across `Files` toolbar (`Hidden: off` / `Hidden: on`), `View` menu,
   `Ctrl+H` / `.` shortcut, `settings.set` (`show_hidden`), `lico files --hidden`,
   and `GET /v1/files?cwd=...&hidden=1`, and `lico` discovers explicit SSH host
   aliases from `~/.ssh/config` (`Include` directives included, wildcards and
   option-shaped names refused) with bounded non-interactive connectivity and
   remote provider probing (`ssh.hosts`, `ssh.check`). (`M06-P01`)
2. Provider accounts carry an optional `ssh_host` binding (`None` = local,
   `Some("<alias>")` = SSH remote), surface **`[ssh: <host>]`** badges across
   `Accounts`, sidebar session-group headings, `New session`, `Rebind`, and
   `account.list`, allow choosing `local` or any registered SSH host when adding
   an account in-window or via CLI/IPC/HTTP, and run supervised `Sign in`
   (`claude auth login` / `codex login`) over SSH inside the remote machine's
   `0700` isolated account home so remote sessions authenticate directly against
   their LLM provider without copying tokens or logging verification codes.
   (`M06-P02`)
3. `Files` and `New session` switch cleanly between `Host: local` and registered
   SSH hosts (`ssh:<host>`), supporting on-demand remote directory listing, `F5`
   refresh, hidden-file toggle, pagination, bounded `64 KiB` UTF-8 text preview,
   and `+ New here` session creation; `claude` and `codex` sessions bound to an
   `[ssh: <host>]` account spawn, stream, approve, collect `7d` quota telemetry,
   and reconnect (`session.resume`) over SSH in their remote working directory;
   and `lico` bumps patch version `0.2.1` -> `0.2.2`, passes cold suite +
   memory/hidden-growth gates + two-clone `suite acceptance`, and installs
   `~/bin/lico` `0.2.2`. (`M06-P03`)

## Why now

Hands-on use of `lico 0.2.1` surfaced two immediate needs: workspace and home
directories contain many dot-prefixed configuration directories (`.git`,
`.cargo`, `.config`, `.local`, etc.) that should stay hidden by default unless
explicitly toggled on; and real engineering work frequently runs on SSH-reachable
compute machines where `claude` or `codex` executes on the remote machine and
authenticates there directly against the LLM provider. Adding agentless SSH
accounts, remote directory browsing, and dotfile filtering in `0.2.2` lets a
single local `lico gui` administer both local and SSH workspaces seamlessly.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M06-P01 | Hidden-file (`.dotfiles`) toggle (default off) and `~/.ssh/config` host discovery & probing (`ssh.hosts`, `ssh.check`) | [Plan](../../../plans/202611/20261112/20261112_08_M06-P01_hidden_files_and_ssh_discovery_plan.md) | [Report](../../../reports/202611/20261112/20261112_08_M06-P01_hidden_files_and_ssh_discovery_report.md) | `00010_M06-P01_hidden_files_and_ssh_discovery` | — | done |
| M06-P02 | SSH remote provider accounts (`[ssh: <host>]`) and supervised remote `Sign in` over SSH | pending | pending | pending | — | planned |
| M06-P03 | SSH remote directory browsing, remote `claude`/`codex` session execution & resume, and `0.2.2` patch release | pending | pending | pending | — | planned |

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-11-12 | Scope and phases `M06-P01` to `M06-P03`, patch version bump `0.2.1` -> `0.2.2` on completion |
| G2 phase plan approval | Approved 2026-11-12 | Continuous implementation across `M06-P01` to `M06-P03` |
| G3 inspection acceptance | Pending | Awaits owner inspection after `M06-P03` |
| G4 merge approval | Pending | Awaits explicit owner approval after `M06-P03` |
