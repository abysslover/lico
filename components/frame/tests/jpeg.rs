//! The contract tests of `lico-frame` for JPEG (SPEC.md 0.2.0). Three kinds of input: the files of `tests/fixtures/` (made by libjpeg's `cjpeg`, and two real frames of the engine; the expected
//! checksums come from libjpeg's own `djpeg -dct int -nosmooth`, see `make_jpeg_fixtures.sh`), hand-made minimal JPEGs (a few bytes whose every bit is known), and damaged copies of both.

use lico_frame::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}.jpg", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn fnv(image: &Image) -> u64 {
    image.pixels.iter().fold(0xcbf29ce484222325u64, |h, p| (h ^ *p as u64).wrapping_mul(0x100000001b3))
}

// (name, width, height, FNV-1a of the pixels as 0xAARRGGBB, row after row) of libjpeg's decode: the accurate integer IDCT and plain (box) chroma upsampling. Printed by make_jpeg_fixtures.sh.
const EXPECTED: &[(&str, u32, u32, u64)] = &[
    ("odd_61x37_444", 61, 37, 0xf9999f17e2a01a57),
    ("odd_61x37_420", 61, 37, 0xb0c97f2d62b476da),
    ("odd_61x37_422", 61, 37, 0x45d721407bee7449),
    ("odd_61x37_440", 61, 37, 0x30f1e0cca4a29bbe),
    ("grey_61x37", 61, 37, 0xa6e08ed664088742),
    ("optimized_61x37_420", 61, 37, 0xb0c97f2d62b476da),
    ("restart_rows_96x48_420", 96, 48, 0x3dceba191615c7a5),
    ("restart_blocks_96x48_444", 96, 48, 0x3ae7d4c22753d7e5),
    ("one_pixel", 1, 1, 0x9e85543d1e57cb74),
    ("grey_8x8", 8, 8, 0xd5140f5139f34285),
    ("moli_320x200_q60_420", 320, 200, 0x87bcc5029151efdf),
    ("moli_320x200_q90_444", 320, 200, 0xbabc936a1c0d3d79),
];

// ---- a minimal JPEG whose every bit is known --------------------------------------------------------------------------------------------------------

fn put_segment(out: &mut Vec<u8>, marker: u8, body: &[u8]) {
    out.extend([0xFF, marker]);
    out.extend(((body.len() + 2) as u16).to_be_bytes());
    out.extend(body);
}

fn dht(class_and_id: u8, counts: &[u8; 16], symbols: &[u8]) -> Vec<u8> {
    let mut b = vec![class_and_id];
    b.extend(counts);
    b.extend(symbols);
    b
}

/// A grey baseline JPEG of `w` x `h` with flat quantisation tables of 1 and the given Huffman tables and entropy-coded bytes.
fn tiny(w: u16, h: u16, dc: &[u8], ac: &[u8], data: &[u8]) -> Vec<u8> {
    tiny_with(w, h, dc, ac, data, false)
}

fn tiny_with(w: u16, h: u16, dc: &[u8], ac: &[u8], data: &[u8], sixteen_bit_quantisers: bool) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    let mut dqt = vec![if sixteen_bit_quantisers { 0x10u8 } else { 0 }];
    for _ in 0..64 {
        if sixteen_bit_quantisers {
            dqt.push(0);
        }
        dqt.push(1);
    }
    put_segment(&mut out, 0xDB, &dqt);
    let mut sof = vec![8u8];
    sof.extend(h.to_be_bytes());
    sof.extend(w.to_be_bytes());
    sof.extend([1, 1, 0x11, 0]);
    put_segment(&mut out, 0xC0, &sof);
    put_segment(&mut out, 0xC4, dc);
    put_segment(&mut out, 0xC4, ac);
    put_segment(&mut out, 0xDA, &[1, 1, 0x00, 0, 63, 0]);
    out.extend(data);
    out.extend([0xFF, 0xD9]);
    out
}

/// DC table: symbol 0 is the code `0`, symbol 7 is the code `1`. AC table: the symbol 0 (end of block) is the code `0`.
fn two_symbol_tables() -> (Vec<u8>, Vec<u8>) {
    let mut c = [0u8; 16];
    c[0] = 2;
    let dc = dht(0x00, &c, &[0, 7]);
    c[0] = 1;
    let ac = dht(0x10, &c, &[0]);
    (dc, ac)
}

#[test]
fn a_hand_made_block_gives_the_pixels_worked_out_by_hand() {
    // the bits: DC code `1` (symbol 7), seven bits 1000000 (= 64), then the end of the block `0`; padded with ones: 1100 0000 | 0111 1111
    let (dc, ac) = two_symbol_tables();
    let image = decode(&tiny(8, 8, &dc, &ac, &[0xC0, 0x7F]), &Limits::default()).unwrap();
    assert_eq!((image.width, image.height), (8, 8));
    // a flat block of DC 64 and quantiser 1: ((64 * 4 + 16) >> 5) + 128 = 136
    assert!(image.pixels.iter().all(|p| *p == 0xFF88_8888), "{:08x?}", &image.pixels[..4]);
}

