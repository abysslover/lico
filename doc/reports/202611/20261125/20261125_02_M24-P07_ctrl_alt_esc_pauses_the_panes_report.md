# M24-P07: Ctrl+Alt+Esc pauses the agents' control of the panes

- Version: 1.0.0
- Date: 2026-11-25
- Branch: `phase/M24-P07-agent-pane-pause` from `main` (`8930894`, the release `0.8.1`, PR #12); code `432c37c`; not pushed
- Source: the owner's word in the chat on 2026-11-25 ("일시정지로 변경"), answering the open item D2 of [M24-P05b](../../../plans/202611/20261124/20261124_07_M24-P05b_agent_pane_switch_plan.md); plan: [M24-P07](../../../plans/202611/20261125/20261125_02_M24-P07_agent_pane_pause_plan.md)
- Labels: MEASURED (a run), READ (from the code or the plan), COMPUTED.

## 1. What the person sees now (READ from the guide `1.21.0` and the code, MEASURED in the real window test)

| Place | Before (`0.8.1`) | Now |
|---|---|---|
| **Ctrl+Alt+Esc** | stops real-screen control and switches the agents' control of the panes off **for good** (until `More > Settings`) | stops real-screen control and **pauses** the agents' control of the panes for **five minutes**; the notice says so |
| Pressed again during the pause | nothing to stop | the five minutes start anew; it never resumes |
| The end of the pause | (none) | by itself, with no one doing anything; the tool is offered again at the next message |
| Menu `More > Settings > Agent pane control` | `on` or `off` | also `paused`; one click on `paused` resumes at once; on and off are the switch for good, as before |
| An agent's call while paused | `off: ...` | `paused: the person has paused the agents' control of their panes for a few minutes`; the tool is not in the list |
| `lico settings --agent-panes on` | on | on, and it ends a pause; there is no command-line word that pauses |

## 2. How it is built (READ)

- The pause is the key `agent_panes_paused_until` (seconds since the epoch) in the same settings file as the other switches, five minutes ahead of the host's clock. The host, the list of tools and the window read that file at each use already, so none of them needed a new path to the others.
- `agent_panes_state_at(path, now)` gives **on**, **off** or **paused with the seconds left**. `agent_panes: false` is off whatever a pause says; writing `agent_panes` (either way) removes a pause; a pause asked while it is off for good is not written (it would come back when the person switches it on). **Any doubt is off**: a value that is no whole number, or an end more than five minutes ahead of the clock (a clock that went back, a hand-written file), never turns the tool on.
- `status` says `"agent_panes": false` while paused (an older window reads "not allowed") and, only then, `"agent_panes_paused": <seconds left>`. The window shows `paused` in the menu and, in its test dump, the seconds.
- The cue's words stay ("Ctrl+Alt+Esc stops agents"): a pause is a stop for a time.

## 3. Checks (MEASURED)

- Host unit tests `1101` to `1120` (`19` new), all pass; `tests/test_pane_tool.py` (`10` tests) and `tests/test_pane_switch_window.py` pass; the module-graph tests (`19`) pass with the cycle of the hubs still at `50`; `cargo xtask check` passes; clippy shows nothing new in the changed code.
- **Mutation checks**: `22` mutants, all killed (`21` by the unit tests, the notice's direction by the real window test). Three first did not compile and were rewritten.
- **The commit hook** refused twice (the size ratchet: `gui/ui.rs` `1118` lines against `1114`, fixed by moving a function; and the hook's `300 s` wall limit at a load of `16` to `17`) and passed the third time: `572` tests, `167.0 s`, peak `1,038.6 MiB`. In the first run, in addition, `GUARD: the build's peak memory 1858.4 MiB is over the bound of 1536 MiB` (the owner's rule of 2026-11-25: the bound stays and an excess is told).
- The tests were written before the code but **not run red** first (without the code they did not compile); the mutation checks stand in for that.

## 4. What is not decided (the owner's) and known limits

- **Five minutes** is the lead's default (`PAUSE_SECS` in `clipboard/settings.rs`, one place); the owner said "pause" and no length.
- **Real-screen control** is still stopped for good by the same key (its grants are 15 minutes at most and are asked for again); only the control of the panes is a pause.
- An agent that has a shell of its own can run `lico settings --agent-panes on` and end a pause, as it could switch the control on again before (P05b D6).
- The pause is reached by the key, by the red banner's button and by the menu entry `Stop real-screen control` (all three are the window's stop); the entry is disabled when no real-screen control is active, so only the key is always there.
- No version bump and no CHANGELOG entry (the file has no `Unreleased` form): the next release carries it and says it.
- **Push, pull request and merge** are the owner's word. Level C (`agy` as a Lico session, `0.9.0`) and **G3** are as before.
- The build peak: `1,858.4 MiB` in one hook run against the bound of `1,536` (`1,550.1` in P06); the run that follows passes with the build kept. The cause is the build of the whole crate after many modules changed; the owner may want it lightened.
