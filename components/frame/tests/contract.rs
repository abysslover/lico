//! The contract tests of `lico-frame` (SPEC.md). PNGs are made here by a small encoder of the test's own (stored blocks, every colour type, every filter), by `lico-png` (real
//! compression) and by one image compressed with zlib at level 9 (dynamic Huffman blocks; `tests/fixtures/rgba_48x32_zlib9.bin`, regenerated from its formula below).

use lico_frame::*;

const ZLIB9: &[u8] = include_bytes!("fixtures/rgba_48x32_zlib9.bin");

// ---- the test's own encoder ------------------------------------------------------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= *b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for x in data {
        a = (a + *x as u32) % 65521;
        b = (b + a) % 65521;
    }
    b << 16 | a
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(kind);
    out.extend(data);
    let mut crc_input = kind.to_vec();
    crc_input.extend(data);
    out.extend(crc32(&crc_input).to_be_bytes());
    out
}

fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    if blocks.is_empty() {
        blocks.push(&[]);
    }
    for (i, b) in blocks.iter().enumerate() {
        out.push(if i + 1 == blocks.len() { 1 } else { 0 });
        out.extend((b.len() as u16).to_le_bytes());
        out.extend((!(b.len() as u16)).to_le_bytes());
        out.extend(*b);
    }
    out.extend(adler32(raw).to_be_bytes());
    out
}

fn ihdr(w: u32, h: u32, depth: u8, ctype: u8, interlace: u8) -> Vec<u8> {
    let mut d = w.to_be_bytes().to_vec();
    d.extend(h.to_be_bytes());
    d.extend([depth, ctype, 0, 0, interlace]);
    chunk(b"IHDR", &d)
}

fn png_from(w: u32, h: u32, depth: u8, ctype: u8, before_idat: &[Vec<u8>], zlib: &[u8]) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    out.extend(ihdr(w, h, depth, ctype, 0));
    for c in before_idat {
        out.extend(c);
    }
    out.extend(chunk(b"IDAT", zlib));
    out.extend(chunk(b"IEND", &[]));
    out
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (p, pa, pb, pc) = (a as i32 + b as i32 - c as i32, (a as i32 + b as i32 - c as i32 - a as i32).abs(), (a as i32 + b as i32 - c as i32 - b as i32).abs(), (a as i32 + b as i32 - c as i32 - c as i32).abs());
    let _ = p;
    if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }
}

/// The scanlines of `rows` (raw bytes of each row) with the filter that `filter_of(y)` names.
fn scanlines(rows: &[Vec<u8>], bpp: usize, filter_of: &dyn Fn(usize) -> u8) -> Vec<u8> {
    let mut out = Vec::new();
    for (y, cur) in rows.iter().enumerate() {
        let prev = if y > 0 { rows[y - 1].clone() } else { vec![0; cur.len()] };
        let f = filter_of(y);
        out.push(f);
        for i in 0..cur.len() {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            let predicted = match f {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => paeth(a, b, c),
                _ => panic!(),
            };
            out.push(cur[i].wrapping_sub(predicted));
        }
    }
    out
}

fn limits() -> Limits {
    Limits::default()
}

// ---- constants -------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_constants_are_named_with_the_values_the_spec_gives() {
    assert_eq!((MAX_INPUT, MAX_PIXELS, MAX_CHUNKS), (4 * 1024 * 1024, 1920 * 1200, 64));
    let l = Limits::default();
    assert_eq!((l.max_input, l.max_pixels), (MAX_INPUT, MAX_PIXELS));
}

// ---- pixels ----------------------------------------------------------------------------------------------------------------------------------------

fn formula(x: usize, y: usize) -> [u8; 4] {
    [(x * 4) as u8, (y * 6) as u8, if (x / 8 + y / 8) % 2 == 0 { 200 } else { 40 }, if (x + y) % 7 != 0 { 255 } else { 128 }]
}

fn argb(p: [u8; 4]) -> u32 {
    (p[3] as u32) << 24 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32
}

