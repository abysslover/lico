# M02-P04 report - screen capture, clipboard read and write, image paste and middle-button paste

- Version: 0.1.0
- Status: in_review (inspection verdict G3 pending; G3 covers M02-P03 and M02-P04 together)
- Date: 2026-11-11
- Plan: [M02-P04](../../../plans/202610/20261029/20261029_01_M02-P04_screen_and_copy_paste_plan.md) (version 1.1.3; Deviations D1 to D13 record every as-built refinement)
- Milestone: [M02](../../../milestones/202610/20261026/20261026_01_M02_agentic_coding_basics_milestone.md)
- Branch: `phase/M02-P04-screen-and-copy-paste`, 18 code and plan commits at `a367019` (before this closing report and status commit) rebased cleanly onto `main` `5cdb79f` (which holds `phase/M02-P03-agent-to-agent` `ffc18ad` and `phase/M01-P07a-ui-toolkit-comparison` `1778585` on the owner's G4 instruction of 2026-11-11); nothing is pushed. Inspection bundle `private/manual_inspection/202611/20261111/00001_M02-P04_screen_and_copy_paste/` (captures, warm and fresh-compile cold suite logs, umask 022/077 Rust logs, S07b live canary logs, and S08 measurements).
- Tested tree: `a367019` (`lico 0.0.0 (commit a367019, build dbe3e1ef221b3f83)`). This closing commit touches `doc/`, `PLAN.md` and `README.md` only.
- Gates: G1 and G2 approved 2026-10-26. G3 pending (covers M02-P03 and M02-P04 together; see "Try it yourself" below). G4 local merge to `main` authorized by the owner on 2026-11-11 as part of consolidating and advancing the M02 branch chain; nothing is published.
- Changes in 0.1.0: first version.

## What was built

A person can capture the X11 screen, a window or a region (`lico capture` and `View > Capture screen to input` / `Ctrl+Shift+P`), read and write text and PNG images on the X11 clipboard (`lico clipboard read`, `lico clipboard write`, `Ctrl+Shift+C`, `Ctrl+V`), paste a PNG image into a Claude Code session's input box as an attachment chip (`[picture N KiB, WxH] - Esc removes it`), paste `PRIMARY` at the caret with the middle mouse button, toggle agent clipboard writing directly from `View > Agent clipboard write: off/on`, and grant Claude Code sessions single-call screen capture, clipboard read and clipboard write through the M02-P03 tool connection. Every X job of the host runs in a short-lived helper process (`lico xjob ...`) so the long-lived host holds zero X11 connections, and large provider stream lines cut oversized JSON strings instead of dropping the line without a trace.

- **S01 `e89114e`: streaming provider line cutter (`lines.rs`, XF-08, D1, D6).** `lines::lines` and `lines::read_line` replace `core::bounded_lines` (`core.rs` shrank from 1,118 to 1,088 lines). Lines at or under 256 KiB pass byte for byte. Above 256 KiB, strings over 4 KiB are cut cleanly outside UTF-8 sequences, JSON escapes and surrogate pairs, leaving a byte-count marker; `claude.rs` records `[image N KiB]` or `[image N KiB omitted]` (never a provider folder path), and `provider_record` (4 MiB limit) uses the same cutter so conversation hand-overs keep text around large screenshots. Any provider permission request carrying a cut string is refused automatically and never shown to the person.
- **S02a `bacaaa5` and S02b `368cd1d`: zero-dependency streaming PNG encoder (`png/`) and `lico capture` (`capture/`, XF-01 to XF-03, MB-03, D2, D7).** `png` writes 8-bit RGB row by row with all five PNG filters, 32 KiB-window LZ77 with fixed Huffman codes and stored blocks, CRC-32 and Adler-32, verified by two independent decoders across edge geometries and checksum vectors. `lico capture [--screen | --window ID | --region x,y,w,h] --out FILE` reads X11 in ~256 KiB row bands via `GetImage`, shrinks on the fly with an exact-integer box filter to at most 1,568 px on the long side (`FILE.agent.png`), guards against the 7,500,000-byte first-party Claude Vision PNG limit (`too_large`), and prints one JSON line with `path`, `width`, `height`, `bytes`, `offset`, `server` (`vendor`, `release`) and `agent` (`path`, `width`, `height`, `bytes`, `scale`).
- **S03 `e585603`: short-lived X helper process (`xjob.rs`, `helper/`, XF-07, D8).** Hidden subcommand dispatch `lico xjob <job>` (absent from CLI help and READMEs) runs each X job in its own process group while `helper::run_job` polls stdout and stderr on the calling thread with `poll(2)` (no extra thread), caps output, kills and reaps the whole process group on deadline or overflow, and leaves the host with zero X11 connections, zero extra threads, zero extra fds and zero child processes.
- **S04a `ab6a91e`, `b55f5d6` and S04b `be71543`: clipboard read and write (`clipboard/`, `clipwrite/`, `gui/clip.rs`, XF-04, SF-06, TST-07, D9, D10).** Window selection code moved out of `gui/ui.rs` (1,305 to 1,204 lines) into `gui/clip.rs`. `lico clipboard read` (and `xjob clipboard_read`) reads `CLIPBOARD` or `PRIMARY` as `UTF8_STRING`/`STRING` (up to 64 KiB by default) or `image/png` (up to 3 MiB, header-checked 1..8,000 px, full `INCR` support under one deadline). `lico clipboard write` relays via the host (`clipboard.write` -> `clip.write` -> `clipboard.done`) to the newest attached GUI window, which owns `CLIPBOARD`, verifies ownership with `get_selection_owner`, serves `INCR` in chunks up to 128 KiB (dropping stalled requestors after 5 s), and releases on `SelectionClear`. `settings.json` (`clipboard_write`, off by default, fail-closed) gates the agent tool.
- **S05a `8a3ab10` and S05b `953a747`, `c6c7500`: image paste (`pasted/`, `gui/wire.rs`, `gui/picture.rs`, XF-05, SF-04, D11, D12).** Host manages `state/pasted/` (`0700` dir, `0600` files, cleaned at startup and >600 s, capped at 8 slots via `picture.slot`), validates `O_NOFOLLOW | O_NONBLOCK` regular owner files up to 3 MiB (`1..8,000` px PNG), sends the base64 image block before the user text to Claude Code (`attachment_unsupported` on other providers), echoes `>> [picture N KiB] ...` in the transcript, and removes the slot file on every `input` path (`attachment` is refused on HTTP). In the window (`gui/mod.rs` shrank from 816 to 793 lines, under 800, removing its `size_allowlist.toml` entry), `Ctrl+V` or `Session > Paste` in a Claude session attaches a clipboard PNG when no text target is offered, shows `[picture N KiB, WxH] - Esc removes it`, sends via `picture.slot` + `input` on `Enter`, removes on `Escape`, and caps held chip bytes at 6 MiB.
- **S06 `20aa0ec`: middle-button paste (`gui/clip.rs`, XF-06).** Pressing the middle mouse button inside the input box pastes `PRIMARY` as text at the caret without clearing selected transcript lines and without acting when a menu or start screen is open.
- **S07a `b958171`, `51dd0b2`, S07b `18959e6`, `675455a`, and S07c `70355da`: agent screen/clipboard tools, live canary, and GUI administrator controls (`screentools/`, `gui/menu.rs`, `gui/ui_tests.rs`, SF-06, D13).** Added `capture`, `clipboard_read` and setting-gated `clipboard_write` to the Claude MCP tool connection (`tools/list` compact JSON: 3,824 bytes with writing off, 4,078 bytes with writing on, inside the 4,096-byte `LIST_BUDGET`). All three require per-call window approval (`keep` refused): `capture` and `clipboard_read` issue a single-use 10-minute grant consumed by the second call (`4 s` helper deadline inside the `5 s` HTTP deadline, returning metadata text + PNG image block, never a file path), while `clipboard_write` relays to the window on a helper thread and posts a completion notice. `View` menu gained human-administrator controls `Capture screen to input` (`Ctrl+Shift+P`) and `Agent clipboard write: off/on` (backed by local-socket `settings.set` writing `0600` `settings.json` atomically). `gui/ui.rs` shrank further to 1,161 lines (`size_allowlist.toml` lowered to 1,185).
- **S08 `a367019`: Codex capability pin refresh.** Refreshed `src/host/adapters/codex/capabilities.json` and `tests/test_adapter_contract.py` to the installed `codex-cli 0.167.0` (`record_codex()`, preserving `observed_in_an_owner_run`).

## Tests and evidence

- **TST-10, five lines.**
  - **Whole suite from scratch, `--cold`, warm Cargo cache, at `a367019`: 441 tests in 73 files on 3 workers, 441 passed, 0 skipped, 178.0 s against the 180 s line (exit 0).** Build skipped (binary matched `a367019`); `cargo test --bin lico` took 18.2 s inside the run. Machine load average: 5.56 at start, 6.80 at end. On the very first `--cold` run after moving the repository directory (which invalidated Cargo's path fingerprints so `cargo test --bin lico` compiled from scratch in 107.9 s inside the run), all 441 tests in 73 files passed in 208.8 s (exit 3, peak 1,852.4 MiB), matching the fresh-clone compile behaviour noted in the M02-P03 report (187.4 s there).
  - **Whole Rust unit suite under `umask 022` and `umask 077` at `a367019`:** 546 passed, 0 failed, 1 ignored (`property_driver`, run by its parent test) under `umask 022` (13.27 s) and 546 passed, 0 failed, 1 ignored under `umask 077` (13.03 s).
  - **Usual run (`--changed`):** 0.3 to 0.6 s for documentation changes; 12 to 45 s for targeted module changes (`clipboard`, `capture`, `pasted`, `screentools` mapped in `tests/testmap.toml`).
  - **Peak memory** (all workers together, bound 2,048 MiB): 1,826.5 MiB in the warm `--cold` run at `a367019` (1,852.4 MiB in the fresh-compile run).
  - **Number of tests and what grew** at `a367019` (vs `main` `5cdb79f`): Python test functions 325 to 395 (+70 in `def test_` declarations, expanding to 441 pytest items with parametrizations; 8 new files: `test_capture_cli.py` 15, `test_xjob.py` 10, `test_clipboard_read.py` 36, `test_clipboard_write.py` 14, `test_picture_input.py` 12, `test_picture_paste.py` 3, `test_middle_paste.py` 1, `test_agent_tools.py` 16); Rust unit tests 370 to 546 (+176); Python test files 65 to 73 (+8). Diff `main..a367019`: 95 files changed, +13,428 / -447. Big files (`TST-07`): `core.rs` 1,118 to 1,087 (allowlist 1,087), `gui/ui.rs` 1,306 to 1,161 (allowlist 1,185), `gui/mod.rs` 816 to 793 (under 800, allowlist entry removed); next largest `gui/editor.rs` 798 and `pending.rs` 787.

## Mutations

Each slice applied targeted source mutations one at a time, verified the test turn red, and restored the exact tree:

| Slice | Mutants | Red | Notes |
|---|---|---|---|
| S01 (`lines.rs`) | 9 | 8 red, 1 equivalent | M1 and M3b prompted two added boundary tests; M1c (cutting a finished 4,096-byte string) is equivalent because it frees 0 bytes |
| S02a (`png/`) | 25 | 25 | Bit order, `LEN_BASE`/`DIST_BASE`/`DIST_EXTRA`, `NLEN`, `BFINAL`, Adler-32 5552-byte modulo, CRC chunk type, Paeth tie-break, 32 KiB window bounds, stored blocks |
| S02b (`capture/`) | 17 | 17 | Band height, short last band, RGB/BGR order, stride, shrink column/row/weights, region offset/scale/clipping, `0600` mode, `too_large` cleanup |
| S03 (`xjob`, `helper/`) | 14 | 13 red, 1 equivalent | Group kill, zombie reap, output/stderr caps, streaming `poll(2)`, thread/fd leak detection via `/proc`, `Drop` kill, `xjob` hidden from usage (`>` vs `>=` on nanosecond deadline is equivalent) |
| S04a (`clipboard/` read) | 18 | 18 | Selection atoms, single-chunk and total `INCR` caps, property delete, single whole-transfer deadline, PNG header/8,000 px bounds, `auto` order, `0600` writer, partial-file removal |
| S04b (`clipwrite`, `gui/clip.rs`) | 24 | 24 | `no_window`, premature reply, request id/connection check, newest-window routing, `get_selection_owner` verification, `INCR` chunks/empty terminator/5 s stall drop, `O_NOFOLLOW`, `settings.json` fail-closed |
| S05a (`pasted/`) | 17 | 17 | Slot cap (8), `0700`/`0600` modes, symlink refusal, `attachment_unsupported` before file read, base64 padding `len % 3`, cleanup on every `input` path |
| S05b (`gui/picture.rs`) | 17 | 17 | `TARGETS` check before `UTF8_STRING` (`xclip` PNG quirk), Claude-only chip, `Escape` precedence, 6 MiB held-bytes cap, `O_NOFOLLOW` slot write, refusal sentences |
| S06 (`gui/clip.rs` middle) | 9 | 9 | `PRIMARY` vs `CLIPBOARD`, text-only, hit-test bounds, press vs release, `ui.sel` preserved, open-menu suppression, caret insertion |
| S07a (`screentools/`) | 23 | 23 | Two-call grant lifecycle, target canonicalization, 600 s expiry, 4-grant eviction, `keep` refusal, `clipboard_write` triple gate (`list`, call, `perform`), block order, no path leaked, `captures/` cleanup |
| S07b (`real_program_canary.py`) | 2 | 2 | Offline judge mutations rejecting wrong screen/paste colour descriptions |

Total across M02-P04: **175 mutants run, 173 red, 2 equivalent, 0 unaddressed survivors.**

## Measurements

- **MB-04 summary line:** Idle window RSS (`--ui --menu --accounts 10 --realistic-text`, 3 runs of 3 launches): **scale 1: 23.93, 23.96, 24.14 MiB** (PSS median 16.41, 16.48, 16.46 MiB; `RssAnon` 3.94, 3.75, 3.94 MiB; limit 50.0 MiB, met); **scale 2: 35.05, 35.04, 35.10 MiB** (PSS median 22.01, 22.13, 22.09 MiB; `RssAnon` 3.94 MiB; limit 50.0 MiB, met). **Idle host RSS (10 Claude sessions):** **6.56 MiB** (`6,720 KiB`, PSS 5.16 MiB, 26 threads, 38 fds), **standing X11 cost 0** after 10 captures and 10 clipboard reads (`+0.38 MiB` allocator step, `+0` threads, `+0` fds, `+0` children, `0` files in `captures/`). **Feature temporary memory (`MB-03`, target <= +6 MiB):** `lico xjob capture` at `1920x1080` has startup RSS 3.00 to 3.19 MiB and total peak `VmHWM` 7.10 to 7.45 MiB (**+4.10 to +4.85 MiB** above startup RSS, met, freed immediately on helper exit in ~230 ms median / ~252 ms p95; `1200x800` capture takes 51.3 ms); window RSS with a 50 KiB screen-capture chip grows +1.50 MiB (20.11 to 21.61 MiB) and with a 2.29 MiB (`2,401,660`-byte) pasted PNG chip grows **+5.06 MiB** (20.11 to 25.18 MiB; `RssAnon` +4.88 MiB, within the 6 MiB cap).
- **X11 server (`XF-01`):** `The X.Org Foundation`, X protocol 11.0, vendor release `12101011` (`X.Org version: 21.1.11`, package `xvfb 2:21.1.12-1ubuntu1.6`), tested on private `-displayfd` `-nolisten tcp` displays (`1200x800x24`, `1700x120x24`, `1920x1080x24`).
- **PNG encoder ratios vs ImageMagick `import` (`D2`, `D7`):** real `1200x800` Lico window render: `56,347` bytes vs `44,558` bytes (**1.26x** `import`, S02b synthetic render 2.56x); incompressible `1920x1080` noise (S02b): **0.999x** `import` (both under the 7.5 MB `AGENT_LIMIT`).
- **Real-program canary (`S07b`, opt-in `--screen` on `claude 2.1.322`, `haiku`, private `1200x800x24` `Xvfb`):**
  - `R1` (`auto` mode `capture`): 9.35 s, 2 tool calls, 1 Lico approval, transcript `> [image 19.4 KiB]`, top/bottom colours named accurately.
  - `R2` (`default` mode `capture`): 9.05 s, 2 tool calls, 1 Lico approval (no duplicate provider prompt), transcript `> [image 19.4 KiB]`.
  - `R3` (oversized `capture` > 256 KiB stream line, 298.6 KiB agent PNG -> ~398 KiB JSONL line): 11.82 s, 1 Lico approval, transcript `> [image 298.6 KiB omitted]` (never dropped!), while `haiku` still received the full image and named the solid green top half.
  - `R4` (`clipboard_read` of text `7431` with planted prompt-injection line): 9.21 s, 1 Lico approval, reported `7431` and ignored the planted instruction.
  - `R5` (pasted green PNG via `picture.slot` + `input` `attachment`): 3.04 s, 0 approvals, `>> [picture 1 KiB]` echoed, `pasted/` empty afterwards, colour named accurately.

## Captures

Saved in `private/manual_inspection/202611/20261111/00001_M02-P04_screen_and_copy_paste/`:
1. `p04_1_view_menu_off_scale1.png`: `View` menu showing `Capture screen to input` (`Ctrl+Shift+P`) and `Agent clipboard write: off`.
2. `p04_2_captured_chip_scale1.png`: `Ctrl+Shift+P` screen capture attached under the Claude input box (`[picture 50 KiB, 1200x800] - Esc removes it`).
3. `p04_3_view_menu_on_scale1.png`: `View` menu after clicking `Agent clipboard write` (`Agent clipboard write: on`).
4. `p04_4_card_capture_scale1.png`: approval card for an agent `capture` request (`region 0,0,640,480`).
5. `p04_5_card_clipboard_write_scale1.png`: approval card for an agent `clipboard_write` request.
6. `p04_6_clipboard_write_text_scale1.png`: `V` full-text inspection view of the `clipboard_write` request before answering.

## Security

- **Consent and least privilege (`SF-01`, `SF-04`, `SF-06`, `SF-08`):** `capture`, `clipboard_read` and `clipboard_write` each require per-call window approval (`keep` is refused). `clipboard_write` is off by default (`settings.json` mode `0600`) and checked at `tools/list`, at call time and at `perform`. `capture` and `clipboard_read` never expose a file path to the model, and `clipboard_read` wraps text with an explicit data-not-instructions note. Temporary files in `state/pasted/` and `state/captures/` are `0600` inside `0700` folders outside the repository, opened with `O_NOFOLLOW | O_NONBLOCK`, verified with `fstat`, and deleted immediately after use.
- **Known limits:** (1) `D4`: window capture reads root-window pixels over the window's rectangle (occluding windows appear; Composite is out of scope); (2) `D10(f)` / `SF-08`: a session with shell permissions skipped runs as the same OS user and can run `lico capture` or edit `settings.json` directly; (3) after dropping a multi-MiB chip with `Escape`, glibc keeps freed arena pages mapped in the GUI process (~5 MiB warm reuse, well under the 50 MiB limit; P07 reviews allocator trim across all features).

## Try it yourself (for G3 - M02-P03 and M02-P04)

G3 falls at the end of M02-P04 and covers **M02-P03** (sending between sessions and the Claude tool channel) and **M02-P04** (screen capture, clipboard and image paste) together. Run on an X11 display (`echo $DISPLAY`) using a private socket so your own host is untouched:

```bash
T=$(mktemp -d)
lico host --socket "$T/host.sock" --state-dir "$T/state" --sessions 0 --producing 0 &
export LICO_SOCKET="$T/host.sock"
lico gui --socket "$T/host.sock"
```

1. **Identity and CLI X11 commands (no sign-in needed):**
   - Check `Help > About Lico` and `lico --version`.
   - Run `lico capture --screen --out "$T/shot.png"` and verify the JSON line (`path`, `agent.path`, `server.vendor`, `server.release`).
   - Run `lico clipboard write --text "hello from lico"` (with a session open in the window) and `lico clipboard read --kind text`.
2. **Sign in and create two Claude Code sessions** (`Ctrl+A` to add/sign in if needed; `Ctrl+N` twice for `api-server` and `docs`).
3. **M02-P03 - send between sessions and hand over conversation:**
   - Type text in `api-server`, press `Ctrl+Shift+S`, choose `docs`, press `Enter` (short text arrives inline under dimmed `>> [from session 'api-server' ...]`; text over 4 KiB arrives as a file in `docs`'s private inbox).
   - Try `Session > Hand over conversation...` to deliver the full transcript record as an owner-only file.
4. **M02-P04 - image paste, screen-capture-to-input, and middle-button paste:**
   - In `api-server`, press `Ctrl+Shift+P` (or `View > Capture screen to input`): `[picture ... KiB, WxH] - Esc removes it` appears under the input box. Press `Escape` to drop it, or type `"Describe the top bar of this window"` and press `Enter` to send the image + prompt to Claude.
   - Copy any PNG to your X11 clipboard and press `Ctrl+V` in the input box to attach it; select text on screen and middle-click inside the input box to paste `PRIMARY`.
5. **M02-P03 & M02-P04 - agent tools and approvals:**
   - Ask `api-server` to list sessions (`sessions`), send a note to `docs` (`send`), capture a screen region (`capture`), and read your clipboard (`clipboard_read`). Approve (`Y` after the 800 ms dwell) or inspect (`V`) each card.
   - Open `View > Agent clipboard write: off` to switch it `on`, then ask `api-server` to put a command on your clipboard (`clipboard_write`), press `V` to inspect, and `Y` to allow.
6. **Clean up:** `lico --socket "$T/host.sock" host stop --force && rm -rf "$T"`.
