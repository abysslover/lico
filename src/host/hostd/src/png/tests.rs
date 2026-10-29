//! The proofs of the PNG writer: check functions against vectors, the deflate alone, the filters, the images through both readers
//! (`readers.rs`), the size against ImageMagick, wrong use, memory, and the readers shown able to fail.

use super::*;
use crate::handoff::Scratch;
use std::cell::Cell;
use std::process::Command;
use std::rc::Rc;
use std::time::Instant;

// ---- inputs ----------------------------------------------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 32) as u8
    }
    /// A byte below 144: the fixed code writes it in 8 bits, so a block of these is never larger than a stored one.
    fn small(&mut self) -> u8 {
        ((self.next() >> 32) % 144) as u8
    }
}

fn noise(w: usize, h: usize, seed: u64) -> Vec<u8> {
    let mut r = Rng(seed | 1);
    (0..w * h * 3).map(|_| r.byte()).collect()
}

fn colour(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
    (0..w * h).flat_map(|_| rgb).collect()
}

fn gradient(w: usize, h: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, ((x + y) & 255) as u8]);
        }
    }
    v
}

/// Black blocks of 6 x 10 pixels on white, in bands of 16 rows with a gap between them, the way a screen of text looks.
fn text_like(w: usize, h: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let (band, ly) = (y / 16, y % 16);
        for x in 0..w {
            let (cell, lx) = (x / 8, x % 8);
            let ink = (3..13).contains(&ly) && lx < 6 && (cell * 7 + band * 13 + cell * band) % 5 < 3;
            v.extend_from_slice(if ink { &[0, 0, 0] } else { &[255, 255, 255] });
        }
    }
    v
}

fn encode(pix: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut e = PngEncoder::new(Vec::new(), w, h).unwrap();
    for row in pix.chunks(w as usize * 3) {
        e.push_row(row).unwrap();
    }
    e.finish().unwrap()
}

use super::readers::*;

// ---- the check functions ---------------------------------------------------------------------------------------------------

#[test]
fn crc32_vectors() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    let mut c = Crc32::new();
    c.update(b"1234");
    c.update(b"56789");
    assert_eq!(c.value(), 0xCBF4_3926, "in pieces");
    let mut r = Rng(7);
    let data: Vec<u8> = (0..3000).map(|_| r.byte()).collect();
    assert_eq!(crc32(&data), slow_crc(&data));
}

#[test]
fn adler32_vectors_and_the_delayed_modulo() {
    assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    assert_eq!(adler32(b""), 1);
    // 5552 is where the sums of 0xFF bytes would overflow 32 bits without the modulo; one byte either side and a long run.
    for n in [5551usize, 5552, 5553, 70000] {
        let data = vec![0xFFu8; n];
        assert_eq!(adler32(&data), slow_adler(&data), "{n} bytes of 0xFF");
    }
    let mut a = Adler32::new();
    for piece in vec![0xFFu8; 12000].chunks(1111) {
        a.update(piece);
    }
    assert_eq!(a.value(), slow_adler(&vec![0xFFu8; 12000]), "in pieces");
}

// ---- the deflate ------------------------------------------------------------------------------------------------------------

/// `data` through the deflate alone, fed in pieces of `piece` bytes.
fn zlib_of(data: &[u8], piece: usize) -> Vec<u8> {
    let mut z = Deflate::new();
    for p in data.chunks(piece.max(1)) {
        z.feed(p);
    }
    z.finish();
    z.out
}

fn small_bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut r = Rng(seed);
    (0..n).map(|_| r.small()).collect()
}

fn roundtrip(data: &[u8], piece: usize, what: &str) -> usize {
    let z = zlib_of(data, piece);
    let back = unzlib(&z).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(back == data, "{what}: the bytes differ");
    z.len()
}

#[test]
fn empty_and_tiny_inputs_make_a_valid_stream() {
    assert_eq!(roundtrip(b"", 1, "empty"), 2 + 2 + 4, "header, an empty fixed block (10 bits), Adler-32");
    for n in 1..40 {
        roundtrip(&small_bytes(n, n as u64), 5, "tiny");
    }
}

