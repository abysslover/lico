//! The contract tests of `lico-png`: each states one line of `SPEC.md` and uses only the public names (`PngEncoder`, `MAX_SIDE`, `IDAT_MAX`) and the
//! test support (`lico_png::testing`, the feature `testing`, turned on for this crate's own tests by its dev-dependency on itself).

use lico_png::testing::*;
use lico_png::{PngEncoder, IDAT_MAX, MAX_SIDE};
use std::cell::Cell;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::Instant;

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


#[test]
fn the_size_against_imagemagick_on_a_screen_of_text_a_gradient_and_noise() {
    for (name, pix, factor) in [("text-like", text_like(1920, 1080), RATIO_TEXT), ("gradient", gradient(1920, 1080), RATIO_GRADIENT), ("noise", noise(1920, 1080, 77), 1.001)] {
        let ours = encode(&pix, 1920, 1080).len();
        let theirs = imagemagick_size(&pix, 1920, 1080);
        eprintln!("png: 1920 x 1080 {name}: ours {ours} bytes, ImageMagick {theirs} bytes, ratio {:.3} (ceiling {factor})", ours as f64 / theirs as f64);
        assert!(ours as f64 <= theirs as f64 * factor + 64.0, "{name}: ours {ours}, ImageMagick {theirs}, ceiling {factor}");
    }
}

/// The ceilings of the ratio (ours / ImageMagick's) on the synthetic images, from the measurement (24.1 and 4.8) with margin. The text-like one is far over the lead's stop level of 3 (116 point 3) because the fixed Huffman codes stay: accepted by the lead on 2026-10-29 (mail 119); the report names the figures. The ceilings are pins so that the encoder cannot get worse.
pub(crate) const RATIO_TEXT: f64 = 30.0;
pub(crate) const RATIO_GRADIENT: f64 = 6.0;

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

/// I7 of the specification: within one version the same width, height and rows give the same bytes (any run, any sink).
#[test]
fn the_same_input_gives_the_same_bytes() {
    for (w, h, pix) in [(1usize, 1usize, noise(1, 1, 3)), (37, 29, noise(37, 29, 5)), (200, 90, text_like(200, 90)), (64, 64, gradient(64, 64))] {
        let first = encode(&pix, w as u32, h as u32);
        for _ in 0..3 {
            assert!(encode(&pix, w as u32, h as u32) == first, "{w}x{h}: the bytes differ between two runs");
        }
        let mut file = std::io::Cursor::new(Vec::new());
        let mut e = PngEncoder::new(&mut file, w as u32, h as u32).unwrap();
        for row in pix.chunks(w * 3) {
            e.push_row(row).unwrap();
        }
        e.finish().unwrap();
        assert!(file.into_inner() == first, "{w}x{h}: another kind of sink gives other bytes");
    }
}

/// I1 of the specification, byte for byte: the signature and the header of a one-pixel file, and the end chunk.
#[test]
fn the_header_of_a_one_pixel_file_is_exactly_the_bytes_of_the_specification() {
    let file = encode(&[10, 20, 30], 1, 1);
    let head: [u8; 33] = [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // the signature
        0, 0, 0, 13, b'I', b'H', b'D', b'R', // the length and the type of the header chunk
        0, 0, 0, 1, 0, 0, 0, 1, // width 1, height 1
        8, 2, 0, 0, 0, // 8 bits, RGB, deflate, filter method 0, no interlace
        0x90, 0x77, 0x53, 0xDE, // its check sum
    ];
    assert_eq!(&file[..33], &head[..]);
    let end: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];
    assert_eq!(&file[file.len() - 12..], &end[..]);
    // a wider header carries the sizes big-endian
    let wide = encode(&noise(258, 1, 9), 258, 1);
    assert_eq!(&wide[16..24], &[0, 0, 1, 2, 0, 0, 0, 1]);
}

/// The API lines of the specification, exactly: the names the crate makes public are these and no others (the check functions are not among
/// them). It reads the source of the writer, so a new `pub` item cannot slip in without this list, and so the specification, changing.
#[test]
fn the_public_names_are_exactly_these() {
    let source = include_str!("../src/lib.rs");
    let mut found: Vec<String> = source
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("pub ") && !l.starts_with("pub("))
        .map(|l| l.trim_start_matches("pub ").split(|c| c == '(' || c == '<' || c == ':' || c == '{' || c == '=' || c == ';' || c == ' ').take(2).collect::<Vec<_>>().join(" "))
        .collect();
    found.sort();
    let mut expected: Vec<String> = ["const IDAT_MAX", "const MAX_SIDE", "mod testing", "struct PngEncoder", "fn buffer_bytes", "fn finish", "fn new", "fn push_row"].iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected);
}
