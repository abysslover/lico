# M22-P02 Plan — The Chat Index: A List That Is Ready Before It Is Asked For

- Status: planned (2026-11-18)
- Version: 1.0.0
- Branch: `phase/M22-P02-chat-index` (stacked on `phase/M22-P01-chats-of-all-sessions`, `0900c95`)
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md)
- The owner's words (the chat, 2026-11-18): listing everything may need performance work and a background index; experiment, think it through, leave no delay.

## The problem, measured on this machine's own homes (timing only; nothing of their text was printed or kept)

`session.chats all` with the provider's own homes included, a page of 100: first call 147 ms, a repeat 19 ms, the second page 79 ms; the host's resident memory 8 MiB. Three causes:

1. **Every call opens the head of every record** (up to 64 KiB each) to learn its folder and title, though a record's head does not change when it grows.
2. **Every page opens each record of the page whole** to count its turns (`turns::count`, up to 8 MiB each): the 79 ms.
3. **A defect:** the listing counts every regular file, not only `.jsonl`. Codex keeps 2,415 files for 50 chats, so the listing hit its bound and only 153 of 157 chats were found, with `truncated` true. (Claude's project folders hold only `.jsonl` files at the level that counts: 107.)

The numbers are small today and grow with every chat, and a cold disk (a host started long after the files were last read) costs far more than a warm cache.

## The design: one index in the host, read by every scope

- **What it holds, per home** (a provider and a directory: an account's home, or the provider's own): for each `.jsonl` record, its path, size and modification time, and what the head said (session id, folder, title). A turn count is added to a record the first time a page needs it and is kept with the (size, time) it was counted at.
- **When it is fresh:** a call refreshes the home it reads before answering, by an INCREMENTAL pass: list the directories, `stat` each record, open a record again only when its size or time changed or it is new, drop one that is gone. A record that has not changed is never opened again. This keeps the answer exact (a record written a moment ago is listed; the tests that write and then ask still hold) and costs one `stat` per record, measured in step 2.
- **Built before the first call:** a thread started with the host builds the index of every home that exists then (the provider's own homes when included, the accounts' homes), so the first call finds it ready and the disk was read in the background. A home that appears later (a new account, the setting turned on) is built by the first call that needs it, and the answer says nothing different.
- **No call waits behind another:** a home's index is behind its own lock; a refresh holds it for its own pass only; the builder thread takes the same lock a call takes, so the worst wait is one pass.
- **Bounded:** a home holds at most `MAX_SCAN_FILES` (2,500) records, the newest by modification time, and says `truncated` when it left older ones out; a record's title and folder are as long as today's. About 300 bytes a record.
- **Every scope reads it:** a folder, Home (the wire's `global`) and `all` filter the same records. `scan_claude_home`, `scan_codex_home` and `newest.rs` (P01's listing for `all`) are replaced by the index, so there is one way to read a home and a cap cannot differ between scopes.
- **Not persisted:** the index is memory only. A record's text (titles) is not copied to a second file. The measured first build must be short enough that this costs nothing.
- **No watcher:** a file-watching mechanism (inotify) is not used: it needs a dependency or unsafe code, it misses events on network mounts, and the incremental pass is already cheap. If step 2 shows the pass is not cheap enough, a watcher is considered then, with the measurement.

## The provider's own home is on by default

`native::state` is `on` unless the person chose `off`; the `Ask` state and the question that goes with it are no longer reached (`ask` in `settings.set` still takes a choice back, which now means `on`). The window's native prompt is removed in P03 with the rest of what the window no longer asks. `account.list`'s `native` entries say `on`, and their `chats` count is the index's. The manual says that Lico reads the provider's own home by default and how to turn it off (`lico settings --native-claude off`).

## Experiments (each one a number in the report; the budget is the lead's, to be confirmed by the owner)

Budget: a warm page in **under 5 ms**; the build in the background **under 1 s** for this machine's homes; **no call slower than 50 ms** with a cold page cache; a refresh after one new record under 10 ms; memory under 2 MiB for 2,500 records.

| # | Experiment | How |
|---|---|---|
| E1 | the baseline (done, above) | the real homes, timing only |
| E2 | the cost of the incremental pass | `stat` of N records warm and after `posix_fadvise(DONTNEED)` on them (a cold page cache without root), N = 100, 1,000, 2,500, 5,000 synthetic and the real homes |
| E3 | the build | the head reads of the real homes, warm and cold, and of 5,000 synthetic records; one thread, then 4 threads, to see whether the disk or the CPU is the limit |
| E4 | the turn counts | the page of 100 with the cache cold, with it warm, and after one record grew |
| E5 | contention | a call while the builder thread holds the lock; 8 calls at once on one home |
| E6 | the end to end | the same probe as E1 against the new host: first call, repeat, second page, after a new record |

If E2 shows the pass above the budget on a cold cache, the answer is a background refresher (a thread that refreshes every home that was read in the last minutes, and calls read its snapshot without waiting) and the exactness is lost for tests: that choice is then written down with the measurement before it is taken.

## Steps (tests first; green-only commits)

- [ ] 1. Red tests: the Rust unit tests of the index (a record that did not change is not opened again, counted by a counter in the test; a grown record is read again; a deleted one goes; a link is never followed; only `.jsonl`; the bound and `truncated`; the turn count is kept and recounted when the size changed); a system test with Codex's other files in the home (the defect) and 3,000 records (a count, the order and the bound); `native` default `on` in the existing native tests (they say `ask` today: they change first and fail first).
- [ ] 2. E2 and E3 on the real homes and synthetic ones; the budget is confirmed or the design changes before more code.
- [ ] 3. `chats/index.rs`: the index; `collect` reads it for the three scopes; `newest.rs` and the two scanners are removed (their tests move to the index); `turns` read through the cache.
- [ ] 4. The builder thread at host start; the setting's change builds the home at once.
- [ ] 5. `native::state` default; `account.list`; the manual and the guide.
- [ ] 6. E4, E5, E6; the figures in the plan and, later, the report.
- [ ] 7. `suite run --changed --receipts`, `cargo xtask check|verify|graph`; commit.

## Stop and ask

Stop only if: E2 or E3 shows a budget cannot be met without a persistent index (a store of the titles in the state directory, which is the person's text) or a watcher; the default `on` of the provider's own home breaks a test that cannot be changed without hiding a real behaviour; or a step needs a push or a publication.
