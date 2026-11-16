# The browser adapter (`browser/`) — specification

Version 1.0.0 (M17-P06). Plan and approvals: `doc/plans/202611/20261117/20261117_19_M17-P06_browser_host_adapter_plan.md` (approved by the lead under the owner's delegation, mails 149 and 150). **MEASURED** facts about the engine are in the two measurement notes of M17; marks below say what is proved by a test of this crate.

## What it is

A browser tab with no window yet. The host starts the engine (Obscura, a pinned release) only when a tab asks, behind `lico-sandbox` and the fetch guard `lico-proxy`, drives it with `lico-cdp`, keeps one `lico-browser-view` model per tab, and ends the engine with the last tab. It plugs in through the operations port of `driver.rs` (`register_ops`, `ops_valid`, `ops_call`, `stop_all`) and is registered once from `unified.rs`; no hub names it, so the strongly connected component of the module graph is unchanged.

## Operations (socket and `POST /v1` answer the same: one validation and one dispatch)

| Operation | Fields | Answer |
|---|---|---|
| `browser.open` | `url` (http or https), optional `localhost` (this tab may reach this computer's loopback addresses) | `tab` (`b1`, `b2`, ...), `phase`; `phase` `absent` with `release`, `download`, `archive_sha256`, `engine_sha256`, `directory`, `put` when the engine is not installed. Errors: `bad_request`, `tab_limit`, `engine_unsafe`, `engine_not_pinned`, `engine_setting`, `engine_unreadable`, `engine_fence`, `engine_failed` |
| `browser.list` | none | `tabs` (tab, url, title, phase, loading, notice, viewport, has_frame, localhost, unpinned, ...), `engines` (one per network class: running, unpinned, what the engine says it is), `max_tabs` |
| `browser.close` | `tab` | `closed`, `engine_stopped` (true when it was the last tab of that engine); `unknown_tab` |
| `browser.shot` | `tab`, absolute `path` | the newest frame decoded by `lico-frame` and written again by `lico-png` to a NEW file of mode 0600 (never over a file); `width`, `height`; waits up to 3 s for a first frame; errors `no_frame`, `exists`, `bad_request`, `write_failed`, `unknown_tab` |

## Limits

At most **8 tabs** (`MAX_TABS`) in all: each is a page in the engine; on Lico's side a tab holds at most one compressed frame (4 MiB cap, so 32 MiB for eight, COMPUTED) and, while it is painted, one decoded picture (9.2 MB at the largest size). At most **two engines**: one for ordinary tabs (the guard refuses private, loopback and other non-public addresses) and one for tabs opened with `localhost` (the guard also allows loopback); each has its own guard. A tab opened from the command line is shown at 1024 x 768 so that it has a screencast.

## The engine file

`DATA/lico/browser/v0.2.6/obscura` (DATA is `XDG_DATA_HOME`, else `HOME/.local/share`) with `obscura-worker` next to it; Lico never downloads. Accepted only if the directory and EVERY directory below the home directory down to it are real directories (no symbolic link) owned by the user and not writable by group or others, and the file is a regular file of the user, opened once without following a link, not writable by others, with an execute bit. The open file is hashed from its descriptor (SHA-256, product constants) and run through THAT descriptor (`lico_sandbox::confine_fd`): what runs is what was hashed, whatever is put at the path afterwards. What remains, said exactly: `obscura-worker` is hashed with the engine but the engine's `serve` does not start it (one `execve` only, measured); if a later engine version starts it, it would be started by path. **No environment variable and no flag skips the hash.** The only other engine is the one named in the owner-only file `browser.json` of the state directory (`{"engine_dir": "/absolute/path"}`, a regular file of the user with no group or other rights; there is no operation to set it); every tab of such an engine shows "running an engine that is not the pinned release". A damaged setting is an error, never a fall back.

## The fence (decided by the lead, mail 149)

Read and run the engine's directory and `/usr`, `/lib`, `/lib64`; read `/etc/ld.so.cache`, `/dev/urandom` and the Korean font directory; write one scratch directory (mode 0700, under the state directory, removed at the end) and `/dev/null`; TCP connect to the guard's port ONLY; bind the CDP port. Environment: the CDP token, `HOME` (the empty scratch directory), `OBSCURA_PROXY` with per-start credentials; the token and the credentials are never on a command line. Limits: `RLIMIT_DATA` 4 GiB and 1024 open files. MEASURED with the real engine (P06 probe): at 1 GiB the engine cannot start (V8 fails to reserve its code range); at 2 and 3 GiB it starts and a page that allocates without end stops it (the host stays up); at 4 GiB the same, and it is the first value that is not flaky; with no limit that page runs on. So the cap binds at the figure given, and an engine that dies is shown in its tabs. The ports are picked by Lico (a bind error is retried up to 3 times with a log line). What the fence does not close is on the first screen of the P03 measurement note.

## Processes, threads, wake-ups

No process, thread or socket exists without a tab. One thread per engine (it spawns the engine, so the engine's death-with-parent signal follows it) owns the CDP client, feeds the models, executes their effects and ends everything at the end. **While an engine runs that thread wakes ten times a second** (the client's 100 ms read timeout) and so does the guard's accept loop (20 ms): the first item of the input phase removes the first, and the guard's accept poll is reported with the measured CPU time. The engine is ended with `Browser.close`, then its process group is killed after one second; the host's stop ends every engine first. An engine that dies on its own gives its tabs the `failed` phase, is cleaned up (guard stopped, scratch removed), and is started again by the next `browser.open`.

## What a page can and cannot do

Nothing from a page reaches a Lico operation, tool, session or clipboard: the host reads only a frame (decoded by `lico-frame`), an address, a title and the engine's navigation events, all cleaned and bounded by the model. The honest user-agent `Mozilla/5.0 (X11; Linux x86_64) Lico/VERSION Obscura/RELEASE` is set for each page session (`Obscura/unpinned` for an unpinned engine); the page may still see a Windows `platform` and `vendor` because the engine decides those. A refused or failed fetch gives the engine no error and a blank page: the guard's refusal (403, 502, 504) becomes a notice (host only) in the loading tabs with that host, else in every loading tab (the guard does not know the tab: a limitation). The engine refuses an address on this computer or a private network BEFORE it asks the guard, by number and as the name `localhost` (MEASURED, P05 for numbers and P06 for the name: `Page.navigate` answers an error); the tab says "the engine refuses addresses on this computer or a private network, by number and as localhost". **So the per-tab `localhost` opt-in has no effect with this engine and this fence profile** (no private-network variable): an opted-in tab gets the same refusal. With `OBSCURA_ALLOW_PRIVATE_NETWORK=1` in the engine's environment the engine passes both to the guard, which then decides (MEASURED once, for the question to the lead only: the fixture was reached through the guard for an opted-in tab and not for an ordinary one); that variable is NOT set.

## Logging

The host's log says: the engine started or ended, its exit code, the HOST of a page, a bind retry, a failed call. Never a token, a port, a credential, a path, a query, page content or a frame.
