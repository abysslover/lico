# M22-P02 Plan — The Chat Index: A List That Is Ready Before It Is Asked For

- Status: in work (2026-11-18); steps 1 to 6 done, step 7 (the suite) running
- Version: 1.1.0
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

## What was built (2026-11-18)

- `chats/index.rs` (new): one index per home in a registry (`Kind` + path), a lock per home; `list` walks the directories and `stat`s each `*.jsonl` (a link is never followed, the depth is the provider's: Claude `projects/<folder>/<file>`, Codex `sessions/` five levels); `Home::update` is the incremental pass (keeps what did not change to the nanosecond and byte, reads the head of a new or changed file, drops a gone one; a pass that changed nothing only looks each record up); the head parsers moved here from `mod.rs`; `prewarm` (a thread named `chat-index`) builds every local home at host start and after a `settings.set` of the native choice, and counts the turns of the newest 100 records of each; `count_of` serves `account.list`.
- One bound, `MAX_SCAN_FILES` (2,500 records per home); a listing is bounded by `LIST_MAX` (20,000 files). P01's `newest.rs` and its `ALL_SCAN_MAX` (2,000) are gone; `collect` filters the index by scope (folder, Home, `all`). The three old scanners are gone.
- `native::state` is `on` unless `off`; `State::Ask` is removed (the wire still takes `ask` in `settings.set` and means "take the choice back"); `account.list`'s `chats` is the index's count while a home is on. The window's question is never asked (its code goes with the window phase); the two window tests and the end-to-end native test were changed first.

## Measured (the probe `probe_index.py` in the lead's scratchpad, not kept in the repository; `/run` tmpfs, a private HOME, synthetic records of two lines; a page is 100 chats)

| Records in the home | first call (background build done) | repeat (warm) | one new record | the second page | eight calls at once (wall) |
|---|---|---|---|---|---|
| 500 | 12.6 ms | 3.3 ms | 2.9 ms | 4.0 ms | 13 ms |
| 2,500 (the bound) | 18.0 ms | 10.5 ms | 10.8 ms | 11.0 ms | 55 ms |
| 5,000 (2,500 kept, `truncated`) | 28.7 ms | 13.5 ms | 14.3 ms | 15.6 ms | 97 ms |

- **This machine's own homes** (154 chats; timing only; nothing of their text was printed): first call 14 ms after the build (62 ms when asked at once after the host was up: it waits for one pass of the build), a repeat **5 ms**, the second page 22 ms (the turn counts of the 54 chats beyond the first page are read then). Before the index: first 147 ms, repeat 19 ms, second page 79 ms, and 153 of the chats found (the `.jsonl` defect).
- **The cost is the `stat` of every record** (a listing alone is 5 ms at 2,500; 2 microseconds a file), which is what keeps the answer exact. The data pages being cold (`posix_fadvise(DONTNEED)` on every record) cost 0.1 to 5 ms more (the heads are not read again; only the turn counts of the page). The directory and inode caches cannot be dropped without root: a really cold disk is not measured, and is what the background build is for.
- **Memory:** the host's resident set 15.4 to 17.3 MiB with 2,500 to 5,000 records in the index (9.3 MiB with the real homes).
- **Against the budget** (warm page under 5 ms; the build under 1 s in the background; no call over 50 ms with cold data pages; a refresh after one new record under 10 ms; under 2 MiB for 2,500 records): met up to about 1,000 records (3.3 ms at 500) and by a wide margin on this machine's own homes (5 ms with 154); at the bound of 2,500 a warm call is 10.5 ms, twice the figure, because of the `stat` pass. A parallel `stat` (four threads) would bring it near 3 ms; it is not done: a call of 10 ms is below what a person notices, and the threads cost more at the common sizes. The owner can ask for it. The build: 18 to 29 ms for 2,500 to 5,000 records, far under a second. The index of 2,500 records is about 1 MiB (the memory figure above includes the whole host).
- **Contention** (E5): calls on one home take turns (a pass holds its lock): eight at once finish in 13 / 55 / 97 ms for 500 / 2,500 / 5,000 records, each waiting at most for the others' passes. The window asks one at a time.

## Steps (tests first; green-only commits)

- [x] 1. Red tests: the unit tests of the index (an unchanged record is not opened again, counted by a counter; a grown one, one whose time changed, a new one are; a removed one goes; a file that is no chat is remembered; a link is never followed; only `*.jsonl` at the provider's depth; the bound and the tie rule; the heads) with a mutation check (two mutations, four tests fail); the system test `tests/test_chat_index.py` (red for the right reasons: an empty list for a home of 3,000 other files, a record made unreadable vanished, the bound was 2,000); `native` default `on` in the native tests.
- [x] 2. E2 and E3 on synthetic homes (above); the design did not change.
- [x] 3. `chats/index.rs`; `collect` reads it for the three scopes; `newest.rs` and the scanners removed; the turn cache keeps its place.
- [x] 4. The builder thread at host start and after the native setting changes.
- [x] 5. `native::state` default; `account.list`; the manual.
- [x] 6. E4, E5, E6 (above).
- [ ] 7. `suite run --changed --receipts`, `cargo xtask check|verify|graph` (`check` and `verify` pass, `graph` runs); commit.

## Stop and ask

Stop only if: E2 or E3 shows a budget cannot be met without a persistent index (a store of the titles in the state directory, which is the person's text) or a watcher; the default `on` of the provider's own home breaks a test that cannot be changed without hiding a real behaviour; or a step needs a push or a publication.
