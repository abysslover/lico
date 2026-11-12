# M05 — Working-directory restore across restarts, paged directory navigation, and unified file opening (`lico 0.2.1`)

- Status: done
- Approved: 2026-11-12 (G1 and G2 approved by the owner for continuous execution; patch version bump `0.2.0` -> `0.2.1` on completion)
- Project: Lico
- Updated: 2026-11-12
- Implementation: M05-P01 to M05-P03 built, verified, accepted by the owner at G3 (2026-11-12), and merged and published on `main` at G4 (`lico 0.2.1`, 2026-11-12)
- Next gate: None (milestone achieved and closed on 2026-11-12)
- Milestone branch: `milestone/M05-workspace-continuity-and-file-navigation` (created at the closing commit of M05 on `main`; the phase branches are kept)

## Goal

Complete the everyday workspace-directory workflow in `lico gui` while keeping
Lico's single-executable footprint and `< 50.0 MiB` idle GUI RSS budget:
automatically restore the previous working directory and workspace view across
`lico` restarts, eliminate top-row clipping in panels and add explicit page
navigation (`Page X / Y`, `< Prev` / `Next >`, `PgUp`/`PgDn`, mouse wheel) for
large directories and text previews, and unify file activation so text-family
files open directly in the in-window viewer while web, document, image, media,
and binary files launch in the system default handler (`xdg-open`) without a
separate `V` HTML-source shortcut.

## Done when

1. Navigating folders in `Files` (`Step::Files`), selecting/starting a session
   with a working folder, or closing `lico gui` persists `last_cwd` and the last
   workspace view context (`last_view`, `last_session`) atomically in owner-only
   `0600` host state (and updates `session.options` `recent` / `last.cwd`), and
   reopening `lico gui` restores the previous working directory (re-opening
   `Files` in that folder, re-attaching the active session in its folder, and
   pre-populating `+ New` / `Files` with `last_cwd` even if no sessions remain).
   (`M05-P01`)
2. Panels anchor from the top row in `compose()` so header/action rows are never
   clipped regardless of window height or `12..=20` px font scaling; `folder.list`
   supports large directories (`<= 10,000` entries); and `Step::Files` provides
   full viewport-sized page navigation (`Page X / Y`, clickable `< Prev` and
   `Next >` toolbar controls, `PgUp`/`PgDn`, `[`/`]`, and mouse-wheel scrolling)
   across both directory listings and regular-file text previews. (`M05-P02`)
3. The separate `V view HTML source` shortcut is removed; clicking or pressing
   `Enter` on any regular file opens UTF-8 text-family files directly in the
   in-window paged viewer while launching system-handled rich/document/web/image/
   archive/binary files via the OS default application (`xdg-open`, respecting
   `LICO_OPEN_CMD` and `LICO_NO_AUTO_OPEN`); `lico` bumps patch version `0.2.0`
   -> `0.2.1`, passes cold suite + memory/hidden-growth gates + two-clone `suite
   acceptance`, and installs `~/bin/lico` `0.2.1`. (`M05-P03`)

## Why now

Hands-on daily use of `lico 0.2.0` highlighted three remaining friction points in
the directory-centric workflow: closing and reopening `lico` lost the previously
browsed working directory; directories with many entries could clip at the top or
lacked clear page-by-page navigation and mouse-wheel support; and keeping a
dedicated `V view HTML source` shortcut was less natural than opening text files
directly in-window and delegating `.html`/documents/images/binaries to the OS
default viewer on click. Delivering these refinements as patch release `0.2.1`
makes workspace browsing seamless.

## Phases

| ID | Phase | Plan | Report | Inspection | PR | Status |
|---|---|---|---|---|---|---|
| M05-P01 | Persist and restore previous working directory and workspace view across `lico` restarts | [Plan](../../../plans/202611/20261112/20261112_05_M05-P01_restore_working_directory_plan.md) | [Report](../../../reports/202611/20261112/20261112_05_M05-P01_restore_working_directory_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |
| M05-P02 | Top-anchored panel layout, large-directory listing, and page/wheel navigation (`Page X / Y`, `< Prev` / `Next >`) | [Plan](../../../plans/202611/20261112/20261112_06_M05-P02_directory_pagination_plan.md) | [Report](../../../reports/202611/20261112/20261112_06_M05-P02_directory_pagination_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |
| M05-P03 | Unified file activation (direct text preview vs system default `xdg-open`), remove `V` shortcut, and `0.2.1` patch release | [Plan](../../../plans/202611/20261112/20261112_07_M05-P03_smart_file_open_and_0_2_1_plan.md) | [Report](../../../reports/202611/20261112/20261112_07_M05-P03_smart_file_open_and_0_2_1_report.md) | Owner G3: accept (2026-11-12) | Fast-forward `main` | done |

## Gates

| Gate | State | Boundary |
|---|---|---|
| G1 milestone approval | Approved 2026-11-12 | Scope and phases `M05-P01` to `M05-P03`, patch version bump `0.2.0` -> `0.2.1` on completion |
| G2 phase plan approval | Approved 2026-11-12 | Continuous implementation across `M05-P01` to `M05-P03` |
| G3 inspection acceptance | Approved 2026-11-12 | Owner accepted `M05-P01` to `M05-P03` and bundles `00006`, `00007`, and `00008` |
| G4 merge approval | Approved 2026-11-12 | Fast-forward merge into `main`, milestone branch `milestone/M05-workspace-continuity-and-file-navigation`, and publication to `origin` |

## Blocked on

None. M05 achieved and closed on 2026-11-12.

## When done

Closed on 2026-11-12 at branch `milestone/M05-workspace-continuity-and-file-navigation` (`lico 0.2.1`):
- **Delivered (`3/3` M05 phases):** automatic working-directory (`last_cwd`) and workspace-view (`last_view`, `last_session`) persistence and restoration across `lico` restarts (`M05-P01`); top-anchored panel rendering in `compose()` with zero top-row clipping, `10,000`-entry `folder.list` cap, viewport-sized `Page X / Y` pagination, clickable `< Prev` / `Next >` toolbar controls, `PgUp`/`PgDn` and `[`/`]` keys, and mouse-wheel scrolling (`M05-P02`); and unified file activation opening UTF-8 text-family files directly in-window while delegating web, document, image, media, archive, and binary files to the OS default handler (`xdg-open`, removing the separate `V` shortcut), verified within `< 50.0 MiB` idle RSS (`24.36` / `35.34 MiB`), `+0.45 MiB` hidden growth, `97.1 s` cold 467-test suite, and two-clone reproducible `0.2.1` build (`M05-P03`).
