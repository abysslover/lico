# M17 — A Browser in a Tab, with the Obscura Engine (Phase 0)

- Status: planned; P01 (Phase 0, read-only) waits for the lead's approval of its plan (the owner is away: approvals are given by the lead under the owner's delegation and recorded as such); later phases are sketches until the Phase 0 report is decided
- Version: 1.0.0
- Approved: not yet (G1 for the milestone waits for the Phase 0 report)
- Project: Lico
- Updated: 2026-11-17
- Base: the M16-P08 tip `bd99509` (M16 is not merged yet: the owner's G3 and G4 wait for their return); M17 is stacked on it and touches no M16 branch
- Implementation: `phase/M17-P01-browser-tab-phase0`; later phases get their prefix when the Phase 0 report is decided
- Version of the program: P01 bumps nothing

## Goal

The owner asked (2026-11-17) that a browser opens in a tab of Lico's window, built on **Obscura**, a lightweight headless browser engine written in Rust that runs JavaScript with V8 and speaks the Chrome DevTools Protocol (the project's own description, not yet checked by us). It is a milestone of its own because it is big. It replaces nothing: Claude, Codex and Local LLM sessions are unchanged.

## Rules that bind

Lico's own side keeps its rules: the component rules 1.0.1 (new code is new components with a version, downward-only dependencies, `SPEC.md`, contract tests, `RECEIPT.json`, 256 MiB and 30 s per component run, the short named system-test list), no new upward edge into the hubs, no new Python, a GUI with CLI and HTTP parity, tests that never touch the owner's real services, accounts or files, nothing secret on disk or in a log, and the idle host and window with no browser tab open must not change (the engine is never started until a tab asks for it). **The owner's rule for this milestone (2026-11-17): do not mix Lico's design with what Obscura uses.** The 50 MiB idle RSS rule, the one-executable principle and the component budgets bind Lico's own modules only; the engine is a separate program and its memory and file size are not counted against Lico's 50 MiB (they are measured and reported as information). What lives in Lico's process for a tab (frame buffers, state) is Lico's and is bounded and counted. The page is untrusted data. Lico does not use or expose any anti-detect ("stealth") feature of the engine. Nothing is installed or downloaded without the owner's or the lead's approval; the owner installs the engine, Lico never downloads one silently.

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M17-P01 | Phase 0: how to embed, rendering, input, security, agents, components, one measurement (read-only; a report, then stop) | [Plan](../../../plans/202611/20261117/20261117_11_M17-P01_browser_tab_phase0_plan.md) | pending | plan waits for the lead's approval |
| later | decided by the Phase 0 report | sketch | | after P01 |

## Cost

Estimate for P01: 5 to 9 USD (reading, one measurement, the report). The front-end limit is 235; the counter since mail 137 stood at about 33.9 USD when this note was written.
