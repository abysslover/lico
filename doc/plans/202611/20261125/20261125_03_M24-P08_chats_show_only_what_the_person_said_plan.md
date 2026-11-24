# M24-P08 Plan — Chats Show Only What The Person Said

- Status: done (2026-11-25), code `d01f923`; not pushed (the prefix `phase/M24-P08-` was approved by the owner at commit time)
- Version: 1.0.0
- Branch: `phase/M24-P08-chats-only-what-was-said` from `phase/M24-P07-agent-pane-pause` (`88750bd`; P07 is not pushed, so the two phases are published together)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row before the publication of P07
- The owner's words (2026-11-25), before the push of P07: "push 병합 하기 전에 Chats에 내용을 보면 `<command-name>/model</command-name>` 이렇게 사용자가 명령 관련된 내용을 적은게 보이는 경우가 있는데 이것은 수정. 어떤 것은 You have 1 orchestration message. 처럼 내가 치지 않은 orca 대화 내용이 남아서 보이는 것이 있음. 이걸 수정"
- This phase changes what Chats cards and resumed chats show, so it is a phase of its own. No version bump (the next release carries it).

## What goes wrong today (READ from the code, MEASURED on the owner's own records, counts only)

All three readers of a chat (the card's title, the card's count of turns, the transcript shown when a chat is resumed) go through one reading function, `claude_msg_text` (Claude) and `codex_msg_text` (Codex), and the title of an `agy` conversation through `derive_auto_title`. They take every `user` record that has text as something the person said. Two kinds of record are not that:

1. **A slash command is stored as markup.** Claude Code writes `<command-name>/model</command-name>`, `<command-message>…`, `<command-args>…` as a `user` record and the command's output as `<local-command-stdout>…` as another. The first line of such a record is no slash line, so the title rule of M23 (a slash command is never the title) does not catch it: the card is titled `<command-name>/model</command-name>`. Of the Claude records of this machine, `28` start with such a record.
2. **Orca types its wake-up sentence into an agent's terminal** (`You have 1 orchestration message. Run `orca orchestration check --terminal …`.`), and the agent's record keeps it as a prompt of the person. `agy` keeps it in its `history.jsonl` (`462` lines on this machine), so an `agy` card can be titled by it; Claude records keep it too (`2` on this machine). No Codex user record of it was found; the Codex reader gets the same rule because Orca types into every agent the same way.

The same two kinds of record also sit near them: the notices of background tasks (`<task-notification>`) and the relays between teammates (`<teammate-message>`), which no person typed either.

## What this phase changes

One rule, in one small module (`injected.rs`, no dependency on the others): **a message of a chat is what a person said or what an agent answered; what a program wrote on the person's behalf is not a message.**

| Record | Title | Count of turns | Resumed transcript |
|---|---|---|---|
| a command with **no words** after its name (`/model`, `/effort`, `/login`) | no | no | no |
| a command **with words** (`/orchestration fix the bug`) | no (a slash line is no title, as before) | yes, one | yes, as the person typed it: `/orchestration fix the bug` |
| a command's output and caveat (`<local-command-…>`) | no | no | no |
| a background-task notice, a teammate relay | no | no | no |
| Orca's wake-up sentence (`You have N orchestration message…`) | no | no | no |
| everything else | as before | as before | as before |

The title of a chat whose only records are those is what it is for any chat with no question: the provider and the first eight characters of the id (`claude (3f2a91bc)`), as for a chat that begins with `/model` today.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | One rule for the three providers: Claude records, Codex records, `agy` history, and every title made by `derive_auto_title` | the owner's two examples are two providers; the rule is "not typed by the person", not "these two strings" |
| D2 | A command with words after it stays as `/name words` | the words are the person's (a skill is started that way: `/orchestration the task`); without them a command is a control action, not conversation |
| D3 | The wake-up sentence is matched by its shape: `You have`, a number, ` orchestration message` (so also `messages`), whatever follows it | Orca may add a preview of the message after it; the preview is the other agent's words |
| D4 | **Not changed:** `[PRI-HANDOFF] …` (a hand-off prompt written for the person), `[from session …]`, and the first-prompt choice of the python script that lists the chats of an **SSH machine** (it takes the first text of the record; the title is cleaned on this side, so the card shows the provider-and-id title, not the markup) | not asked; the owner may want `[PRI-HANDOFF]` hidden too (a word from them), and the remote script is a second code path I cannot run here |
| D5 | No setting, no switch, no version bump | the fix is the behaviour the owner asked for |

## Steps

1. [x] **Tests first**, run red where the code they call exists: the reading function (`claude_msg_text`, `is_codex_system_text`), the heads of a Claude and a Codex transcript (`head_claude`, `head_codex`), the count (`turns::count_lines`), the transcript read for a resume (`read_turns_from_jsonl`), the `agy` history (`agy::load`) and `derive_auto_title`; the host test (`session.chats` over a synthetic home).
2. [x] **The module** `injected.rs` (`person_text`, `orchestration_wakeup`, `not_a_title_line`) and the three call sites; `testmap.toml` entry.
3. [x] **Mutation checks** of the new rule (one edit at a time, the file restored and checked).
4. [x] **Words**: the guide `doc/manual_end_to_end.md` (what Chats shows), the protocol note is unchanged (no field changed).
5. [x] `cargo xtask check` and the module-graph tests by hand (the module graph must stay at the pinned cycle), the changed-files suite through the commit hook.
6. [x] **The owner's build** (`lico gui` shows the last commit); the report.

## Not decided here

- Push, pull request and merge (the owner's word; P07 and P08 are published together). Level C and G3 are as before.
- Whether `[PRI-HANDOFF]` prompts should be hidden as well (D4).

## Results (2026-11-25)

- **Tests first, run red**: `7` unit tests were written against the readers that already existed and all `7` failed before the module was wired; they pass after (`53` tests of `chats::`, `injected::` and `folders::`; the host unit tests are `1142`). The new module's own tests were written with it.
- **The commit** `d01f923` (13 files, `+428 -5`): `injected.rs` (`person_text`, `orchestration_wakeup`, `not_a_title_line`), its three call sites (`claude_msg_text`, `is_codex_system_text`, `derive_auto_title`; `folders.rs` stays at its `800` lines), the host test `tests/test_chats_only_what_was_said.py`, the entry of the module and of the test in `tests/testmap.toml`, the guide `1.22.0`, the plan and the report. D1 to D5 held as written; no limit was raised and no version was bumped.
- **Mutation checks**: `20` mutants, all killed (one survived the first time: a number is needed, `You have  orchestration message`; a case was added and it was killed). The host test also fails with the rule switched off, with the symptom of the owner's words (title `<command-na...`, `4` turns instead of `2`).
- **The module graph**: `247` modules (one more), the cycle of the hubs stays at `50`; `cargo xtask check`, the module-graph tests (`19`) and clippy (nothing new in the changed files) pass.
- **The commit hook refused once**: the new test file was not listed in `tests/testmap.toml` (`tests/test_selection.py`, my omission); the same run had `598` of `599` tests green in `166.6 s`, peak `1,023.7 MiB` (no bound exceeded). The second run passed (`39` tests run, the rest taken from the receipt of the first). Nothing was bypassed.
