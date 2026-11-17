# M17-P08 Plan And Result — No Timers, And The Person's Input

- Status: **done** (see the Result); approved by the lead under the owner's delegation (the owner away), mail 152 section 3 (no `question` file; the plan is this note)
- Version: 1.0.0
- Branch: `phase/M17-P08-input` (stacked on the P07 tip `ef41fab`)
- Milestone: [M17](../../../milestones/202611/20261117/20261117_02_M17_browser_tab_with_obscura_milestone.md); before: [P06](20261117_19_M17-P06_browser_host_adapter_plan.md), [P07](20261117_20_M17-P07_window_tab_plan.md)

## Plan

1. **First: the two wake-ups** (mail 150 item 6, mail 151 section 0). The reader that blocks, commands written by the caller through a writer half: `lico-ws 0.3.0` (`Conn::writer`, one lock for the socket's writes), `lico-cdp 0.2.0` (`link`: a `Reader` that blocks and a cloneable `Caller` with `call`, `fire` and `shutdown`); the guard's accept replaced by a blocking accept that a stop wakes: `lico-proxy 0.4.0`. The supervisor becomes a reader thread and a command thread on ONE queue (commands, the engine's events, the guard's refusals): no timer except the half second after a tab is shown (the model's one fallback screenshot). Gates: a command reaches the engine in under 20 ms at the 95th percentile (fake engine, and once with the real engine); no wake-up of the host while a still page is shown (voluntary context switches of the host's threads from `/proc`, over ten seconds with a still page and with no tab); the host's CPU over 60 seconds, reported.
2. **Input** from the person only, never from a page or a program: `browser.input` (mouse move, press, release, wheel, key down, key up, committed text); the model decides (a tab that is not shown, not active (starting, absent, failed) or whose focus is the address row takes none); in the window: one request in flight, in order, a move not yet sent replaced by the newer one; Lico's own keys never reach the page (Ctrl, Alt, the logo key, the function keys, Escape, `Ctrl+L`, `Ctrl+W`, `Alt+Left/Right`, `F5`, `F1`); the plain keys of the person go to the page when it has the focus (a click on it, or the page coming up), printable text as `Input.insertText`, editing and moving keys as key events; the input method's committed text goes to the page, the composition shows in the address row only; `Ctrl+V` pastes as one capped insert and only on the person's own paste; nothing is copied out of a page.
3. Estimate with P07 7 to 8, stop rule 12 (mail 152).

## Result (2026-11-17)

**Built (tests first):** `lico-ws 0.3.0` (two tests: four threads write while the reader blocks and every frame is whole; a close or a cancel refuses every half; the write lock itself is the one mutant that a test cannot fail on this kernel, which serialises whole `send` calls: said), `lico-cdp 0.2.0` (the link: 5 tests; mutants: 3 caught, 1 equivalent (a dropped sender already makes a waiting call fail), 1 not covered (the cleanup of the table of waiting calls when a write fails)), `lico-proxy 0.4.0` (a test in a process of its own counts the voluntary context switches of the guard's threads: **76 in 1.5 s before, at most 6 allowed after**; a stop wakes the accept and returns in under 0.8 s), the supervisor on two threads, the `browser.input` operation (validation of every kind; no input to an absent, hidden or unknown tab), the window's routing (mouse, wheel, keys, the input method's commit, paste), and four window tests with the fake engine and one fourth system test.

### The gates, MEASURED

| Gate | Result |
|---|---|
| a command reaches the engine in under 20 ms at the 95th percentile (fake engine, 60 mouse moves, each stamped when sent and when the engine got it) | **p50 0.17 ms, p95 0.22 ms, worst 0.26 ms** |
| the same with the REAL engine, as an upper bound (a mouse move to the page's own request at the fixture: the command, the engine, the page's handler, the guard), 50 of 50 seen | p50 1.1 ms, **p95 7.5 ms**, worst 10.6 ms |
| no wake-up while a still page is shown (fake engine, ten seconds each) | **10 with a still page shown, 10 with no tab at all** (the host's own one-second background threads) |
| the same with the real engine over 60 s | host wake-ups **0.8 a second** (was 60.2), host CPU **0.00 to 0.01 s** in 60 s (was 0.11 to 0.12), 11 to 13 threads with one engine and one tab, host RSS 11.0 to 11.2 MiB; the engine's own 88 wake-ups a second and 0.17 s CPU are information |

How the wake-ups are counted: the sum of `voluntary_ctxt_switches` of every thread of the host in `/proc/PID/task/*/status`, before and after the interval. The only timers left: the half second after a tab is shown; the connection's idle limit of an hour (the reader goes on waiting); the host's own background threads (about one wake a second, as with no tab).

### Input with the real engine (host level, the page's own handlers report to the fixture; the archive of the pinned release, SHA-256 equal to the pin, deleted afterwards)

A press at (50, 30) reached the page as a `mousedown` at x=50, y=30; committed Korean text arrived as an `input` event with the value `안녕하세요`; a `Backspace` key event shortened it to `안녕하세`; a wheel step arrived as `deltaY` 120; 50 of 50 mouse moves arrived. At the window level (`tests/probe_browser_window.py`): the person clicks the page's input, composes the sentence with the input method (ibus Hangul), and the page's input events end with the value `안녕하세요 한글 ` exactly; screenshots in `private/scratch/202611/20261117/m17_p07_screens/` (`9_*`). **A finding for the owner's list:** the engine paints Korean in a form control (the `<input>` of the page) as boxes even with a Korean `font-family` on it, while the same text in the page body is painted with the font directory it was given (the Korean page of P07); the value the page holds is right (it arrives at the fixture exactly), so this is the engine's own rendering of form controls, information, not something Lico can mend.

### Tests

System test 4 (`browser_input_reaches_the_engine_at_once_and_nothing_wakes_while_a_still_page_is_shown`, 24.4 MiB, 23.2 s of the 60 s budget): a press, a release, a wheel step, an Enter and committed Korean reach the engine in the order given and with their own parameters; the latency above; a hidden tab takes no input; the wake-ups above. Window tests (`tests/test_browser_tab.py`, now 7 tests, a virtual display): a click on the page is a press and a release at the point in the page's own coordinates; a wheel step; typed text and Enter; Lico's own keys (`Alt+Left`, `F5`, `Ctrl+L`) and the address row's typing never reach the page; a click on the page gives it the keys again; Korean composed with the input method and committed reaches the page as text and the address row keeps its own; `Ctrl+V` is one capped insert. `tests/test_korean_ime.py` stays green in the whole-suite run.

**What differs from the plan, plainly:** (1) a page that is starting, absent or failed has no focus: the keys go to the address row until the page comes up; after an address is sent, the page has the keys and `Ctrl+L` or a click on the address row gives them back; (2) no input method pre-edit is shown in the page (the engine has no composition events: decided in M17), it shows in the address row; (3) a middle or right press over the page is sent to the page (as a press and a release); (4) nothing the person did is retried or replayed.

**Whole suite (cold, last run):** 564 tests, 562 passed; the two failures were the size ratchet (`ui.rs` 1,122 lines against 1,120: fixed, now 1,118) and `test_first_view` (the window test that is timing-flaky under load, as in P06: it passes alone and in the later run; P09 reproduces or explains it, on the M16-P08 tip too).

**Cost:** the counter stood at 72.3 after the `done` of P07 and at 79.0 here: about 6.7 USD for P08 (the lead's estimate with P07 7 to 8 each, stop rule 12). The counter moved by about 2 USD while I only waited for the suites; I cannot say why.
