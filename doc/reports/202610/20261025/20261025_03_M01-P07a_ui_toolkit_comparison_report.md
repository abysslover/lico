# M01-P07a report: lightweight Rust UI toolkits against the 50 MiB budget

Status: measured, awaiting review (G3). Nothing here is merged into the product; all code is under `exp/`.
Plan: [20261025_02_M01-P07a_ui_toolkit_comparison_plan.md](../../../plans/202610/20261025/20261025_02_M01-P07a_ui_toolkit_comparison_plan.md).
Result files are in the `data` subdirectory of `exp/toolkit_compare`.
Method: the M01-P01 rig (`src/lico_measure`, unchanged) and its report [20261023_01_M01-P01_architecture_measurement_report.md](../20261023/20261023_01_M01-P01_architecture_measurement_report.md).

## 1. What was built and measured

| Client | Toolkit and renderer (pinned) | Menu bar |
|---|---|---|
| reference | the hand-drawn client of the product (winit + softbuffer + cosmic-text), re-measured in the same run, not edited | drawn by hand |
| egui | egui 0.34.3 with egui_software_backend 0.0.3 (CPU path only; third-party crate, 0.0.x) | egui `MenuBar` |
| iced | iced 0.14.0, tiny-skia renderer (iced_tiny_skia 0.14.1), iced_aw 0.14.1 for the menu (community crate) | iced_aw `menu_bar` |
| slint | slint 1.18.1, software renderer, winit X11 backend, system fonts | native `MenuBar` of the toolkit |

Every candidate has a top menu bar (File, Session, Help), a 100-entry session list, one text-entry line and a read-only output area, and speaks the same
stdin/stdout and host-socket protocol as the reference (shared crate `exp/toolkit_common`). The Session menu holds a "find" text field (egui, iced: inside
the menu popup; slint: in a popup window opened from a menu item). The two 16 px marker blocks the rig reads sit at the window's top-left.
All clients are minimal: the output area is re-laid out from the whole text on every record, which matters for the noisy-output rows (section 3).

Runs: one rig run at display scale 1 and one at scale 2, all candidates in the same run, 1200x800 window, Xvfb with no window manager, software
rendering, agent-shaped UTF-8 text workload with 10000-line transcripts, `--ui`. The host was shared with other sessions (1-minute load average 2.7 to
10 during the runs), so timings carry that noise and there was no repeat run. Commands, receipts and raw samples: `run_s1`,
`run_s2` (receipt records the commit; the checkout paths were replaced by `<checkout>` in the committed copies, nothing else was edited).
Reproduce: `exp/toolkit_compare/build.sh {host,egui,iced,slint}`, then `exp/toolkit_compare/measure.py OUT SCALE`.

Rig adaptation, only under `exp/toolkit_compare`: the rig puts unix sockets under its repository root, which exceeds the 108-byte limit in a worktree, so
`toolkits.py` points the rig's root at a short private directory under `/dev/shm`; it registers the candidates and allows the Korean experiment for them.
`glyph_stress` was not run for the candidates (the clients have no stress command).

## 2. Comparison table (reference beside the candidates)

Idle RSS is the maximum of three launches (PERF-01 <= 50 MiB); PSS is the median. Growth is the paired 100-versus-10 session difference (PERF-02 <= 10 MiB).
Activation and restart are p95 (PERF-03 <= 100 ms, PERF-04 <= 500 ms). Idle CPU is the maximum of a 60 s window (PERF-05 <= 0.5 %).

Scale 1:

| Row | reference | egui | iced | slint |
|---|---|---|---|---|
| idle RSS, MiB | 22.75 | 27.29 | 25.40 | 31.96 |
| idle PSS, MiB | 13.22 | 21.45 | 15.75 | 20.94 |
| growth 10 to 100 entries, MiB | 0.22 | 0.56 | 0.92 | 0.43 |
| activation p95, ms | 70.2 | 8.0 | **102.8 (missed)** | 28.6 |
| restart p95, ms | 100.8 | 38.2 | 144.2 | 214.1 |
| cold start, median of 10, ms | 107 | 57 | 163 | 237 |
| idle CPU, % | 0.03 | 0.00 | 0.00 | 0.07 |
| noisy-output CPU, % (reported, no target) | 14.4 | 47.5 | 97.9 | 94.1 |

Scale 2:

| Row | reference | egui | iced | slint |
|---|---|---|---|---|
| idle RSS, MiB | 33.73 | **69.15 (missed)** | 39.38 | 43.23 |
| idle PSS, MiB | 18.61 | 57.93 | 24.27 | 27.16 |
| growth 10 to 100 entries, MiB | 0.12 | 0.13 | 0.97 | 0.51 |
| activation p95, ms | 68.4 | 29.7 | **160.1 (missed)** | 47.4 |
| restart p95, ms | 123.7 | 83.2 | 211.8 | 224.2 |
| cold start, median of 10, ms | 126 | 97 | 259 | 291 |
| idle CPU, % | 0.00 | 0.00 | 0.00 | 0.08 |
| noisy-output CPU, % (reported, no target) | 17.9 | 87.2 | 96.3 | 94.5 |

Packaging facts:

| Row | reference | egui | iced | slint |
|---|---|---|---|---|
| executable, bytes | 6455160 | 6599760 | 5561168 | 16142192 |
| crates reaching the binary (locked graph, Linux) | 101 | 152 | 126 | 367 |
| files mapped by the running client | 78 | 23 | 66 | 213 |
| self-contained executable from locked public dependencies | yes | yes | yes | yes |

The reference's crate count is for the whole hostd package, which also contains the session host, so it overstates the client alone. The mapped-file count
and bytes include system fonts the clients open themselves; egui reads two font files into its heap instead (so its file mappings look small and its
anonymous memory large). The inventory in `run_s*/raw.json` lists the largest mappings.

## 3. Reading of the numbers