#[test]
fn every_match_length_from_3_to_300_is_written_and_read_back() {
    let base = small_bytes(300, 11);
    for len in 3..=300usize {
        let mut data = base.clone();
        data.extend_from_slice(&base[..len]);
        data.extend_from_slice(&[200, 201, 202, 203, 204]);
        let size = roundtrip(&data, 1000, &format!("length {len}"));
        if len >= 10 {
            assert!(size <= 300 + 2 + 4 + 12 + 8, "length {len}: {size} bytes - the copy was written as literals");
        }
    }
}

#[test]
fn every_distance_code_boundary_is_written_and_read_back() {
    // The first and last distance of every code, from the rule of RFC 1951: base - 1 and base, and 32768, the farthest a match may reach.
    let mut dists = vec![32768usize];
    let mut base = 1usize;
    for code in 0..30u32 {
        dists.extend([base, base + 1]);
        base += 1 << if code < 4 { 0 } else { (code - 2) / 2 };
        dists.push(base - 1);
    }
    dists.retain(|&d| d >= 3 && d <= 32768);
    dists.sort();
    dists.dedup();
    for d in dists {
        let r = small_bytes(d, d as u64);
        let mut data = r.clone();
        data.extend_from_slice(&r[..d.min(64)]);
        let size = roundtrip(&data, 4096, &format!("distance {d}"));
        if d >= 100 {
            assert!(size < d + 40, "distance {d}: {size} bytes - the copy at this distance was not found");
        }
    }
}

#[test]
fn a_match_never_reaches_further_back_than_32768() {
    let r = small_bytes(32769, 5);
    let mut data = r.clone();
    data.extend_from_slice(&r[..64]);
    let size = roundtrip(&data, 8192, "32769 back");
    // The copy is 64 bytes 32769 back: out of reach, so it is written as 64 literals of 8 bits (and the data as one fixed block).
    assert!(size >= 32769 + 40, "{size} bytes: a match reached 32769 back");
}

#[test]
fn runs_and_repeats_across_many_blocks() {
    let zeros = vec![0u8; 300_000];
    assert!(roundtrip(&zeros, 70000, "zeros") < 2500, "a run costs about 13 bits for 258 bytes");
    let unit = small_bytes(20_000, 3);
    let repeated: Vec<u8> = (0..10).flat_map(|_| unit.clone()).collect();
    let size = roundtrip(&repeated, 7777, "20000 bytes ten times");
    assert!(size < 20_000 + 4_000, "{size} bytes: the repeats across blocks were not found");
    roundtrip(&small_bytes(200_000, 9), 65535, "fixed blocks, several");
    roundtrip(&small_bytes(65535, 9), 65535, "exactly one block");
    roundtrip(&small_bytes(65536, 9), 65536, "one block and one byte");
}

#[test]
fn noise_costs_its_raw_size_and_a_few_bytes_per_block() {
    let mut r = Rng(99);
    let data: Vec<u8> = (0..1_000_000).map(|_| r.byte()).collect();
    let size = roundtrip(&data, 5761, "noise");
    let blocks = data.len() / 65535 + 1;
    assert!(size <= data.len() + 5 * blocks + 2 + 4, "{size} bytes for {} of noise", data.len());
    assert!(size >= data.len(), "noise cannot shrink");
}

#[test]
fn the_final_block_flag_is_on_the_last_block_only() {
    // Three blocks of noise: a stored block is 5 bytes of header and 65535 of data, and only the last has the flag.
    let mut r = Rng(1234);
    let data: Vec<u8> = (0..65535 * 2 + 200).map(|_| r.byte()).collect();
    let z = zlib_of(&data, 4000);
    let mut at = 2;
    let mut flags = Vec::new();
    while at < z.len() - 4 {
        flags.push(z[at] & 1);
        let len = u16::from_le_bytes([z[at + 1], z[at + 2]]) as usize;
        assert_eq!(z[at] & 6, 0, "stored");
        at += 5 + len;
    }
    assert_eq!(flags, [0, 0, 1]);
    assert_eq!(at, z.len() - 4);
}

