# M17-P10: the browser engine installs itself when a tab is opened, release 0.5.1

- Version: 1.0.0
- Date: 2026-11-17
- Branch: `phase/M17-P10-engine-auto-install-0-5-1`, stacked on the M18-P01 tip `1943dd8`; last code commit `1592da5` (the release was built from it)
- Source: lead mail 155 (the stop rule was raised to 9 by mail 156). **Requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner.** This reverses one rule of M17 ("Lico never downloads the engine"); everything else of M17 stays.
- Labels: MEASURED (a run of this phase), READ (from the code), COMPUTED.

## 1. What the person sees

Opening a browser tab (the window's `+B`, or `lico browser open`) with no engine installed shows "installing the browser engine (about 72 MB from github.com, SHA-256 starts eed986e478c5)", with no question; when the install ends the engine starts and the page asked for loads. `lico browser open` waits and prints the same, then "the engine is installed and the tab is starting". `lico browser list` shows `installing`, then `starting` / `ready`, or `absent` with a `notice` when the install failed (a hash that is not the pinned one begins `SECURITY:`) and the manual steps. Nothing is fetched at host start or without a tab. One attempt for each tab opened, no retry loop; two tabs opened together install once. `{"auto_install": false}` in the owner-only `browser.json` turns the self-install off (it can only turn it off).

## 2. How it is built (READ)

- `lico-engine-pack` 0.1.0 (new, pure, no dependency): the pinned release, address and three SHA-256 values, the hash verdict (`verdict`), the judgement of a `tar -tv` listing (`check_listing`: exactly the two named regular files; refuses `..`, absolute paths, links, devices, pipes, folders, a third or a repeated file, a size over the cap), the plain failure words and the security flag, the manual steps.
- `lico-browser-view` 0.6.0: `Phase::Installing`, `EngineEvent::{Installing, Starting, InstallFailed}`; the F1 help says the engine installs itself.
- Adapter `browser/installer.rs`: the system `curl` as a helper process (cleared environment, `-q`, `--fail`, `--proto =https --proto-redir =https`, `--max-filesize`, `--max-time`, `--noproxy '*'`) into a fresh `0700` staging folder beside the destination; the archive is hashed through the open file and compared with the pinned SHA-256; the system `tar` lists it (the pure part judges the listing) and unpacks it with `--no-same-owner --no-same-permissions`; both files are hashed again against their pins, set to `0700`; one atomic rename puts the folder in place; the staging folder (and the archive in it) is removed on every way out.
- `browser/ops.rs`: `Browser` is built with `Arc::new_cyclic`; an install thread starts when a tab is opened and no engine is found; when it ends it starts the engine for every waiting tab (or tells each why not) and wakes the watchers (no timer).
- No TLS and no new dependency in Lico's binary (the only new crate is the pure `lico-engine-pack`).
- The test seam: the cargo feature `install_seam` (built only by `cargo xtask system`) lets the host take an address and hashes from a file named by a variable; the plain build, the suite and the release have none of it (a test checks that the variable's name is not in the plain build and is in the seam build).

**The curl sandbox, as it is: curl is NOT run under the sandbox.** It runs with a cleared environment, no configuration file, no proxy, https only, in the private staging folder, started by the adapter; what it fetched is used only after the pinned SHA-256 matches, and the archive is then listed before anything is unpacked. What this means: a hostile server cannot make Lico run or keep anything that is not the pinned archive; but the curl process itself has the user's file rights and any network access while it runs (a fence of TCP 443 only was not tried because name resolution under the fence was not measured; no claim is made).

## 3. Tests (MEASURED)

| Check | Result |
|---|---|
| `lico-engine-pack` contract tests | 6 passed; mutants on `verdict` and `check_listing`: 10 made, **9 killed**; the tenth (removing the `..` test) is equivalent: the named-file check right after it refuses every such name. Four mutants survived the first tests and were killed by tests added (a one-bit difference in a digest, a duplicate engine, a stranger as the second file, a link as the worker) |
| `lico-browser-view` 0.6.0 | 36 passed (3 new), receipt recorded |
| Installer against a fake release server and fake archives (host unit tests) | 8 passed: good archive (modes, nothing else left), wrong archive hash (security failure, nothing left), wrong pin of a file, truncated, over the cap (also when curl cannot know the size), curl missing, no network, a full disk, `../`, absolute, link, pipe and a stranger file, the race with another installer. Mutants on the decisions: **6 of 6 killed** (two needed tests added) |
| Install flow in the browser manager | 3 passed: `installing` reply with size, source and hash start; a wrong hash ends the tab as a `SECURITY:` notice with nothing on disk; two tabs, one download; a tab opened later is a new attempt; an engine already there costs zero requests. Mutants: **3 of 3 killed** (one by a hang of the test, not by an assertion) |
| CLI | the help flag test (`--localhost` was in the help, only `--local` was accepted: fixed; every flag the help names is accepted and the reverse), the lines the command prints |
| System test `browser_engine_installs_itself` (real host binary built with the seam, real `curl` and `tar`, fake release server, fake engine) | passed, 0.5 s, peak 29.4 MiB: the command says installing and waits, the engine is in the pinned place with modes `0700`, no staging folder or archive left, the page paints, the second tab makes no request. All nine system tests pass (peaks 19.1 to 29.4 MiB; the input one 23.2 s of 60) |
| Whole suite | first run (with its build): 565 tests, 563 passed, **2 failed**: the two tests that pin the version `0.5.0` (fixed to `0.5.1`; both pass); 142.0 s, peak **1,953.3 MiB** of 2,048 (margin 94.7 MiB, thinner than the 150.8 of M17-P09: the build and the new test binary; not traced further). Last run: **567 passed of 567**, 139.0 s, peak 1,645.9 MiB; its build step was skipped (the sources matched the binary), so it is not a fully cold run |
| `check`, `verify`, `graph` | green; graph: 199 modules, the biggest cycle still 48 |
| Two clean clones | identical, `164a14e0839ab7ab6bf2ac2d2244929504b3a4bb70f93f4a7abbdcc035c2ae23` |

## 4. The one real end-to-end acceptance (MEASURED, once)

The release artifact above, in a private test home (not the owner's), one real download of the pinned archive from its release page (hash-checked), `lico browser open --local` of a loopback fixture page with Korean text, window and host running on a private display:

- the command said "installing the browser engine: about 72 MB from github.com, SHA-256 starts eed986e478c5" at once, and ended **5.28 s** after the start (download, hash, unpack, move: the sum; the download alone was not timed separately because a second download was not allowed);
- the first painted frame (`has_frame`, `ready`, the Korean title right, `unpinned` false: the hash was checked) came **5.55 s** after the open command; `shot` wrote a 1024 x 768 PNG, 29,283 bytes;
- RSS: host idle 7.5 MiB, **during the install at most 7.5**, with the page painted 11.1, after close 8.1; window idle 22.7 MiB, during the install 22.7, with the page painted 22.7, after close 22.7 (the window sampled every 0.25 s, 21 samples during the install);
- after `close` (engine stopped): 0 processes naming the private home (no curl, tar or engine), the browser folder holds only `v0.2.6` (no staging folder, no archive), both files mode `0700`;
- the second open in the same home: no installing line, **0.58 s** to the command's answer, painted after 0.74 s, the install folder untouched.

The record is `private/scratch/202611/20261117/m17_p10_real/accept_real.txt` (with a window picture of the real page).

## 5. The window pictures (MEASURED)

`private/scratch/202611/20261117/m17_p10_screens/`: `1_installing.png` (the header says installing, about 72 MB, the source, the hash start, no question), `2_ready_painted.png` (the page painted, `ready`), `3_failed_install_notice.png` (a wrong hash from the fake server: the `SECURITY:` notice and the manual way). They were taken with the seam build and the fake engine and a fake server; the real page is in section 4. (The header's manual line was changed after the pictures, in `1592da5`, to name the archive's SHA-256 beside its address; the pictures show the older wording with the engine file's hash.)

## 6. The release

`lico 0.5.1 (commit 1592da5, build 253fd8822fc72e8b)`, `10,722,312` bytes: **+216,568 bytes (+2.06 %) over 0.5.0** (`10,505,744`). Installed to `~/bin/lico` (atomic replace); the 0.5.0 binary is kept at `private/release_0_5_0_backup/lico` (sha256 `f09c2a6e...4925`, a different file from `private/release_0_4_0_backup/lico`). The installed copy takes effect at the next start; nothing was killed (an old host is running).

## 7. The owner's own check in their real home (to do by the lead with the owner)

Their `~/.local/share/lico/browser` is empty, which makes it a true first run. It needs the system `curl` and `tar` and a network. 1. `lico host stop` (ends the old 0.5.0 host). 2. `lico gui` (starts 0.5.1). 3. Click `+B`, type an address (for a first look at a real site: the owner's own choice) and press Enter. 4. Watch the header: "installing the browser engine (about 72 MB from github.com, SHA-256 starts eed986e478c5)", no question; on this machine's line it took about 5 s in the private test; a slow line may take longer (the transfer is stopped after 15 minutes). 5. The page loads by itself. Afterwards `~/.local/share/lico/browser/v0.2.6` holds `obscura` and `obscura-worker` (mode 0700) and nothing else under `browser/`. A failure looks like: the tab turns `absent` and its header says why ("no network", "curl was not found", "no room", or, shown as `SECURITY:`, "the downloaded file is not the pinned release") and then the manual way; nothing is left on disk; typing an address in that tab is a new attempt. The command line does the same: `lico browser open https://SITE` prints the installing line and waits.

## 8. Not done, not claimed

- curl is not under the sandbox (section 2). The download time alone was not measured. The plain release was not run against a failing real network (the failures are tested with fake servers). The tab picture 3 shows the old manual wording (section 5). The whole suite's last run had its build skipped (section 3).

## 9. Cost

The counter stood at 89.7 USD before P10 and at about 97.7 when this report was written: **about 8.0 USD** against the estimate of 3 to 5 and the stop rule of 9 (raised from 7 by mail 156). Most of it was the slow queued builds and waits of the release build, the test binary link and the system build (each wait turn re-reads the context). The ledger limit of 230 was not reached.
