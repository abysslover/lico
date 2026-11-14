# M15-P03 Plan — The First Component, `png`

- Status: implemented; report [M15-P03](../../../reports/202611/20261114/20261114_13_M15-P03_first_component_png_report.md); the interface specification below was approved by the owner on 2026-11-14 (G2 of this phase)
- Version: 1.2.0
- Branch: `phase/M15-P03-first-component-png` (stacked on the P02 tip `5a49682`)
- Milestone: [M15](../../../milestones/202611/20261114/20261114_04_M15_component_foundation_milestone.md)

## Decisions of the owner after Phase 0 (2026-11-14)

The first component is `png` (`dbus` is the second component, after M15). The interface specification of `png` was shown to the owner first (each line SEEN IN CODE or PROPOSED). System tests are a short named list in one place with their own measured peak and time (512 MiB and 60 s per test); a test that is not on the list gets the component default (256 MiB, 30 s); no test is moved onto the list to make it pass.

## What `png` is today (SEEN IN CODE)

`src/host/hostd/src/png/`: `mod.rs` 513 lines (the encoder), `readers.rs` 334 (test-only readers), `tests.rs` 514 (21 tests). A PNG writer of our own, std only: 8-bit RGB, no interlace, rows pushed one by one to any `Write`, a deflate of our own. One consumer in production: `capture/mod.rs` (`PngEncoder`); its tests and `clipboard/tests.rs` use it and the test helpers. Dependencies: none in production; in tests `handoff::Scratch` and, outside the crate, `python3`, `identify` and `convert`. Measured in Phase 0: 21 tests, 11.0 s alone-sum, highest tree peak 61 MiB.

## The interface specification (approved)

| Line | Content | Mark |
|---|---|---|
| Name | component `lico-png` in `components/png/`, library `lico_png` | PROPOSED |
| Purpose | Writes an 8-bit RGB, non-interlaced PNG row by row to any `Write` with memory that depends on the width and not on the height. It owns nothing else: no reading of PNGs, no files, no screen. | SEEN IN CODE (module doc) / wording PROPOSED |
| API | `PngEncoder<W: Write>`; `new(sink, width, height) -> io::Result<_>`; `push_row(&mut self, &[u8])` (3 × width bytes, R G B); `finish(&mut self) -> io::Result<W>`; `buffer_bytes(&self) -> usize` (kept public as the witness of the memory invariant: PROPOSED); `MAX_SIDE = 0x7FFF_FFFF`; `IDAT_MAX = 32768` | SEEN IN CODE |
| Not API | `Crc32`, `Adler32`, `crc32`, `adler32` become private (unused outside) | PROPOSED (Q1) |
| Test support | feature `testing` (off by default; only the consumer's `[dev-dependencies]` turn it on): `read_png`, `Decoded`, `both_readers`, `outside`, `noise`, `gradient`, `text_like`, a local `Scratch` replacing `handoff::Scratch`; not in the production binary | helpers SEEN IN CODE; feature PROPOSED (Q3) |
| I1 | valid PNG: signature, IHDR (8 bit, colour type 2, deflate, filter 0, no interlace), IDAT, IEND, every CRC right, zlib stream ends where the data ends, data length `h * (1 + 3w)` | SEEN IN CODE |
| I2 | lossless for edge sizes, noise, text, gradient | SEEN IN CODE |
| I3 | every IDAT chunk non-empty and at most `IDAT_MAX`; a tall image has many | SEEN IN CODE |
| I4 | streaming: bytes leave as rows arrive; buffers do not grow with the height | SEEN IN CODE |
| I5 | no global state: two encoders side by side do not touch each other | SEEN IN CODE |
| I6 | compression not worse than ImageMagick's by more than the pinned ratio | SEEN IN CODE |
| I7 | deterministic within one version (same input, same bytes) | PROPOSED (new test) |
| E1 | wrong use is `Err(InvalidInput)`, never a panic (size 0 or over `MAX_SIDE`; a row of the wrong length; too many rows; `finish` early; any call after `finish`) | SEEN IN CODE |
| E2 | a sink's write error comes back as it is; every later call is an `Err`, so no file with a hole is completed | SEEN IN CODE |
| Dependencies | none (std only); used by `lico` (`capture` in production, tests of `capture` and `clipboard`); layer 0 | SEEN IN CODE |
| Edge to cut | test-only `handoff::Scratch` in `readers.rs` and `tests.rs`, replaced by the local `Scratch` before the move | SEEN IN CODE / replacement PROPOSED |
| External requirements | `python3` (with `-I`), `identify`, `convert` (ImageMagick), for the tests of I1, I2, I6 only; declared in the manifest | SEEN IN CODE |
| Versioning rule | major = the contract breaks; minor = it grows; patch = no change to it. Output bytes are not contract across versions (a better deflate is a patch), within a version they are (I7). First version `1.0.0` | PROPOSED (Q4, Q5) |
| Contract tests | the 8 existing tests of the public surface move as they are; new: `the_same_input_gives_the_same_bytes` (I7), `the_header_of_a_one_pixel_file_is_exactly_the_bytes_of_the_specification` (I1), `the_public_names_are_exactly_these` (API). The two tests of the test decoder stay with the `testing` support | SEEN IN CODE / new PROPOSED |
| White-box tests | the 11 tests of the deflate, filters and check sums stay inside the crate as unit tests; not contract | PROPOSED |
| Pattern | a plain extraction (no upward edge to invert, so no port), plus the `testing` feature as the adapter for the consumer's tests | PROPOSED |
| Budget | the default 256 MiB and 30 s, to be confirmed by `cargo xtask test png` | PROPOSED |

## Steps (small green commits; a move and a behaviour change never share a commit)

- [x] 1. Branch from the P02 tip; this note completed (documentation only).
- [x] 2. Characterization tests I7, the header bytes, the public names: green on the unchanged code, shown to fail under mutations before they count.
- [x] 3. Cut the edge: `Scratch` local to `png`.
- [x] 4. The move to `components/png/` (a move only; the same tests pass in the new place); `lico` depends on `lico-png`; `Crc32`/`Adler32` private.
- [x] 5. Manifest, `SPEC.md`, `cargo xtask check`, `cargo xtask test png --record`, `RECEIPT.json`.
- [x] 6. Test selection by component: a change under `components/<name>/` selects `cargo xtask test <name>` (skipped when its receipt is valid) and the consumers' tests; a change in a consumer selects the consumer's tests only.
- [x] 7. Mutation check of the Python tests that exercise `png`; retire only what a Rust contract test replaces, say why for the rest.
- [x] 8. Measure and report: the component's own test cost; a typical `capture` change before and after.

## Acceptance Criteria

`cargo xtask check` and `verify` pass; `cargo xtask test png` inside its budget; `cargo xtask graph` shows `png` as a component with no edge into `lico`; `cargo test -p lico-xtask` and the program's tests (less those that moved, which run in the component) pass; the Python suite is unchanged except for named retirements; the binary behaves the same.
