# M25-P07 Plan — The Security Note, The Guide, The CHANGELOG Draft

- Status: done (2026-11-26)
- Version: 1.0.0
- Branch: `phase/M25-P07-agy-note-and-guide` from `phase/M25-P06-agy-peers`
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P07
- The owner's words: the standing yes of 2026-11-25 (advance approval of the prefix, the commits, the merge and the push through level C); the owner's rule that a research note changes no code and its gaps wait for the owner's choice

## What this phase makes

No code. Three documents say what P01 to P06 made, in the words a person who did not read the code can follow, and what they do **not** do.

1. **The security note** [Security of an agy session](20261126_14_agy_session_security_note.md): what the program is (a child of the host that runs as the person, not fenced), the table of what holds (the environment of seven names, the fixed arguments, standard error discarded, the process group, who answers a permission request, the limits, no tool of Lico's, the exchange rules of P06) and ten gaps (the program is not fenced and decides what asks; `auto`; the option ids are the documentation's; a key in the environment does not reach it; a spawn failure told as `busy`; the telemetry flag; a prompt-injected answer can send; text only; no whole-conversation handoff; nothing real was run), each with its place in the code or the word *not verified*.
2. **The guide** (`doc/manual_end_to_end.md`, `1.36.0`): the new section *agy sessions* (start, ask first, come back to a conversation, Send to / `@name` / a Call Room, what is not there, what is not checked), and the three sentences of the terminal and wizard paragraphs that still said that the **agy** picture types `agy` in a terminal and that a card opens a terminal (they were true of levels A and B, and P05 made them false). The product version in the guide's title stays `0.8.2` until P08 pins the versions.
3. **The CHANGELOG draft** (`0.9.0`, `unreleased`): the highlights, the *not verified* paragraph and the known limits; P08 fills in the commit, the pull request and the measured numbers.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| C1 | every row of the note's table names a place in the code or the capability record that was read, and every claim about the real program says *not verified* | the review of the note against the code (done while writing: the environment names, the arguments, standard error, the process group, the approval limits, the capability record's client capabilities, the sign-in line, the exchange limits) |
| C2 | the guide no longer says that the **agy** picture or a card opens a terminal; its CLI line (`lico session create --kind agy [--cwd DIR] [--name TEXT]`) is one the command line accepts | a read of the guide against `cli.rs` and the window's source |
| C3 | no middle dot, no local path, no secret in any new text | the guards of the commit (`git diff --cached -U0 main` for `U+00B7`; the public guard) |

## Steps

1. [x] Write the note from the code, not from memory
2. [x] Rewrite the outdated sentences of the guide and add the section
3. [x] Draft the CHANGELOG entry
4. [x] The plan's results, the milestone row, `PLAN.md`, the index rows, the commit (the hook)

## Not decided here

Whether the owner wants the gaps of the note closed before the release or after the hands-on check; the note's last section proposes the check first.

## Results (2026-11-26)

Done as planned. The note is `1.0.0`, the guide `1.36.0`, the CHANGELOG entry is a draft. Nothing was run: this phase changes no code and adds no test.
