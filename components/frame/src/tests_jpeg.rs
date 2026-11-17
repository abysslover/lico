//! Tests of the parts of the JPEG decoder that the contract tests (tests/jpeg.rs) reach only through whole files: the Huffman tables, the bit reader and the inverse DCT.

use crate::huffman::{Bits, Table};
use crate::idct;
use crate::Error;

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// ---- the inverse DCT --------------------------------------------------------------------------------------------------------------------------------

/// The textbook inverse DCT in floating point: f(x, y) = 1/4 * sum C(u) C(v) F(u, v) cos((2x + 1) u pi / 16) cos((2y + 1) v pi / 16), plus the level shift of 128, clamped and rounded.
fn float_idct(coef: &[i32; 64]) -> [u8; 64] {
    let mut out = [0u8; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0.0f64;
            for v in 0..8 {
                for u in 0..8 {
                    let c = |k: usize| if k == 0 { std::f64::consts::FRAC_1_SQRT_2 } else { 1.0 };
                    sum += c(u) * c(v) * coef[v * 8 + u] as f64 * (((2 * x + 1) * u) as f64 * std::f64::consts::PI / 16.0).cos() * (((2 * y + 1) * v) as f64 * std::f64::consts::PI / 16.0).cos();
                }
            }
            out[y * 8 + x] = (sum / 4.0 + 128.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn integer_idct(coef: &[i32; 64], dc_only: bool) -> [u8; 64] {
    let mut out = [0u8; 64];
    idct::block(coef, dc_only, &mut out, 8);
    out
}

#[test]
fn the_integer_idct_is_within_one_of_the_textbook_one() {
    let mut rng = Xorshift(0x1234_5678_9ABC_DEF1);
    for round in 0..400 {
        let mut coef = [0i32; 64];
        // a few coefficients of the size a real image has (the low frequencies large, the high ones small)
        let count = 1 + (rng.next() % 12) as usize;
        for _ in 0..count {
            let k = (rng.next() % 64) as usize;
            let reach = if k < 10 { 600 } else { 80 };
            coef[k] = (rng.next() % (2 * reach + 1)) as i32 - reach as i32;
        }
        if round % 5 == 0 {
            coef = [0; 64];
            coef[0] = (rng.next() % 2000) as i32 - 1000;
        }
        let (a, b) = (integer_idct(&coef, false), float_idct(&coef));
        for i in 0..64 {
            assert!((a[i] as i32 - b[i] as i32).abs() <= 1, "round {round} sample {i}: {} against {}", a[i], b[i]);
        }
    }
}

#[test]
fn the_one_step_of_a_block_with_only_its_first_coefficient_is_the_general_arithmetic() {
    for dc in (-3000..3000).step_by(7) {
        let mut coef = [0i32; 64];
        coef[0] = dc;
        assert_eq!(integer_idct(&coef, true), integer_idct(&coef, false), "dc {dc}");
    }
}

#[test]
fn a_block_of_absurd_coefficients_is_a_picture_and_never_a_panic() {
    let mut rng = Xorshift(77);
    for _ in 0..200 {
        let mut coef = [0i32; 64];
        for c in coef.iter_mut() {
            *c = match rng.next() % 4 {
                0 => 1 << 24,
                1 => -(1 << 24),
                2 => (rng.next() % 100_000) as i32 - 50_000,
                _ => 0,
            };
        }
        let _ = integer_idct(&coef, false);
    }
}

// ---- the tables of codes ----------------------------------------------------------------------------------------------------------------------------

/// A table with one code of each length from 1 to 16: `0`, `10`, `110`, ... 1111111111111110; the symbol of the code of length `l` is `l`.
fn ladder() -> Table {
    let mut counts = [0u8; 16];
    counts.iter_mut().for_each(|c| *c = 1);
    let symbols: Vec<u8> = (1..=16).collect();
    Table::build(&counts, &symbols).unwrap()
}

/// The bits of the symbols of the ladder, as bytes padded with ones, with every `FF` escaped.
fn ladder_stream(symbols: &[u8]) -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    for s in symbols {
        bits.extend(std::iter::repeat(true).take(*s as usize - 1));
        bits.push(false);
    }
    while bits.len() % 8 != 0 {
        bits.push(true);
    }
    let mut out = Vec::new();
    for chunk in bits.chunks(8) {
        let byte = chunk.iter().fold(0u8, |a, b| a << 1 | *b as u8);
        out.push(byte);
        if byte == 0xFF {
            out.push(0);
        }
    }
    out
}

#[test]
fn codes_of_every_length_from_one_to_sixteen_decode_through_the_lookup_and_the_comparison() {
    let table = ladder();
    let order: Vec<u8> = vec![1, 16, 2, 15, 3, 14, 9, 10, 8, 11, 16, 16, 1, 1, 12, 13, 4, 5, 6, 7];
    let data = ladder_stream(&order);
    let mut bits = Bits::new(&data, 0);
    for s in &order {
        assert_eq!(bits.symbol(&table), Ok(*s));
    }
    assert!(!bits.overrun());
}

#[test]
fn the_unused_code_space_is_an_error_when_a_stream_reaches_it() {
    // the ladder leaves 1111111111111111 unassigned
    let table = ladder();
    let data = [0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00];
    let mut bits = Bits::new(&data, 0);
    assert!(matches!(bits.symbol(&table), Err(Error::Huffman(_))));
}

#[test]
fn a_table_must_agree_with_itself() {
    let mut counts = [0u8; 16];
    counts[1] = 3;
    assert!(Table::build(&counts, &[1, 2]).is_err(), "three codes announced, two symbols given");
    assert!(Table::build(&counts, &[1, 2, 3, 4]).is_err(), "three codes announced, four symbols given");
    assert!(Table::build(&counts, &[1, 2, 3]).is_ok(), "three codes of two bits fit in the four there are");
    counts[1] = 5;
    assert!(Table::build(&counts, &[1, 2, 3, 4, 5]).is_err(), "five codes of two bits do not");
    let mut full = [0u8; 16];
    full[15] = 255;
    assert!(Table::build(&full, &[0u8; 255]).is_ok());
    full[14] = 1;
    assert!(Table::build(&full, &[0u8; 256]).is_ok(), "256 symbols are the most a table has");
    full[14] = 2;
    assert!(Table::build(&full, &[0u8; 257]).is_err(), "257 symbols, though the codes would fit");
}

// ---- the bit reader -----------------------------------------------------------------------------------------------------------------------------------

#[test]
fn an_escaped_ff_is_a_byte_fill_is_skipped_and_a_marker_ends_the_data() {
    // the byte FF (escaped), the byte 12, a fill byte FF, and the marker FF D9
    let data = [0xFF, 0x00, 0x12, 0xFF, 0xFF, 0xD9];
    let mut bits = Bits::new(&data, 0);
    assert_eq!(bits.receive(8), 0xFF, "FF 00 is the byte FF");
    assert_eq!(bits.receive(8), 0x12);
    assert!(!bits.overrun());
    bits.receive(1);
    assert!(bits.overrun(), "past the marker only padding is given");
    assert_eq!(bits.find_marker(), Some(0xD9), "the fill byte is skipped");
    assert_eq!(bits.position(), 4, "the marker is not taken");
}

#[test]
fn the_end_of_the_data_is_padding_too() {
    let data = [0xAB];
    let mut bits = Bits::new(&data, 0);
    assert_eq!(bits.receive(8), 0xAB);
    assert!(!bits.overrun());
    assert_eq!(bits.receive(5), 0);
    assert!(bits.overrun());
    assert_eq!(bits.find_marker(), None);
}

#[test]
fn finding_a_marker_skips_what_is_before_it_and_the_escaped_bytes() {
    let data = [1, 2, 3, 0xFF, 0x00, 4, 0xFF, 0xFF, 0xD3, 9];
    let mut bits = Bits::new(&data, 0);
    assert_eq!(bits.find_marker(), Some(0xD3));
    assert_eq!(bits.position(), 7);
    bits.skip_marker();
    assert_eq!(bits.position(), 9);
    assert_eq!(bits.receive(8), 9, "the bits go on after the marker");
    // a marker whose first byte comes right after a skipped byte
    let data = [7, 0xFF, 0xD1];
    let mut bits = Bits::new(&data, 0);
    assert_eq!(bits.find_marker(), Some(0xD1));
    assert_eq!(bits.position(), 1);
}
