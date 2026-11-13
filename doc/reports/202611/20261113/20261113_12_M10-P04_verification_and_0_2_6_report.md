# M10-P04 Report — End-to-End Verification, Measurements, and `lico 0.2.6`

- Status: implemented, awaiting owner inspection (G3) and merge (G4)
- Version: 1.0.0
- Branch: `phase/M10-P04-verification-and-0-2-6` (stacked on the P03 tip; code commit `438f573`)
- Plan: [M10-P04](../../plans/202611/20261113/20261113_12_M10-P04_verification_and_0_2_6_plan.md)

## What changed

- `lico` is `0.2.6` (`Cargo.toml`, `Cargo.lock`, `tests/test_single_executable.py`, `tests/test_control_parity.py`; the two version tests were run red against `0.2.5` first).
- `tests/test_chats_end_to_end.py`: the whole M10 flow in one window on mocked providers: tab labels; the Chats tab lists the folder's chats newest first; `lico chat latest` returns the newest chat that has a transcript (the open blank tab is not one); the dialog shows the same chat with `(2 turns)`; Resume opens a tab whose transcript is that chat (`--resume`, seeded text in its log); the Chats tab then shows it open; `session.past` is an ordinary `unknown_op`.
- The Chats tab now refreshes every time it is opened (found by the end-to-end test: it kept an earlier listing and did not show a chat opened since as open).
- Docs: CHANGELOG `0.2.6`, PLAN, roadmap, README, manual, `doc/index_4.md`, milestone and plan statuses.

## Measured verification (`lico 0.2.6`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `626 passed; 0 failed` (`1` ignored, both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`), third run | `504` tests in `100` files, `504 passed`, **`109.6 s`**, 4-worker tree peak **`1,732.4 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle window RSS, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on, 3 launches) | `max = 25.18 MiB` (`24.80..25.18`), median PSS `16.80 MiB`, anon `3.94 MiB`, host `7.61 MiB` | `< 50 MiB` | Met |
| Idle window RSS, scale 2 | `max = 36.08 MiB` (`35.82..36.08`), median PSS `22.07 MiB`, anon `4.12 MiB`, host `7.70 MiB` | `< 50 MiB` | Met |
| Hidden background growth (`hidden_growth`, 10 vs 100 sessions, 3 repetitions) | `max = +0.35 MiB` (deltas `-0.01`, `+0.35`, `+0.20`; GUI median `25.20` -> `25.43 MiB`; host `8.91` -> `20.56 MiB`) | `<= +10 MiB` | Met |
| Two clean clones build identical binaries (`suite acceptance`) | `1 passed` in `89.65 s` | identical | Met |
| `session.chats` latency, 2,500 transcripts (P02) | `limit 20`: `23.4` -> `24.2 ms` median | no regression beyond noise | Met |

The measurements ran on the committed tree (`438f573`, clean) on a shared host with a load average of about 6 to 8 on 24 cores, in the heavy lane, one after the other. Raw results are in `private/measure/` (local).

## Honest notes

- **The cold suite was not green on the first two runs.** Run 1: `504` tests, `502 passed`, `114.9 s`, peak `1,917.3 MiB`; one failure was my own retired-names scan reading the new end-to-end test, which says the old word on purpose (fixed: the scan skips that file and itself), the other was `tests/test_first_view.py`, 15.6 s over its 15 s budget. Run 2: `503 passed`, `112.4 s`, peak `1,976.3 MiB`; one failure in `tests/test_picture_paste.py` (a note waited for 10 s). Run 3 is the clean one above. The time and memory bounds were met in all three runs, but the memory peaks of `1,917`, `1,976` and `1,732 MiB` show a spread of about 245 MiB between runs of the same code, so the margin to `2,048 MiB` is thin and load-dependent.
- Across M10 five different GUI or tool tests failed once each inside a full parallel run and passed when run alone (`test_call_rooms_and_mentions`, `test_tool_history`, `test_first_view`, `test_picture_paste`, and the P03 click-area race that was a real fault and is fixed). The rest are timing-sensitive tests on a loaded shared host; I did not find a cause that points to M10, but I cannot rule out a small contribution either (for instance the added hit-target registration per frame). A comparison run of the old tree under the same load was not made.
- The two version tests and the new end-to-end test were not all run red on their own: the version tests were; the end-to-end test passed on its first run except for the Chats tab refresh above.
- Install: see the mailbox report (the installed file is built by `src/host/hostd/build-release.sh` from the clean closing commit; its `lico --version` line carries that commit).

## Not done / left to the owner

G3 acceptance and G4 merge; the branch `milestone/M10-chats-tab-rename-and-resume` is created at the closing commit after acceptance, as for the earlier milestones.
