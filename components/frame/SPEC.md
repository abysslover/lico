# lico-frame 0.1.0 — specification

Written with the P04 plan (M17-P04); approved by the lead under the owner's delegation (the owner away), mail 147. Marks: **KNOWN FROM DOCUMENTATION** (PNG, RFC 2083 / ISO 15948; zlib and deflate, RFC 1950 and 1951), **MEASURED** (the frames the engine sends in M17-P03: PNG, RGBA, 8 bits, not interlaced, one `IDAT`, filters Sub, Up and Paeth; this decoder gives the same pixels as an independent decoder on four of them: identical FNV checksums; 9.2 to 9.8 ms per 1200x800 frame in a release build, best of five).

## Purpose

Decode the PNG frames of a browser tab into pixels, safely: the stream is written by a program outside our control, so every size is bounded from the header before anything is allocated and nothing indexes past what was checked. Std only, no `unsafe`. It writes nothing and keeps no state.

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `decode(&[u8], &Limits) -> Result<Image, Error>` | the whole decode |
| `header(&[u8]) -> Result<Header, Error>` | the header alone (its CRC checked): `Header { width, height, colour, depth }` |
| `Image { width, height, pixels: Vec<u32> }` | `0xAARRGGBB`, row after row; a colour type without alpha gives alpha `0xFF` |
| `Limits { max_input, max_pixels }`, `Default` | the caps below |
| `Error { NotPng, Truncated, Chunk(&'static str), Crc, Unsupported(&'static str), TooBig, Inflate(&'static str), Adler, Dimensions }` | every way a PNG can be refused |

Read: colour types 0 (grey), 2 (RGB), 3 (palette), 4 (grey and alpha) and 6 (RGBA) at 8 bits, and at 16 bits for 0, 2, 4 and 6 (the high byte of each sample is kept); filters 0 to 4; several `IDAT` chunks (joined); ancillary chunks (skipped; a transparency chunk is ignored, so a palette image is opaque). **Not read, and said so:** interlaced images (`Unsupported`), bit depths below 8 (`Unsupported`), an unknown critical chunk (`Unsupported`), a second header, a palette image without its palette, an index past the palette, bytes after `IEND` (`Chunk`).

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MAX_INPUT` | 4 MiB | the cap of one frame of the tab (the same as `lico-ws` 0.2.0 and `lico-browser-view`); the largest frame the engine sent was 69,612 bytes |
| `MAX_PIXELS` | 1920 x 1200 | the largest viewport of a tab; it bounds the decode to 9.2 MB of pixels (and the same again for its scratch, which is freed before the function returns) |
| `MAX_CHUNKS` | 64 | a frame has three chunks (header, data, end); the room is for ancillary ones and stops a stream of empty chunks |

## Invariants

1. **The size is checked from the header first:** an image over `max_pixels` or an input over `max_input` is `TooBig` before anything is allocated; the inflated size must be exactly `height * (1 + bytes per row)`, and more or less data is `Inflate` (the output buffer never reserves more than 1032 bytes for each byte the stream gives, the most deflate can do).
2. Every chunk CRC and the zlib Adler-32 are checked; a zlib header that is not the deflate method with no dictionary, a stored block whose length does not match its complement, a code that is over-subscribed or (for a dynamic code of more than one symbol) incomplete, a distance that reaches before the start, a length or distance code that does not exist, a block of the reserved type, bytes after the checksum: `Inflate`.
3. **It never panics, whatever the bytes:** every prefix of a valid PNG, every single-bit flip, every byte of the compressed data flipped with the CRCs made again, and random bytes are tested; the result is an `Err` or an `Image`.
4. Deterministic: the same bytes give the same result. The scratch of a decode (the inflated rows) is allocated inside `decode` and freed when it returns, so a caller that decodes only what it shows holds no scratch between frames.
5. No thread, no file, no environment variable, no `unsafe`.

## Versioning and budget

`0.1.0` while it has met one encoder (the engine's); semantic after. Budget: the default 256 MiB and 30 s. The test `tests/real_frames.rs` is ignored in a normal run (it reads frames saved outside the repository).
