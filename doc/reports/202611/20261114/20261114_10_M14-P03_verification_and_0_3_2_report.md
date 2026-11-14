# M14-P03 Report — End-to-End Verification, Measurements, and `lico 0.3.2`

- Status: implemented, waiting for the owner's G3 and for the owner's check on the real desktop
- Version: 1.0.0
- Branch: `phase/M14-P03-verification-and-0-3-2` (stacked on `phase/M14-P02-maximized-first-start` at `928acf2`; code commit `ba7884a`)
- Plan: [M14-P03](../../plans/202611/20261114/20261114_10_M14-P03_verification_and_0_3_2_plan.md)

## What changed

- `lico` is `0.3.2` (`Cargo.toml`, `Cargo.lock`, `tests/test_single_executable.py`, `tests/test_control_parity.py`).
- `tests/test_m14_end_to_end.py` (private Xvfb, the window-manager model, a synthetic home; no real terminal, home or window manager): the window starts maximized (the state read from the X server); the path bar is the last row and shows the session's folder; a pointed directory row shows its full path; the icon in the menu row switches to windowed and back (the bar stays the last row, the icon stays in the menu row); two idle seconds draw nothing.
- Docs: CHANGELOG `0.3.2`, PLAN, roadmap, README, manual, `doc/index_4.md`, milestone and plan statuses.

## Measured verification (`lico 0.3.2`)

| Check | Measured result | Budget / Gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `670 passed; 0 failed` (`1` ignored, both umasks) | `0` failures | Met |
| Full cold suite (`suite run --cold`), three runs, each started on a host with load below `9` | `550` tests in `117` files, `550 passed` each: `125.1 s` / peak `1,472.2 MiB`; `124.7 s` / `1,455.4 MiB`; `123.5 s` / `1,469.0 MiB` | `<= 180.0 s`, `<= 2,048.0 MiB` | Met; margin `576..593 MiB` and `55..56 s` |
| Idle window RSS, scale 1 (`idle_memory`, 10 accounts, menu on, 3 launches) | `max = 25.48 MiB` (`25.13..25.48`), PSS `17.72..17.98`, host `7.95..8.09 MiB` | `< 50 MiB` | Met |
| Idle window RSS, scale 2 | `max = 36.29 MiB` (`36.21..36.29`), PSS `23.28..23.43`, host `7.93..8.02 MiB` | `< 50 MiB` | Met |
| Hidden background growth (`hidden_growth`, 10 vs 100 sessions, 3 repetitions) | `max = +0.50 MiB` (deltas `+0.22`, `+0.50`, `+0.14`; GUI `25.68..25.84` at 100 sessions; host `20.84..20.94`) | `<= +10 MiB` | Met |
| Two clean clones build identical binaries (`suite acceptance`) | `1 passed` in `205.37 s` | identical | Met |
| Tool list size (`tools/list`, both ends of the clipboard-write switch) | `4,849` bytes with writing off, `5,103` with writing on (unchanged) | `<= 5,120` | Met with `17` bytes to spare |
| Edges into the hubs by the new files | `grep crate:: gui/pathbar.rs gui/windowstate.rs`: no match (only `super::` and external crates) | no new upward edge into `core`, `session`, `folders`, `transfer`, `handoff`, `codex` | Met |

The measurements ran on the committed clean tree in the heavy lane, one after the other, each waiting for the host's load average to fall under `12` (another user's work had pushed it above `60` several times and made two earlier cold runs fail in load-sensitive tests; those runs are not counted). Compared with `0.3.1` (M13-P04): idle about the same (`25.13..25.48` against `25.45..25.64`), the cold peak inside the earlier range (`1,414.2..1,489.4`), the cold time `123.5..125.1 s` against `118.9..124.9 s` with ten more tests (`550` against `540`).

## Not verified here (the owner's check)

The maximized start was reproduced and fixed on a MODEL of a window manager (`lico_measure/fakewm.py`), not on the owner's mutter, and the cause on mutter is not proven (P02 report). The icon is the real test of whether mutter honours a maximize message for this window. After installing `0.3.2`, run `lico gui` three times from a terminal and tell me: does the window open maximized; does the icon at the right end of the menu row switch it to windowed and back; if not, the output of `xprop -id <window id> _NET_WM_STATE`.

## Proposed spec requirements

None added; LLM-01 to LLM-03 (M12 report) still wait for the owner's yes. M14 changes no contract of the host.

## Honest notes

- The end-to-end test was not run red: it composes behaviour that P01 and P02 built and tested; the version tests changed together with the version.
- Install: see the mailbox report (built by `src/host/hostd/build-release.sh` from the clean commit of this report; the `0.3.1` binary is kept at `private/release_0_3_1_backup/lico`).

## Not done / left to the owner

G3 acceptance and G4 merge of the M14 branches; the `milestone/M14-*` branch (the lead creates it); the check on the real desktop above; the component foundation that follows under the owner's order.
