# M11-P02 Report — Running Elsewhere and the Empty-Resume Hint

- Status: implemented, awaiting owner inspection (G3)
- Version: 1.0.0
- Branch: `phase/M11-P02-running-elsewhere-and-hint` (stacked on the P01 tip)
- Plan: [M11-P02](../../plans/202611/20261113/20261113_14_M11-P02_running_elsewhere_and_hint_plan.md)

## What changed

- **Live check** (`chats/live.rs`). A chat is live elsewhere when either signal holds (the cautious side): `recent_write` (the transcript was modified in the last 180 s; `LICO_LIVE_WRITE_SECS` changes it, for the tests), or a same-user `claude`/`codex` process that is not one of this host's own agents (its parent chain does not reach this host) either names the session id on its command line (`process_has_session`, also as `--resume=ID`), or runs in the chat's folder and started no later than the chat's last write plus 2 s (`process_in_folder`; a closed chat older than the process stays free). An unreadable `cwd` or command line is unknown: it never makes a chat live and never frees one; an unknown start time counts as "may be" (cautious). A chat open in a tab of Lico is never judged (it is ours), and neither is a remote chat. Provider programs hold no transcript open (checked on this machine by shape only), so no file-descriptor signal exists.
- **Cost bounds**: `/proc` is read comm first, then `cwd`, `cmdline` and `stat` only for processes named `claude` or `codex` of the same user; the list is kept for 2 s; the check is made only for the items of a returned page and for the chat a creation would continue, and only for a chat the write time has not already marked.
- **Marks and refusal**: `session.chats` items carry `running_elsewhere` and `live_reason`; the Chats tab shows `[elsewhere]` and refuses a click on such a chat with a sentence (the host refuses it too); `session.create` with `resume_id` or `resume_latest` answers `chat_running_elsewhere` and starts nothing, and `resume_latest` does not pick an older chat in its place; `New` always works. The tool `create` says the reason in words. The dialog shows `<Last Session line> - running in another terminal; it cannot be resumed from here. Choose New.` and Enter on Resume keeps the dialog open with that sentence; New works.
- **Empty-Resume hint** (`chats/others.rs`): `session.chats {others: true}` on an empty list (and `lico chat list --others`) adds `others {total, by_account [{account, label, chats}]}`: the chats of the folder under every other account and under both default homes, the default homes counted even before Native is chosen (the first lines of their transcripts are read to match the folder, nothing but the count leaves the host, no title). The dialog says `No earlier chat ... Resume will open a new one. 3 chats of this folder exist under other accounts or homes (ClaudeWork: 2; Native (~/.claude): 1).`; `--resume-latest` and the tool say the same in `note` and in `others`.

## Evidence (as measured)

| Check | Result |
|---|---|
| Rust unit tests | `chats::live` 3 (write time, id, folder and start time, unknown cannot free, ours and remote never judged, the `stat` line), `chats::others` 1, `gui::newchat` line and hint cases; all `gui::` tests `203 passed` |
| `tests/test_running_elsewhere.py` (fake provider processes: a script named `claude` that waits, in a synthetic folder; synthetic homes) | 4 passed, repeated 3 times: recent write live and old chat free, a tab of ours never elsewhere; process signals alone (window set to 0): none before the process, `process_in_folder` and `process_has_session` while it runs, the closed chat of the same folder free, all free again after the processes end; refusals by id and as the latest with nothing started, New allowed, a closed chat resumes; the hint with counts only and no title, and `--resume-latest` saying it |
| `tests/test_running_elsewhere_window.py` (private Xvfb) | 1 passed, repeated 5 times: `[elsewhere]` in the Chats tab, the dialog's sentence, Enter refused with nothing started, New works, the hint for an empty account |
| Cost of the live check in `session.chats limit 20` (`941` entries in `/proc` of this machine, about 27 `claude`/`codex` processes), 30 calls 2.1 s apart so that the list is read every time | median `13.8 ms`, worst `20.1 ms` with the scan, against median `0.9 ms`, worst `8.4 ms` for 20 chats answered by their write time alone: the scan adds about 13 ms per listing, at most every 2 s |
| Full changed suite on the staged tree | 484 tests in 95 files, 484 passed, 114.8 s, tree peak 1,751.3 MiB |

## Honest notes

- The tests were written before the code but not run red on their own (the first run of the file did fail for the right reasons: the fields and the error did not exist). Two of my test designs were wrong and were corrected: a future modification time counts as recent (cautious, kept), and `sleep` rejects `--resume`, so the fake process is a script (a script is named after its file in `/proc`).
- The scan cost was measured on this machine, whose `/proc` holds the owner's own `claude` and `codex` processes; the host compares their folders, names and start times in memory and nothing is printed or kept. The tests themselves use only fake processes.
- The limits are stated in the manual: an idle terminal that wrote nothing and names no chat is recognised by folder and start time only; a chat resumed inside a terminal with `/resume` and not yet written to is not seen; ssh chats are not inspected; the 180 s window can also mark a chat that a program wrote and left a minute ago.
- Not exercised: a real Claude Code or Codex terminal (by rule); the owner's own check at G3 is the real test.

## Next

P03: end-to-end verification, measurements, `lico 0.2.7`.
