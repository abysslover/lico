# lico-frame 0.2.0 — specification

0.1.0 (M17-P04): the PNG decoder; approved by the lead under the owner's delegation (the owner away), mail 147. 0.2.0 (M20-P02): the JPEG decoder, written because the engine of the tab changed to Moli, whose PNG frames are 20 to 30 times larger than the first engine's (3.8 MB for a 1920 x 1200 page, against the 4 MiB cap of one frame) and whose JPEG frames are 130 KB. Marks: **KNOWN FROM DOCUMENTATION** (PNG, RFC 2083 / ISO 15948; zlib and deflate, RFC 1950 and 1951; JPEG, ITU T.81 / ISO 10918-1 and the JFIF text), **MEASURED** (the frames the engine sends: PNG in M17-P03 and M20; JPEG in M20-P02 with Moli 1.1.25, below).

## Purpose

Decode the PNG and JPEG frames of a browser tab into pixels, safely: the stream is written by a program outside our control, so every size is bounded from the header before anything is allocated and nothing indexes past what was checked. Std only, no `unsafe`. It writes nothing and keeps no state.

## API (`tests/contract.rs` and `tests/jpeg.rs` pin the names)

| Name | Meaning |
|---|---|
| `decode(&[u8], &Limits) -> Result<Image, Error>` | the whole decode; PNG or JPEG, told by the first bytes |
| `header(&[u8]) -> Result<Header, Error>` | the header alone (a PNG's CRC checked; a JPEG's frame header): `Header { width, height, colour, depth, format }` |
| `format(&[u8]) -> Option<Format>` | `Png` for the PNG signature, `Jpeg` for `FF D8 FF`, `None` for anything else |
| `Image { width, height, pixels: Vec<u32> }` | `0xAARRGGBB`, row after row; a kind without alpha gives alpha `0xFF` |
| `Limits { max_input, max_pixels }`, `Default` | the caps below |
| `Error { NotAnImage, Truncated, Chunk, Crc, Unsupported, TooBig, Inflate, Adler, Dimensions, Marker, Huffman }` | every way an image can be refused (`Chunk`, `Crc`, `Inflate` and `Adler` say a PNG is wrong, `Marker` and `Huffman` that a JPEG is) |

### PNG

Read: colour types 0 (grey), 2 (RGB), 3 (palette), 4 (grey and alpha) and 6 (RGBA) at 8 bits, and at 16 bits for 0, 2, 4 and 6 (the high byte of each sample is kept); filters 0 to 4; several `IDAT` chunks (joined); ancillary chunks (skipped; a transparency chunk is ignored, so a palette image is opaque). **Not read, and said so:** interlaced images (`Unsupported`), bit depths below 8 (`Unsupported`), an unknown critical chunk (`Unsupported`), a second header, a palette image without its palette, an index past the palette, bytes after `IEND` (`Chunk`).

### JPEG