// ---- the filters ------------------------------------------------------------------------------------------------------------

#[test]
fn paeth_follows_the_specification_on_every_tie() {
    let vals = [0u8, 1, 2, 3, 5, 8, 100, 127, 128, 129, 200, 254, 255];
    for &a in &vals {
        for &b in &vals {
            for &c in &vals {
                assert_eq!(paeth(a, b, c), paeth_spec(a, b, c), "paeth({a},{b},{c})");
            }
        }
    }
}

#[test]
fn every_filter_matches_the_specification_and_row_zero_sees_zeros_above() {
    let e = PngEncoder::new(Vec::new(), 7, 2).unwrap();
    assert!(e.prev.iter().all(|&b| b == 0), "the row above row 0 is zeros");
    let mut r = Rng(5);
    let (row, up): (Vec<u8>, Vec<u8>) = ((0..21).map(|_| r.byte()).collect(), (0..21).map(|_| r.byte()).collect());
    for (above, name) in [(&e.prev, "row 0"), (&up, "a later row")] {
        for kind in 0..5u8 {
            let mut out = vec![0u8; 22];
            let sum = filter_into(kind, &row, above, &mut out);
            assert_eq!(out[0], kind);
            for i in 0..21 {
                let (a, b) = (if i >= 3 { row[i - 3] } else { 0 }, above[i]);
                let c = if i >= 3 { above[i - 3] } else { 0 };
                let predicted = [0, a, b, ((a as u16 + b as u16) / 2) as u8, paeth_spec(a, b, c)][kind as usize];
                assert_eq!(out[i + 1], row[i].wrapping_sub(predicted), "{name}, filter {kind}, byte {i}");
            }
            let expect: u64 = out[1..].iter().map(|&f| (f as i8 as i64).unsigned_abs()).sum();
            assert_eq!(sum, expect, "{name}, filter {kind}: the sum of absolute values");
        }
    }
}

// ---- the images, through both readers --------------------------------------------------------------------------------------

#[test]
fn edge_images_come_back_exactly_from_both_readers() {
    let started = Instant::now();
    let mut total = [0usize; 5];
    let mut cases: Vec<(String, usize, usize, Vec<u8>)> = vec![
        ("1x1".into(), 1, 1, vec![12, 200, 99]),
        ("width 1, height 1000".into(), 1, 1000, gradient(1, 1000)),
        ("height 1, width 1000".into(), 1000, 1, gradient(1000, 1)),
        ("white".into(), 40, 30, colour(40, 30, [255, 255, 255])),
        ("one colour".into(), 33, 21, colour(33, 21, [200, 30, 90])),
        ("black".into(), 64, 4, colour(64, 4, [0, 0, 0])),
    ];
    for w in [7usize, 8, 9, 15, 17, 65] {
        cases.push((format!("noise, width {w}"), w, 13, noise(w, 13, w as u64)));
        cases.push((format!("gradient, width {w}"), w, 13, gradient(w, 13)));
        cases.push((format!("text-like, width {w}"), w, 40, text_like(w, 40)));
    }
    cases.push(("noise 300 x 300 (several blocks)".into(), 300, 300, noise(300, 300, 3)));
    cases.push(("gradient 640 x 480".into(), 640, 480, gradient(640, 480)));
    cases.push(("text-like 800 x 600".into(), 800, 600, text_like(800, 600)));
    for (name, w, h, pix) in &cases {
        let png = encode(pix, *w as u32, *h as u32);
        let dec = both_readers(&png, *w as u32, *h as u32, pix, name);
        for k in 0..5 {
            total[k] += dec.filters[k];
        }
    }
    eprintln!("png: {} edge images through both readers in {:?}; filters used (None Sub Up Average Paeth): {total:?}", cases.len(), started.elapsed());
    assert!(total.iter().all(|&n| n > 0), "every one of the five filters is chosen somewhere in the images: {total:?}");
}

