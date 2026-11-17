# M20: Moli is the engine of the browser tab, JPEG frames, release 0.7.0

- Version: 1.0.0
- Date: 2026-11-17
- Branches: `phase/M20-P01-moli-engine` (`23d0562`), `phase/M20-P02-jpeg-frames` (`fe313f9`), `phase/M20-P03-release-0-7-0` (stacked on each other, on the M17-P12 tip `8b7b420`); release commit `49ff991`
- Source: the owner's words in the chat, 2026-11-17: "As another alternative, look into moli and put it in Obscura's place, as an experiment"; then "make moli the default, the biggest problem, the large frames, has to be solved, obscura is scrapped"; "if we can take Moli's strengths and make it our own, that may be better"; "but if we change the code there may be dependency problems later, so we must treat moli or obscura as one component and manage the version internally". The lead built it under the owner's delegation (the owner away); every approval is the lead's and is to be confirmed by the owner.
- Labels: MEASURED (a run of this milestone), READ (from the code or the engine's own help), COMPUTED.

## 1. What changed in the rule

The browser tab runs on **Moli** (`lexmount/moli`, release `v1.1.25`, the official Linux binary, no patch). **Obscura is gone**: its pins, its install path, its stealth switch. The engine is no longer a pair of constants in the host: it is the component **`lico-engine`**, which Lico versions itself (section 3). BROW-01 to BROW-09 of the product spec follow (BROW-05, the stealth sentence, is rewritten again: no engine of the tab has a stealth switch; the owner's "Lico can use stealth" stands for an engine that has one). `lico-frame` reads JPEG, and the host asks the engine for JPEG frames (section 4).

## 2. What the person gets

- A browser tab that opens Google and lets you type Korean in its box (the owner's real wish, MEASURED in the trial of M17, [the trial note](20261117_31_M17_moli_trial_note.md): Obscura was sent to Google's "unusual traffic" page). The engine uses about 75 MiB idle (Obscura: 215).
- A page of 1920 x 1200 arrives as about 130 KB a frame, not 3.8 MB, so a large window is possible and the window's peak memory stays under its gate (section 4).
- The first tab installs the engine by itself as before (the pinned release, one file, hash on the open file), now about 46 MB.
- Gone: the `"stealth": true` setting of 0.6.2 (an old file that says it is read and ignored, not an error). See section 9: the owner has to confirm this.

## 3. The engine as a component: `lico-engine` 0.1.0 (P01)

It replaces `lico-engine-pack` and holds ONE record:

| Field | Value (READ from `components/engine/src/lib.rs`) |
|---|---|
| Upstream | `lexmount/moli`, release `v1.1.25`, source commit `080c3fc2...`, licence `MIT OR Apache-2.0` |
| Archive | `moli-x86_64-unknown-linux-gnu.tar.gz`, 45,655,260 bytes, SHA-256 `04c73204...087f` |
| The engine file | `moli` (112,357,968 bytes), SHA-256 `e452e116...2a25`; one member of the archive, taken out with `--strip-components=1` |
| Lico's version of the record | `0.1.0` |
| The engine contract | number `1`: how the host starts the engine, finds its socket (the discovery page) and takes frames |
| Ledger of Lico's patches | empty: the file is the official one, byte for byte |

The host holds `HOST_CONTRACT = 1` and a compile-time assertion that it equals `lico_engine::CONTRACT`: a new upstream release, a patch of ours, or another engine is a change of that component's record and nothing else, which is what the owner asked for ("treat it as one component and manage the version internally"). The lead did NOT rewrite Moli (about 2.6 million lines): the large frames are met on Lico's side (section 4); the things only a patch can do (a token on the engine's socket, PNG compression in the engine) are **M21**, which waits for the owner's word (it means building and publishing our own binary).

The adapter (READ): `moli serve --host 127.0.0.1 --port N --layout --resource --screencast-interval-ms 100 [--ca-cert FILE] --log-level warn`; `HOME` and `TMPDIR` in the scratch folder, a fontconfig file the host writes (system fonts and the Korean font folder), the proxy of the guard in the proxy variables, the CA bundle FILE read through the fence; the socket path read from the discovery page (`/json/version`, strict check), never assumed; the user agent `Mozilla/5.0 (X11; Linux x86_64) Lico/VERSION Moli/RELEASE` set per page with `Emulation.setUserAgentOverride`.

**MEASURED with the real Moli, in the real host** (P01): the first frame comes in about 1.8 s; the user-agent override works (the header and `navigator.userAgent`), `navigator.platform` stays `Win32`; no private-network switch is needed (a local tab loads `localhost` and `127.0.0.1` through its loopback-only guard, an ordinary tab is refused by the guard alone and the fixture sees no request); click, Korean text, Backspace and the wheel reach the page; mouse moves reach only the element under the pointer; a page that eats memory ends the engine (`RLIMIT_DATA` 4 GiB), the host lives, the next `browser.open` starts it again; the engine's idle RSS is about 74 MiB.

**A bug found with the real engine and fixed (host, P01):** Moli numbers its page sessions again after a restart, so a tab of the dead engine kept the old session number and took the events of the NEW engine's tab (the new tab stayed "loading"). The supervisor now clears the target and the session of every tab of an engine that has ended; a system test pins it.

## 4. The large frame: JPEG (P02)

The trial found Moli's PNG frames 21 to 28 times larger than Obscura's: 3,773,168 bytes for a page of 1920 x 1200, against the 4 MiB cap of one frame, and a window peak of 51.7 MiB at scale 2 (gate 50). The owner's order was to solve it. The solution needs no change of the engine:

| | PNG (before) | JPEG quality 90 (now) |
|---|---|---|
| a frame of 1920 x 1200 | 3,773,168 B | 131,842 B |
| a frame of 1024 x 640 | 1,253,163 B | 76,533 B |
| decode, 1920 x 1200 (release build, best of five) | 46.9 ms | 6.0 ms |
| window peak at scale 2 (gate 50 MiB) | 51.7 MiB | **44.8 MiB** |
| a frame in the window at scale 2 | 1.9 MB | 57 KB |

(MEASURED, the same page and the real Moli 1.1.25; the 1024 x 640 and scale-2 lines are from the window probe.)

- **`lico-frame` 0.2.0** (std only, no `unsafe`) reads JPEG as well as PNG, told by the first bytes (`89 50 4E 47` or `FF D8 FF`): baseline and extended-sequential Huffman JPEG at 8 bits, grey or YCbCr, any sampling that divides, one scan, restart intervals. The arithmetic is libjpeg's (the accurate integer DCT of `jidctint` and the JFIF constants), so it is **identical to libjpeg 9** (`djpeg -dct int -nosmooth`) on all 17 files tried, every sample of 12 million pixels, including five real frames of the engine. The size is checked from the header before anything is allocated; every prefix, every bit flip and every header byte is tested to give an error and never a panic. Not read, and said so (`Unsupported`): progressive, arithmetic, lossless, CMYK, several scans. SPEC.md has the full list.
- **Test check by hand:** 118 mutants of the three JPEG modules (the parser and the scan 65, the Huffman tables and the bit reader 23, the inverse DCT 30): after the second set of tests every one is caught but one that is equivalent (a plane made 8 samples wider than it needs be changes nothing a caller can see).
- **`lico-cdp` 0.3.0**: `Page.startScreencast` and `Page.captureScreenshot` take `format` and `quality`; the host asks for `jpeg` at quality `90` (one constant, used for frames and for shots; real Korean text stays sharp).
- **`lico browser shot` still writes a PNG** (the newest held frame, decoded and re-encoded by `lico-png`; `captureScreenshot` is asked for only when no frame is held). The wire key `png` and `EngineEvent::Frame` keep their names though they now carry either format.
- `lico-browser-view` 0.7.3 (comments, spec rows), browser adapter 2.1.0.

## 5. A flake found by the release run and fixed (P03)

In the second of three cold runs of the whole suite, `tests/test_browser_tab.py::test_a_local_tab_says_so_and_refusals_say_why` failed once ("condition not reached": the header never said "only this machine's own services"). The cause is real, not only a slow test (READ): the guard (`lico-proxy`) wrote its 403 answer FIRST and told its observer AFTERWARDS; the host attaches a refusal only to tabs that are still loading; under load the engine could take the answer, finish the page and the tab leave "loading" before the host heard of the refusal, and the person was then never told why the page was refused. **`lico-proxy` 0.4.1** tells the observer (and counts) before it writes the answer. The test of the fix holds the observer and checks that the client has received nothing meanwhile: it fails on 0.4.0 (the client had 64 bytes) and passes on 0.4.1. `lico-sandbox`'s receipt follows (it depends on the proxy). The three cold runs of section 7 were made again on the fixed build.

## 6. Components touched (MEASURED, the receipts)

| Component | Version | Tests |
|---|---|---|
| `lico-engine` (new, replaces `lico-engine-pack`) | 0.1.0 | 9 passed |
| `lico-frame` | 0.1.0 -> 0.2.0 | 77 passed, 1 ignored (the real frames, kept outside the repository) |
| `lico-cdp` | 0.2.0 -> 0.3.0 | 23 passed |
| `lico-cdp-fake` | 0.2.0 -> 0.3.0 | 16 passed |
| `lico-browser-view` | 0.7.1 -> 0.7.3 | 39 passed |
| `lico-proxy` | 0.4.0 -> 0.4.1 | 35 passed |
| `lico-sandbox` (receipt only) | unchanged | 29 passed, 2 ignored |

## 7. Whole suite and checks (MEASURED)

| Check | Result |
|---|---|
| whole cold suite (`suite run --cold`), three runs on the release build | **574 of 574** each: `160.6 / 167.2 / 159.5 s`, peak `1,988.1 / 1,831.0 / 1,937.1 MiB` of 2,048 (worst margin **59.9 MiB**: thin) |
| the same before the fix of section 5 (the build of `3e2d7a0`) | three runs: 574 of 574 (`158.4 s`, `2,010.6 MiB`, margin 37.4), 573 of 574 (`156.9 s`, `1,817.8 MiB`; the flake of section 5), 574 of 574 (`158.1 s`, `1,882.3 MiB`) |
| window tests `tests/test_browser_tab.py` | 10 of 10 in every run but the one flake |
| size ratchet (`tests/test_size_ratchet.py`, inside the suite) | green |
| `cargo xtask check`, `verify` | green (`every receipt matches its inputs`) |
| `cargo xtask graph` | 202 modules, 71 nodes, 434 production edges; the biggest cycle still 48 modules |
| `cargo xtask system --record` | 10 of 10 passed (the browser ones run on the fake engine in Moli's shape); the figures are recorded in `tests/system_tests.toml` |
| `cargo xtask accept` | two clean clones build the same `lico`: `8c66ebad8e6cd104d4044ca796065a769b185b604e48cd4d10447d6a5c1600e9` (equal to the installed file) |

The margin to the suite's 2,048 MiB bound was 83.8 MiB at 0.6.2 and is 59.9 MiB now (37.4 in the worst single run of the first round): it falls with every release. A run over the bound fails the suite by rule (`tests/budget.toml`). The cause is not one test but the sum of four workers' window tests and the cargo test binaries at the start of the run; the diet is the owner's open decision (section 9).

## 8. Size, acceptance and install

- **Size:** `lico 0.7.0` is **10,875,976 bytes**, `+57,680` over 0.6.2 (10,818,296): the JPEG decoder (about 720 lines, std only), the engine record and the Moli adapter, less Obscura's code.
- **One real install, from the pinned URL, in a private test home** (the release binary of 0.7.0, a home and a runtime folder of its own, the host started like the test rig; the only traffic to GitHub was the single archive download): `lico browser open https://example.com` printed "installing the browser engine: about 46 MB from github.com, SHA-256 starts 04c732046260 (no question; this waits until it is done)", then "the engine is installed and the tab is starting", and returned in **4.8 s** counting the download. The installed file `moli-v1.1.25/moli` is 112,357,968 bytes, mode 0700, SHA-256 `e452e116...2a25` (the pinned one). `lico browser list` said `Moli v1.1.25 (lico-engine 0.1.0, contract 1)`, the engine running, `socket_password: false`, the tab `ready`, title "Example Domain", viewport 1024 x 768. `lico browser shot` wrote a 1024 x 768 PNG (208,428 bytes) of the real page (its text in Latin, Arabic, Chinese, Cyrillic and other scripts drawn with the machine's fonts). `lico browser close` stopped the engine; no engine process was left; the host's log has three lines (engine started, page of example.com, engine ended with exit code 0).
- **Installed:** `~/bin/lico` is 0.7.0 (`lico 0.7.0 (commit 49ff991, build 4b2427ec4b4d047c)`, mode 0700, replaced by rename; a host already running keeps the old binary until it is started again). The 0.6.2 binary is at `private/release_0_6_2_backup/lico` (and `private/release_0_6_2/lico`), the 0.7.0 artifact at `private/release_0_7_0/lico`. Recorded as done by the lead under the owner's delegation (the release install); to be confirmed by the owner. The engine was NOT installed into the owner's own home: the first tab the owner opens installs it (about 46 MB, once).

## 9. Known limits and what is open (for the owner)

- **The stealth setting is removed with Obscura.** You said Lico CAN use stealth. Moli has no such switch, so there is nothing to turn on; if a later engine has one, the setting comes back. Please confirm that this removal is what you want (the lead decided it under your delegation).
- **Moli's control socket has no password** (READ and MEASURED: the engine checks the `Origin` header, which stops a web page and not another program of yours). The tab and `lico browser list` say so (`socket_password: false`); the fence and the guard keep a page from it. A token needs a build of our own (M21).
- **The old engine's folder** `~/.local/share/lico/browser/v0.2.6` (Obscura, about 72 MB) is not used any more and Lico does not delete it: remove it when you like.
- `navigator.platform` is `Win32` (the engine decides).
- **The suite's memory margin is thin**: the peak is about the 2,048 MiB bound (section 7). A diet of the suite (fewer workers, or a split of the heaviest window tests) is the owner's earlier open decision; it is more pressing now.
- **M21** (our own build of Moli with a patch ledger, a token on the socket, PNG compression, a way to follow upstream releases, a place to publish) waits for the owner's word, because it means a build we have not measured (time, disk, the V8 archive pin) and a publication.
- A request to the Moli project (for a token option or PNG compression) is a publication: it is not made.
- Nothing is pushed and nothing is merged (G3 and G4 are the owner's) for M16, M17 (P01 to P12), M18-P01, M19 and M20.
