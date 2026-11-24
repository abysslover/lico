# M24-P07 Plan — Ctrl+Alt+Esc Pauses The Agents' Control Of The Panes

- Status: done (2026-11-25), code `432c37c`; not pushed (the prefix `phase/M24-P07-` was approved by the owner at commit time)
- Version: 1.0.0
- Branch: `phase/M24-P07-agent-pane-pause` from `main` (`8930894`: the release `0.8.1`, PR #12)
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), a follow-up row after the release, from the open item D2 of [M24-P05b](../20261124/20261124_07_M24-P05b_agent_pane_switch_plan.md)
- The owner's words (2026-11-25), answering the question whether the stop key should pause instead of switching the control off for good: "일시정지로 변경".
- This phase changes behaviour, so it is a phase of its own and not part of the release `0.8.1`; it has no version bump (the next release carries it: level C is `0.9.0`).

## What this phase changes

Today `Ctrl+Alt+Esc` stops real-screen control and switches the agents' control of the panes **off for good** (the setting `agent_panes` becomes `false` until the person switches it on in `More > Settings`). After this phase it **pauses** that control for five minutes and gives it back by itself:

1. The window's stop key asks the host to pause (`settings.set` with the new key `agent_panes_pause: true`); the host writes the time the pause ends (`agent_panes_paused_until`, seconds since the epoch) into the same settings file as the other switches.
2. While paused the tool `pane` is not offered (as when off), a call that is made anyway is refused with a sentence that says it is a pause, and the cues are not told.
3. The pause ends by itself; the person ends it at once with the menu entry `Agent pane control` (it says `paused` and a click resumes) or with `lico settings --agent-panes on`.
4. `status` says `"agent_panes": false` while paused (an older window reads "not allowed") and, only while paused, `"agent_panes_paused": <seconds left>`; the window shows it in the menu and in the notice after the stop key.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **Five minutes** (`PAUSE_SECS = 300`), one constant | "a few minutes" of D2 of P05b; long enough to take over a pane, short enough that the help comes back; real-screen control's fifteen is a grant, this is a stop |
| D2 | **The stop key pressed again during a pause starts the five minutes anew**; it never resumes | a stop key must not undo itself by a double press; the way back is the menu or the end of the time |
| D3 | **The permanent switch `agent_panes` stays and wins**: with `false` the state is off whatever a pause says; any write of `agent_panes` (on or off) removes a pause | the person's `off` is stronger than a pause; `on` means on |
| D4 | **Any doubt about the pause is off**: a value that is no whole number, or an end more than `PAUSE_SECS` ahead of the clock (a clock that went back, or a hand-written file), is off, not paused and not on | the rule of P05b: this is the person's way to stop it, so a doubt never turns it on; the person switches it on in the menu |
| D5 | **The pause lives in the settings file**, not in the host's memory | the tool list of the tool connection and the host read the file at each message already (`Switches::read`), so a pause is seen by both at once, with no new path between them |
| D6 | **The cue's words stay** ("Ctrl+Alt+Esc stops agents"); the notice after the key says it is a pause | a pause is a stop for a time; less change for the person |
| D7 | **The command line gets no `pause` word**; `lico settings --agent-panes on|off` stay | the pause is the stop key's; an agent with a shell of its own can run `lico settings` either way (P05b D6) |
| D8 | **No version bump**; no limit raised | the next release carries it |

## Steps

1. [x] **Tests first**, seen red: the settings (state, doubts, write, status), the tool (`Switches`, refusal), the window (the notice, the menu word, the sync) and the real window (`tests/test_pane_switch_window.py`: the stop key pauses, the tool is not listed, the menu resumes).
2. [x] **The host**: `agent_panes_state`, the key `agent_panes_pause`, the status field, the refusal sentence.
3. [x] **The window**: the stop key pauses, the notice, the menu word and the resume.
4. [x] **The protocol and the words**: `v1.py`, the protocol note `0.1.13`, the guide `1.21.0`; the CHANGELOG has no `Unreleased` form, so the next release's entry says it.
5. [x] **Mutation checks** of the new behaviour; `cargo xtask check`; the changed-files suite.
6. [x] **The owner's build** (`lico gui` shows the last commit); the report.

## Not decided here

- Push, pull request and merge of this phase (the owner's word); level C (`agy` as a Lico session, `0.9.0`) has its own plan; G3 is the owner's.

## Results (2026-11-25)

- **Tests first**: the tests were written before the code, but not run red: without the code they did not compile (the names they use did not exist), and the run that would have shown them red was skipped. The mutation checks below stand in for it.
- **The commit** `432c37c` (18 files, `+507 -53`): the host (`agent_panes_state_at`, `PAUSE_SECS = 300`, the key `agent_panes_pause`, the field `agent_panes_paused` of `status`, the refusal `paused: ...`), the window (the stop key asks for a pause, the notice says five minutes, the menu entry says `paused` and a click resumes), the protocol file and note `0.1.13`, the guide `1.21.0`. D1 to D8 held as written; no limit was raised and no version was bumped.
- **Tests**: `19` new host unit tests (host `1101` to `1120`, all pass), `1` new host test (`tests/test_pane_tool.py`, `10` tests) and the real window test (`tests/test_pane_switch_window.py`) extended: the stop key pauses, the notice and the header say it, a second press starts the five minutes anew, the tool is not listed, and the menu resumes. `cargo xtask check` and the module-graph tests (`19`) pass, the cycle of the hubs stays `50`.
- **Mutation checks**: `22` mutants (the seconds, both ends of the five minutes, the doubt, the switch that wins, the writes that end a pause, the refusal, the status, the window's reading, the request of the key and of the menu, the words): all `22` killed. `21` by the unit tests; the notice's direction (`Paused` and `WasOff` swapped) cannot be seen by a unit test that has no host and was killed by the real window test. Three mutants first did not compile (an overflow, an irrefutable pattern, a non-exhaustive match) and were rewritten and run again.
- **The commit hook refused twice before it passed** (the same as in P06, nothing bypassed): the size ratchet (`gui/ui.rs` grew to `1118` lines against its `1114`: the dump's field moved into `pane_cue.rs`, `ui.rs` is `1114` again) with, in the same run, `GUARD: the build's peak memory 1858.4 MiB is over the bound of 1536 MiB`; and the hook's own wall limit of `300 s` at a machine load of `16` to `17`. The third run passed: `572` tests in `125` files, `167.0 s`, peak `1,038.6 MiB`.