#[test]
fn the_dc_value_is_predicted_from_the_block_before_and_the_blocks_sit_side_by_side() {
    // block 1: DC +64 as above. block 2: `1`, then 0111111 (= 63, which is 63 - 127 = -64), then the end `0`: back to 0 = grey 128.
    // 1 1000000 0 | 1 0111111 0 + padding: 1100 0000 | 0101 1111 | 1011 1111
    let (dc, ac) = two_symbol_tables();
    let image = decode(&tiny(16, 8, &dc, &ac, &[0xC0, 0x5F, 0xBF]), &Limits::default()).unwrap();
    assert_eq!((image.width, image.height), (16, 8));
    for row in image.pixels.chunks(16) {
        assert!(row[..8].iter().all(|p| *p == 0xFF88_8888) && row[8..].iter().all(|p| *p == 0xFF80_8080), "{row:08x?}");
    }
}

#[test]
fn an_image_that_is_not_a_whole_number_of_blocks_is_cut_to_its_size() {
    // 3 x 2 pixels need one block; the rest of the block is dropped
    let (dc, ac) = two_symbol_tables();
    let image = decode(&tiny(3, 2, &dc, &ac, &[0xC0, 0x7F]), &Limits::default()).unwrap();
    assert_eq!((image.width, image.height, image.pixels.len()), (3, 2, 6));
    assert!(image.pixels.iter().all(|p| *p == 0xFF88_8888));
}

#[test]
fn a_huffman_table_with_more_codes_than_the_lengths_allow_is_refused() {
    let mut c = [0u8; 16];
    c[0] = 3; // three codes of one bit: there are only two
    let dc = dht(0x00, &c, &[0, 7, 8]);
    let (_, ac) = two_symbol_tables();
    assert!(matches!(decode(&tiny(8, 8, &dc, &ac, &[0xC0, 0x7F]), &Limits::default()), Err(Error::Huffman(_))));
}

#[test]
fn a_code_that_no_symbol_has_is_an_error() {
    // the AC table has only the code `0`; the data are ones: DC `1`, 1111111, and then `1` is no code of the AC table
    let (dc, ac) = two_symbol_tables();
    let r = decode(&tiny(8, 8, &dc, &ac, &[0xFF, 0x00, 0xFF, 0x00]), &Limits::default());
    assert!(matches!(r, Err(Error::Huffman(_))), "{r:?}");
}

#[test]
fn data_that_ends_before_the_blocks_do_is_cut_short() {
    // a 16 x 8 image with the data of one block only
    let (dc, ac) = two_symbol_tables();
    assert_eq!(decode(&tiny(16, 8, &dc, &ac, &[0xC0, 0x7F]), &Limits::default()), Err(Error::Truncated));
}

// ---- the files --------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_fixtures_decode_to_exactly_the_pixels_of_libjpeg() {
    for (name, w, h, sum) in EXPECTED {
        let image = decode(&fixture(name), &Limits::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((image.width, image.height, image.pixels.len()), (*w, *h, (*w * *h) as usize), "{name}");
        assert_eq!(fnv(&image), *sum, "{name}: the pixels differ from libjpeg's");
    }
}

#[test]
fn the_header_alone_gives_the_size_and_the_kind() {
    for (name, w, h, _) in EXPECTED {
        let head = header(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let colour = if name.starts_with("grey") { 0 } else { 2 };
        assert_eq!(head, Header { width: *w, height: *h, colour, depth: 8, format: Format::Jpeg }, "{name}");
    }
}

#[test]
fn a_grey_jpeg_has_equal_channels_and_full_alpha() {
    let image = decode(&fixture("grey_61x37"), &Limits::default()).unwrap();
    assert!(image.pixels.iter().all(|p| p >> 24 == 0xFF && (p >> 16) & 0xFF == (p >> 8) & 0xFF && (p >> 8) & 0xFF == p & 0xFF));
}

#[test]
fn a_frame_of_the_engine_shows_what_was_drawn() {
    // the page of the real-engine frames (a white background and four coloured boxes under a heading; the boxes at x 6..57, 70..121, 134..185 and y 102..181): the colours survive the coding
    let image = decode(&fixture("moli_320x200_q90_444"), &Limits::default()).unwrap();
    let at = |x: usize, y: usize| image.pixels[y * 320 + x];
    let near = |p: u32, want: (i32, i32, i32)| {
        let (r, g, b) = (((p >> 16) & 0xFF) as i32, ((p >> 8) & 0xFF) as i32, (p & 0xFF) as i32);
        (r - want.0).abs() <= 6 && (g - want.1).abs() <= 6 && (b - want.2).abs() <= 6
    };
    assert!(near(at(5, 5), (255, 255, 255)), "the background: {:08x}", at(5, 5));
    assert!(near(at(31, 141), (0xe5, 0x39, 0x35)), "the red box: {:08x}", at(31, 141));
    assert!(near(at(95, 141), (0x1e, 0x88, 0xe5)), "the blue box: {:08x}", at(95, 141));
    assert!(near(at(159, 141), (0x43, 0xa0, 0x47)), "the green box: {:08x}", at(159, 141));
}

#[test]
fn the_kind_is_told_by_the_first_bytes() {
    assert_eq!(format(&fixture("odd_61x37_420")), Some(Format::Jpeg));
    assert_eq!(format(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]), Some(Format::Png));
    assert_eq!(format(b"GIF89a"), None);
    assert_eq!(format(b""), None);
    assert_eq!(format(&[0xFF, 0xD8]), None, "a start of image alone is no JPEG: the marker after it is needed");
    assert_eq!(decode(b"", &Limits::default()), Err(Error::NotAnImage));
    assert_eq!(decode(b"GIF89a not an image at all", &Limits::default()), Err(Error::NotAnImage));
    assert_eq!(header(b"RIFF....WEBP"), Err(Error::NotAnImage));
}

