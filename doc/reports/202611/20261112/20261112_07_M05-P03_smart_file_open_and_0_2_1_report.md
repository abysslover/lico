# M05-P03 Report — Unified file activation, remove `V` HTML-source shortcut, and `0.2.1` patch release

- Status: done
- Version: 1.0.0
- Date: 2026-11-12
- Milestone: [M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)](../../../milestones/202611/20261112/20261112_02_M05_workspace_continuity_and_file_navigation_milestone.md)
- Plan: [M05-P03 plan](../../../plans/202611/20261112/20261112_07_M05-P03_smart_file_open_and_0_2_1_plan.md) (G1 and G2 approved by the owner on 2026-11-12)
- Branch: `phase/M05-P03-smart-file-open-and-0-2-1`
- Inspection bundle: `202611/20261112/00008_M05-P03_smart_file_open_and_0_2_1/`

## Summary

`M05-P03` completes Milestone `M05` (`lico 0.2.1` patch release) by removing the
separate `V view HTML source` shortcut from `Files`, unifying file activation so
UTF-8 text-family files open directly in the in-window paged viewer while web,
document, image, audio/video, archive, database, and binary files launch via the
system default handler (`xdg-open`), bumping `lico` from `0.2.0` to `0.2.1`, and
re-verifying the cold test suite, memory/hidden-growth gates, and clean
two-clone reproducible build (`suite acceptance`).

## Delivered changes

1. **S02 — Unified file classification and activation (`src/host/hostd/src/fsview.rs`, `src/host/hostd/src/gui/files_ui.rs`, `src/lico_protocol/v1.py`, `tests/test_workspace_files.py`)**:
   - Added `uses_system_opener(name: &str) -> bool` in `fsview.rs` (`429` lines) and `"external": bool` on `folder.list` entries (matching `src/lico_protocol/v1.py`) for `.html`/`.htm`/`.xhtml`, documents (`.pdf`, `.doc`/`.docx`, `.ppt`/`.pptx`, `.xls`/`.xlsx`, `.odt`/`.ods`/`.odp`, `.epub`, `.rtf`), images (`.png`, `.jpg`/`.jpeg`, `.gif`, `.webp`, `.svg`, `.bmp`, `.ico`, `.tif`/`.tiff`, `.avif`), audio/video, archives, databases, and native binaries.
   - Removed the `Key::V` (`view HTML source`) key handler and footer hint from `files_ui.rs` (`753` lines).
   - Unified `files_activate()` so clicking or pressing `Enter` on an entry with `external == true` (or a regular file whose `folder.read` probe reports `binary == true`) launches the OS default opener (`xdg-open`, respecting `LICO_OPEN_CMD` and `LICO_NO_AUTO_OPEN`, passing the canonical path directly via `Command::new` with zero shell interpolation and reporting `Opened with system default: <name>`), while UTF-8 text-family files open directly in the bounded `64 KiB` paged viewer.
   - Updated `tests/test_workspace_files.py` to assert `"external"` on `folder.list`, verify `.html`, `.pdf`, and binary `.bin` activation via `LICO_OPEN_CMD`, verify pressing `v` does not open HTML source, and verify `.md` opens directly in-window.
2. **S03 — Patch version bump (`0.2.0` -> `0.2.1`) and documentation (`Cargo.toml`, `Cargo.lock`, `tests/test_control_parity.py`, `tests/test_single_executable.py`, `doc/manual_end_to_end.md`, `PLAN.md`, `README.md`, `roadmap.md`)**:
   - Bumped `lico` package version from `0.2.0` to `0.2.1` per the owner's patch-increment rule.
   - Updated version assertions and public manual/status documentation (`main` remains noted at `0.2.0` until G4 merge approval).
3. **S04 — Full cold suite, memory/hidden-growth pass, clean two-clone build check, `~/bin/lico` `0.2.1` install, and manual inspection bundle**:
   - Ran Rust unit tests under both `umask 022` and `umask 077`, full cold test suite, `idle_memory` at scales 1 and 2, `hidden_growth` (`10` vs `100` sessions), and `suite acceptance`.

## Measured verification (`lico 0.2.1`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `581 passed; 0 failed` (`1` ignored, both umasks; + `2 passed` in `synth_agent`) | `0` failures | Met |
| Full cold suite (`suite run --cold`) | `467` tests in `86` files, `0` failed, **`97.1 s`**, peak **`1,745.5 MiB`** | `<= 180.0 s`, `<= 2,048.0 MiB` | Met |
| Idle GUI memory, scale 1 (`idle_memory`, 10 accounts, 10k realistic lines, menu on) | `max = 24.36 MiB` (`24.23..24.36 MiB`), median PSS `14.10 MiB`, anon `3.94 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Idle GUI memory, scale 2 (`idle_memory`, 2x HiDPI) | `max = 35.34 MiB` (`35.27..35.34 MiB`), median PSS `22.41 MiB`, anon `3.94 MiB` | `<= 50.0 MiB` (`PERF-01`) | Met |
| Hidden background session growth (`hidden_growth`, 10 vs 100 sessions) | `max = +0.45 MiB` (`24.20 MiB` -> `24.52 MiB`; host `8.52` -> `20.32 MiB`) | `<= +10.0 MiB` (`PERF-02`) | Met |
| Clean two-clone reproducible build (`suite acceptance`) | Verified on clean committed `HEAD` (`lico 0.2.1` identical across two clean clones) | Byte-for-byte match | Met |
| Module size ratchets (`core.rs` / `ui.rs` / `fsview.rs` / `files_ui.rs` / `panel.rs`) | `988` / `1,115` / `429` / `753` / `624` lines | `<= 988` / `<= 1,120` / `<= 800` | Met |

## Gate status

All three phases of Milestone `M05` (`M05-P01` through `M05-P03`, `lico 0.2.1`) are complete on `phase/M05-P03-smart-file-open-and-0-2-1` and await owner G3 inspection acceptance and G4 merge approval.