#[test]
fn a_png_compressed_with_dynamic_huffman_blocks_decodes_to_the_pixels_of_its_formula() {
    let png = png_from(48, 32, 8, 6, &[], ZLIB9);
    let img = decode(&png, &limits()).unwrap();
    assert_eq!((img.width, img.height, img.pixels.len()), (48, 32, 48 * 32));
    for y in 0..32 {
        for x in 0..48 {
            assert_eq!(img.pixels[y * 48 + x], argb(formula(x, y)), "({x}, {y})");
        }
    }
}

#[test]
fn every_filter_on_every_row_gives_the_same_pixels() {
    let (w, h) = (13usize, 9usize);
    let rows: Vec<Vec<u8>> = (0..h).map(|y| (0..w).flat_map(|x| formula(x, y)).collect()).collect();
    let want: Vec<u32> = (0..h).flat_map(|y| (0..w).map(move |x| argb(formula(x, y)))).collect();
    for f in 0..5u8 {
        let png = png_from(w as u32, h as u32, 8, 6, &[], &zlib_stored(&scanlines(&rows, 4, &|_| f)));
        assert_eq!(decode(&png, &limits()).unwrap().pixels, want, "filter {f}");
    }
    let png = png_from(w as u32, h as u32, 8, 6, &[], &zlib_stored(&scanlines(&rows, 4, &|y| (y % 5) as u8)));
    assert_eq!(decode(&png, &limits()).unwrap().pixels, want, "the filters mixed, row by row");
}

#[test]
fn each_colour_type_at_eight_bits_becomes_argb() {
    let (w, h) = (3usize, 2usize);
    let make = |ctype: u8, bpp: usize, rows: Vec<Vec<u8>>, extra: &[Vec<u8>]| {
        let png = png_from(w as u32, h as u32, 8, ctype, extra, &zlib_stored(&scanlines(&rows, bpp, &|y| (y % 3) as u8)));
        decode(&png, &limits()).unwrap().pixels
    };
    assert_eq!(make(0, 1, vec![vec![0, 128, 255], vec![1, 2, 3]], &[]), vec![0xFF000000, 0xFF808080, 0xFFFFFFFF, 0xFF010101, 0xFF020202, 0xFF030303], "grey");
    assert_eq!(make(2, 3, vec![vec![1, 2, 3, 4, 5, 6, 7, 8, 9], vec![255, 0, 0, 0, 255, 0, 0, 0, 255]], &[]), vec![0xFF010203, 0xFF040506, 0xFF070809, 0xFFFF0000, 0xFF00FF00, 0xFF0000FF], "RGB: alpha is opaque");
    assert_eq!(make(4, 2, vec![vec![10, 255, 20, 128, 30, 0], vec![40, 1, 50, 2, 60, 3]], &[]), vec![0xFF0A0A0A, 0x80141414, 0x001E1E1E, 0x01282828, 0x02323232, 0x033C3C3C], "grey and alpha");
    assert_eq!(make(6, 4, vec![vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], vec![0; 12]], &[]), vec![0x04010203, 0x08050607, 0x0C090A0B, 0, 0, 0], "RGBA: the alpha is kept");
    let plte = chunk(b"PLTE", &[255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]);
    assert_eq!(make(3, 1, vec![vec![0, 1, 2], vec![3, 2, 1]], &[plte]), vec![0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFF090909, 0xFF0000FF, 0xFF00FF00], "palette");
}

#[test]
fn sixteen_bits_keep_the_high_byte() {
    let png = png_from(2, 1, 16, 2, &[], &zlib_stored(&scanlines(&[vec![0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xff, 0xff, 0x00, 0x00, 0x80, 0x00]], 6, &|_| 0)));
    assert_eq!(decode(&png, &limits()).unwrap().pixels, vec![0xFF12569A, 0xFFFF0080], "RGB16");
    let png = png_from(1, 1, 16, 6, &[], &zlib_stored(&scanlines(&[vec![0x11, 0x00, 0x22, 0x00, 0x33, 0x00, 0x44, 0xff]], 8, &|_| 0)));
    assert_eq!(decode(&png, &limits()).unwrap().pixels, vec![0x44112233], "RGBA16");
    let png = png_from(1, 1, 16, 0, &[], &zlib_stored(&scanlines(&[vec![0x7f, 0xff]], 2, &|_| 0)));
    assert_eq!(decode(&png, &limits()).unwrap().pixels, vec![0xFF7F7F7F], "grey16");
    let png = png_from(1, 1, 16, 4, &[], &zlib_stored(&scanlines(&[vec![0x20, 0x00, 0x90, 0x01]], 4, &|_| 0)));
    assert_eq!(decode(&png, &limits()).unwrap().pixels, vec![0x90202020], "grey alpha 16");
}

