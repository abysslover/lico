# M23-P04a Plan — The Terminal Tab Knows What Runs In It (`claude`, `codex`, `agy`)

- Status: in_progress (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M23-P04a-tab-knows-agent` from `phase/M23-P03d-terminal-start-left` (`d6de701`, published as `main`); the prefix `phase/M23-P04a-` is approved by the owner (2026-11-19, with P04b and P04c)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), row P04, level A of [the P04 plan](20261119_10_M23-P04_agent_in_terminal_plan.md)
- The owner's words (2026-11-19): "A+B만 먼저하고 C는 향후로" and "모든 기본 아이콘 크기는 세션 시작할 때 보이는 Claude, Codex 아이콘을 기본으로 하고 그정도 간결성(거의 텍스트가 없는 수준)" and "거의 컴맹 수준의 사람도 쓸 수 있는 직관성"
- This phase adds one read-only fact to two answers of the host (`terminal.list` and `terminal.watch` say `agent`) and one picture to a terminal tab. It sends nothing to the program, reads nothing of its screen or of its conversation, and changes no setting.

## What is true before this phase

- A terminal tab is a shell of the person's on a pty the host owns. Whatever the person starts in it (`claude`, `codex`, `agy`, `vim`) is that program's own screen. The host does not know what runs; the tab's caption is `>_ ` and the screen's title or the folder's name.
- The window shows the caption as text in the tab strip; every tab of the strip is text only. The pictures of Claude and Codex exist (`icons.rs`, the tiles of step one) and have no picture for `agy`.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **The host reads the foreground process group of the pty** (`tcgetpgrp` on the master, the one new system call, in `pty.rs`), then `/proc/<leader>/cmdline` (same user, bounded to 4096 bytes). It names the program by the last component of `argv[0]`; when that is an interpreter (`node`, `nodejs`, `bun`, `deno`, `python*`, `sh`, `bash`, `dash`, `env`) by the last component of the first argument that is not a flag (and not `NAME=value` after `env`). Only three names are ever answered: `claude`, `codex`, `agy`; anything else is no agent. | the programs are started as a native binary (`agy`), as a launcher script run by `node` (`codex`, `claude` in older installs) or under their own process title; a name that is not exact (`claude.txt`, `cat claude`) is not an agent |
| D2 | **`terminal.list` rows and `terminal.watch` answers carry `agent`**: `"claude"`, `"codex"`, `"agy"` or null. It is worked out when the answer is made (no thread, no timer): the program's first output and the shell's prompt after it both move the change number, so a watch sees the start and the end. | no new loop; the cost is two system calls and a small read per answer |
| D3 | **The window shows a picture, not a word**: the tab's caption starts with the program's picture (the Claude and Codex pictures of step one, and a new picture for `agy`) in the place of the text `>_`, and the caption is then the title or the folder as before. A terminal with no agent is as it was. | the owner's philosophy: pictures of the size of the Claude and Codex tiles, almost no text |
| D4 | **A tab that is not shown keeps the last picture it had** (the window keeps one watch, for the shown tab). It is corrected when the tab is shown again. | the window does not poll tabs in the background; a limit, written in the guide |
| D5 | **No new setting, no request field, no approval**: nothing is sent to the program. `agy` is never given its flag that skips permissions by this phase (P04c starts plain `agy`). | the security note's gap 3: Lico's approvals do not reach a program started in a terminal, and A only shows what runs |

## Steps

1. [ ] **Tests first**: Rust unit tests (`terminal/agent.rs`: the names, the interpreters, the near-misses, a damaged or empty command line, a bound; `terminal/tests.rs`: a real pty whose foreground program is a copy of `sleep` named `claude`, a script named `codex` (run by `/bin/sh`) and a plain `sleep`, the answer of `terminal.list` and `terminal.watch`, and `agent` null again after the program ends; `gui/terminal`: `Screen::parse` keeps only the three names, the tab strip's entries carry the picture); one window test (`tests/test_terminal_tab.py`: a program named `agy` started in the shell, the dump says `agy` for the tab).
2. [ ] **The host**: `pty.rs` (`foreground_group`), `terminal/agent.rs`, `tab.rs` (`row` and `snapshot`).
3. [ ] **The window**: `Screen.agent`, the picture `Icon::Agy`, the tab strip.
4. [ ] **The docs**: the protocol note (rows `terminal.list` and `terminal.watch`), the guide.
5. [ ] **Gates**: mutation checks, the changed-files suite, `cargo xtask check`, the owner's build.

## Not decided here

- The picture of `agy` is a shape of our own (an upward chevron over a dot: "anti-gravity"), like the Claude and Codex pictures, which are no vendor logos. The owner may want another.
- Naming other programs (`vim`, `htop`, a build) in the tab: not asked; the same mechanism could, but each name would be a decision about what the person sees.
