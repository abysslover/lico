# M01-P07a — Lightweight UI toolkit comparison under the memory budget (2026-10-25)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P07a-ui-toolkit-comparison` (experiment code only under `exp/`; the product source is not touched)
> Approval: G2 approved by the owner in the chat of 2026-10-25 ("agree to try option A": keep the 50 MiB principle, compare the lightweight libraries first). G3 and G4 pending.
> Cost cap: no provider-API cost; one independent development session, reported with the result.

## Why

The owner keeps the product's central promise: low memory and fast, with the idle client at or below 50 MiB RSS (PERF-01) and one self-contained
executable. The window is drawn by hand (winit, softbuffer, cosmic-text): idle RSS is 23 MiB at scale 1 and 34 MiB at scale 2, but every basic control
(editing, menu, list, selection, accessibility) must be written and reviewed by hand. The architecture comparison of M01-P01 measured GTK 3
(91.2 MiB), Qt 5 (93.0 MiB; 41.8 MiB with its GL integration off) and WebKitGTK (396.5 MiB), and did not measure Rust UI libraries that bring ready
controls. This phase measures them against the unchanged budget so that the choice for M02 is made on numbers.

## Scope

Candidates (each pinned to one exact version, at least two weeks old, public license read and recorded): `egui` with a CPU renderer path, `iced` with its
`tiny-skia` renderer, `slint` with its software renderer. A candidate whose license does not allow an MIT project to ship it is recorded and not built.
The current hand-drawn client is the reference (measured again in the same run).

Each candidate gets the same small client: a window with a top menu bar (File, Session, Help), a 100-entry session list, a text-entry line, and a
read-only output area, driven by the same synthetic host as M01-P01.

Measurements, same rules as M01-P01 (process-tree RSS and PSS, three launches, scale 1 and 2):
1. idle RSS against 50 MiB; growth from 10 to 100 list entries against 10 MiB;
2. activation p95 against 100 ms and restart p95 against 500 ms; cold start (median of 10);
3. idle CPU against 0.5 % of a core;
4. Korean composition through ibus on X11 in the text-entry line and in the menu (typed with xdotool, read from pixels);
5. what the toolkit gives for free: editing keys (Backspace, Delete, Home, End, selection, clipboard), menu keyboard use, accessibility bridge;
6. build: one self-contained executable from a public clone with locked, publicly available dependencies; crate count; binary size.

Out of scope: any change to `src/`, a decision to switch (that is the owner's), Wayland, Windows, provider sessions.

## Steps

- S01 plan approval, candidate versions and licenses recorded; the measurement rig of M01-P01 reused unchanged.
- S02 one candidate at a time: minimal client, red test for each acceptance row, green, raw numbers committed under `exp/`.
- S03 comparison table beside the reference, with each missed budget stated as a decision for the owner (no target is converted to a result).
- S04 report with the options, trade-offs and a recommendation; independent review by a fresh session.

## Acceptance

A table of measured values for every candidate and the reference, raw data and the commands that reproduce it, the license finding per candidate,
and the options for the owner: keep the hand-drawn client, switch to one candidate, or keep it and build a small shared control set. Nothing is merged to the
product.

## Progress

- 2026-10-25: Draft written after the owner confirmed that the 50 MiB principle stays; G2 approved the same day, execution starts.
