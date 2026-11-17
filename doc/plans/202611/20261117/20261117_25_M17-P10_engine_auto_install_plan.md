# M17-P10 Plan — The Browser Engine Installs Itself When A Tab Is Opened (`lico 0.5.1`)

- Status: in progress; approved by the lead (mail 155, 2026-11-17) on the OWNER's own words in the chat (requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner)
- Version: 1.0.0
- Branch: `phase/M17-P10-engine-auto-install-0-5-1` (stacked on the M18-P01 tip `1943dd8`; a patch of M17 done after the M18 Phase 0 report)
- Milestone: [M17](../../../milestones/202611/20261117/20261117_02_M17_browser_tab_with_obscura_milestone.md)

## What changes, and what does not

This reverses ONE rule of M17, "Lico never downloads the engine": opening a browser tab (by the person: the window or `lico browser open`) with no engine installed now fetches the pinned archive and installs it, with no question, and the tab starts the engine and loads the page when it is done. Nothing else changes: the pinned version and hashes (compiled in, no variable, file or argument reaches them), the hash check on the open file, the run by descriptor, the fence, the guard, the honest user-agent, no stealth switch, no stored cookies, the person's input only.

## The way (chosen; reasons)

- **Download by the system `curl`** as a helper process of the browser adapter (no TLS, no dependency, no HTTP client in Lico): `-q` first, `--fail`, `--proto =https --proto-redir =https`, `--location` (the release page redirects), `--max-filesize`, `--max-time`, a cleared environment, into a fresh `0700` staging folder under `~/.local/share/lico/browser/`. Run under `lico-sandbox` (read the system libraries and the few `/etc` files name resolution and certificates need; write the staging folder; TCP to port 443 only): kept only if a real run proves it works (the 1 GiB lesson); otherwise said and what is done instead.
- **Safety is the pinned hash, not the channel.** Lico hashes the downloaded file through the open descriptor and goes on only on a match; a mismatch is shown as a security notice and nothing stays on disk.
- **Extract with the system `tar`** of exactly the two named members after the listing is checked (regular files named `obscura` and `obscura-worker` and nothing else; no `..`, no absolute path, no link, no device); verify both against their pinned hashes through open descriptors; modes `0700`; one atomic rename of the staging folder to `v0.2.6/`; on any failure the staging folder is removed.
- **A pure component `lico-engine-pack`** holds the pins, the listing check, the hash verdict, the failures (their words, which is a security notice, the manual fallback text) and the reading of curl's exit codes. The process-running part is the adapter's `installer.rs`. The model `lico-browser-view` gets the states `Installing` and the install failure.
- **One install for two tabs:** the host holds the state; a second tab waits (the model shows "installing"); the watchers are woken when the helper ends (no timer, no polling). Already installed: no process, no request.
- **A switch that only turns the install off:** `{"auto_install": false}` in the owner-only `browser.json` (no environment variable, no flag). The tests of the window use it so that no test downloads anything.
- **The test seam** (a URL, a hash, a port) exists only in the unit tests and in a `install_seam` cargo feature that `cargo xtask system` builds for the fake server; a test pins that the shipped binary contains none of its names and that the release URL is https.

## Steps (tests first; a stop only on the three causes of mail 155)

- [ ] 1. `lico-engine-pack` (tests first, mutants on the verdict and the listing check).
- [ ] 2. `lico-browser-view` 0.6.0: `Installing`, the install failure.
- [ ] 3. The adapter: `installer.rs`, the install state, the tab flow, `list`, the CLI wait; unit tests with a fake release server and a fake archive (success to a painted page, mismatch, truncated, over the cap, curl missing, bad entries, two openers, already installed, no space).
- [ ] 4. The real end-to-end acceptance, once, in a private test home; the window screenshots; the cli help test (`--local`); spec, `CHANGELOG`, `README`, guide, milestone note; version `0.5.1`; the release as in P09 for a patch; the `done` file (141).

## Cost

Estimate 3 to 5 USD, stop rule 7 (mail 155); the ledger stops me at 230. Waits at most 4:50.
