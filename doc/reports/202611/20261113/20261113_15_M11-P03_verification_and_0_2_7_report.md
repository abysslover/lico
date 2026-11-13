# M11-P03 Report — End-to-End Verification, Measurements, and `lico 0.2.7`

- Status: implemented, awaiting owner inspection (G3) and merge (G4)
- Version: 1.0.0
- Branch: `phase/M11-P03-verification-and-0-2-7` (stacked on the P02 tip; code commit `9ca6260`)
- Plan: [M11-P03](../../plans/202611/20261113/20261113_15_M11-P03_verification_and_0_2_7_plan.md)

## What changed

- `lico` is `0.2.7` (`Cargo.toml`, `Cargo.lock`, `tests/test_single_executable.py`, `tests/test_control_parity.py`; the two version tests were run red against `0.2.6` first).
- `tests/test_native_end_to_end.py` (private Xvfb, synthetic homes, mocked providers, no billable turn; the other terminal is a fake: a script named `claude` that waits in the project folder): a person with chats from before Lico gets the first-run question (`Claude Code: 2 chats in ~/.claude`), answers Yes, finds the chats in the Chats tab labelled `Native (~/.claude)` with only the chat the other terminal writes marked `[elsewhere]`; the dialog lists the managed accounts and both Native entries, shows the live chat as the latest with the sentence that it cannot be resumed from here, refuses Enter on Resume and starts nothing; the closed chat of the same folder resumes from the Chats tab (program started with `--resume`, its process environment is the synthetic default home and nothing of a managed account, the header beside the input target says `Native (~/.claude)`); an account without a chat here says `2 chats of this folder exist under other accounts or homes (Native (~/.claude): 2)`. Repeated 4 times, 4 green.
- Inspection bundle (local only): `private/manual_inspection/202611/20261113/00007_M11-P03_native_and_running_elsewhere/` with `1_first_run_question.png`, `2_chats_tab_elsewhere_mark.png`, `3_dialog_native_live_chat_refused.png`, `4_dialog_hint_other_accounts.png`.
- Docs: CHANGELOG `0.2.7`, PLAN, roadmap, README, manual, `doc/index_4.md`, milestone and plan statuses.

## Measured verification (`lico 0.2.7`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `636 passed; 0 failed` (`1` ignored, both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`), three runs | `517` tests in `105` files, `517 passed` each: `115.8 s` / peak `1,708.3 MiB`; `116.9 s` / `1,645.1 MiB`; `114.5 s` / `1,775.9 MiB` | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle window RSS, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on, 3 launches) | `max = 25.10 MiB` (`25.01..25.10`), median PSS `16.66 MiB`, anon `3.94 MiB`, host `7.92 MiB` | `< 50 MiB` | Met |
| Idle window RSS, scale 2 | `max = 36.29 MiB` (`35.91..36.29`), median PSS `22.14 MiB`, anon `3.94 MiB`, host `7.93 MiB` | `< 50 MiB` | Met |
| Hidden background growth (`hidden_growth`, 10 vs 100 sessions, 3 repetitions) | `max = +0.23 MiB` (deltas `+0.20`, `+0.23`, `+0.20`; GUI median `25.27` -> `25.47 MiB`; host `9.02` -> `20.84 MiB`) | `<= +10 MiB` | Met |
| Two clean clones build identical binaries (`suite acceptance`) | `1 passed` in `101.26 s` | identical | Met |
| Live check in a 20-item listing (P02) | median `13.8 ms` with the scan, `0.9 ms` without | no regression beyond the scan | Met |

The measurements ran on the committed clean tree (`9ca6260`) in the heavy lane, one after the other, on a shared host with a load average of about 5 to 10 on 24 cores. The host's idle RSS rose from `7.61` / `7.70 MiB` (M10) to `7.92` / `7.93 MiB`: about `0.3 MiB` for the new modules and the per-listing `/proc` read; still far below the budgets.

## The four tests that failed once each in M10

In M10, `test_call_rooms_and_mentions`, `test_tool_history`, `test_first_view` and `test_picture_paste` each failed once inside a full parallel run and passed when run alone: four flaky failures in about eight full runs. In M11 I made six full runs: P01 one cold run (10 failures, all my own and deterministic: a schema field and a test-map entry) and one changed-suite run (clean); P02 one changed-suite run (clean); P03 three cold runs (clean, above). Every commit also passed its hook run. In none of these six did one of the four tests fail for a flaky reason: 0 flaky failures in 5 clean full runs, 3 of them cold on the final tree. That is a cleaner sample than M10's, but the host load differs from hour to hour and I did not run the M10 tree next to the M11 tree under the same load, so I do not claim that M11 changed the cause, only that this sample is clean. I still do not know the cause of the M10 flakes.

## Honest notes

- The end-to-end test was written before the version bump and the first run needed two corrections of my own expectations (Codex has no managed account in that window, so its Native entry is open and listed; the host sets no `CLAUDE_CONFIG_DIR` there, so the program uses `$HOME/.claude`).
- Native for a provider with no managed account is open without a question (the legacy first run, now labelled): a window of a fresh install lists `Native (~/.codex)` in the dialog as soon as it has no Codex account. That is decision 2 of the plan.
- The `Resume` action button of the dialog stays green when the latest chat is live; Enter is refused with the sentence, but the button is not greyed. Left as is.
- Not exercised: a real Claude Code or Codex default home or terminal (by rule). The owner's check at G3 with a real open terminal on a project folder is the real test of the live signals.
- Install: see the mailbox report (the installed file is built by `src/host/hostd/build-release.sh` from the clean closing commit; the `0.2.6` binary is kept at `private/release_0_2_6_backup/lico`).

## Not done / left to the owner

G3 acceptance and G4 merge of M10 and M11; the milestone branches `milestone/M10-...` and `milestone/M11-native-provider-home-chats` are created at the closing commits after acceptance.
