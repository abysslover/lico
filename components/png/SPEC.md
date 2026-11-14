# lico-png 1.0.0 — specification

Approved by the owner on 2026-11-14 (M15-P03, the interface specification shown before anything moved). This file is the contract; `tests/contract.rs` states each line below as a test and uses only the names listed under API and Test support. Nothing in the crate outside these names is contract.

## Purpose

Writes an 8-bit RGB, non-interlaced PNG row by row to any `Write`, with memory that depends on the width and not on the height. It owns nothing else: no reading of PNGs, no files, no screen.

## API

| Name | Meaning |
|---|---|
| `struct PngEncoder<W: Write>` | the writer |
| `PngEncoder::new(sink: W, width: u32, height: u32) -> io::Result<PngEncoder<W>>` | writes the signature and the header |
| `push_row(&mut self, row: &[u8]) -> io::Result<()>` | the next row: `3 * width` bytes, R G B of each pixel from the left |
| `finish(&mut self) -> io::Result<W>` | writes the rest and the end chunk, gives the sink back |
| `buffer_bytes(&self) -> usize` | the capacity of the encoder's own buffers; the witness of invariant I4 |
| `const MAX_SIDE: u32 = 0x7FFF_FFFF` | the largest width or height |
| `const IDAT_MAX: usize = 32768` | the most compressed bytes in one IDAT chunk |

`Crc32`, `Adler32` and their functions are private (decision Q1). The public names of `src/lib.rs` are exactly these (and `mod testing`); `the_public_names_are_exactly_these` reads the source to pin it.

## Test support (the feature `testing`)

Off by default; a consumer turns it on in its `[dev-dependencies]` only, so the production binary holds none of it. `lico_png::testing` holds: `read_png`, `Decoded` (the test decoder), `both_readers`, `outside` (Python's zlib checker, `identify`, `convert`), `inflate`, `unzlib`, `slow_crc`, `slow_adler`, `paeth_spec`, `run`, `imagemagick_size`, `encode`, the inputs `noise`, `gradient`, `text_like`, `colour`, `Rng`, and a `Scratch` folder. These are for tests and are not contract: they may change in a minor version.

## Invariants

- **I1** the output is a valid PNG: signature, IHDR (8 bit, colour type 2, deflate, filter method 0, no interlace), IDAT chunks, IEND; every CRC right; the zlib stream ends where the data ends; the data length is `h * (1 + 3w)`.
- **I2** lossless: every pixel comes back exactly (edge sizes, noise, text, gradient).
- **I3** every IDAT chunk is non-empty and at most `IDAT_MAX` bytes; a tall image has many.
- **I4** streaming and memory: bytes leave as rows arrive; the buffers do not grow with the height.
- **I5** no global state: two encoders side by side do not touch each other.
- **I6** compression is not worse than ImageMagick's by more than the ratios the test pins (text-like 30, gradient 6, noise 1.001).
- **I7** deterministic within one version: the same width, height and rows give the same bytes, for any sink.

## Errors

- **E1** wrong use is `Err` of kind `InvalidInput` and never a panic: width or height 0 or over `MAX_SIDE`; a row that is not `3 * width` bytes; more rows than the height; `finish` with fewer rows; any call after `finish`.
- **E2** a write error of the sink comes back as it is; every later call is an `Err`, so no file with a hole is ever completed.

## Dependencies and direction

None (std only). Used by `lico` (`capture` in production; tests of `capture` and `clipboard`). Direction: `lico` depends on `lico-png`, never the other way. External programs for the tests of I1, I2 and I6 only: `python3` (run with `-I`), `identify` and `convert` (ImageMagick); a missing one fails the test, it never skips.

## Versioning rule

Semantic. Major: the contract breaks (a name removed or changed, an invariant weakened, another colour type or an interlaced file). Minor: the contract grows (a new public name, for example RGBA). Patch: nothing in the contract changes. The bytes of the output are not contract across versions (a better deflate is a patch); within one version they are (I7). The consumer names the version it depends on in its manifest.

## Contract tests (`tests/contract.rs`)

| Test | Lines |
|---|---|
| `edge_images_come_back_exactly_from_both_readers` | API, I1, I2 |
| `the_screen_sized_cases_with_noise_and_text` | I2 |
| `a_tall_image_has_many_idat_chunks_of_bounded_size` | I3, `IDAT_MAX` |
| `the_file_is_written_as_the_rows_come_and_the_buffers_do_not_grow_with_the_height` | I4, `buffer_bytes` |
| `two_encoders_side_by_side_do_not_touch_each_other` | I5 |
| `the_size_against_imagemagick_on_a_screen_of_text_a_gradient_and_noise` | I6 |
| `the_same_input_gives_the_same_bytes` | I7 |
| `the_header_of_a_one_pixel_file_is_exactly_the_bytes_of_the_specification` | I1 |
| `wrong_use_is_an_error_and_never_a_panic_or_a_bad_file` | E1 |
| `a_write_error_comes_back_as_it_is_and_the_encoder_then_refuses` | E2 |
| `the_public_names_are_exactly_these` | API |
| `the_limits_have_the_values_of_the_specification` | API (`MAX_SIDE`, `IDAT_MAX`) |

The 13 tests of `src/tests.rs` (the deflate, the filters, the check sums, and the two that show the test decoder able to fail) are white-box proofs of the inside; they are not contract.

## Pattern and budget

A plain extraction (there was no upward edge to invert, so no port), plus the feature `testing` as the adapter through which a consumer's tests reach the readers and inputs. Budget: the default, 256 MiB and 30 s (see `RECEIPT.json` for the measured cost).