#[test]
fn progressive_arithmetic_and_other_kinds_are_refused_by_name() {
    let name = |file: &str| match decode(&fixture(file), &Limits::default()) {
        Err(Error::Unsupported(what)) => what,
        other => panic!("{file}: {other:?}"),
    };
    assert!(name("progressive_61x37").contains("progressive"));
    assert!(name("arithmetic_61x37").contains("arithmetic"));
}

#[test]
fn a_second_scan_is_refused_not_misread() {
    let mut jpeg = fixture("odd_61x37_444");
    let eoi = jpeg.len() - 2;
    assert_eq!(&jpeg[eoi..], &[0xFF, 0xD9]);
    jpeg.splice(eoi..eoi, [0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xAA]);
    assert!(matches!(decode(&jpeg, &Limits::default()), Err(Error::Unsupported(w)) if w.contains("scan")), "{:?}", decode(&jpeg, &Limits::default()));
}

#[test]
fn the_size_is_checked_from_the_header_before_anything_is_allocated() {
    let mut jpeg = fixture("odd_61x37_444");
    let sof = jpeg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    // marker (2) + length (2) + precision (1), then height and width
    jpeg[sof + 5..sof + 9].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(decode(&jpeg, &Limits::default()), Err(Error::TooBig));
    assert_eq!((header(&jpeg).unwrap().width, header(&jpeg).unwrap().height), (65535, 65535), "the header alone can be read");
    jpeg[sof + 5..sof + 9].copy_from_slice(&[0, 0, 0, 61]);
    assert_eq!(decode(&jpeg, &Limits::default()), Err(Error::Dimensions), "no height");
    jpeg[sof + 5..sof + 9].copy_from_slice(&[0, 37, 0, 0]);
    assert_eq!(decode(&jpeg, &Limits::default()), Err(Error::Dimensions), "no width");
}

#[test]
fn the_limits_are_exact() {
    let jpeg = fixture("odd_61x37_420");
    assert!(decode(&jpeg, &Limits { max_input: jpeg.len(), max_pixels: 61 * 37 }).is_ok());
    assert_eq!(decode(&jpeg, &Limits { max_input: jpeg.len() - 1, max_pixels: 61 * 37 }), Err(Error::TooBig));
    assert_eq!(decode(&jpeg, &Limits { max_input: jpeg.len(), max_pixels: 61 * 37 - 1 }), Err(Error::TooBig));
    assert_eq!(Limits::default().max_input, 4 * 1024 * 1024);
}

#[test]
fn the_same_bytes_give_the_same_image() {
    let jpeg = fixture("moli_320x200_q60_420");
    assert_eq!(decode(&jpeg, &Limits::default()), decode(&jpeg, &Limits::default()));
}

#[test]
fn bytes_after_the_end_and_a_missing_end_are_errors() {
    let jpeg = fixture("odd_61x37_420");
    let mut more = jpeg.clone();
    more.push(0);
    assert!(matches!(decode(&more, &Limits::default()), Err(Error::Marker(_))), "bytes after the end of image");
    assert_eq!(decode(&jpeg[..jpeg.len() - 2], &Limits::default()), Err(Error::Truncated), "no end of image");
}

#[test]
fn a_restart_marker_that_is_wrong_or_missing_is_an_error() {
    for name in ["restart_rows_96x48_420", "restart_blocks_96x48_444"] {
        let jpeg = fixture(name);
        let at = jpeg.windows(2).rposition(|w| w[0] == 0xFF && (0xD0..=0xD7).contains(&w[1])).unwrap_or_else(|| panic!("{name}: no restart marker"));
        let mut wrong = jpeg.clone();
        wrong[at + 1] = if wrong[at + 1] == 0xD3 { 0xD4 } else { 0xD3 };
        assert!(matches!(decode(&wrong, &Limits::default()), Err(Error::Marker(_))), "{name}: a wrong number");
        let mut gone = jpeg.clone();
        gone.drain(at..at + 2);
        assert!(decode(&gone, &Limits::default()).is_err(), "{name}: a missing marker");
    }
}

#[test]
fn a_scan_without_its_tables_is_an_error() {
    let mut jpeg = fixture("odd_61x37_420");
    // the tables of Huffman codes become comments: the scan then has none
    let mut i = 0;
    while i + 1 < jpeg.len() {
        if jpeg[i] == 0xFF && jpeg[i + 1] == 0xC4 {
            jpeg[i + 1] = 0xFE;
        }
        i += 1;
    }
    assert!(matches!(decode(&jpeg, &Limits::default()), Err(Error::Marker(w)) if w.contains("table")), "{:?}", decode(&jpeg, &Limits::default()));
}


#[test]
fn a_table_of_sixteen_bit_quantisers_is_read() {
    let (dc, ac) = two_symbol_tables();
    let image = decode(&tiny_with(8, 8, &dc, &ac, &[0xC0, 0x7F], true), &Limits::default()).unwrap();
    assert!(image.pixels.iter().all(|p| *p == 0xFF88_8888));
}