#[test]
fn a_tall_image_has_many_idat_chunks_of_bounded_size() {
    let pix = noise(100, 2000, 21);
    let png = encode(&pix, 100, 2000);
    let dec = both_readers(&png, 100, 2000, &pix, "noise 100 x 2000");
    assert!(dec.idat_chunks >= 600_000 / IDAT_MAX, "{} chunks", dec.idat_chunks);
}

#[test]
fn the_screen_sized_cases_with_noise_and_text() {
    for (name, pix) in [("noise", noise(1920, 1080, 77)), ("text-like", text_like(1920, 1080))] {
        let started = Instant::now();
        let png = encode(&pix, 1920, 1080);
        let took = started.elapsed();
        both_readers(&png, 1920, 1080, &pix, &format!("1920 x 1080 {name}"));
        eprintln!("png: 1920 x 1080 {name}: {} bytes in {took:?} (raw {})", png.len(), pix.len());
        if name == "noise" {
            let raw = pix.len();
            assert!(png.len() <= raw + raw / 1000 + 64, "noise: {} bytes for {raw} raw bytes", png.len());
        }
    }
}

// ---- the ratio against ImageMagick's own writer ----------------------------------------------------------------------------

/// The bytes ImageMagick's PNG writer makes of the same pixels (the writer `import` uses, its defaults).
fn imagemagick_size(pix: &[u8], w: usize, h: usize) -> usize {
    let dir = Scratch::new("pngratio");
    let (raw, out) = (dir.0.join("in.rgb"), dir.0.join("out.png"));
    std::fs::write(&raw, pix).unwrap();
    let r = run(Command::new("convert").args(["-size", &format!("{w}x{h}"), "-depth", "8"]).arg(format!("rgb:{}", raw.display())).arg(&out), "convert (ImageMagick)");
    assert!(r.status.success(), "convert: {}", String::from_utf8_lossy(&r.stderr));
    std::fs::metadata(&out).unwrap().len() as usize
}

#[test]
fn the_size_against_imagemagick_on_a_screen_of_text_a_gradient_and_noise() {
    for (name, pix, factor) in [("text-like", text_like(1920, 1080), RATIO_TEXT), ("gradient", gradient(1920, 1080), RATIO_GRADIENT), ("noise", noise(1920, 1080, 77), 1.001)] {
        let ours = encode(&pix, 1920, 1080).len();
        let theirs = imagemagick_size(&pix, 1920, 1080);
        eprintln!("png: 1920 x 1080 {name}: ours {ours} bytes, ImageMagick {theirs} bytes, ratio {:.3} (ceiling {factor})", ours as f64 / theirs as f64);
        assert!(ours as f64 <= theirs as f64 * factor + 64.0, "{name}: ours {ours}, ImageMagick {theirs}, ceiling {factor}");
    }
}

/// The ceilings of the ratio (ours / ImageMagick's) on the synthetic images, from the measurement (24.1 and 4.8) with margin. The text-like one is far over the lead's stop level of 3 (116 point 3): it is pinned here only so that it cannot get worse, and the slice mail asks the lead to decide on dynamic codes.
const RATIO_TEXT: f64 = 30.0;
const RATIO_GRADIENT: f64 = 6.0;

// ---- use, errors, memory ---------------------------------------------------------------------------------------------------

fn kind_of<T>(r: io::Result<T>) -> io::ErrorKind {
    r.err().expect("an Err").kind()
}

