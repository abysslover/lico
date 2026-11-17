# M17-P12: stealth as the owner's option, release 0.6.2

- Version: 1.0.0
- Date: 2026-11-17
- Branch: `phase/M17-P12-stealth-option-0-6-2`, stacked on the M17-P11 tip `f561b1f`; release commit `962d9c2`
- Source: the owner's words in the chat, 2026-11-17: "That is a wrong specification. It is right that Lico CAN use stealth, though it may also not." The lead built it under the owner's delegation; the owner's statement is the decision (the spec sentence it corrects came from the Phase 0 report, not from the owner).
- Labels: MEASURED (a run of this phase), READ (from the code or the engine's own help), COMPUTED.

## 1. What changed in the rule

BROW-05 said "Lico passes no stealth switch and sets no stealth variable". It is now: stealth is the owner's to use and it is OFF unless the owner says so. The M17 milestone note has a pointer, and the 0.6.1 report and CHANGELOG entry (which said "no stealth, on purpose") stay as history.

## 2. What the person gets

- **The setting:** `{"stealth": true}` in the owner-only `browser.json` of the state folder (next to `auto_install`; a file that others can read says nothing, as for every flag there). It applies when the engine next starts (after the last tab is closed).
- **What it does:** the ordinary engine is started with `--stealth`; no user-agent is set over the engine's own (an honest name beside a consistent stealthy fingerprint would be the one inconsistent thing); the tab's header says "stealth on (your setting): the page sees the engine's own identity"; `browser.list` shows `stealth` for each tab and each engine.
- **What it does not do:** the tab for this machine's own services (`--local`) never gets it; the fence, the fetch guard and the ports are exactly the same (a test compares the fence with and without it). Stealth changes what a page sees, not what the engine may reach.
- The F1 help names a search that works (below) and says that stealth is the owner's setting, off by default; the guide says how to turn it on.

## 3. Does it get through Google? (MEASURED, one search of each kind)

The same search (`google.com/search?q=...`) from this machine with the real engine, the page's final address and title read from `browser.list`:

| Engine | Result |
|---|---|
| the pinned plain build, no stealth (the honest user-agent) | Google's block page (`/sorry/index`) |
| the pinned plain build with `--stealth` | Google's block page |
| the release built with stealth (`obscura-x86_64-linux-stealth.tar.gz`, 75,180,401 bytes, SHA-256 `ac805737...08c5` equal to the digest the release publishes, two files, `obscura` 84,069,464 and `obscura-worker` 79,215,696 bytes) with `--stealth` | Google's block page |

So stealth, even with the build that adds TLS impersonation, did not change Google's answer from this machine. The causes are NOT known: Google also judges the address's reputation, and this machine's address had already sent Google a few requests from this session's diagnosis, besides the owner's own tries (the same address); it was not tested from another address. Lico therefore does NOT pin or install the stealth build: it costs 75 MB more, a second pinned hash and an install path, and it was not seen to help. Should the owner want it later, it is a separate small milestone (the release exists, the hash is known).

What works (same method, plain engine, no stealth): `https://html.duckduckgo.com/html/?q=WORDS` shows real results (read from the picture); Bing's page loads its frame but no results appear in this engine. The help names the DuckDuckGo address.

## 4. How it is built (READ)

- `browser/engine.rs`: `stealth_on(state_dir)`; the flag reading is one helper shared with `auto_install_off`; a setting file that only says `stealth` names no engine.
- `browser/profile.rs`: `Params.stealth`; `args` adds `--stealth` when it is on and the engine is not the local one.
- `browser/supervisor.rs` and `ops.rs`: the flag reaches the start, the engine's log line (no secret), the skipped `Emulation.setUserAgentOverride`, the tab's row and the list.
- `gui/browser_view.rs`: the header text.
- `lico-browser-view` 0.7.1: two help sentences (SPEC section).
- A latent defect found by the new system test and fixed: a new engine start swept EVERY `scratch-*` folder of the state's `browser` folder, so the engine for this machine's own services, starting while the ordinary one ran, deleted the running engine's HOME and working folder. The folder name now holds the host's process number and the sweep leaves the folders of this host alone (it still removes what an earlier host left).

## 5. Tests (MEASURED)

| Check | Result |
|---|---|
| host unit tests (`engine`, `profile`) | the setting is read only from an owner-only file and only `true` turns it on (8 cases); `--stealth` only when chosen and never for the local engine; the fence is equal with and without it |
| `lico-browser-view` contract tests | 39 passed (the help test names the new sentences) |
| system test `browser_engine_stealth_by_the_owners_setting` (new, with the fake engine recording its arguments and the user-agent calls) | passed: off by default (no flag, the honest user-agent sent once); on (flag, no user-agent call, tab and list say so); the local engine without it (two engines, one flag); a setting readable by others is refused as a whole |
| window tests `tests/test_browser_tab.py` | 10 passed (one new: the header says "stealth on" with the setting and nothing of it without) |
| `suite run --changed --receipts` (a changed file nobody had declared selected everything) | 574 of 574 passed, **170.9 s**, peak **1,964.2 MiB** of 2,048 (margin 83.8 MiB: thin; one run) |
| `cargo xtask check`, `verify`, `graph`, `accept` | green; graph: the biggest cycle still 48 modules |
| Two clean clones | identical, `9075ce36dc85f1c18c55b2f13d3cb87aadfc3270ceb11d67f49a1d46534a6566` |

## 6. Size (MEASURED)

0.6.2: **10,818,296 bytes**, +151,088 over 0.6.1 (10,667,208) and -1,968 against 0.6.0 (10,820,264). The 0.6.1 binary was smaller than 0.6.0 although it added code, which the 0.6.1 report said was not explained; this build is at the 0.6.0 size again, so that was most probably a swing of the optimiser, not a loss of anything.

## 7. Installed

`~/bin/lico` is 0.6.2 (`lico 0.6.2 (commit 962d9c2, build f746bd8a53aef83f)`, mode 0700, replaced by rename; running hosts keep the old binary until the next start). The 0.6.1 binary is at `private/release_0_6_1_backup/lico`, the 0.6.2 artifact at `private/release_0_6_2/lico`. Recorded as done by the lead under the owner's delegation (the release install); to be confirmed by the owner. The setting is NOT written: stealth is off in the owner's home until they put `"stealth": true` in their `browser.json`.

## 8. Known limits and what is open

- Stealth does not open Google (section 3). Whether the owner wants the stealth build pinned anyway, or a different search site as the default help, is theirs.
- The setting is a file; there is no menu entry. A tab says when it is on.
- The change applies at the next engine start (after the last tab is closed).
- Nothing is pushed and nothing is merged (G3 and G4 are the owner's) for M16, M17 (P01 to P12), M18-P01 and M19.
