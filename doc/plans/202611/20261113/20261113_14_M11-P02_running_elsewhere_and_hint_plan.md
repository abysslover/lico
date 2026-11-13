# M11-P02 Plan — Running Elsewhere and the Empty-Resume Hint

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M11-P02-running-elsewhere-and-hint`
- Milestone: [M11](../../../milestones/202611/20261113/20261113_04_M11_native_provider_home_chats_milestone.md)

## Decisions

- A chat is **live elsewhere** when either signal holds: (a) its transcript was modified within the last 180 s; (b) a same-user `claude` or `codex` process that is not one of this host's own agents names the session id on its command line, or runs in the chat's folder and started no later than the chat's last modification. An unreadable `cwd` or command line is "unknown", never live and never free. A chat open in a Lico tab is Lico's own and is never "elsewhere". Known limit: an idle terminal that has written nothing and gives no id is recognised by the folder-and-start-time rule only.
- A refused Resume does not fall back to an older chat and does not silently open a new one: the error is `chat_running_elsewhere`, `New` is the way on.
- The hint counts the folder's chats under other accounts and homes (counts only, no titles), also before Native is chosen.

## Steps

- [ ] S01: Red tests with fake processes (a copy of `sleep` named `claude` with its `cwd` in a synthetic folder) and transcripts of fresh and old modification time: live and not-live, list mark, refusals (dialog, CLI, tool, `resume_latest`), the hint.
- [ ] S02: `chats/live.rs` (bounded `/proc` scan, cache), `chats/others.rs`, refusal in `new_session.rs`.
- [ ] S03: List mark, dialog text, CLI and tool reports, manual text; measure the scan cost.
- [ ] S04: Report, mailbox.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | A live chat is marked and its Resume refused with a reason; New works; a closed chat of the same folder resumes | host and GUI tests with fake processes |
| 2 | The hint names the number of chats under other accounts and homes | host and GUI tests |
| 3 | The scan is bounded and its cost is measured | measurement in the report |
