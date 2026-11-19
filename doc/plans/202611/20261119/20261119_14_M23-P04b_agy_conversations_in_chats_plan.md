# M23-P04b Plan — The `agy` Conversations Are Cards In Chats, And A Click Brings One Back In A Terminal

- Status: in_progress (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M23-P04b-agy-conversations` from `phase/M23-P04a-tab-knows-agent` (`c12a9bf`); the prefix `phase/M23-P04b-` is approved by the owner (2026-11-19, with P04a and P04c)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), row P04, level B of [the P04 plan](20261119_10_M23-P04_agent_in_terminal_plan.md)
- The owner's words (2026-11-19): "A+B만 먼저하고 C는 향후로" and "거의 컴맹 수준의 사람도 쓸 수 있는 직관성" (every picture the size of the Claude and Codex pictures, almost no text)
- This phase reads one more record of the person's own (the file `agy` keeps of the prompts typed in it) and adds one kind of card to Chats. Nothing is sent to `agy`, no conversation's text is read beyond the first prompt's line (its title), and no setting changes.

## What is true before this phase

- Chats (the window's second tab) lists the Claude and Codex conversations the host reads from the providers' own folders, and the pages the browser remembers (`provider: "browser"`, `pages`, M22-P05c). Nothing of `agy` is listed; a person who used `agy` in a terminal finds nothing of it in Lico.
- `agy` keeps `history.jsonl` in its own folder under the person's home (`.gemini/antigravity-cli/`), one JSON object per prompt typed (found 2026-11-19, keys and types only): `display` (the text), `timestamp` (milliseconds), `workspace` (the folder it ran in), usually `conversationId` (a UUID), and for a slash command `type: "slash_command"`. A few lines have no `conversationId` (a prompt before a conversation exists). The file is mode 0600 and was 860 KB with 2048 lines on the owner's machine; the longest line was about 8.5 KB.
- `agy --conversation ID` resumes a conversation (the P04 plan, from `agy --help`). P04a made a terminal tab show the program that runs in it.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **The host reads `history.jsonl` in `chats/agy.rs`**, read-only and bounded like the other readers: a regular file (not a link), the person's own, with no group or other permission bit (else it reads as empty), at most the last 8 MiB (a line cut at the start of what is read is no JSON object and is skipped like any damaged line), a line over 16 KiB or not a JSON object skipped, and at most 200 conversations kept (the newest). A conversation is the lines with one `conversationId` that is a plain token (`valid_provider_session`: letters, digits, `-`, `_`, at most 64, not starting with `-`); a line without one or with another kind of id is skipped. Its **title** is the first prompt that is not a slash command, cleaned as the other titles are (`derive_auto_title`), else `agy (` and the first eight characters of the id `)`; its **folder** is the last `workspace` that is an absolute path without control characters; its **time** is the last `timestamp` (milliseconds; a value that is plainly seconds is taken as seconds). | the same safety rules as every record Lico reads of another program; the title is the line the person typed, as a Claude card's is; no other text of a conversation is read |
| D2 | **`session.chats` takes a boolean `agy`**, like `pages`: only for the list of this machine (`all`), with no account chosen, adds the conversations as items with `provider: "agy"`, `account` null, `cwd` the folder, `open_session` null. Without the field, with another scope, an ssh host or an account, the list is what it was. | nothing changes for the other callers (`lico chat`, the tools, the CLI); the window asks |
| D3 | **A card of `agy` is a card like the others** with the `agy` picture (P04a's) and its own ink; the first line is the title and the time, the second the folder; the hint is the time, the folder and the id. | the owner's philosophy: the same card, one picture |
| D4 | **A click on the card opens a terminal and types the command into it.** When a terminal tab the window opened for the conversation still shows `agy` as its agent (P04a), the click shows that tab (and says so). Otherwise a new terminal opens in the conversation's folder when it is a folder of this machine, else at the home, and the line `agy --conversation ID` and Enter are typed into it, as the person would. The id is checked again in the window before it is typed. `agy` is never given its flag that skips permissions, and nothing else is added to the line. | no new door on the host (the window types what a person could type); the person sees what runs; a terminal that already runs it is not duplicated |
| D5 | **The window remembers which tab it opened for which conversation** (a tab's `conversation`), for this window's life. A conversation the person resumed by hand is not known. | the host does not read what a program's screen says (P04a is the foreground program's name only) |
| D6 | **Not in this phase:** the check that `agy` is installed (it is the tile's grey in P04c, which needs the same check; here a missing `agy` is the shell's own message), a choice "open in a terminal" for Claude and Codex cards, any reading of a conversation's text, and C (`agy` as a Lico session). | each needs a decision of its own |

## Steps

1. [ ] **Tests first**: Rust unit tests for the reader (`chats/agy.rs`: the conversation, its title, folder and time; slash commands, lines without an id, ids that are not plain tokens, a damaged or long line, a file that is a link or group-readable, a file over the bound, the newest 200); a system test (`tests/test_agy_chats.py`: `session.chats` with and without `agy`, the other scopes, the query, a walk of the pages, a file that is not owner-only, the protocol check of request and answer); Rust unit tests for the window (the card's picture and hint, the click: a terminal opens with the folder and the line, the id that is not a token is refused, a tab that still shows `agy` is shown instead); one window test (Chats shows the card, a click opens a terminal that runs a fake `agy` with `--conversation ID`, a second click shows the same tab).
2. [ ] **The host**: `chats/agy.rs`, the flag in `chats/mod.rs`, the validator in `ipc.rs`, the schema (`lico_protocol/v1.py`).
3. [ ] **The window**: the request, the card, the click, the terminal that types a line.
4. [ ] **The docs**: the protocol note, the guide.
5. [ ] **Gates**: mutation checks, the changed-files suite, `cargo xtask check`, the owner's build.

## Not decided here

- The picture's ink for `agy` cards (a violet near the `agy` picture of the tab; the owner may want another).
- Whether a person who resumes a conversation by hand in a terminal should have the tab known too (it would need to read the program's screen or its command line for `--conversation`; P04a reads only the program's name).
- The "open in a terminal" choice for Claude and Codex cards (P04 plan, level B).