#[test]
fn a_quantiser_multiplies_the_coefficient() {
    // the DC value 64 with a quantiser of 2 is 128: ((128 * 4 + 16) >> 5) + 128 = 144
    let (dc, ac) = two_symbol_tables();
    let mut jpeg = tiny(8, 8, &dc, &ac, &[0xC0, 0x7F]);
    let dqt = jpeg.windows(2).position(|w| w == [0xFF, 0xDB]).unwrap();
    jpeg[dqt + 5] = 2; // the first quantiser (of the DC) follows the marker, the length and the precision-and-id byte
    let image = decode(&jpeg, &Limits::default()).unwrap();
    assert!(image.pixels.iter().all(|p| *p == 0xFF90_9090), "{:08x}", image.pixels[0]);
}

#[test]
fn an_ac_size_over_the_baseline_limit_and_a_coefficient_past_the_block_are_errors() {
    // AC table: `0` is the end of the block, `1` is (run 0, size 11): a size baseline does not have
    let mut c = [0u8; 16];
    c[0] = 2;
    let big = dht(0x10, &c, &[0x00, 0x0B]);
    let (dc, _) = two_symbol_tables();
    // DC `0` (size 0), AC `1`
    let r = decode(&tiny(8, 8, &dc, &big, &[0x7F]), &Limits::default());
    assert!(matches!(r, Err(Error::Huffman(_))), "{r:?}");
    // AC table: `0` is the end of the block, `1` is (run 15, size 1): four of them run past the 63rd coefficient. bits: DC `0`, then (`1`, value bit `0`) four times
    let long = dht(0x10, &c, &[0x00, 0xF1]);
    let r = decode(&tiny(8, 8, &dc, &long, &[0x55, 0x7F]), &Limits::default());
    assert!(matches!(r, Err(Error::Huffman(_))), "{r:?}");
    // three of them stay inside the block (the end of the block is `0`): DC `0`, (`1`, `0`) x 3, `0` = 0 10 10 10 0
    let r = decode(&tiny(8, 8, &dc, &long, &[0x54]), &Limits::default());
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn sixteen_zeros_in_a_row_move_on_by_sixteen_and_a_coefficient_goes_to_its_place() {
    // AC table: `0` is the end of the block, `10` is (run 15, size 0) = sixteen zeros, `11` is (run 0, size 10). The block: DC `0`, sixteen zeros, then 512 (ten bits 1000000000), the end.
    // Zigzag place 17 is natural place 19: row 2, column 3, a wave of three half-periods across and two down: f = 128 + 128 cos((2x + 1) 3 pi / 16) cos((2y + 1) 2 pi / 16).
    let mut c = [0u8; 16];
    c[0] = 1;
    c[1] = 2;
    let ac = dht(0x10, &c, &[0x00, 0xF0, 0x0A]);
    let (dc, _) = two_symbol_tables();
    // bits: 0 | 10 | 11 | 1000000000 | 0 = 0101 1100 0000 0000
    let image = decode(&tiny(8, 8, &dc, &ac, &[0x5C, 0x00]), &Limits::default()).unwrap();
    for y in 0..8 {
        for x in 0..8 {
            let pi = std::f64::consts::PI;
            let want = 128.0 + 128.0 * (((2 * x + 1) * 3) as f64 * pi / 16.0).cos() * (((2 * y + 1) * 2) as f64 * pi / 16.0).cos();
            let got = (image.pixels[y * 8 + x] & 0xFF) as f64;
            assert!((got - want).abs() <= 1.5, "({x}, {y}): {got} against {want:.1}");
        }
    }
}

#[test]
fn an_extended_sequential_frame_of_8_bits_is_read_as_baseline() {
    let mut jpeg = fixture("grey_8x8");
    let want = decode(&jpeg, &Limits::default()).unwrap();
    let sof = jpeg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    jpeg[sof + 1] = 0xC1;
    assert_eq!(decode(&jpeg, &Limits::default()).unwrap(), want);
    jpeg[sof + 4] = 12; // 12 bits per sample
    assert!(matches!(decode(&jpeg, &Limits::default()), Err(Error::Unsupported(w)) if w.contains("precision")));
}

fn patched(name: &str, marker: u8, offset: usize, value: u8) -> Vec<u8> {
    let mut jpeg = fixture(name);
    let at = jpeg.windows(2).position(|w| w == [0xFF, marker]).unwrap();
    jpeg[at + offset] = value;
    jpeg
}

#[test]
fn the_sampling_factors_are_checked() {
    // the three components of the frame header are at offsets 10, 13 and 16 from the marker; their factors follow the number
    let with = |f: [(u8, u8); 3]| {
        let mut jpeg = fixture("odd_61x37_444");
        let at = jpeg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        for (i, (h, v)) in f.iter().enumerate() {
            jpeg[at + 11 + 3 * i] = h << 4 | v;
        }
        jpeg
    };
    assert!(header(&with([(2, 2), (2, 2), (1, 2)])).is_ok(), "ten blocks in a unit are the most");
    assert!(matches!(header(&with([(3, 3), (1, 1), (1, 1)])), Err(Error::Marker(w)) if w.contains("ten blocks")), "eleven are too many");
    assert!(matches!(header(&with([(2, 2), (3, 1), (1, 1)])), Err(Error::Unsupported(w)) if w.contains("divide")), "2 and 3 do not divide");
    assert!(matches!(header(&with([(0, 1), (1, 1), (1, 1)])), Err(Error::Marker(_))), "a factor of 0");
    assert!(matches!(header(&with([(1, 5), (1, 1), (1, 1)])), Err(Error::Marker(_))), "a factor of 5");
    assert!(matches!(patched("odd_61x37_444", 0xC0, 9, 4).get(..).map(|j| header(j)), Some(Err(Error::Unsupported(_)))), "four components");
    assert!(matches!(patched("odd_61x37_444", 0xC0, 12, 4).get(..).map(|j| header(j)), Some(Err(Error::Marker(_)))), "a table of quantisers 4");
}

#[test]
fn the_scan_header_is_checked() {
    // offsets from the marker of the scan: 4 the number of components, 5 and 6 the first (its number, its tables), 11 the first and 12 the last coefficient, 13 the approximation
    let refused = |jpeg: Vec<u8>| decode(&jpeg, &Limits::default()).unwrap_err();
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 4, 2)), Error::Marker(_) | Error::Unsupported(_)), "two of three components");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 11, 1)), Error::Unsupported(_)), "a first coefficient of 1");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 12, 62)), Error::Unsupported(_)), "a last coefficient of 62");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 13, 1)), Error::Unsupported(_)), "an approximation");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 13, 0x10)), Error::Unsupported(_)), "a high approximation");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 6, 0x40)), Error::Marker(_)), "a DC table 4");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 6, 0x04)), Error::Marker(_)), "an AC table 4");
    assert!(matches!(refused(patched("odd_61x37_444", 0xDA, 5, 9)), Error::Marker(_)), "a component the frame does not have");
    let mut twice = fixture("odd_61x37_444");
    let at = twice.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    twice[at + 7] = twice[at + 5];
    assert!(matches!(refused(twice), Error::Marker(_)), "a component twice");
}

