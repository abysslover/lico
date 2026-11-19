# M23-P04 Plan — An Agent Started In A Terminal Tab (`agy`, `claude`, `codex`) Meets Lico

- Status: research done; the owner chose levels **A and B** (2026-11-19), C is for later; P04a is done (`5a8d8c9`) and P04b (`e57552e`), P04c (`4b65a51`) are done as well: levels A and B are complete
- Version: 1.4.0
- Branch: `phase/M23-P04a-...`, `phase/M23-P04b-...`, `phase/M23-P04c-...` in turn, each from the one before (the three prefixes are approved by the owner, 2026-11-19)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), row P04 and its question Q1
- The owner's words (2026-11-19): "사실 terminal에서 지금 agy를 실행해도 실행은 되긴 하는데 원래 그 agy의 형태로 보이는 거라서 과연 그렇게 열었을 때 lico와 연결되는지는 잘 모르겠음."
- This note changes no code. It says what is true today, what "connected" could mean, and what each level costs.

## What is true today (2026-11-19)

1. **It is not connected.** A terminal tab is a shell of the person's in a pseudo-terminal the host owns (M23-P01 to P03c). Whatever the person starts there (`agy`, `claude`, `codex`, `vim`) is that program's own screen drawn by the grid. Lico does not know what runs in the tab, and the Chats tab lists only the Claude and Codex records it reads from their folders (M11); nothing of `agy` is read.
2. **What Lico already has that a link could use**: the host owns the pty, so `lico terminal list|show|send|open|close|resize` can read and drive the tab; the shell is given `LICO_SOCKET` and no other `LICO_*` variable; the Chats list and its cards (M22-P04c) and the `⇄` mark that says a conversation is open in a room; the reader of the providers' own records (`chats/`, M11).
3. **What `agy` offers** (read from `agy --help` and `agy changelog` on this machine; the newest changelog entry is `1.5.12`): `--conversation ID` and `--continue` (resume), `--project`, `--model`, `--effort`, `--mode`, `--dangerously-skip-permissions`, `--log-file`, a print mode with `--output-format stream-json` and `--input-format stream-json` ("one NDJSON message per line from stdin, a turn for each"), the subcommands `acp` (an Agent Client Protocol stdio server) and `mcp` (manage MCP servers).
4. **What `agy` keeps** (only names and key names were looked at, no conversation was read): `~/.gemini/antigravity-cli/history.jsonl` (one JSON object per prompt, with the keys `display`, `timestamp`, `workspace`, `conversationId`; 2003 lines here), `~/.gemini/antigravity-cli/brain/<conversationId>/` (one folder per conversation; 378 here), `cli.log`. So the folder a conversation belongs to and its id are written in plain JSON, as Claude's and Codex's records are.

## Three levels of "connected" (from least to most)

