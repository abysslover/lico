# M24-P08: Chats show only what the person said

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P08-chats-only-what-was-said` from `phase/M24-P07-agent-pane-pause` (`88750bd`); code `CODE_SHA`; not pushed (P07 and P08 are published together, on the owner's word)
- Source: the owner's word in the chat on 2026-11-25, before the push of P07 (quoted in the [plan](../../../plans/202611/20261125/20261125_03_M24-P08_chats_show_only_what_the_person_said_plan.md))
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What was wrong and what the person sees now (MEASURED on this machine's records, counts only; READ from the code)

| Place | Before | Now |
|---|---|---|
| The card of a Claude chat that began with a slash command | titled `<command-name>/model</command-name>` (`28` such chats on this machine), counted the command and its output as two turns | titled by the first thing the person said; the command and its output are no turn |
| The card of an `agy` chat that began with Orca's wake-up sentence (`You have 1 orchestration message. Run ...`) | titled by the sentence (`462` such lines in `agy`'s history file) | titled by the first real prompt, else `agy (3f2a91bc)` |
| A Claude or Codex chat with the sentence typed into it by Orca | the sentence was a title, a turn and a line of the transcript shown on resume | none of the three |
| A task notice or a teammate relay | a title, a turn and a line | none of the three |
| A slash command **with** words (`/orchestration fix the bug`) | markup | the line the person typed, one turn, never a title |
| A command with no words (`/model`) and its output | markup, two turns | nothing |
| A chat of nothing else | `<command-name>...` or the sentence | `claude (3f2a91bc)`: the provider and the first eight characters of the id |

## 2. How it is built (READ)

- One new leaf module, `injected.rs` (no other module is used by it; the module graph is `247` modules and the cycle of the hubs is still `50`): `person_text` (a user record's words, or `None`), `orchestration_wakeup` (`You have`, a number, ` orchestration message`, whatever follows) and `not_a_title_line`.
- Three call sites, no new reader: `claude_msg_text` (the one function behind the title, the count of turns and the resumed transcript of a Claude chat), `is_codex_system_text` (the same for Codex, and for its events) and `derive_auto_title` (every title, `agy`'s too). `derive_auto_title` is in `folders.rs`, which is exactly at the size ratchet's `800` lines: one condition was replaced and no line was added.
- Only a **user** record is judged: an agent's answer is kept as it is, even if it begins like the sentence.

## 3. Checks (MEASURED)

- **Tests first, run red**: `7` new unit tests were written against the existing readers and `7` failed before the module was wired (the shortfall of P07, tests not run red, did not repeat here); then `53` tests of `chats::`, `injected::` and `folders::` pass (host unit tests `1120` to `1142`).
- A host test (`tests/test_chats_only_what_was_said.py`): a real host, a synthetic Claude home and an `agy` history, `session.chats`: the titles and the counts are what the table says and no markup and no sentence is in the answer. With the rule switched off it fails with the symptom of the owner's words (title `<command-na...`, `4` turns instead of `2`).
- **Mutation checks**: `20` mutants of the rule (the number, the plural, the white space, each markup, the command's words, the slash, the element's text, each title line, each reader, the side that is judged), the unit tests of the chats, the rule and the titles: all `20` killed. One survived the first time (a number is needed: `You have  orchestration message` with two spaces and no number) and a case for it was added.
- `cargo xtask check`, the module-graph tests (`19`) and clippy (nothing new in the changed files) pass.
- The commit hook: HOOK_RESULT

## 4. What is not decided (the owner's) and known limits

- **A chat on a machine reached by ssh** is listed by a script that runs there; it still takes the first record of the chat as it is. Its title is cleaned on this side, so the card shows `claude (3f2a91bc)` and not the markup, but a real question that came after the command is not found. I did not change that script: it has no test path here.
- **`[PRI-HANDOFF] ...`** prompts (a hand-off written for the person) are not hidden: they were not named, and they say what the chat is. Say the word and they go the same way.
- A command **with** words is kept and counts as a turn; if you want the commands gone altogether, it is one line (`person_text`).
- No version bump and no CHANGELOG entry: the next release (`0.8.2`) carries it.
- **Push, pull request and merge** are the owner's word.
