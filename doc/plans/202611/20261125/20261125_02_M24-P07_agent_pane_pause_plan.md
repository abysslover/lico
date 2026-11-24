# M24-P07 Plan — Ctrl+Alt+Esc Pauses The Agents' Control Of The Panes

- Status: in_progress (2026-11-25)
- Version: 0.1.0
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

1. [ ] **Tests first**, seen red: the settings (state, doubts, write, status), the tool (`Switches`, refusal), the window (the notice, the menu word, the sync) and the real window (`tests/test_pane_switch_window.py`: the stop key pauses, the tool is not listed, the menu resumes).
2. [ ] **The host**: `agent_panes_state`, the key `agent_panes_pause`, the status field, the refusal sentence.
3. [ ] **The window**: the stop key pauses, the notice, the menu word and the resume.
4. [ ] **The protocol and the words**: `v1.py`, the protocol note, the guide, the CHANGELOG (an `Unreleased` entry if the file has the form).
5. [ ] **Mutation checks** of the new behaviour; `cargo xtask check`; the changed-files suite.
6. [ ] **The owner's build** (`lico gui` shows the last commit); the report.

## Not decided here

- Push, pull request and merge of this phase (the owner's word); level C (`agy` as a Lico session, `0.9.0`) has its own plan; G3 is the owner's.
