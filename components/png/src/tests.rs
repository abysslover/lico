//! The white-box proofs of the writer: the check functions against vectors, the deflate alone, the filters, and the test decoder shown able
//! to fail. They know the inside of the crate and are not its contract (the contract is `tests/contract.rs`).

use crate::testing::*;
use crate::*;

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
