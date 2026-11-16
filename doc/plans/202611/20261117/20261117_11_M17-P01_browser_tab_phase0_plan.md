# M17-P01 Plan — Phase 0 for a Browser Tab with Obscura (Read-Only)

- Status: proposed; waits for the lead's approval under the owner's delegation (the owner away); nothing is read or measured before it
- Version: 1.0.0
- Branch: `phase/M17-P01-browser-tab-phase0` (from the M16-P08 tip `bd99509`)
- Milestone: [M17](../../../milestones/202611/20261117/20261117_02_M17_browser_tab_with_obscura_milestone.md)

## Objective

One report, then stop: `doc/reports/202611/20261117/20261117_12_M17-P01_browser_tab_phase0_report.md`, English, an owner page of one screen at its top, every figure marked MEASURED, COMPUTED or ASSUMED, section 7 the open questions with recommendations. No product code, no installs. The owner's rule binds: the engine's memory and size are information, not a gate; Lico's side keeps its rules.

## Contents (the eight items of mail 140 section 2)

1. **How to embed**: (a) a separate process started with the first browser tab and ended with the last, driven over loopback CDP, frames painted by Lico; (b) a library inside the host; (c) bundled. For each: the split of the two sides, the build and toolchain, the supply chain (pinned tag and SHA-256, the Apache-2.0 notice, the owner installs the engine, a plain message in the tab when it is absent).
2. **Rendering path** into the hand-drawn window: frames or a draw list; fit with the display-free part plus thin painter and repaint-only-when-needed (no new idle thread); cost per frame (bytes, CPU, latency at a window size); the honest list of what will not render.
3. **Input**: mouse, wheel, keys, text, the Korean input method (the existing IME tests are the bar), links, back and forward, an address field, title and loading state, new-window requests, downloads, file choosers and dialogs (refuse with a note), cookies and storage (none persisted by default).
4. **Security**: the page as untrusted data; no Lico operation reachable from it; what the engine process may reach (working directory, cleared environment, permissions, what the OS allows without a sandbox); CDP on loopback with a token that is on no command line and no disk; TLS verification on; the private-network block with an explicit opt-in; no stealth; what is logged.
5. **Agents**: whether a session may use the tab (recommended not in the first phases); CLI and HTTP parity of the tab itself.
6. **Components** under rules 1.0.1: candidates with downward-only dependencies and a fake CDP server for contract tests, the order with stop points, system tests, lines and USD per phase at the rate measured in M16 (1.2 to 1.4 USD per 1,000 lines for components; I will state the margin I use, since P06 ran at 6.7).
7. **One measurement**, only under the conditions of mail 140 section 2 item 7: ONE release archive of a pinned tag over HTTPS into a new empty directory under `private/scratch/` (dated layout); the tag, the URL, the SHA-256 and the published checksum recorded; the one hashed binary run only under `pri-heavy` with a memory limit, on pages from my own loopback fake, no stealth option, no proxy, nothing that browses; the archive and the extracted files deleted, the hashes kept; no script from inside it is run; nothing installed, V8 not built. The download is announced and pinned by exact tag and URL in the approval prompt before it is made. If the download fails or anything looks wrong I stop the measurement and report from documentation.
8. **Decision gate**: a fact that contradicts the aim (no usable rendering, or not drivable over loopback without handing the engine more than a page should get) stops the phase with a `question` file and at least two alternatives and a recommendation.

## Steps

- [ ] 1. The lead's approval of this plan (question 126).
- [ ] 2. Reading: the project's README, wiki and release page (WebFetch), Lico's own code where the painter, tabs, IME and input meet a new tab kind (targeted greps), `cargo xtask graph` for the edges.
- [ ] 3. The measurement of item 7, if its conditions hold.
- [ ] 4. The report; the `done` mail; stop.

## Estimate and the stop rule

**5 to 9 USD** (the lead's range; mine: reading 2 to 3, the measurement 1 to 2, the report 2 to 3; no code). If the phase has used about 13 USD and is not near done, I stop and write a `question` file with the figures.

## Acceptance Criteria

The report exists with the owner page first and every figure marked; nothing but documents changed (and the deleted download); the link scan is clean; the phase stops there with a `done` file.