#[test]
fn markers_that_do_not_belong_are_errors() {
    let jpeg = fixture("grey_8x8");
    let with = |bytes: &[u8]| {
        let mut j = jpeg.clone();
        j.splice(2..2, bytes.iter().copied());
        j
    };
    assert!(matches!(decode(&with(&[0xFF, 0xD0]), &Limits::default()), Err(Error::Marker(_))), "a restart marker before the scan");
    assert!(matches!(decode(&with(&[0xFF, 0xD8]), &Limits::default()), Err(Error::Marker(_))), "a second start of image");
    assert!(matches!(decode(&with(&[0xFF, 0xD9]), &Limits::default()), Err(Error::Marker(_))), "the end before the data");
    assert!(matches!(decode(&with(&[0xFF, 0xDD, 0x00, 0x05, 0, 0, 0]), &Limits::default()), Err(Error::Marker(_))), "a restart interval of three bytes");
    assert!(matches!(decode(&with(&[0xFF, 0xDC, 0x00, 0x04, 0, 1]), &Limits::default()), Err(Error::Unsupported(_))), "the height defined later");
    assert_eq!(decode(&with(&[0x12, 0x34]), &Limits::default()), Err(Error::NotAnImage), "the third byte of a JPEG is the start of a marker");
    let mut late = jpeg.clone();
    late.splice(4..4, [0x12, 0x34]); // inside the first segment's length: the segment is read wrongly or the marker after it is not one
    assert!(decode(&late, &Limits::default()).is_err());
    assert!(matches!(decode(&with(&[0xFF, 0x00]), &Limits::default()), Err(Error::Marker(_))), "FF 00 is no marker");
    assert!(matches!(decode(&with(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 63, 0]), &Limits::default()), Err(Error::Marker(_))), "a scan before the frame header: the frame is behind it");
    let mut twice = jpeg.clone();
    let sof = twice.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    let frame: Vec<u8> = twice[sof..sof + 2 + 11].to_vec();
    twice.splice(sof..sof, frame);
    assert!(matches!(decode(&twice, &Limits::default()), Err(Error::Marker(w)) if w.contains("second frame")), "{:?}", decode(&twice, &Limits::default()));
    // comments and application data are skipped, fill bytes before a marker too
    let ok = with(&[0xFF, 0xFE, 0x00, 0x04, b'h', b'i', 0xFF, 0xFF, 0xFF, 0xEE, 0x00, 0x03, 0x00]);
    assert_eq!(decode(&ok, &Limits::default()), decode(&jpeg, &Limits::default()));
}

#[test]
fn a_table_of_quantisers_or_of_codes_that_is_cut_short_or_unknown_is_an_error() {
    assert!(matches!(decode(&patched("grey_8x8", 0xDB, 4, 0x04), &Limits::default()), Err(Error::Marker(_))), "a table of quantisers 4");
    assert!(matches!(decode(&patched("grey_8x8", 0xDB, 4, 0x20), &Limits::default()), Err(Error::Marker(_))), "a precision of 2");
    assert!(matches!(decode(&patched("grey_8x8", 0xC4, 4, 0x20), &Limits::default()), Err(Error::Marker(_))), "a class of 2");
    assert!(matches!(decode(&patched("grey_8x8", 0xC4, 4, 0x04), &Limits::default()), Err(Error::Marker(_))), "a table of codes 4");
    // the segment of a table of codes claims to be shorter than its 16 counts
    let mut short = fixture("grey_8x8");
    let at = short.windows(2).position(|w| w == [0xFF, 0xC4]).unwrap();
    short[at + 3] = 10;
    assert!(decode(&short, &Limits::default()).is_err());
}

// ---- what a first set of mutants showed the tests did not say -------------------------------------------------------------------------------------

