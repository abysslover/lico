# M17-P04 Plan — `lico-frame`, `lico-browser-view` and the lower frame cap of `lico-ws`

- Status: approved by the lead under the owner's delegation (the owner away), 2026-11-17, in mail 147 (the re-plan of P04 to P09 from the P03 figures; P04 has no stop: this note is written, the approval recorded, and the phase goes on); in progress; every approval of mail 147 is on the list "for the owner to confirm on return"
- Version: 1.0.0
- Branch: `phase/M17-P04-frame-and-browser-view` (stacked on the P03 tip `6a7390c`)
- Milestone: [M17](../../../milestones/202611/20261117/20261117_02_M17_browser_tab_with_obscura_milestone.md); measurements: [P03 note](../../../reports/202611/20261117/20261117_15_M17-P03_first_measurement_note.md)

## Objective

Pure components, tests first, mutation-checked, no change to the program (it does not depend on them yet): (1) the default message cap of `lico-ws` lowered to 4 MiB from the P03 figure, in its own first commit, with its dependents re-accepted; (2) `lico-frame`, a PNG decoder; (3) `lico-browser-view`, the display-free model of one browser tab: address editing, history, viewport and input mapping, the keeper of the newest frame with its drop count, and the loading, absent and error states as data. The engine's behaviour that these rest on is MEASURED in P03: frames are **PNG, RGBA, 8 bits, not interlaced, one `IDAT`, filters Sub, Up and Paeth** (read from the saved frames); a still page sends one frame; each frame needs an acknowledgement; the largest frame seen was 69,612 bytes (base64).

## Decisions of this phase (each one is the lead's decision of mail 147 or mine, marked)