#[test]
fn what_the_program_s_own_png_writer_makes_is_read_back_whole() {
    let (w, h) = (120u32, 80u32);
    let mut out = Vec::new();
    let mut enc = lico_png::PngEncoder::new(&mut out, w, h).unwrap();
    let mut want = Vec::new();
    for y in 0..h {
        let row: Vec<u8> = (0..w).flat_map(|x| [(x * 2) as u8, (y * 3) as u8, if (x / 10 + y / 10) % 2 == 0 { 200 } else { 40 }]).collect();
        want.extend(row.chunks(3).map(|p| 0xFF000000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32));
        enc.push_row(&row).unwrap();
    }
    enc.finish().unwrap();
    let img = decode(&out, &limits()).unwrap();
    assert_eq!((img.width, img.height), (w, h));
    assert_eq!(img.pixels, want);
}

#[test]
fn ancillary_chunks_are_skipped_and_an_image_in_several_idat_chunks_is_joined() {
    let rows: Vec<Vec<u8>> = (0..4).map(|y| (0..4).flat_map(|x| formula(x, y)).collect()).collect();
    let z = zlib_stored(&scanlines(&rows, 4, &|y| y as u8));
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    out.extend(ihdr(4, 4, 8, 6, 0));
    out.extend(chunk(b"gAMA", &[0, 1, 0x86, 0xa0]));
    out.extend(chunk(b"tEXt", b"Comment\0hello"));
    out.extend(chunk(b"IDAT", &z[..10]));
    out.extend(chunk(b"IDAT", &z[10..30]));
    out.extend(chunk(b"IDAT", &z[30..]));
    out.extend(chunk(b"IEND", &[]));
    let img = decode(&out, &limits()).unwrap();
    assert_eq!(img.pixels[5], argb(formula(1, 1)));
    let h = header(&out).unwrap();
    assert_eq!((h.width, h.height, h.colour, h.depth), (4, 4, 6, 8));
}

#[test]
fn a_stream_of_many_chunks_is_stopped_at_the_cap() {
    let rows: Vec<Vec<u8>> = (0..2).map(|y| (0..2).flat_map(|x| formula(x, y)).collect()).collect();
    let z = zlib_stored(&scanlines(&rows, 4, &|_| 0));
    let many = |n: usize| {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        out.extend(ihdr(2, 2, 8, 6, 0));
        for _ in 0..n {
            out.extend(chunk(b"teXt", b"x"));
        }
        out.extend(chunk(b"IDAT", &z));
        out.extend(chunk(b"IEND", &[]));
        out
    };
    assert!(decode(&many(MAX_CHUNKS - 4), &limits()).is_ok(), "just under the cap");
    assert!(matches!(err(&many(MAX_CHUNKS + 4)), Error::Chunk(_)), "over the cap");
}

#[test]
fn decoding_is_deterministic() {
    let png = png_from(48, 32, 8, 6, &[], ZLIB9);
    assert_eq!(decode(&png, &limits()), decode(&png, &limits()));
}

// ---- refusals ---------------------------------------------------------------------------------------------------------------------------------------

fn err(png: &[u8]) -> Error {
    decode(png, &limits()).unwrap_err()
}