#[test]
fn a_dc_size_of_eleven_is_read_and_one_of_twelve_is_refused_even_when_the_rest_is_in_order() {
    let (_, ac) = two_symbol_tables();
    let mut c = [0u8; 16];
    c[0] = 2;
    // symbol 11 is the code `1`. bits: DC `1`, then eleven bits 10000000000 (= 1024), then the end of the block `0`: 1100 0000 | 0000 0111
    let r = decode(&tiny(8, 8, &dht(0x00, &c, &[0, 11]), &ac, &[0xC0, 0x07]), &Limits::default()).unwrap();
    assert!(r.pixels.iter().all(|p| *p == 0xFFFF_FFFF), "128 + 128 clamps to white: {:08x?}", &r.pixels[..2]);
    // symbol 12 is the code `1`: DC `1`, twelve bits 100000000000, the end `0`; everything else is valid, so only the size can be the fault: 1100 0000 | 0000 0011
    let r = decode(&tiny(8, 8, &dht(0x00, &c, &[0, 12]), &ac, &[0xC0, 0x03]), &Limits::default());
    assert!(matches!(r, Err(Error::Huffman(w)) if w.contains("DC")), "{r:?}");
}

#[test]
fn a_symbol_with_no_size_and_a_run_other_than_fifteen_ends_the_block() {
    // AC table: `0` is (run 14, size 0), `1` is (run 0, size 1). The block: DC `0`, then `0`: the end of the block. Padding bits (ones) follow, and they must not be read as coefficients.
    let mut c = [0u8; 16];
    c[0] = 2;
    let ac = dht(0x10, &c, &[0xE0, 0x01]);
    let (dc, _) = two_symbol_tables();
    let image = decode(&tiny(8, 8, &dc, &ac, &[0x3F]), &Limits::default()).unwrap();
    assert!(image.pixels.iter().all(|p| *p == 0xFF80_8080), "{:08x?}", &image.pixels[..2]);
}

#[test]
fn the_dc_table_of_a_component_is_the_one_its_scan_names_and_not_its_ac_table() {
    // DC table 0: `0` is size 0, `1` is size 7. DC table 1: `0` is size 7, `1` is size 0. AC table 0: `0` is the end. The scan names DC table 1 and AC table 0 (the byte 0x10).
    let (dc0, ac) = two_symbol_tables();
    let mut c = [0u8; 16];
    c[0] = 2;
    let mut dc = dc0;
    dc.extend(dht(0x01, &c, &[7, 0]));
    let mut jpeg = tiny(8, 8, &dc, &ac, &[0x40, 0x7F]); // bits: DC `0` (size 7), 1000000 (= 64), the end `0`, padding
    let sos = jpeg.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    assert_eq!(jpeg[sos + 6], 0x00);
    jpeg[sos + 6] = 0x10;
    let image = decode(&jpeg, &Limits::default()).unwrap();
    assert!(image.pixels.iter().all(|p| *p == 0xFF88_8888), "{:08x?}", &image.pixels[..2]);
}

#[test]
fn a_grey_image_is_scanned_block_by_block_whatever_sampling_it_names() {
    let plain = decode(&fixture("grey_61x37"), &Limits::default()).unwrap();
    for factors in [0x22, 0x21, 0x12, 0x44] {
        assert_eq!(decode(&patched("grey_61x37", 0xC0, 11, factors), &Limits::default()).unwrap(), plain, "factors {factors:02x}");
    }
}

#[test]
fn sampling_factors_out_of_range_repeated_or_not_dividing_vertically_are_refused() {
    let with = |ids_and_factors: [(u8, u8, u8); 3]| {
        let mut jpeg = fixture("odd_61x37_444");
        let at = jpeg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        for (i, (id, h, v)) in ids_and_factors.iter().enumerate() {
            jpeg[at + 10 + 3 * i] = *id;
            jpeg[at + 11 + 3 * i] = h << 4 | v;
        }
        jpeg
    };
    let ok = [(1, 1, 1), (2, 1, 1), (3, 1, 1)];
    assert!(header(&with(ok)).is_ok());
    assert!(matches!(header(&with([(1, 5, 1), (2, 1, 1), (3, 1, 1)])), Err(Error::Marker(w)) if w.contains("out of range")), "a horizontal factor of 5");
    assert!(matches!(header(&with([(1, 1, 3), (2, 1, 2), (3, 1, 2)])), Err(Error::Unsupported(w)) if w.contains("divide")), "3 and 2 do not divide vertically");
    assert!(matches!(header(&with([(1, 1, 2), (2, 1, 2), (3, 1, 1)])), Ok(_)), "2 and 1 do");
    assert!(matches!(header(&with([(1, 1, 1), (1, 1, 1), (3, 1, 1)])), Err(Error::Marker(w)) if w.contains("one number")), "two components with one number");
    assert!(matches!(header(&with([(1, 1, 1), (2, 1, 1), (2, 1, 1)])), Err(Error::Marker(w)) if w.contains("one number")), "the second and the third");
}

/// The jpeg with the segment of its scan header replaced by one with the given body.
fn with_scan_body(name: &str, body: &[u8]) -> Vec<u8> {
    let mut jpeg = fixture(name);
    let at = jpeg.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    let old = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
    let mut segment = vec![0xFF, 0xDA];
    segment.extend(((body.len() + 2) as u16).to_be_bytes());
    segment.extend(body);
    jpeg.splice(at..at + 2 + old, segment);
    jpeg
}

