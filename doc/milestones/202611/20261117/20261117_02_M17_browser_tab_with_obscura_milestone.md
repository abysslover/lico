# M17 — A Browser in a Tab, with the Obscura Engine (Phase 0)

- Status: **all nine phases P01 to P09 implemented, and the patch P10 (`lico 0.5.1`, commit `1592da5`; done after the M18 Phase 0 report)**; every approval was given by the LEAD under the owner's delegation (the owner away) and is to be confirmed by the owner; the milestone is on `main` since 2026-11-18 by the owner's G4 and push yes in the chat (a fast-forward of `main` through the M20-P03 tip); G3 (the owner's inspection) is pending: inspection pending. Its engine, Obscura, was replaced by Moli in M20 (release `0.7.0`); this note stays as the history of the tab
- Version: 1.0.0
- Approved: not yet (G1 for the milestone waits for the Phase 0 report)
- Project: Lico
- Updated: 2026-11-17
- Approvals of mail 147 (the re-plan P04 to P09: PNG frames, a 4 MiB frame cap, acknowledge at once and keep the newest, Lico-side memory counted, an honest user-agent, a Korean font directory, commit-only Korean input in the page, `http` and `https` only, the proxy design of its section 2, the phases and their stops) were given by the lead under the owner's delegation (the owner away) and are for the owner to confirm on return
- Approvals of mail 144 (the Phase 0 answers: separate process the owner installs, Landlock as containment, commit-only Korean input in the page, `localhost` opt-in per tab, `v0.2.6` pinned for testing, stealth surfaces accepted with a sentence, no persistence, no agents, release `0.5.0`; the order P02 then P03) were given by the lead under the owner's delegation (the owner away) and are for the owner to confirm on return
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
| M17-P01 | Phase 0: how to embed, rendering, input, security, agents, components, one measurement (read-only; a report, then stop) | [Plan](../../../plans/202611/20261117/20261117_11_M17-P01_browser_tab_phase0_plan.md) | [Report](../../../reports/202611/20261117/20261117_12_M17-P01_browser_tab_phase0_report.md) | implemented; waits for the lead and the owner |
| M17-P02 | `lico-ws`, `lico-cdp-fake`, `lico-cdp` | [Plan](../../../plans/202611/20261117/20261117_13_M17-P02_ws_and_cdp_plan.md) | [Result](../../../plans/202611/20261117/20261117_13_M17-P02_ws_and_cdp_plan.md) (in the plan note) | implemented |
| M17-P03 | `lico-sandbox` (the Landlock launcher) and the first measurement of the engine, then a stop for a re-plan | [Plan](../../../plans/202611/20261117/20261117_14_M17-P03_sandbox_and_first_measurement_plan.md) | [Measurement note](../../../reports/202611/20261117/20261117_15_M17-P03_first_measurement_note.md) | implemented; waits for the lead's re-plan |
| M17-P04 | `lico-frame` (PNG decoder), `lico-browser-view` (display-free tab model), the 4 MiB default cap of `lico-ws` | [Plan](../../../plans/202611/20261117/20261117_16_M17-P04_frame_and_browser_view_plan.md) | [Result](../../../plans/202611/20261117/20261117_16_M17-P04_frame_and_browser_view_plan.md) (in the plan note) | implemented |
| M17-P05 | `lico-proxy` (the fetch guard) and the second measurement (does the engine honour `--proxy`) | [Plan](../../../plans/202611/20261117/20261117_17_M17-P05_lico_proxy_and_second_measurement_plan.md) | [Measurement note](../../../reports/202611/20261117/20261117_18_M17-P05_second_measurement_note.md) | implemented; waits for the lead's fence profile |
| M17-P06 | the host adapter `browser/`, CLI and HTTP parity, system tests | [Plan and result](../../../plans/202611/20261117/20261117_19_M17-P06_browser_host_adapter_plan.md) | done | adapter, `lico browser`, two system tests, real-engine probe measured; the localhost opt-in has no effect with this engine (question to the lead) |
| M17-P07 | the tab kind in the window | [Plan and result](../../../plans/202611/20261117/20261117_20_M17-P07_window_tab_plan.md) | done | tab bar, `+B`/`+BL`, page painted, address row, states, help; real engine measured in the window; scale-2 peak 50.4 MiB |
| M17-P08 | no timers; mouse, wheel, keys, text, paste | [Plan and result](../../../plans/202611/20261117/20261117_21_M17-P08_input_and_no_timers_plan.md) | done | wake-ups 60 a second to 0.8; command p95 0.22 ms (fake) and 7.5 ms (real, upper bound); Korean typed into a real page |
| M17-P09 | verification and the release `0.5.0` | [Report](../../../reports/202611/20261117/20261117_22_M17-P09_verification_and_0_5_0_report.md) | done | six counted cold runs green (565 tests), two clean clones identical, installed, smoke-tested with the real engine; the scale-2 peak `44.1 MiB`; the owner's list and the first real-site check in the report |
| M17-P10 | the engine installs itself when a tab is opened, release `0.5.1` | [Report](../../../reports/202611/20261117/20261117_26_M17-P10_engine_auto_install_and_0_5_1_report.md) | done | **requested by the owner in the chat, 2026-11-17; the spec wording approved by the lead, to be confirmed by the owner** (it replaces the M17 rule "Lico never downloads the engine"); one real install measured `5.28 s`, first frame `5.55 s`; curl not fenced; the owner's real-home check is to do |

## The owner's first real-site check

Written out in the [P09 report](../../../reports/202611/20261117/20261117_22_M17-P09_verification_and_0_5_0_report.md), section 6: install the pinned engine (the release URL, the SHA-256 values, the folder `~/.local/share/lico/browser/v0.2.6` and its modes), then `lico browser open https://SITE`; what to expect and the known limits. No test touches a real site.

## Cost

The M17 cost table is in the P09 report (section 8): about `54` USD counted against the Phase 0 estimate of about `30`.


Estimate for P01: 5 to 9 USD (reading, one measurement, the report). The front-end limit is 235; the counter since mail 137 stood at about 33.9 USD when this note was written.

Update 2026-11-17 (M17-P12): the Phase 0 sentence "Lico uses and exposes no anti-detect feature" was not the owner's; the owner corrected it in the chat: Lico may use the engine's stealth mode, and may not. It is the owner's option, off by default (`{"stealth": true}` in the owner-only `browser.json`); see BROW-05 and the [M17-P12 report](../../../reports/202611/20261117/20261117_30_M17-P12_stealth_as_the_owners_option_and_0_6_2_report.md).