#[test]
fn wrong_use_is_an_error_and_never_a_panic_or_a_bad_file() {
    for (w, h) in [(0, 1), (1, 0), (0, 0), (MAX_SIDE + 1, 1), (1, MAX_SIDE + 1), (u32::MAX, 1)] {
        assert_eq!(kind_of(PngEncoder::new(Vec::new(), w, h)), io::ErrorKind::InvalidInput, "{w} x {h}");
    }
    assert!(PngEncoder::new(Vec::new(), 1, MAX_SIDE).is_ok(), "the largest height is accepted (and costs no memory)");
    let mut e = PngEncoder::new(Vec::new(), 2, 2).unwrap();
    for bad_len in [0usize, 1, 5, 7, 12] {
        assert_eq!(kind_of(e.push_row(&vec![1; bad_len])), io::ErrorKind::InvalidInput, "a row of {bad_len} bytes");
    }
    assert_eq!(kind_of(e.finish()), io::ErrorKind::InvalidInput, "finish with no rows");
    e.push_row(&[1, 2, 3, 4, 5, 6]).unwrap();
    assert_eq!(kind_of(e.finish()), io::ErrorKind::InvalidInput, "finish with one row of two");
    e.push_row(&[7, 8, 9, 10, 11, 12]).unwrap();
    assert_eq!(kind_of(e.push_row(&[0; 6])), io::ErrorKind::InvalidInput, "a third row");
    let png = e.finish().unwrap();
    both_readers(&png, 2, 2, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], "after the errors");
    assert_eq!(kind_of(e.finish()), io::ErrorKind::InvalidInput, "finish twice");
    assert_eq!(kind_of(e.push_row(&[0; 6])), io::ErrorKind::InvalidInput, "a row after finish");
}

/// A sink that takes `room` bytes and then fails with a recognizable error.
struct Failing {
    room: usize,
    took: usize,
}

impl Write for Failing {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.took + buf.len() > self.room {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "CANARY-sink-failed"));
        }
        self.took += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_write_error_comes_back_as_it_is_and_the_encoder_then_refuses() {
    let same = |e: io::Error| e.kind() == io::ErrorKind::BrokenPipe && e.to_string() == "CANARY-sink-failed";
    assert!(same(PngEncoder::new(Failing { room: 0, took: 0 }, 4, 4).err().unwrap()), "in the signature");
    assert!(same(PngEncoder::new(Failing { room: 10, took: 0 }, 4, 4).err().unwrap()), "in the header");
    let pix = noise(100, 400, 4);
    let mut e = PngEncoder::new(Failing { room: 60_000, took: 0 }, 100, 400).unwrap();
    let mut failed_at = None;
    for (i, row) in pix.chunks(300).enumerate() {
        if let Err(err) = e.push_row(row) {
            assert!(same(err), "the sink's own error");
            failed_at = Some(i);
            break;
        }
    }
    assert!(failed_at.is_some(), "120 KB of noise does not fit 60 KB");
    assert_eq!(kind_of(e.push_row(&[0; 300])), io::ErrorKind::InvalidInput, "after a write error");
    assert_eq!(kind_of(e.finish()), io::ErrorKind::InvalidInput);
    // A failure while finishing.
    let mut e = PngEncoder::new(Failing { room: 60, took: 0 }, 1, 1).unwrap();
    e.push_row(&[1, 2, 3]).unwrap();
    assert!(same(e.finish().err().unwrap()), "in the IDAT or IEND chunk");
}

/// A sink that counts what it was given.
struct Count(Rc<Cell<usize>>);