#[test]
fn what_is_not_a_png_or_is_cut_is_an_error() {
    assert_eq!(err(b""), Error::NotPng);
    assert_eq!(err(b"GIF89a not a png at all"), Error::NotPng);
    let good = png_from(2, 2, 8, 6, &[], &zlib_stored(&scanlines(&[vec![0; 8], vec![0; 8]], 4, &|_| 0)));
    for cut in [8, 12, 20, 33, good.len() - 13, good.len() - 1] {
        assert!(matches!(err(&good[..cut]), Error::Truncated | Error::Chunk(_)), "cut at {cut}: {:?}", err(&good[..cut]));
    }
    let mut after = good.clone();
    after.extend(b"junk");
    assert!(matches!(err(&after), Error::Chunk(_)), "bytes after IEND");
    assert!(matches!(header(&good[..10]), Err(Error::Truncated) | Err(Error::NotPng)));
}

#[test]
fn a_chunk_with_a_wrong_crc_is_refused() {
    let mut p = png_from(2, 2, 8, 6, &[], &zlib_stored(&scanlines(&[vec![0; 8], vec![0; 8]], 4, &|_| 0)));
    let idat = p.windows(4).position(|w| w == b"IDAT").unwrap();
    p[idat + 6] ^= 1;
    assert_eq!(err(&p), Error::Crc);
    let mut p = png_from(2, 2, 8, 6, &[], &zlib_stored(&scanlines(&[vec![0; 8], vec![0; 8]], 4, &|_| 0)));
    p[20] ^= 1; // inside the IHDR
    assert_eq!(err(&p), Error::Crc);
}

#[test]
fn headers_this_decoder_does_not_read_are_refused_with_the_reason() {
    let z = zlib_stored(&[0, 0, 0, 0, 0]);
    let with = |ihdr: Vec<u8>, idat: &[u8]| {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        out.extend(ihdr);
        out.extend(chunk(b"IDAT", idat));
        out.extend(chunk(b"IEND", &[]));
        out
    };
    assert!(matches!(err(&with(ihdr(1, 1, 8, 6, 1), &z)), Error::Unsupported(w) if w.contains("interlace")), "Adam7");
    for (depth, ctype) in [(1u8, 0u8), (2, 0), (4, 0), (4, 3), (1, 3), (2, 3)] {
        assert!(matches!(err(&with(ihdr(1, 1, depth, ctype, 0), &z)), Error::Unsupported(_)), "depth {depth} type {ctype}");
    }
    for (depth, ctype) in [(8u8, 1u8), (8, 5), (8, 7), (16, 3), (3, 0), (8, 9), (0, 6)] {
        assert!(matches!(err(&with(ihdr(1, 1, depth, ctype, 0), &z)), Error::Chunk(_) | Error::Unsupported(_)), "depth {depth} type {ctype}");
    }
    assert_eq!(err(&with(ihdr(0, 5, 8, 6, 0), &z)), Error::Dimensions);
    assert_eq!(err(&with(ihdr(5, 0, 8, 6, 0), &z)), Error::Dimensions);
    let mut bad_method = ihdr(1, 1, 8, 6, 0);
    bad_method[8 + 10] = 1; // compression method
    let crc = crc32(&bad_method[4..bad_method.len() - 4]);
    let n = bad_method.len();
    bad_method[n - 4..].copy_from_slice(&crc.to_be_bytes());
    assert!(matches!(err(&with(bad_method, &z)), Error::Chunk(_)));
}

#[test]
fn a_palette_image_needs_its_palette_and_an_unknown_critical_chunk_is_refused() {
    let z = zlib_stored(&[0, 0]);
    assert!(matches!(err(&png_from(1, 1, 8, 3, &[], &z)), Error::Chunk(_)), "no PLTE");
    assert!(matches!(err(&png_from(1, 1, 8, 3, &[chunk(b"PLTE", &[1, 2])], &z)), Error::Chunk(_)), "a palette that is not whole entries");
    let idx = zlib_stored(&[0, 5]);
    assert!(matches!(err(&png_from(1, 1, 8, 3, &[chunk(b"PLTE", &[1, 2, 3])], &idx)), Error::Chunk(_)), "an index past the palette");
    assert!(matches!(err(&png_from(1, 1, 8, 6, &[chunk(b"ABCD", &[1])], &zlib_stored(&[0, 0, 0, 0, 0]))), Error::Unsupported(_)), "a critical chunk it does not know");
    assert!(decode(&png_from(1, 1, 8, 6, &[chunk(b"abcd", &[1])], &zlib_stored(&[0, 0, 0, 0, 0])), &limits()).is_ok(), "an ancillary one is skipped");
}

