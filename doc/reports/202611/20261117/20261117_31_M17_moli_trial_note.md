# M17 trial: Moli in Obscura's place

- Version: 1.0.0
- Date: 2026-11-17
- Branch: `experiment/M17-moli-trial`, off the M17-P12 tip `8b7b420`. An EXPERIMENT: nothing here is a release, nothing is installed at `~/bin/lico`, nothing is pushed or merged.
- Source: the owner's request in the chat, 2026-11-17: "As another alternative, look into moli and put it in Obscura's place, as an experiment." The lead did it under that request.
- Labels: MEASURED (a run of this trial), READ (from the engine's own help, README or release page, or from the code), COMPUTED.

## 1. What Moli is (READ)

- `lexmount/moli`: a headless browser written in Rust, not a Chromium wrapper; JavaScript by V8 (`rusty_v8`); licence Apache-2.0 or MIT at the user's choice; release v1.1.25 of 2026-11-15 (v1.1.22 to v1.1.25 in nine days: a fast-moving project).
- One port speaks CDP, WebDriver Classic and WebDriver BiDi (`moli serve --port N`). Layout and paint are built on demand, only with `--layout` (and `--resource` for images, fonts and media); without it there are no pictures.
- The Linux x86_64 release is one file: `moli-x86_64-unknown-linux-gnu.tar.gz`, 45,655,260 bytes, SHA-256 `04c73204...087f` (equal to the digest GitHub publishes); the `moli` inside is 112,357,968 bytes and links the system's libstdc++, fontconfig, freetype, expat, libpng, zlib and bzip2 (Obscura's two files are 80.6 + 75.8 MB and link only libc, libm and libgcc).
- Upstream's own benchmark (not checked here) puts Moli above Obscura on its compatibility corpus; this note does not rely on it.

## 2. How it was put in Obscura's place (the rig)

The owner's `engine_dir` setting (an unpinned engine) names a folder that holds a file called `obscura`, the file the host opens and runs. Three things had to be added; none of them is in the tree outside this branch except the third:

1. **A launcher** called `obscura` (4.4 MB, std only, in `private/scratch/202611/20261117/m17_moli_probe/shim/`) that starts the `moli` next to it. Lico starts the engine as `serve --port N [--font-dir D] [--stealth]` with `OBSCURA_CDP_TOKEN`, `HOME`, `OBSCURA_PROXY` in the environment; Moli needs other things, which the launcher supplies: `--layout --resource --screencast-interval-ms 100`; no `--font-dir` (Moli has none: it asks fontconfig, so the launcher writes a small fontconfig file in the scratch folder naming `/usr/share/fonts` and the Korean font folder); the proxy by `HTTP_PROXY` and `HTTPS_PROXY` (read from the environment, as Lico wants: nothing secret in an argument); `TMPDIR` in the scratch folder (Moli makes a storage folder in `/tmp` otherwise and the fence refused it); `--ca-cert` with a bundle placed next to the engine (Moli's TLS roots are `/usr/lib/ssl/cert.pem`, a link into `/etc`, which the fence does not let it read: every page failed with `net::ERR_FAILED` until the bundle was given).
2. **The fence is unchanged**: the same confinement (read and run the engine folder and `/usr`, `/lib`, `/lib64`; write the scratch folder; TCP to the guard's port only; bind the CDP port; the data limit). The tab said "running an engine that is not the pinned release". The guard counted the page loads (`guard_accepted`), so the traffic went through it.
3. **A change in the host** (`browser/supervisor.rs`, 25 lines, no test): Moli answers a WebSocket only at `/devtools/browser/<id>` (the path its `/json/version` page names); the host asks for the fixed `/devtools/browser` and got 404 (`engine_failed`). For an unpinned engine the host now asks `/json/version` once and uses the path it gives, else the fixed one.

The trial build is in `private/build_moli` (a separate target folder; the tests' build and `~/bin/lico` are untouched).

## 3. Results (MEASURED)

### 3.1 In the real window, through the fence and the guard

| Check | Moli | Obscura (earlier measurements) |
|---|---|---|
| Google search, honest user-agent (`Lico/0.6.2 Obscura/unpinned`, the host's override) | real results, title "obscura headless browser - Google 검색", pictures and links drawn | Google's block page (`/sorry/index`) in all three configurations (M17-P12) |
| Click in Google's box, type English, Enter | results of "lico moli browser" | not possible to reach results |
| Korean typed with the real input method into Google's box, Enter | the box shows the Korean, Google's suggestion list is drawn, the search ends with title "안녕하세요 한글 - Google 검색" | boxes in a single-line input; Google blocked |
| Korean typed into a single-line `<input>` of a page (the P08 probe) | drawn correctly with a caret; the page got the text (`안녕하세요 한글 `) | the page got the text; the field painted boxes |
| `html.duckduckgo.com/html/?q=` | loads ("moli at DuckDuckGo") | loads |
| Window RSS with one shown tab, scale 1 | idle 23.0, with a tab 27.6, **peak 31.5 MiB** | 23.0, 26.2, peak 28.0 (P09) |
| Window RSS with one shown tab, scale 2 | idle 34.2, with a tab 42.2, **peak 51.7 MiB** | 33.4, 39.4, peak 44.1 (P09); gate 50 MiB |
| A frame, compressed (866x612, scale 1; 1732x1188, scale 2) | 810,025 and 1,905,109 bytes | 38,366 and 69,124 bytes (P07 probe) |

### 3.2 The engine alone (a loopback fixture, then real sites; the same CDP calls the host makes)

| Check | Result |
|---|---|
| Calls of `open_tab` and `wire_of`: `Target.createTarget`, `attachToTarget` (flatten), `Page.enable`, `Runtime.enable`, `Emulation.setUserAgentOverride`, `setDeviceMetricsOverride`, `Page.navigate`, `startScreencast` and `screencastFrameAck`, `Input.dispatchMouseEvent` (move, press, release, wheel), `dispatchKeyEvent`, `Input.insertText`, `Page.captureScreenshot` | all accepted |
| Events the map uses | `Page.frameNavigated`, `loadEventFired`, `frameStartedLoading`, `Target.attachedToTarget` seen; a link click navigated; `history.back()` went back |
| Input into a page | a button `onclick` ran, `keyDown`/`char`/`keyUp` typed (a probe that sent a `keyDown` with `text` and a `char` got every letter twice; the host's own key pattern typed "lico moli browser" in the window with no doubled letter), `Input.insertText` of Korean into a single-line input, a textarea and a `contenteditable` (the last takes focus and shows a caret; Obscura does not focus it), a wheel scrolled by 300 |
| Frames | a frame only when the page changes (one in four idle seconds), PNG, about 1 MB at 1024 x 640 |
| User-agent | `Emulation.setUserAgentOverride` is honoured by `navigator.userAgent`; the engine's own default claims Chrome/156 on Windows, `navigator.webdriver` is false, `navigator.platform` is `Win32` |
| Proxy | `HTTP_PROXY` and `http_proxy` in the environment are honoured as the `--http-proxy` argument is |
| Memory, no tab | 14.8 MiB at start; 80 MiB after the fixture, 132 MiB after three real sites |
| One still page with the screencast on, 60 s | fixture: RSS 75 MiB, engine CPU 0.39 s, 83.8 wake-ups/s; the Google home page: RSS 138 MiB, CPU 3.49 s, 151 wake-ups/s. Obscura (P06): RSS 215 MiB, CPU 0.26 s, 105.9 wake-ups/s |
| Real searches (standalone, same machine, one try each) | Google: results (34 links); DuckDuckGo html: results; Bing: loads, 5,824 characters of text (not examined further) |

Google was fetched about ten times in all (standalone and in the window), always with results. That is a small number from one address; it is not a measure of how long it keeps working.

## 4. What does not fit yet (READ and MEASURED)

1. **The CDP socket has no authentication.** Obscura needs the bearer token Lico makes per start; Moli has none (only `--remote-allow-origins`, which concerns browsers' cross-origin requests). Any process of this machine that finds the port (loopback, random per start) can drive the tab and read what the engine reaches. A fix is not in Lico's reach (Moli's discovery page even prints the socket path to anyone); it would need an upstream option, or the host in front of the engine, or a network namespace. This is the main security difference and the owner should weigh it.
2. **The window's memory at scale 2** rises by 7.6 MiB at the peak (44.1 to 51.7, gate 50) because Moli's PNG frames are 21 to 28 times larger. Asking for a smaller picture (JPEG, a lower quality) was not tried; Lico's decoder reads PNG only.
3. **Fonts, TLS roots and the temporary folder** each needed a launcher or a different fence (section 2). A real integration would decide them in Lico: a bundled or readable CA file, a fontconfig file made by the host, `TMPDIR`.
4. **`--layout` and `--resource` are options of the engine, not of the protocol**; the screencast interval defaults to one second (set to 100 ms here).
5. **The WebSocket path** is per start (host change of section 2).
6. **System libraries**: Moli needs fontconfig and freetype on the machine; a minimal server without them would not start it.
7. **Pinning**: a new release is a new hash, and the project releases every few days; Lico would pin one and move on purpose. The installer (one 45.7 MB archive, one file) is simpler than Obscura's (two files) but is new work.
8. **Not tested**: the local-services tab with a private network by Moli's own rule (the guard allowed the fixture, which is the host's rule); downloads; WebSocket pages; long sessions; a crash of the engine; whether Google keeps answering.

## 5. What the lead recommends

Moli does what the owner asked for and Obscura did not: Google opens, a search can be typed in Korean and run, `contenteditable` takes input, the caret is drawn, Korean in an input paints. It also uses less engine memory (75 against 215 MiB for a page). The costs are the unauthenticated socket (4.1), a heavier frame (4.2) and a younger, faster-moving project. Two ways on:

- **A. Moli as a second engine, chosen by the owner's setting, Obscura stays the pinned default.** A small milestone: an engine "flavour" in the host (arguments, environment, socket discovery, CA and font files made by the host), a pinned Moli release and its installer, tests against the fake engine for both flavours, the memory gate checked at scale 2. The CDP token gap is stated in the tab's header for Moli, as it says it for an unpinned engine now.
- **B. Moli as the default.** Larger: it also needs an answer to 4.1 first (upstream option or a front proxy) and a decision on the window's memory gate.

The lead recommends A, after the owner has seen it work. The experiment's host change is not mergeable as it stands (no test, no flavour, `--log-level debug` in the launcher).

## 6. What is open (the owner's)

- Whether to build A (or B), and who decides the CDP-token gap.
- Whether to report the Korean `<input>` boxes of Obscura upstream (unchanged).
- Push and G3/G4 for M16, M17 (P01 to P12), M18-P01 and M19 (unchanged; this branch is on top of the same stack).
