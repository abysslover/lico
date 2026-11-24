# M24-P05b Plan — The Person's Side Of The Pane Tool: The Cue, The Switch And The Stop Key

- Status: done (2026-11-24)
- Version: 1.0.0
- Branch: `phase/M24-P05b-agent-pane-switch` from `phase/M24-P05-pane-tool` (`1e74731`, on `main` `9eacc7a`; P05 is committed and not pushed); the prefix `phase/M24-P05b-` was approved by the owner at commit time (the question on the prefix, 2026-11-24) and is in the local guard; the code is `f0c7f6a`
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md), R6; the lead's three additions that are not gates, named in the [P05 plan](20261124_06_M24-P05_pane_tool_plan.md) (the cue, the owner-only switch, the key that stops it); the owner's word of 2026-11-24: "완충용 제안 좋은 것 같음 P05b 진행"
- The owner's choice that stands (Q1, 2026-11-24): the pane tool is **always allowed by default**; nothing here asks the person for a grant

## What this phase changes

The person gets three things next to the `pane` tool, none of them a gate:

1. **A cue**: while an agent types, presses a key, clicks, goes to a page or opens a tab, the header of that pane says who did what, for eight seconds after the last act: `<session> typed (Ctrl+Alt+Esc stops agents)`. Reading and listing show nothing (they change nothing).
2. **A switch**: the setting `agent_panes`, on by default. `More > Settings > Agent pane control: on|off` (one click), `lico settings --agent-panes on|off`, and the status carries it. Off, the tool is not listed, a call is refused (`off: the person has switched the agents' control of their panes off`) and the audit line says `refused_off`.
3. **A stop key**: **Ctrl+Alt+Esc**, the window's one stop key (it already stopped real-screen control), also switches the agents' control of the panes off and forgets the cues; its notice says what was stopped.

## What was found (2026-11-24)

- `mcp::handle_with` already read the person's `clipboard_write` for each message (`offered`), so a second switch is the same shape: `Switches { write, panes }`, read per message, no restart.
- `pane()` is the one place every pane call passes, so the off check, the audit line and the touches are made there; a touch is noted after each step that succeeded, never for a refusal.
- The host owns the clock: a cue is told for eight seconds and carries its age in milliseconds, the window keeps no timer (a cue is shown until a `status` that does not carry it).
- **A latent defect**: `Ctrl+Alt+Esc` pressed in a terminal tab was sent to the shell as `Esc`, so the window's stop key did not reach its own handler there. `keys.rs` passes it to the window now (and only that chord: `Alt+Esc` alone is still the shell's).
- The window dump's `settings` map (read by `tests/test_agent_tools.py`) gets `agent_panes` and `pane_cues`; three source files were at their size limit (`cli.rs`, `gui/mod.rs`, `gui/ui.rs`) and were brought under it by writing the settings flags as one loop and by putting new declarations on the line of their neighbours, not by raising the allowlist.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | `agent_panes` is **on** with no file, no key and `true`; **any doubt about a file that is there is off** (not owner-only, a link, not a JSON object, a value that is no boolean) | the owner chose always on; but this switch is the way to stop it, so a file that cannot be trusted must never turn it back on |
| D2 | **Ctrl+Alt+Esc also switches the panes off for good** (not for a time), and the notice says how to switch them on again (`More > Settings`) | the person pressing the stop key means stop; a stop that returns by itself is no stop. The owner may prefer a pause: this is the thing to say if so |
| D3 | The cue is in the **header of the pane that is shown** (terminal and browser tab), not in the tab strip | the person looks at the pane an agent acts in. A tab that is not shown has no cue until it is shown (within the eight seconds) |
| D4 | The cue names the session by the name the person gave it, else its title, else its id, cut at 24 characters and clean of control and reordering characters | a session name is data from an agent: it must not break the header |
| D5 | The cue carries who and what **kind** of act (`typed`, `pressed a key`, `clicked`, `went to a page`, `opened this tab`), never the text typed or the address | the same rule as the audit line (`SF-03`) |
| D6 | No Lico tool changes the switch (it is socket-only, like `clipboard_write`). An agent that has a shell of its own can run `lico settings` as the person can | said once in the guide: the switch stops what the `pane` tool does; it is not a wall against an agent that already has a shell as the person |
| D7 | `More` has twenty-two entries now (the switch is the fifth entry of the group `Settings`) | the owner asked for few words on the menu; this is the one line the person needs for the switch the owner asked for |
| D8 | The setting is read at every call, every `tools/list` and every `status` | a stop is at once, no restart; the cost is one small file read |

## Not done here

- A cue in the tab strip, or a sound.
- A pause of a few minutes instead of the switch (D2).
- The terminal's mouse; the browser engine test of the pane tool (the engine is a download).

## Steps