#[test]
fn the_size_is_checked_from_the_header_before_anything_is_allocated() {
    let t = std::time::Instant::now();
    let huge = png_from(100_000, 100_000, 8, 6, &[], &zlib_stored(&[0; 16]));
    assert_eq!(err(&huge), Error::TooBig);
    let just_over = png_from(1921, 1200, 8, 6, &[], &zlib_stored(&[0; 16]));
    assert_eq!(err(&just_over), Error::TooBig, "one column over the pixel cap");
    assert!(matches!(err(&png_from(1920, 1200, 8, 6, &[], &zlib_stored(&[0; 16]))), Error::Inflate(_)), "exactly the cap is allowed (and then its data is short)");
    assert!(t.elapsed() < std::time::Duration::from_secs(2));
    let cap = Limits { max_pixels: 100, ..Limits::default() };
    let at = |w: u32| png_from(w, 1, 8, 6, &[], &zlib_stored(&scanlines(&[vec![0; w as usize * 4]], 4, &|_| 0)));
    assert!(decode(&at(100), &cap).is_ok(), "exactly the cap");
    assert_eq!(decode(&at(101), &cap), Err(Error::TooBig), "one pixel over");
    let small = Limits { max_input: 100, ..Limits::default() };
    assert_eq!(decode(&png_from(48, 32, 8, 6, &[], ZLIB9), &small), Err(Error::TooBig), "the input over its cap");
    let tight = Limits { max_pixels: 100, ..Limits::default() };
    assert_eq!(decode(&png_from(48, 32, 8, 6, &[], ZLIB9), &tight), Err(Error::TooBig));
    assert_eq!(header(&huge).unwrap().width, 100_000, "the header alone can be read");
}

#[test]
fn a_stream_that_says_a_different_size_than_the_header_is_refused() {
    let rows = vec![vec![0u8; 8]; 2];
    let good = scanlines(&rows, 4, &|_| 0);
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &zlib_stored(&good[..good.len() - 1]))), Error::Inflate(_)), "one byte short");
    let mut more = good.clone();
    more.push(0);
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &zlib_stored(&more))), Error::Inflate(_)), "one byte over");
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &zlib_stored(&[good[0]; 0]))), Error::Inflate(_)), "nothing");
    let mut bad_filter = good.clone();
    bad_filter[0] = 5;
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &zlib_stored(&bad_filter))), Error::Chunk(_)), "filter type 5");
}

#[test]
fn a_broken_zlib_stream_is_an_error_of_its_kind() {
    let good = zlib_stored(&scanlines(&[vec![1; 8], vec![2; 8]], 4, &|_| 0));
    let mut p = good.clone();
    p[0] = 0x79;
    p[1] = 0x18; // method 9 with check bits that are right: only the method is wrong
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Inflate(_)));
    let mut p = good.clone();
    p[1] = 0x02; // the method is right and the check bits are wrong
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Inflate(_)));
    let mut p = good.clone();
    p[1] = 0x20; // check bits right, a preset dictionary announced: not supported
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Inflate(_)));
    let mut p = good.clone();
    let n = p.len();
    p[n - 1] ^= 0xff;
    assert_eq!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Adler);
    let mut p = good.clone();
    p[5] ^= 0xff; // NLEN is no longer the complement of LEN (the data itself is whole)
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Inflate(_)));
    let mut p = good.clone();
    p.extend([0, 0]);
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Inflate(_)), "bytes after the checksum");
    let mut p = good.clone();
    p[2] = 0x07; // block type 3 (reserved)
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &p)), Error::Inflate(_)));
    // a reserved block, then the rest of the stream laid out so that a decoder that skipped the reserved block would read the whole data
    let body = scanlines(&[vec![1; 8], vec![2; 8]], 4, &|_| 0);
    let mut sneaky = vec![0x78, 0x01, 0x06]; // not final, type 3, then zero bits: "not final, stored" if the reserved block were skipped
    sneaky.extend((body.len() as u16).to_le_bytes());
    sneaky.extend((!(body.len() as u16)).to_le_bytes());
    sneaky.extend(&body);
    sneaky.extend([0x01, 0, 0, 0xff, 0xff]);
    sneaky.extend(adler32(&body).to_be_bytes());
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &sneaky)), Error::Inflate(_)));
    assert!(matches!(err(&png_from(2, 2, 8, 6, &[], &good[..2])), Error::Inflate(_)), "the stream ends at once");
}