#[test]
fn a_scan_header_of_the_wrong_size_or_of_the_wrong_number_of_components_is_refused_by_name() {
    let mut body = vec![3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0];
    assert!(decode(&with_scan_body("odd_61x37_444", &body), &Limits::default()).is_ok(), "the header as it should be");
    body.push(0);
    let r = decode(&with_scan_body("odd_61x37_444", &body), &Limits::default());
    assert!(matches!(&r, Err(Error::Marker(w)) if w.contains("wrong length")), "one byte too many: {r:?}");
    let r = decode(&with_scan_body("odd_61x37_444", &[2, 1, 0x00, 2, 0x11, 0, 63, 0]), &Limits::default());
    assert!(matches!(&r, Err(Error::Unsupported(w)) if w.contains("only some")), "two of three components, the header whole: {r:?}");
    let r = decode(&with_scan_body("odd_61x37_444", &[4, 1, 0x00, 2, 0x11, 3, 0x11, 3, 0x11, 0, 63, 0]), &Limits::default());
    assert!(matches!(&r, Err(Error::Unsupported(w)) if w.contains("only some")), "four of three: {r:?}");
    let r = decode(&with_scan_body("odd_61x37_444", &[0, 0, 63, 0]), &Limits::default());
    assert!(matches!(&r, Err(Error::Marker(w)) if w.contains("wrong length")), "none: {r:?}");
}

#[test]
fn short_segments_and_unknown_kinds_of_table_are_refused_by_name() {
    let jpeg = fixture("grey_8x8");
    let with = |bytes: &[u8]| {
        let mut j = jpeg.clone();
        j.splice(2..2, bytes.iter().copied());
        j
    };
    for length in [0u8, 1] {
        let r = decode(&with(&[0xFF, 0xE0, 0x00, length]), &Limits::default());
        assert!(matches!(&r, Err(Error::Marker(w)) if w.contains("shorter")), "a segment of length {length}: {r:?}");
    }
    let r = decode(&with(&[0xFF, 0xDD, 0x00, 0x03, 0x00]), &Limits::default());
    assert!(matches!(&r, Err(Error::Marker(w)) if w.contains("restart")), "a restart interval of one byte: {r:?}");
    let r = decode(&with(&[0xFF, 0xDD, 0x00, 0x04, 0x00, 0x04]), &Limits::default());
    assert!(r.is_ok(), "an interval of 4 that no marker follows: {r:?}");
    let r = decode(&with(&[0xFF, 0xCC, 0x00, 0x04, 0, 0]), &Limits::default());
    assert!(matches!(&r, Err(Error::Unsupported(w)) if w.contains("arithmetic")), "the conditioning of arithmetic coding: {r:?}");
    // a table of quantisers of precision 2 and a table of codes of class 2, whose bodies are as long as a valid table's
    let mut dqt = vec![0x20u8];
    dqt.extend([1u8; 128]);
    let r = decode(&with(&[&[0xFF, 0xDB, 0x00, 0x83][..], &dqt].concat()), &Limits::default());
    assert!(matches!(&r, Err(Error::Marker(w)) if w.contains("unknown kind")), "precision 2: {r:?}");
    let mut c = [0u8; 16];
    c[0] = 1;
    let table = dht(0x20, &c, &[0]);
    let r = decode(&with(&[&[0xFF, 0xC4, 0x00, 0x14][..], &table].concat()), &Limits::default());
    assert!(matches!(&r, Err(Error::Marker(w)) if w.contains("unknown kind")), "class 2: {r:?}");
}

#[test]
fn the_header_alone_needs_nothing_after_the_frame_header() {
    let jpeg = fixture("odd_61x37_444");
    let sof = jpeg.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    let end = sof + 2 + u16::from_be_bytes([jpeg[sof + 2], jpeg[sof + 3]]) as usize;
    let h = header(&jpeg[..end]).unwrap();
    assert_eq!((h.width, h.height, h.format), (61, 37, Format::Jpeg));
    assert!(decode(&jpeg[..end], &Limits::default()).is_err(), "but there is no image behind it");
}

// ---- the colour conversion, every step of it ------------------------------------------------------------------------------------------------------

