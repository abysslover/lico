# M20-P01 Plan — `lico-engine`, The Moli Adapter, Obscura And Stealth Removed

- Status: in progress; approved by the lead under the owner's delegation, on the owner's words in the chat, 2026-11-17 ("make moli the default ... obscura is scrapped"; "treat moli or obscura as one component and manage the version internally"); to be confirmed by the owner
- Version: 1.0.0
- Branch: `phase/M20-P01-moli-engine` (stacked on the M17-P12 tip `8b7b420`; the trial note of `experiment/M17-moli-trial` is brought in by checkout, the 25-line experiment code is NOT)
- Milestone: [M20](../../../milestones/202611/20261117/20261117_32_M20_moli_engine_component_milestone.md)
- Evidence: [the trial note](../../../reports/202611/20261117/20261117_31_M17_moli_trial_note.md) (what Moli needs, measured)

## What changes, and what does not

The tab starts Moli (`moli serve`) instead of Obscura, through a component that owns the pin, the contract and the patch ledger. The fence, the fetch guard, the honest user-agent, the person-only input, the install-by-pinned-hash flow (system `curl` and `tar`, hash on the open file, run by descriptor, atomic rename) and the one-engine-for-all-tabs rule stay. Frames stay PNG in this phase (they are large: P02).

## The component `lico-engine` (replaces `lico-engine-pack`)

- **Pins (compiled in, as before):** upstream `lexmount/moli`, release `v1.1.25`; the Linux x86_64 archive URL, size (45,655,260) and SHA-256; the one member `moli-v1.1.25-x86_64-unknown-linux-gnu/moli` and the SHA-256 of the unpacked file (to be computed in full); caps (archive, file); the licence line.
- **Lico's own numbers:** `ENGINE_VERSION` (Lico's version of the engine record, starts `0.1.0`), `CONTRACT` (starts `1`): the way the host starts the engine, finds the socket (the discovery page, `/json/version`, `webSocketDebuggerUrl`) and takes frames. A pin record carries the contract it satisfies; a unit test fails when the record and the host's constant differ, and the host refuses to start an engine whose record names another contract.
- **Patch ledger:** a list of Lico patches on the upstream (empty in M20). A non-empty ledger changes `ENGINE_VERSION` and the file hash (M21).
- **The listing check** for a nested archive: exactly one required regular member under a size cap, every other entry sane (no link, device, absolute or `..` name), a small entry count; mutation tests on it and on the hash verdict.
- **The failure words and the manual steps** name Moli.

## The adapter

- `engine.rs`: `Pins` with one file; install dir `lico/browser/moli-v1.1.25`; the owner's `engine_dir` option stays (a folder with a `moli` file, run unpinned and said so); `browser.json` is read tolerantly (a file that only says `auto_install` or `stealth` names no engine; `stealth` is ignored).
- `installer.rs`: one member extracted with `--strip-components=1`; the rest as before.
- `profile.rs`: arguments `serve --host 127.0.0.1 --port N --layout --resource --screencast-interval-ms 100 --ca-cert PATH --log-level warn`; environment `HOME` and `TMPDIR` in the scratch folder, `FONTCONFIG_FILE` (a file the host writes in the scratch folder naming the system and Korean font folders), `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` set to the guard; fence: read the CA bundle FILE found among the usual system paths, run the engine file, no more. No `OBSCURA_*` variable. `RLIMIT_DATA` re-measured (Moli's idle is lower).
- `supervisor.rs`: always find the socket path from the discovery page (a plain GET, a strict character set, no redirect); no CDP token (`link(..., None, ...)`); the engine's start log has no secret.
- `map.rs`: user-agent `Mozilla/5.0 (X11; Linux x86_64) Lico/{ver} Moli/{release}`; settled by the real run whether `Emulation.setUserAgentOverride` or `--user-agent` is needed.
- The tab header and `browser.list` say that the engine's socket has no password and is reached only from this machine (known limit, not hidden).
- `tools/fake-engine` takes Moli's shape (`serve --host --port ...`, a discovery page with a path per start, no token, the Moli-named files).
- Stealth removed from `engine.rs`, `profile.rs`, `supervisor.rs`, `ops.rs`, the tab header, `lico-browser-view` help, the guide and the specification (BROW-05 rewritten: no engine of the tab has a stealth switch; the owner's earlier "Lico can use stealth" stands for an engine that has one).

## Steps (tests first; green-only commits)

- [ ] 1. `lico-engine`: contract tests (the pin, the contract number, the nested listing, the failure words, the ledger), mutation tests; remove `lico-engine-pack` and move its users.
- [ ] 2. The adapter units: arguments, environment, fence (the CA file), the socket discovery (accept, reject a foreign host, a bad character, a missing page), the contract refusal, the setting read.
- [ ] 3. The fake engine in Moli's shape; the system tests of the install and the run on it; `lico-browser-view` and the window tests (`tests/test_browser_tab.py`, `probe_install_window.py`, `test_install_seam_absent.py`).
- [ ] 4. A real check, once, with the real Moli in a private test home: install from the pinned URL, one page, a click, Korean typing; the user-agent question settled.
- [ ] 5. `suite run --changed --receipts`, `cargo xtask check|verify|graph`; commit.

P02 (JPEG frames) and P03 (the release) follow on their own branches.

## Stop and ask

Stop only if: the pinned archive does not match the hash recorded in the trial; the real Moli cannot be run inside the fence without opening more than the CA file, the scratch folder and the font folders; or a step needs a push or a publication.