- At scale 1 every client is under 50 MiB, with the reference lowest (22.75) and slint highest (31.96).
- At scale 2 egui is at 69.15 MiB, of which about 45 MiB is anonymous memory (45.3 versus 14.3 at scale 1). I did not isolate the cause (the software
  backend's frame buffers and my font loading are both candidates); the number stands as measured.
- iced misses PERF-03 at both scales (102.8 ms is 2.8 ms over; 160.1 ms at scale 2). Its restart and cold start are also the second slowest.
- Slint has the slowest restart (214 and 224 ms) but is within 500 ms, and has by far the largest executable and graph.
- Noisy-output CPU has no target in the budget. The three minimal clients rebuild the whole output text on every record, while the reference lays out only
  the lines it needs, so this row compares my naive clients with a tuned one and says little about the toolkits themselves. It is reported, not scored.
- Idle CPU and hidden growth are far inside their limits for every client.
- The reference was measured under the same load as the candidates; its own numbers here differ somewhat from P01/P07 reports (different load, scale handling,
  one run), so only same-run comparisons should be drawn from this table.

## 4. Missed budgets, each an owner decision

1. **egui at scale 2: idle RSS 69.15 MiB against 50 MiB (PERF-01).** Decision for the owner: whether egui is dropped, or its scale-2 memory investigated
   (not done in this phase).
2. **iced: activation p95 102.8 ms at scale 1 and 160.1 ms at scale 2 against 100 ms (PERF-03).** Decision for the owner: whether iced is dropped, or its
   view switch is investigated. Not tuned here.

No other row missed its target. Nothing is converted from a target into a result.

## 5. Input and accessibility checks

Run on private X servers with xdotool, a private session bus and ibus 1.5.29 with the Hangul engine; results in `inputs_*.json`
and `korean_entry_*.json`, screenshots in `data/calibration`.

| Check | reference | egui | iced | slint |
|---|---|---|---|---|
| Korean composition in the entry line, normal typing (80 ms/key), exact text | yes (rig) | yes | yes | yes |
| same, fast typing (8 ms/key) and under output, scale 1 / scale 2 | all exact / all exact | all exact / all exact | scale 1: the two fast runs wrong; scale 2: fast, under-output and fast-under-output runs wrong | all exact / one run wrong (fast under output) |
| Korean composition in a menu (text field), 80 ms/key | not built | exact | exact | exact (popup window opened from a menu item) |
| Backspace, Delete, Home, End, shift-selection, select all, copy and paste (clipboard read back through X) in the entry line | not measured | pass | pass | pass |
| selection and copy in the output area by dragging | not measured | copied output text | clipboard unchanged; not shown to work | copied output text, primary selection set |
| menu opened and an item chosen with the mouse | not measured | pass | pass | pass |
| menu used from the keyboard (alt+f then Down/Return, F10, alt+f then Return, Tab x3 then Return) | not measured | no menu event from any route | none | none |
| accessibility bridge | not built (P01) | none on this path | none (iced has no accessibility support) | AccessKit crates compiled in; live bridge not verified |

Notes: all four typing runs of the Korean sentence reuse the rig's own check (exact final text and committed text only grows). The keyboard rows record that
no menu event was observed for the four routes tried; arrow-key navigation inside an already mouse-opened menu was not tried. iced's menu popup is drawn
on tiny-skia as a black box with part of its content visible (see `data/calibration/iced_*`); mouse selection still worked. egui needs one configuration
line so a text field inside a menu does not close it on click. For slint the "find" field is a popup window, so "in a menu" means reached from a menu item.
Accessibility for slint was not verified because the rig runs no accessibility bus; its cost (a build without the feature) was not measured.

## 6. Licences

| Candidate | Finding |
|---|---|
| egui, egui-winit, egui_software_backend | MIT OR Apache-2.0; an MIT project may ship them. The software backend is a third-party 0.0.x crate. |
| iced, iced_tiny_skia, iced_aw | MIT (iced_aw MIT); an MIT project may ship them. iced_aw is a community crate. |
| slint 1.18.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`. Under the GPL-3 option the shipped binary would be GPL-3, which an MIT project cannot ship. The royalty-free licence allows desktop distribution if the project shows the toolkit's attribution widget in an About screen reachable from the top-level menu (or a badge on a public page) and forbids embedded use and exposing the Slint API. It is not an MIT or OSI-permissive licence. **Owner decision** whether those conditions are acceptable. 12 crates are under this triple licence. |

Lists per candidate with each crate's declared licence: `data/licenses_{reference,egui,iced,slint}.txt`. Besides the above, one crate each in egui and iced is
`Apache-2.0 OR GPL-2.0-only` (the Apache option applies); a few crates use the old "MIT/Apache-2.0" notation, still permissive. I read licence texts, not
legal advice; no lawyer was consulted.

## 7. What could not be measured or was not verified

- Slint accessibility bridge (needs an accessibility bus) and the cost of building without it.
- Why egui is large at scale 2, and why iced activation is slow; neither was investigated.
- Free controls and menu behaviour of the reference client were not run through the same scripts.
- Wayland, other input methods, a real GPU or compositor, and packages (only the executable size was measured).
- Repeat runs: one run per scale, on a shared host; differences of a few percent or a few milliseconds are within the noise I could see.
- Effort to build the same controls by hand, and long-term maintenance of third-party crates (egui_software_backend, iced_aw): not measured.
- The glyph-variety stress (needs a stress command the candidates lack).
- iced drag selection in the output area may be a limit of my scripted drag rather than the widget; not separated.

## 8. Process notes

- The acceptance tests (`exp/toolkit_compare/test_acceptance.py`, 15 checks) were written after the first draft of the clients, so they are not strictly
  test-first. Red evidence: an empty build root fails all 15 (`data/acceptance_red_empty_build_root.log`) and a mutation that keeps the markers on hide
  fails the hide check (`data/acceptance_red_mutation_hide_keeps_markers.log`); the green run is `data/acceptance_green.log` (15 passed).
- My first Korean runs failed for all clients including the reference because my shell exported `LANG=C`, which the rig passes on to the clients; the committed
  data all come from runs with `LANG=C.UTF-8`.
- This checkout has no `.venv`, so the repository hook skipped its test suite. A manual run of the existing suite showed one failure
  (`tests/test_client_integration.py` with candidate c2, a client not built in this checkout), unrelated to these changes; the `.venv` I made for that run was
  moved to the ignored `private/`.
- Rule slips of mine: three no-op `python3 -c` invocations that read no data, one of them printing nothing.

## 9. Options and recommendation

Options for the owner:

1. **Keep the hand-drawn client.** Lowest memory at both scales and every budget row met; text selection, accessibility and menu keyboard use remain to be built.
2. **Switch to one candidate.** None clears every row: egui fails scale-2 memory, iced fails activation, slint has the licence question, the largest binary and
   crate graph, and a slow restart (within budget). If one were taken, iced is the only one that is MIT, within the memory budget at both scales and gives Korean
   input in the entry line and in a menu; it would need the activation issue and the menu rendering fixed, and gives no keyboard menu use or accessibility.
3. **Keep the hand-drawn client and build a small shared control set** (entry line editing, selection, clipboard, a menu with keyboard use), using the
   toolkits' behaviour above as a checklist.

Recommendation: **option 3, starting from option 1.** On this evidence the toolkits do not buy back more than they cost: they use as much or more memory at both
scales than the reference, and every one misses a budget row, carries a licence condition, or lacks keyboard menu use. What they do show for free (editing keys,
clipboard, mouse menus, Korean in an entry line and a menu) is a concrete checklist for the hand-drawn client. This is a recommendation from measured resources,
licences and observed behaviour only; it is not a decision, and the missed budgets in section 4 and the Slint licence in section 6 remain the owner's.