/// A JPEG of 2048 x 48 pixels in 4:4:4 whose 6 rows of 256 blocks of 8 x 8 are flat: in each row one of Cb and Cr runs through 0..=255 along the row, the other is 128, and the luma is 128, 40 or 216.
/// The conversion to RGB is then tried at every value of each chroma component (with a hand-written encoder: DC only, quantisers of 1, so a block is its level exactly).
fn colour_ramps() -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    put_segment(&mut out, 0xDB, &[&[0u8][..], &[1u8; 64]].concat());
    let mut sof = vec![8u8];
    sof.extend(48u16.to_be_bytes());
    sof.extend(2048u16.to_be_bytes());
    sof.extend([3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
    put_segment(&mut out, 0xC0, &sof);
    let mut c = [0u8; 16];
    c[3] = 12; // the codes 0000 to 1011 are the sizes 0 to 11
    put_segment(&mut out, 0xC4, &dht(0x00, &c, &(0..12).collect::<Vec<u8>>()));
    c = [0; 16];
    c[0] = 1;
    put_segment(&mut out, 0xC4, &dht(0x10, &c, &[0]));
    put_segment(&mut out, 0xDA, &[3, 1, 0x00, 2, 0x00, 3, 0x00, 0, 63, 0]);
    let mut bits: Vec<bool> = Vec::new();
    let mut put = |value: u32, count: u32| (0..count).rev().for_each(|i| bits.push(value >> i & 1 == 1));
    let mut previous = [0i32; 3];
    for row in 0..6 {
        for x in 0..256i32 {
            let luma = [128, 128, 40, 40, 216, 216][row];
            let level = if row % 2 == 0 { [luma, 128, x] } else { [luma, x, 128] };
            for (c, l) in level.iter().enumerate() {
                let dc = (l - 128) * 8;
                let diff = dc - previous[c];
                previous[c] = dc;
                let size = 32 - (diff.unsigned_abs()).leading_zeros();
                put(size, 4);
                if size > 0 {
                    put(if diff > 0 { diff as u32 } else { (diff + (1 << size) - 1) as u32 }, size);
                }
                put(0, 1);
            }
        }
    }
    while bits.len() % 8 != 0 {
        bits.push(true);
    }
    for byte in bits.chunks(8).map(|b| b.iter().fold(0u8, |a, bit| a << 1 | *bit as u8)) {
        out.push(byte);
        if byte == 0xFF {
            out.push(0);
        }
    }
    out.extend([0xFF, 0xD9]);
    out
}

#[test]
fn the_colour_conversion_is_libjpegs_at_every_value_of_each_chroma_component() {
    // the FNV of libjpeg's own decode (`djpeg -dct int -nosmooth`) of these bytes: every constant of the conversion matters at some value of Cb or Cr
    let jpeg = colour_ramps();
    if let Ok(path) = std::env::var("LICO_DUMP_RAMPS") {
        std::fs::write(path, &jpeg).unwrap();
    }
    let image = decode(&jpeg, &Limits::default()).unwrap();
    assert_eq!((image.width, image.height), (2048, 48));
    assert_eq!(fnv(&image), RAMPS_FNV, "{:016x}", fnv(&image));
}

const RAMPS_FNV: u64 = 0xe03eb3e117a99fe5;

// ---- never a panic ------------------------------------------------------------------------------------------------------------------------------------

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn every_prefix_of_a_jpeg_is_an_error_and_never_a_panic() {
    for name in ["grey_8x8", "one_pixel", "odd_61x37_420", "restart_blocks_96x48_444", "moli_320x200_q60_420"] {
        let jpeg = fixture(name);
        // the small files at every length, the big one at every 41st and near both ends
        let step = if jpeg.len() > 4000 { 41 } else { 1 };
        let mut cuts: Vec<usize> = (0..jpeg.len()).step_by(step).collect();
        cuts.extend(jpeg.len().saturating_sub(64)..jpeg.len());
        for cut in cuts {
            assert!(decode(&jpeg[..cut], &Limits::default()).is_err(), "{name}: the first {cut} of {} bytes", jpeg.len());
            let _ = header(&jpeg[..cut]);
        }
    }
}

#[test]
fn every_single_bit_flip_gives_an_error_or_an_image() {
    for (name, step) in [("grey_8x8", 1), ("one_pixel", 1), ("restart_rows_96x48_420", 3), ("odd_61x37_422", 5)] {
        let jpeg = fixture(name);
        for byte in (0..jpeg.len()).step_by(step) {
            for bit in 0..8 {
                let mut bad = jpeg.clone();
                bad[byte] ^= 1 << bit;
                match decode(&bad, &Limits { max_input: 1 << 20, max_pixels: 1 << 20 }) {
                    Ok(image) => assert_eq!(image.pixels.len(), (image.width * image.height) as usize),
                    Err(_) => {}
                }
            }
        }
    }
}

#[test]
fn every_value_of_every_byte_of_the_headers_gives_an_error_or_an_image() {
    // the bytes up to the start of the scan of a small file, each set to each of 256 values: sizes, factors, tables, lengths
    let jpeg = fixture("grey_8x8");
    let scan = jpeg.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    for byte in 0..scan + 12 {
        for value in 0..=255u8 {
            let mut bad = jpeg.clone();
            bad[byte] = value;
            let _ = decode(&bad, &Limits { max_input: 1 << 20, max_pixels: 1 << 20 });
        }
    }
}

#[test]
fn random_bytes_and_random_scans_never_panic() {
    let mut rng = Xorshift(0x9E37_79B9_7F4A_7C15);
    for _ in 0..3000 {
        let len = (rng.next() % 300) as usize;
        let mut bytes = vec![0xFF, 0xD8, 0xFF];
        bytes.extend((0..len).map(|_| rng.next() as u8));
        let _ = decode(&bytes, &Limits::default());
    }
    // a good header with random entropy-coded data, and a good header with data made of ones and escapes
    let good = fixture("odd_61x37_420");
    let scan = good.windows(2).position(|w| w == [0xFF, 0xDA]).unwrap();
    let head_end = scan + 2 + u16::from_be_bytes([good[scan + 2], good[scan + 3]]) as usize;
    for round in 0..3000 {
        let mut bytes = good[..head_end].to_vec();
        let len = (rng.next() % 2500) as usize;
        for _ in 0..len {
            let b = rng.next() as u8;
            bytes.push(b);
            if b == 0xFF {
                bytes.push(if round % 3 == 0 { 0x00 } else { rng.next() as u8 });
            }
        }
        bytes.extend([0xFF, 0xD9]);
        match decode(&bytes, &Limits::default()) {
            Ok(image) => assert_eq!((image.width, image.height), (61, 37)),
            Err(_) => {}
        }
    }
}
