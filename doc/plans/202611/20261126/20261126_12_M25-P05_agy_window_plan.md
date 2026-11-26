# M25-P05 Plan — The Window Starts And Shows An agy Session

- Status: done (2026-11-26)
- Version: 1.1.0
- Branch: `phase/M25-P05-agy-window` from `phase/M25-P04-agy-resume` (`d33d1a7`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P05
- The owner's words: the standing yes of 2026-11-25 (advance approval of the prefix, the commits, the merge and the push through level C); the UI philosophy of 2026-11-20 (a near-computer-illiterate person, every icon the size of the Claude and Codex ones, almost no text, the left side decides where things start)

## What this phase makes

P02 to P04 made `agy` a session of the host. This phase lets a person use it from the window, in the places where they already start and find the other sessions.

1. **The `agy` picture of New session starts a session at once** (decision D7): `session.create kind=agy mode=default` in the folder the left lists (`terminal_start_dir`, the owner's rule since M23-P04d, the Home scope included). There is no account step (the program signs in by itself, D3), no model, no effort. A refusal leaves the person on the icons with the reason in plain words; a host that could not start the program (`busy`) says `The host could not start it just now.` The window starts the session in `default`: every permission request of the program is asked of the person until the owner has seen a real turn (D5 as amended in P03).
2. **A Chats card of an `agy` conversation resumes it as a session** (decision D8): `session.create kind=agy resume_id=<id> mode=default`, in the folder the card knows; a card whose conversation a session of the window already holds shows that session and starts nothing; a conversation whose folder is gone resumes in a workspace of the host (no `cwd` is sent), never in a different folder by guess.
3. **One card per conversation in Chats** (host, `chats/mod.rs`): a session of the host of kind `agy` is a card, with `open_session` and `open_state`; the card the program's own file gives for the same conversation id is the same card (one, not two); a session that has not said anything yet has no line in the file and is a card of its own, found by the id the program gave it; an `agy` card has no account of Lico's.
4. **The window says it is `agy`'s**: the tab begins with `A` (not Codex's `X`), the strip says `agy | state | mode` with no account (`no account` is a word of the others), the list heading is `agy`, the header line the same, and the model the program names on `session/new` is the one the strip shows (`agy (default)` until it does). `Rebind account` says why it cannot be done (`an agy session has no account of Lico's`); the permission mode of the session can be switched as for the others (the host reads the mode live, P03).
5. **The terminal path for a conversation is removed** (it was M23-P04b/c, level B): the card no longer opens a terminal typing `agy --resume`, so `TermJob::OpenTyping`, the tab's `conversation` and the window's special case are gone. What stays of levels A and B: the Terminal picture, a terminal where the person types `agy` themselves, the program's lookup (`terminal/agy.rs`: `installed`, `installed_here`, `agy_look`), and the Chats cards read from the program's own files.
6. **Not changed**: `session.rs` (800 lines), `folders.rs`, `core.rs`; the host's refusal words are the host's.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| C1 | choosing the `agy` picture starts a session of kind `agy` in `default` in the folder the left lists (Home scope included), opens no terminal, shows that session, labels its tab `A...`; the program is started with `--disable-telemetry` and never with the flag that skips its permissions | window test (`tests/test_agy_tile.py`) |
| C2 | a refusal of the host leaves the person on the icons with the reason (`busy` has a sentence of its own); a found `agy` is chosen at once (no account step) | unit tests (`newsession_wizard_tests.rs`, `newsession_tests.rs`) |
| C3 | a click on an agy card asks `session/resume` with that conversation and never `session/new`, in `default`; a second click on the same card shows that session and starts nothing; a gone folder resumes in a host workspace, and only an agy card's does (`resume_folder`) | window test (`tests/test_agy_chats.py`) and a unit test (`sidebar_chats.rs`) |
| C4 | a conversation that a session holds is one card in `session.chats` with the session on it and the later of the two times, a session with no line in the file yet is a card of its own, no card has an account or an account label | host test (`tests/test_agy_resume.py`) |
| C5 | the strip, the header, the list heading, the rebind reason and the permission mode read right for an `agy` session, with no account word | unit tests (`statusline_tests.rs`, `row.rs`, `admin_ui.rs`, `attn.rs`) and the window test (the strip's text and the model) |
| C6 | no middle dot, no local path, no secret in any new file; no size cap crossed | the guards of the commit and the size ratchet |

## Steps

1. [x] Host: `chats/mod.rs` (the sessions of kind `agy` are cards, merged with the file's by id, no account)
2. [x] Window: `newsession_wizard.rs` (`agy_session_start`), `newsession.rs` (`busy`), `sidebar_chats.rs` (the card resumes as a session), `tabs.rs` (the `A` of the tab), `statusline.rs`, `row.rs`, `attn.rs`, `admin_ui.rs` (no account word)
3. [x] Remove the terminal path (`terminal/view.rs`, `terminal/agy.rs`, `terminal/agy_tests.rs`)
4. [x] Tests (window, host, unit)
5. [x] Mutation checks of the new logic
6. [x] The README, the plan's results, the milestone row, `PLAN.md`, the index row, the commit (the hook), `~/bin/lico` rebuilt

## Personal data and security

- The conversation id of a card comes from the program's own file or from the host's registry and goes to the host as `resume_id`, where the same validation as for every other provider id refuses a bad one before it reaches the registry, a command line or a log (P04). The window sends nothing else of the card.
- The folder of a card is only offered when it exists as a directory; the host checks it again (`check_folder`, the allowed roots). A folder that is gone is not replaced by another one the window picks.
- `default` means every request of the program is shown to the person; nothing in this phase answers one for them.
- No account, token or sign-in is read or written by the window; the sign-in line the host writes (`agy is not signed in: open a Terminal tab, run agy, sign in, then send the message again`) says where the person signs in.

## Not decided here

How a person signs in without a terminal; the model picker and the effort (D10); the stop of a turn (`session/cancel`); an attachment to a driver session (text only); whether the id in the program's files is the same text as the protocol's `sessionId` (the host test uses the same text on both sides; the owner's check, G3, is the real one).

## Results (2026-11-26)

What was made: in the host, `chats/mod.rs` lists a session of kind `agy` as a card (no account, no label), and the card the program's own file gives for the same conversation id is the same card (the session's `open_session`, `open_state` and the later of the two times are put on the file's card); in the window, the picture starts `session.create kind=agy mode=default` in the folder the left lists (`agy_session_start`), the card of Chats resumes by `resume_id` in `default` with `resume_folder` deciding the folder, `tab_caption` gives agy the letter `A`, and the strip (`who_part`), the header, the list heading, `rebind_reason` and the refusal words (`busy`) have no account word for agy. Removed: `TermJob::OpenTyping`, `TTab.conversation`, the terminal path of `terminal/agy.rs` (it keeps the lookup of the program). The permission mode of an agy session is switched as for the others (the host reads `s.mode` when a request comes, P03); D10's "no mode switch" is read as "no picker of the program's own modes", which is still true.

| # | Result |
|---|---|
| C1 | the window test chose the picture in the Directories folder and in the Home scope: one session of kind `agy`, mode `default`, no terminal, the tab label `A...`, the program started in the folder the left lists with `--disable-telemetry` and without the flag that skips permissions; the strip says `agy | ...` with no account word and shows the model the program named (the simulator's) |
| C2 | the unit test: a found `agy` is asked of the host at once, a refusal leaves the pictures with `The session was not started. ...` and opens no terminal; `refusal("busy")` is `The host could not start it just now.` and an unknown word is said as it came |
| C3 | the simulator was asked `session/resume` with the card's conversation and never `session/new`, in `default`; a second click on the same card started nothing and showed that session; a card whose folder is gone resumed under a workspace of the host; `resume_folder` keeps the folder for the other providers and for a folder that is there |
| C4 | one card for a held conversation with the session on it, `account` and `account_label` none, `updated_at` later than the file's last line; a session with no line in the file is a card of its own; after the person closed the session the card stays with no session on it |
| C5 | strip, header, list heading, rebind reason (`an agy session has no account of Lico's`) and the mode switch (allowed) are asserted in the unit tests (assertions added to the four tests of local sessions and strips) and the strip and model in the window test |
| C6 | checked by the guards of the commit and the size ratchet (`sidebar_chats.rs` 650, `newsession.rs` 779, `chats/mod.rs` 706 lines, none over 800) |

Tests: 3 new tests (the folder rule `resume_folder`, the refusal sentences, the card with its session in Chats of the host), against the budget of about 100 (M25 so far 39 + 3 = 42); rewritten: the wizard's agy test, two window tests of the tile, the window test of the card's click; assertions added to four existing unit tests, the P04 host test and the tile test. The window was looked at in a screenshot of the first run (the strip said `agy | no account | idle`, which this phase changed to `agy | idle | mode asks` and the model).

Mutation checks: 20 single-edit mutants (the host's `chats/mod.rs`: the session not listed, an account, a native label, no merge, a merge across providers, the open session, the open state and the time not carried; the window: the start in `auto`, no folder sent, the card resuming in `auto`, the gone folder kept, the gone rule for every provider, no letter for the tab, the strip (two forms), the header, the rebind, the list heading, the `busy` sentence). 16 killed at once; 4 survived: the native label of a session with no line in the file, the later of the two times, the gone-folder rule applied to every provider (the three were answered with stronger tests, and the rule was moved into the small function `resume_folder` with a unit test, then killed on the rerun), and the merge that does not look at the provider: the very same conversation id for `agy` and for another provider would tell it apart, and ids are random UUIDs of different programs (the same kind of survivor as the holder of P04); the check stays in the code.

Not done, as planned: Send to, mentions and rooms (P06), the guide and the security note (P07), the real program's turns (the owner's check, gate G3).