| Level | What the person sees | What it needs | Cost |
|---|---|---|---|
| **A. The tab knows what runs in it** | the tab's caption says `agy`, `claude` or `codex` while it runs there (and the folder it works in); nothing else changes | the host reads the foreground process of the pty (the master's `tcgetpgrp`, then `/proc/<pid>/cmdline` and `cwd`, readable by the same user); the window shows the word | small: one host function with a test on a fake shell, one caption |
| **B. The conversations are in Chats and come back in a terminal** | `agy` conversations are cards in Chats (title from `display`, the folder from `workspace`, the time); a click opens a terminal tab in that folder running `agy --conversation ID`; for Claude and Codex a card gets the same choice (open as a session, or in a terminal tab) | a reader of `history.jsonl` in `chats/` (read-only, bounded, owner-only like the others); the command to type into a new terminal; the card's picture for `agy` | medium: a new provider word in Chats, its reader, tests with a synthetic record file |
| **C. `agy` is a Lico session** | `agy` is a fourth choice of the wizard (account-less like Local LLM, or its own sign-in is the person's), its conversation is a session tab with Lico's transcript, approvals, `Send to`, and the rooms; the terminal is not involved | a provider adapter on `--output-format stream-json` (the way the Claude adapter uses Claude's stream) or on `acp`; a spec of the messages; the approval mapping | large: a new provider is a milestone of its own (M24), as Claude and Codex each were |

## The lead's recommendation

- **Do A and B first (one phase each), leave C to a milestone of its own.** A and B answer the owner's question as asked: a person who starts `agy` in a terminal sees that Lico knows it, finds the same conversation in Chats afterwards, and comes back to it in one click, with no new provider and nothing that can break the existing sessions. C is worth doing only if the owner wants `agy` inside Lico's own session screen.
- **A before B**, because B's "which terminal is this conversation open in" (so a click on a card focuses the tab where it already runs, as a click on a session's card does) needs A's knowledge of what runs where.
- Neither A nor B sends anything to `agy` and neither changes what the shell can do; both are read-only on the person's own records. The host stays the only reader of the records (the window asks `session.chats`).

## The owner's decisions (2026-11-19)

1. **A and B first; C (`agy` as a Lico session) is for later.**
2. **A new picture, `agy`, goes into step one of New session between Claude and Codex**: Claude, agy, Codex, Local LLM, Terminal. In A+B it opens a terminal in the folder on the left and starts plain `agy` in it (never with its flag that skips permissions: the agent keeps its own default). The picture is grey, with a short reason, when `agy` is not installed on this machine.
3. **The UI philosophy** (the owner, 2026-11-19: "거의 컴맹 수준의 사람도 쓸 수 있는 직관성", a person who hardly knows computers must be able to use it; every default icon the size of the Claude and Codex pictures of step one, nearly no text): so the terminal tab's caption says what runs in it by a **picture**, not a word; the Chats card of an `agy` conversation is the same card as the others with `agy`'s picture; nothing new asks a question.
4. `claude` and `codex` started in a terminal get the same treatment as `agy` (the lead's default, accepted by saying A+B).
5. A click on an `agy` card with no terminal open opens a terminal tab in the conversation's folder (when it exists on this machine, else at the home) and starts `agy --conversation ID` in it.

## Steps

1. **P04a (level A, the tab knows what runs in it)**: tests first (the host: a fake shell tree where the foreground process is `agy`, `claude`, `codex` and a plain `sleep`, and the answer in `terminal.list`, `terminal.watch` and the window dump; the window: the picture in the tab's caption), the host function (the foreground process group of the pty: the master's `tcgetpgrp`, then `/proc/<pid>/cmdline`, readable by the same user), the picture, gates.
2. **P04b (level B, the conversations)**: tests first (a synthetic `history.jsonl`: the reader's bounds, a damaged line, a conversation id that is not a plain token, a record file that is group-readable and so ignored as the others are, a card in Chats, a click that starts `agy --conversation ID` in a terminal and focuses the tab where that conversation already runs), the reader in `chats/`, the card and its picture, the click, the guide, gates.
3. **P04c (the picture in New session)**: tests first (the tile order, five tiles that fit, grey when `agy` is not found), the tile with its icon, the start of `agy` in a new terminal, the guide, gates.
4. Each phase: mutation checks, the changed-files suite, `cargo xtask check`, the owner's build for `lico gui`, a Korean report. The security limits of the terminal (gaps 2 to 4 of [the security note](20261119_12_person_facing_surfaces_security_note.md)) are not changed by these phases and are not made worse: nothing sends text to an agent, nothing reads a conversation's text, only its title line, folder, id and time.

## What the owner decided

See the section above: levels A and B now, C later; the `agy` picture between Claude and Codex; `claude` and `codex` alike; a click on a card opens a terminal with `agy --conversation ID`.

## Not decided here

- The `agy` adapter of level C (the stream or `acp`), its approvals and its place in the wizard.
- A Lico `mcp` server that `agy` (or `claude`, `codex`) could be pointed at, so that an agent in a terminal can call Lico's operations (send to a session, read a room). It is a separate idea and a larger security question.
- Anything that reads the text of an agent's conversation: A and B read only the title line, the folder, the id and the time.
