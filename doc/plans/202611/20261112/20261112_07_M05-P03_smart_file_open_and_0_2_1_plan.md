# M05-P03 Plan — Unified file activation, remove `V` HTML-source shortcut, and `0.2.1` patch release

- Status: in_progress
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)](../../../milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md)
- Branch: `phase/M05-P03-smart-file-open-and-0-2-1`
- Approval: G1 and G2 approved by the owner on 2026-11-12.

## Why

In `lico 0.2.0`, only `.html`/`.htm` files launched externally (`xdg-open`) while
all other files went to `folder.read` (showing `(binary file ...)` for PDFs,
presentations, images, or archives), and a separate `V view HTML source` shortcut
remained in `Files`. The owner requested removing the separate `V` shortcut and
making file selection (`Enter` or click) naturally open text-family files
directly in-window while delegating `.html`, documents, images, media, archives,
and binary files to the OS default application (`xdg-open`), followed by a patch
version bump (`0.2.0` -> `0.2.1`).

## Scope and steps

| Step | Scope | Verification |
|---|---|---|
| S01 | Commit this `M05-P03` plan and update milestone/index links | Pre-commit guards pass |
| S02 | Extend `fsview.rs` and `src/lico_protocol/v1.py` with `uses_system_opener` (`external: bool` on `folder.list` entries for `.html`/`.htm`/`.xhtml`, documents `.pdf`/`.pptx`/`.docx`/`.xlsx`/etc., images `.png`/`.jpg`/`.svg`/`.webp`/etc., audio/video, archives, databases, and native binaries); remove `V view HTML source` from `files_ui.rs` and unify `files_activate` so UTF-8 text files open directly in the paged viewer while `external` or binary (`folder.read` `binary == true`) files launch via `xdg-open` (respecting `LICO_OPEN_CMD` and `LICO_NO_AUTO_OPEN`) | Rust unit tests, `test_protocol_conformance.py`, and `test_workspace_files.py` pass |
| S03 | Bump package version `0.2.0` -> `0.2.1` in `src/host/hostd/Cargo.toml` and `Cargo.lock`, update version assertions (`test_control_parity.py`, `test_single_executable.py`) and user/status documentation (`doc/manual_end_to_end.md`, `PLAN.md`, `README.md`, `roadmap.md`) | `cargo test` under `umask 022` and `umask 077` passes |
| S04 | Run cold full suite (`<= 180 s`, `<= 2,048 MiB`), `idle_memory` (scale 1 & scale 2 `< 50 MiB`), `hidden_growth` (`<= +10 MiB`), write `M05-P03` report, commit cleanly, run two-clone `suite acceptance`, install `~/bin/lico` `0.2.1`, capture bundle `00008_M05-P03_smart_file_open_and_0_2_1/`, and stop at `M05` G3/G4 | Cold suite, memory budgets, and `suite acceptance` pass |

## Personal data and security

External opening continues to pass the canonical file path as a single argument
to `Command::new` (`xdg-open` or `LICO_OPEN_CMD`, never via a shell), refuses
symlinks and protected host state directories, and honors `LICO_NO_AUTO_OPEN`.