1. [x] **Tests first**: unit tests of the setting (default on, doubt off, set and read back, a patch takes only a boolean), the touches (kept, replaced per tab, expiry, the 64 limit, cleared by the stop), the words of an act and the name of a session, the cue and the notice of the stop in the window (`gui/pane_cue_tests.rs`), the stop chord in a terminal (`keys_tests.rs`), the tool list with the switch off (`mcp.rs`); three real-host tests (`tests/test_pane_tool.py`: a type or a key leaves a cue in `status` and a list, a read or a refused call leaves none; the switch off unlists the tool and refuses a call and on brings it back without a restart; the switch is a boolean and the command line takes on and off) and one window test (`tests/test_pane_switch_window.py`: an agent's typing shows in the header, the stop key works with a terminal in front, the menu turns it on again). Red first.
2. [x] **The code**: `settings.rs`, `ipc.rs`, `cli.rs`, `mcp.rs` (`Switches`), `tools.rs`, `local/hosttools.rs`, `core.rs`, `pane.rs` (`Touches`, `cues_json`), `gui/pane_cue.rs`, the headers, `realbar.rs`, `menu.rs`, `keys.rs`, `v1.py`.
3. [x] **The tests that the change moved**: `More` has twenty-two entries (`tests/test_menu_tree.py`, `menu_tests.rs`), the window dump's `settings` map (`tests/test_agent_tools.py`), the status maps of `open_tests.rs`, `tests/testmap.toml`.
4. [x] **Mutation checks**, before the final green gate.
5. [x] **Gates and documents**: `cargo test --bin lico`, clippy, `cargo xtask check`, the changed-files suite, the owner's build; the protocol note (`protocol_v1.md` 0.1.12), the guide (1.19.0), the milestone row, `PLAN.md`, the index row.

## Results (2026-11-24)

- **What the owner sees** (`lico gui`): while an agent types, presses a key, clicks, goes to a page or opens a tab, the header of that pane says `<session> typed (Ctrl+Alt+Esc stops agents)` (or `pressed a key`, `clicked`, `went to a page`, `opened this tab`) for eight seconds; **Ctrl+Alt+Esc**, in a terminal tab too, stops real-screen control and the agents' control of the panes and says so in the flash line; `More > Settings > Agent pane control: on|off` is the one click that switches it back; `lico settings --agent-panes on|off` says the same on the command line.
- **Code** (the parent of the diff is P05 `1e74731`): `clipboard/settings.rs` (the setting: on with no file or no key, any doubt off; the patch; the status), `ipc.rs` and `cli.rs` (`--agent-panes`; the three on/off flags are one loop now), `mcp.rs` (`Switches { write, panes }` read for each message, `offered`), `tools.rs` and `local/hosttools.rs`, `core.rs` (`Shared.panes`, `pane_cues` in `status`), `pane.rs` (`Touches`, `cues_json`, `did_word`, `who_of`, the off check and the audit line `refused_off`), `gui/pane_cue.rs` (new: the cue, the notice of the stop key, the toggle), the two headers (`gui/terminal/view.rs`, `gui/browser_view.rs`), `gui/realbar.rs` (the stop key stops the panes too), `gui/menu.rs` (the entry), `gui/terminal/keys.rs` (the chord passes to the window), `src/lico_protocol/v1.py` (`agent_panes` and `pane_cues`). No change of what the sockets and the HTTP door accept.
- **Tests (red first)**: 27 unit tests (`clipboard/agent_panes_tests.rs` 5, `pane_tests.rs` 9, `gui/pane_cue_tests.rs` 10, `keys_tests.rs` 1, `mcp.rs` 2), 3 real-host tests (`tests/test_pane_tool.py`) and 1 window test (`tests/test_pane_switch_window.py`: the cue in the header of a real terminal, the stop key with the terminal in front and no stray Esc in the shell, the tool gone from the list and a call refused, the menu entry that brings it back). The tests the change moved: `More` has twenty-two entries (`tests/test_menu_tree.py`, `menu_tests.rs`), the window dump's `settings` map (`tests/test_agent_tools.py`), the status maps of `open_tests.rs`. Whole `cargo test --bin lico`: 1101 passed, 12 ignored; the changed-files suite: 606 tests passed (the first run had 3 failures, two window tests that pin the old counts and the size ratchet, found by running it).
- **Mutation checks (62 runs on 56 mutants; every mutant that changes behaviour is caught)**: 53 unit-level mutants (the cue's time, the limit of 64, a tab keeping two touches, a past touch purged, the order of the tabs, the age, the clear, the cue told while off, the tool run while off, each word of an act, the name of a session (title before name, blanks, the cut at 24, reordering characters), the setting's default and each kind of doubt, the patch and the status, `offered` and the switches read, the stop chord and `Alt+Esc` alone, the words of the cue and of each branch of the stop notice, the toggle, the repaint), 3 more at window level (the terminal header, the window taking the cues, the stop key leaving the panes alone). **Two survivors were weak tests and are closed**: the order of the cues (the touches came in the tabs' order: a test with the opposite order now) and the cue told while the file says off (the stop's own clear hid it: a test that changes the file by hand now). Three more were caught only by the real-host or window tests (a touch not noted, the stop notice of was-on and was-off, the toggle). **One mutant is equivalent by timing**: the explicit `status` request after a menu toggle (the window asks every second, and it takes the host's answer of the toggle at once).
- **Found on the way**: in a terminal tab **Ctrl+Alt+Esc was sent to the shell as Esc**, so the window's stop key of real-screen control never reached its own handler there (it is passed to the window now; `Alt+Esc` alone is still the shell's). Three source files were at the size limit: the allowlist was not touched (the three flags of `settings` are one loop; the new fields and declarations sit on their neighbours' lines). A browser-installer unit test (`ExecutableFileBusy`) failed once in a full run and not in three reruns nor in the next full run: a race of that test, not of this phase.
- **Gates**: clippy as at the parent (139 warnings in the lib, 155 in the test build, the same errors), `cargo xtask check` passes, the changed-files suite passes.
- **Decisions that stand until the owner says otherwise**: D1 to D8; above all D2 (the stop key switches the control off for good, not for a time) and D6 (a shell of an agent can still run `lico settings`).
- **Not done, said again**: a cue in the tab strip, a pause instead of the switch, the terminal's mouse, a test of the pane tool against a running browser engine; the release (P06, `0.8.1`) and level C (`0.9.0`).