// ---- it never panics ------------------------------------------------------------------------------------------------------------------------------

/// The chunks of `png` with the data of the IDAT chunks replaced by `f(data)` and every CRC made again, so that the damage reaches the inflate and the filters.
fn rewrap(png: &[u8], f: &dyn Fn(&mut Vec<u8>)) -> Vec<u8> {
    let mut out = png[..8].to_vec();
    let mut i = 8;
    while i + 8 <= png.len() {
        let n = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
        let kind: [u8; 4] = [png[i + 4], png[i + 5], png[i + 6], png[i + 7]];
        let mut data = png[i + 8..i + 8 + n].to_vec();
        if &kind == b"IDAT" {
            f(&mut data);
        }
        out.extend(chunk(&kind, &data));
        i += 12 + n;
    }
    out
}

#[test]
fn every_cut_and_every_flipped_bit_is_an_error_or_an_image_and_never_a_panic() {
    let good = png_from(48, 32, 8, 6, &[], ZLIB9);
    for cut in 0..good.len() {
        let _ = decode(&good[..cut], &limits());
    }
    let mut errs = 0;
    for i in 0..good.len() {
        for bit in [0u8, 3, 7] {
            let mut p = good.clone();
            p[i] ^= 1 << bit;
            errs += decode(&p, &limits()).is_err() as usize;
        }
    }
    assert!(errs > good.len(), "the CRCs catch nearly all of them: {errs}");
    // with the CRCs made again the damage reaches the decoder itself
    let mut reached = 0;
    for i in 0..ZLIB9.len() {
        let p = rewrap(&good, &|d| d[i] ^= 0x5a);
        reached += decode(&p, &limits()).is_err() as usize;
    }
    assert!(reached > ZLIB9.len() / 2, "{reached} of {}", ZLIB9.len());
    for cut in 0..ZLIB9.len() {
        let p = rewrap(&good, &|d| d.truncate(cut));
        assert!(decode(&p, &limits()).is_err(), "a stream cut at {cut}");
    }
}

#[test]
fn random_bytes_are_never_a_png() {
    let mut seed = 0x1234_5678_9abc_def0u64;
    for n in [0usize, 1, 7, 8, 9, 40, 200, 3000] {
        let junk: Vec<u8> = (0..n)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        assert!(decode(&junk, &limits()).is_err());
        let mut with_sig = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        with_sig.extend(&junk);
        assert!(decode(&with_sig, &limits()).is_err());
    }
}

#[test]
fn a_second_or_short_header_or_no_data_are_refused() {
    let z = zlib_stored(&scanlines(&[vec![0; 4]], 4, &|_| 0));
    let mut two = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    two.extend(ihdr(1, 1, 8, 6, 0));
    two.extend(ihdr(1, 1, 8, 6, 0));
    two.extend(chunk(b"IDAT", &z));
    two.extend(chunk(b"IEND", &[]));
    assert!(matches!(err(&two), Error::Chunk(_)), "a second header");
    let mut short = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    short.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0]));
    short.extend(chunk(b"IDAT", &z));
    short.extend(chunk(b"IEND", &[]));
    assert!(matches!(err(&short), Error::Chunk(_)), "a header of 12 bytes");
    let mut none = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    none.extend(ihdr(1, 1, 8, 6, 0));
    none.extend(chunk(b"IEND", &[]));
    assert_eq!(err(&none), Error::Chunk("no image data"));
}