1. **The cap (lead's item 2):** `lico-ws` changes its DEFAULT, so that every user of the crate gets the lower cap: `DEFAULT_MAX_MESSAGE` becomes 4 MiB (`0.2.0`: a changed default is a changed behaviour). Reason in the SPEC: the largest frame seen is 69,612 bytes; a 1920x1080 frame is 8.3 MB raw (COMPUTED) and a compressed one is a fraction of it (ASSUMED: under 2 MB for a busy page); 4 MiB leaves room and keeps a hostile page from making the host buffer more. `lico-cdp-fake` follows (its `MAX_MESSAGE` is the same constant) and `lico-cdp` is re-accepted (SPEC text and receipt).
2. **The inflate is written new in `lico-frame`, not promoted** from the test support of `lico-png`: that code lives behind a test-only feature, has no output bound and is not written for a stream that a program outside the owner's control wrote; a product component must not depend on a feature named `testing`. About 220 lines, bounded by the size the header announces, with the Adler-32 and the chunk CRCs checked.
3. **Decoding scope (mine, from the MEASURED frames):** colour types 0, 2, 3, 4 and 6 at 8 bits, and 16 bits for 0, 2, 4 and 6; no interlacing, no sub-byte depths: those are `Unsupported`, said so in the SPEC. The engine sends RGBA8.
4. **Acknowledge at once, keep the newest, decode only what is shown (lead's item 3):** the tab model answers every frame with an acknowledgement effect at once (also a frame it drops, also when hidden), keeps only the NEWEST compressed frame (at most the cap) and counts what it replaced undecoded; the decoded image exists only for a shown tab and only after `paint()` asked for it; `hide()` frees the decoded image and keeps the compressed frame; `show()` starts the screencast again and, if no frame comes within 500 ms (`FALLBACK_AFTER_MS`, from the injected clock), the model asks for one screenshot.
5. **Memory on Lico's side (lead's item 4):** at most one decoded frame per shown tab (4 bytes a pixel, at most `MAX_VIEWPORT` = 1920x1200 = 9.2 MB, COMPUTED) plus the newest compressed frame per tab (at most 4 MiB); the decode allocates its scratch and frees it (a large block is returned to the system when freed). The gate "the window with one shown tab stays under 50 MiB RSS" is measured in the phases that build the window; this phase reports the computed bounds and the decode time on the real frames saved in P03.
6. **Viewport:** the page viewport is the shown area capped at `MAX_VIEWPORT`; when capped, the model says so as data (`viewport_capped`) and the painter shows it in the tab (lead's item 4).
7. **Address rules (lead's item 8):** only `http` and `https`; a text without a scheme is taken as `https://` plus the text; addresses with a user name or password are refused (a common way to disguise a destination); no spaces or control characters; at most `MAX_URL` = 2048 characters; what the engine reports as the current address is cleaned like any text from outside before it is shown.

## Specification drafts (the final text is each component's `SPEC.md`)

**`lico-frame 0.1.0`** (std only). `decode(png: &[u8], &Limits) -> Result<Image, Error>`, `header(png) -> Result<Header, Error>`, `Image { width, height, pixels: Vec<u32> }` (`0xAARRGGBB`, a pixel of an opaque type has alpha `0xFF`), `Header { width, height, colour, depth }`, `Limits { max_input, max_pixels }` (default `MAX_INPUT` = 4 MiB, `MAX_PIXELS` = 1920x1200), `Error { NotPng, Truncated, Chunk(&'static str), Crc, Unsupported(&'static str), TooBig, Inflate(&'static str), Adler, Dimensions }`. Constants with reasons: `MAX_INPUT` (the frame cap), `MAX_PIXELS` (the viewport cap, bounds the decoded size), `MAX_CHUNKS` (a PNG of a frame has three chunks; 64 is room for ancillary ones). Invariants: the output size is computed from the header and checked against `max_pixels` BEFORE anything is allocated; the inflated size must equal exactly what the header says (more or less is `Inflate`); every chunk CRC and the zlib Adler-32 are checked; no panic on any input (tested with truncations, flipped bytes and random data); no `unsafe`; deterministic.

**`lico-browser-view 0.1.0`** (depends on `lico-frame`). Types: `Tab`, `AddressField`, `Phase { Starting, Ready, Loading, Absent { expected_sha256, release }, NoFont, Failed(String) }`, `Effect { SetViewport, StartScreencast, StopScreencast, Ack(frame_id), CaptureScreenshot, Navigate(url), Back, Forward, Reload, Mouse, Key, InsertText }`, `EngineEvent { Started, Navigated(url), LoadStarted, LoadFinished, Title(text), Frame { id, png }, TargetGone, Crashed, Missing { expected_sha256, release }, NoFontFound }`. The model never touches a socket, a window or a clock: the adapter turns effects into `lico-cdp` calls and engine events into `EngineEvent`s, and gives the current time to `tick`. Constants with reasons: `MAX_HISTORY` = 100, `MAX_URL` = 2048, `MAX_TITLE` = 200 characters, `MAX_FRAME_BYTES` = 4 MiB, `MAX_VIEWPORT` = 1920x1200, `MIN_VIEWPORT` = 64x64, `FALLBACK_AFTER_MS` = 500. Invariants as in decisions 4 to 7; nothing a page says is trusted (title and address are cleaned: control and bidirectional-override characters removed, cut at the cap).

## Tests first, and what cannot be red first

Each component starts with its contract tests (compile-red, then red by assertion where a stub would be needed) and white-box tests; `lico-frame` is tested with PNGs made by an encoder in the test (stored blocks, every colour type, every filter), by `lico-png` (real LZ77 and Huffman, a dev-dependency), and by one compressed image made with `zlib` at level 9 (dynamic Huffman blocks; embedded as bytes, the image regenerated from its formula in the test); fuzz-like loops (every truncation, every single-byte flip of a small PNG must be an `Err` or a decode, never a panic). A real-frame decode test is IGNORED in a normal run (it reads the frames saved in P03 under `private/scratch/` through an environment variable) and prints the decode time. What cannot be red first: nothing of the model needs a system call; the address parser and the history are tested as written. Mutation checks as in M16 and M17-P02 for both components, figures reported.

## Steps

- [ ] 1. This note, the approval recorded (the milestone note), `cargo xtask check` as it is.
- [ ] 2. `lico-ws 0.2.0`: the tests that name the cap change first (red), the constant, the SPEC, the receipt; the fake and `lico-cdp` follow in the same commit (their tests, SPEC text, receipts).
- [ ] 3. `lico-frame`: tests red, code, SPEC, receipt, mutation checks; the ignored real-frame test run once on the saved frames.
- [ ] 4. `lico-browser-view`: tests red, code, SPEC, receipt, mutation checks.
- [ ] 5. `cargo xtask check` and `verify`, the tools tests, the selection test (it names every component), the module graph (unchanged), link scan.
- [ ] 6. The result in this note, the `done` file (132), and STOP only because P05 starts on the lead's pointer.

## Estimate and the stop rule

About 2,300 lines (`lico-frame` 450 + 450 tests, `lico-browser-view` 650 + 700, the cap change 50): **4 to 5 USD** (the lead's range; pure components at 1.4 USD per 1,000 lines with margin). If the phase has used about 7.5 USD and is not near done, I stop at a green commit and write a `question` file.

## Acceptance Criteria

Two new components and a re-accepted `lico-ws`, each with a version, `SPEC.md`, contract tests, a receipt and mutation figures; `cargo xtask check` and `verify` pass; every commit builds with `--locked` and has no red test; the program, its binary and its tests unchanged.