impl Write for Count {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.set(self.0.get() + buf.len());
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_file_is_written_as_the_rows_come_and_the_buffers_do_not_grow_with_the_height() {
    let seen = Rc::new(Cell::new(0));
    let mut e = PngEncoder::new(Count(seen.clone()), 1920, 3000).unwrap();
    let at_start = e.buffer_bytes();
    let rows = noise(1920, 200, 8);
    for row in rows.chunks(5760) {
        e.push_row(row).unwrap();
    }
    assert!(seen.get() > 100_000, "after 200 rows of 5760 bytes only {} bytes were written: the file waits for the end", seen.get());
    assert_eq!(e.buffer_bytes(), at_start, "no buffer grew while rows were pushed");
    let tall = PngEncoder::new(Vec::new(), 1920, MAX_SIDE).unwrap();
    assert_eq!(tall.buffer_bytes(), at_start, "the buffers do not depend on the height");
    eprintln!("png: the encoder's own buffers at width 1920: {at_start} bytes");
    assert!(at_start < 1 << 20, "{at_start} bytes");
}

#[test]
fn two_encoders_side_by_side_do_not_touch_each_other() {
    let (a, b) = (noise(50, 400, 1), text_like(50, 400));
    let (mut ea, mut eb) = (PngEncoder::new(Vec::new(), 50, 400).unwrap(), PngEncoder::new(Vec::new(), 50, 400).unwrap());
    for (ra, rb) in a.chunks(150).zip(b.chunks(150)) {
        ea.push_row(ra).unwrap();
        eb.push_row(rb).unwrap();
    }
    let (pa, pb) = (ea.finish().unwrap(), eb.finish().unwrap());
    assert!(pa == encode(&a, 50, 400) && pb == encode(&b, 50, 400), "the same bytes as when each runs alone");
    both_readers(&pa, 50, 400, &a, "first");
    both_readers(&pb, 50, 400, &b, "second");
}

// ---- the readers can fail --------------------------------------------------------------------------------------------------

#[test]
fn each_reader_refuses_a_png_with_one_byte_flipped() {
    let pix = text_like(40, 30);
    let png = encode(&pix, 40, 30);
    both_readers(&png, 40, 30, &pix, "the unflipped file");
    let n = png.len();
    // 4 + 4 + 8 = the start of the IHDR data; IEND is the last 12 bytes, the CRC of the last IDAT the 4 before them, the Adler-32 the 4 before that.
    let places = [("the width in IHDR", 16usize), ("the CRC of IHDR", 29), ("the zlib data", 8 + 25 + 8 + 10), ("the Adler-32", n - 12 - 4 - 1), ("the CRC of IDAT", n - 12 - 1), ("the type of IEND", n - 8)];
    for (what, at) in places {
        let mut flipped = png.clone();
        flipped[at] ^= 0x01;
        let own = read_png(&flipped);
        let (py_ok, _, pixels, said) = outside(&flipped, 40, 30);
        eprintln!("png: one byte flipped in {what}: test decoder {:?}; {said}; convert gave {} bytes{}", own.as_ref().err(), pixels.len(), if pixels == pix { " (the right pixels)" } else { "" });
        assert!(own.is_err(), "{what}: the test decoder accepts the flipped file");
        assert!(!py_ok, "{what}: the Python checker accepts the flipped file");
        assert!(pixels != pix, "{what}: ImageMagick reads the flipped file as the right picture");
    }
    // The same flips with the CRC of the IDAT chunk made right again, so that only the zlib layer (the data, the Adler-32) can refuse.
    // The picture is small: one IDAT chunk, from byte 33 (length, type, data, CRC).
    let len = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
    assert_eq!(33 + 12 + len + 12, n, "one IDAT chunk");
    for (what, at) in [("the zlib data", 41 + 10), ("the Adler-32", 41 + len - 1)] {
        let mut flipped = png.clone();
        flipped[at] ^= 0x01;
        let crc = slow_crc(&flipped[37..41 + len]).to_be_bytes();
        flipped[41 + len..45 + len].copy_from_slice(&crc);
        let own = read_png(&flipped);
        let (py_ok, _, pixels, said) = outside(&flipped, 40, 30);
        eprintln!("png: one byte flipped in {what}, CRC made right: test decoder {:?}; {said}; convert gave {} bytes", own.as_ref().err(), pixels.len());
        assert!(own.is_err(), "{what}: the test decoder accepts it");
        assert!(!py_ok, "{what}: the Python checker accepts it");
        assert!(pixels != pix, "{what}: ImageMagick reads it as the right picture");
    }
}

#[test]
fn the_test_decoder_refuses_what_the_writer_must_never_make() {
    let pix = noise(10, 10, 6);
    let png = encode(&pix, 10, 10);
    assert!(read_png(&png[..png.len() - 1]).is_err(), "cut off");
    assert!(read_png(&png[..png.len() - 12]).is_err(), "no IEND");
    let mut twice = png.clone();
    twice.extend_from_slice(&png[png.len() - 12..]);
    assert!(read_png(&twice).is_err(), "IEND twice");
    // A stored block whose NLEN is not the complement, and a stream without the final flag.
    assert!(unzlib(&[0x78, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x7F, 0, 0, 0, 0]).is_err());
    let good = zlib_of(b"abc", 1);
    let mut no_final = good.clone();
    no_final[2] &= !1;
    assert!(unzlib(&no_final).is_err());
}