Read: baseline (`SOF0`) and extended-sequential (`SOF1`) Huffman JPEG at 8 bits; one component (grey) or three (YCbCr, as JFIF says); one scan holding all the components; any sampling factors from 1 to 4 that divide the largest one (4:4:4, 4:2:2, 4:4:0, 4:2:0, 4:1:1 and the like; at most ten blocks in a unit of the image); tables of 8-bit or 16-bit quantisers and up to four tables of Huffman codes of each class; restart intervals; fill bytes; application data and comments (skipped). The chroma is repeated, not interpolated (libjpeg's plain upsampling). The conversion to RGB and the inverse DCT are libjpeg's integer arithmetic: the accurate DCT (`jidctint`) and the JFIF constants 1.402, 0.344136286, 0.714136286 and 1.772 in 16 bits.

**Not read, and said so (`Unsupported`):** progressive (`SOF2`), arithmetic-coded, lossless and hierarchical JPEG; a precision other than 8 bits; a number of components other than one or three (CMYK); a scan that holds only some of the components (a JPEG of several scans); sampling factors that do not divide each other; a height given later (`DNL`). **Refused (`Marker`, `Huffman`, `Truncated`):** a damaged container, a table that contradicts itself, a code that no symbol has, a DC size over 11 or an AC size over 10 (the limits of baseline), a coefficient past the end of the block, a wrong or missing restart marker, data that end before the blocks do, no end of image, bytes after it. An Adobe marker is not read: three components are YCbCr.

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MAX_INPUT` | 4 MiB | the cap of one frame of the tab (the same as `lico-ws` and `lico-browser-view`); the largest PNG frame of Moli was 3.77 MB (1920 x 1200), the largest JPEG 132 KB (the same page, quality 90) |
| `MAX_PIXELS` | 1920 x 1200 | the largest viewport of a tab; it bounds the decode to 9.2 MB of pixels (and a PNG's scratch the same again, freed before the function returns; a JPEG's scratch is the planes of one row of units of the image: at most a few hundred kilobytes) |
| `MAX_CHUNKS` | 64 | (PNG) a frame has three chunks (header, data, end); the room is for ancillary ones and stops a stream of empty chunks |

## Invariants

1. **The size is checked from the header first:** an image over `max_pixels` or an input over `max_input` is `TooBig` before anything is allocated; (PNG) the inflated size must be exactly `height * (1 + bytes per row)`, and more or less data is `Inflate` (the output buffer never reserves more than 1032 bytes for each byte the stream gives, the most deflate can do).
2. (PNG) Every chunk CRC and the zlib Adler-32 are checked; a zlib header that is not the deflate method with no dictionary, a stored block whose length does not match its complement, a code that is over-subscribed or (for a dynamic code of more than one symbol) incomplete, a distance that reaches before the start, a length or distance code that does not exist, a block of the reserved type, bytes after the checksum: `Inflate`.
3. (JPEG) **The data are read once, row of units by row of units:** the scratch is the planes of one row of units (a few hundred kilobytes at most), the output is written as each row is done. The bits taken past the end of the data (or past a marker) are counted: if any block used them, the image is `Truncated`; a restart marker must be the next one in turn; the end of image must follow the last unit, and nothing after it.
4. **It never panics, whatever the bytes:** every prefix of a valid image, every single-bit flip, every byte of the headers set to every value, and random bytes are tested; the result is an `Err` or an `Image`. The arithmetic of the inverse DCT is in 64 bits and a dequantised coefficient is clamped to ±2^24, so a damaged stream gives a wrong picture, never an overflow.
5. Deterministic: the same bytes give the same result. The scratch of a decode is allocated inside `decode` and freed when it returns, so a caller that decodes only what it shows holds no scratch between frames.
6. No thread, no file, no environment variable, no `unsafe`.

## Measured (M20-P02)

- **The frames of Moli 1.1.25** (`Page.startScreencast` and `Page.captureScreenshot` with `format: "jpeg"`; `tests/fixtures/moli_320x200_*`): baseline `SOF0`, 8 bits, three components with the numbers 0, 1 and 2, one scan, two tables of quantisers, four tables of Huffman codes, no restart interval, a JFIF application marker; 4:2:0 (luma sampled 2 x 2) for a quality below 90 and 4:4:4 from quality 90 (the encoder is `jpeg-encoder` 0.6).
- **Against libjpeg 9** (`djpeg -dct int -nosmooth`): the pixels of all of the 17 files tried (the 12 of the contract test, and five real frames of 1024 x 640 to 1920 x 1200, 12 million pixels) are identical, every sample.
- **Size** of the same page at 1920 x 1200: PNG 3,773,168 bytes; JPEG quality 90 (4:4:4) 131,842, quality 75 75,142, quality 40 61,877. At 1024 x 640: PNG 1,253,163; JPEG quality 90 76,533, quality 60 37,999.
- **Time** (release build, best of five): a 1920 x 1200 frame 6.0 ms (JPEG, quality 90), 10.7 ms (quality 75, 4:2:0), 46.9 ms (the PNG of the same page); 1024 x 640: 2.3 ms against 15.1 ms.

## Tests

`tests/jpeg.rs`: the fixtures against libjpeg's pixels (`tests/fixtures/make_jpeg_fixtures.sh` makes them and prints the table of checksums: 4:4:4, 4:2:2, 4:4:0, 4:2:0, grey, optimised tables, restart intervals in rows and in blocks, one pixel, 8 x 8, odd sizes, and two real frames of the engine); minimal JPEGs whose every bit is worked out by hand (DC prediction, blocks side by side, cut to the size, sixteen-bit quantisers, a quantiser that multiplies, a run of sixteen zeros that lands where it should, a code that no symbol has, a table with more codes than its lengths allow, sizes over the limits, a coefficient past the block); the refusals by name; the limits exact; damaged copies of every kind. `src/tests_jpeg.rs`: the integer DCT against the textbook one (within one), its one-step shortcut against its general arithmetic, absurd coefficients, codes of every length from 1 to 16, the bit reader (escaped bytes, fill, markers, padding). The colour conversion at every value of Cb and of Cr: a JPEG built by the test (2048 x 48, DC only, so every block is its level exactly) against the checksum of libjpeg's own decode of the same bytes (`LICO_DUMP_RAMPS=path` makes the test write the file, to be given to `djpeg`). **Mutation check by hand** (a program that changes one condition or constant at a time and runs the tests; `private/scratch/202611/20261117/m20_p02_mutation`): 118 mutants of the three JPEG modules (65 of the parser and the scan, 23 of the Huffman tables and the bit reader, 30 of the inverse DCT); 46 + 21 + 30 were caught by the first set of tests, and after the second set every one is caught but one that is equivalent (a plane's rows made 8 samples wider than they need be changes nothing a caller can see).

## Versioning and budget

`0.2.0`: semantic from here (a new kind of input is a minor step while below 1.0). Budget: the default 256 MiB and 30 s. The test `tests/real_frames.rs` is ignored in a normal run (it reads frames saved outside the repository).
