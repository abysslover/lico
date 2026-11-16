//! White-box tests of the parts that are not public: the checksums with their published vectors, and the inflate with its three kinds of block.

use super::*;

#[test]
fn the_checksums_give_the_published_values() {
    assert_eq!(png::crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(png::crc32(b""), 0);
    assert_eq!(inflate::adler32(b"Wikipedia"), 0x11E6_0398);
    assert_eq!(inflate::adler32(b""), 1);
    assert_eq!(inflate::adler32(&vec![255u8; 100_000]), {
        let (mut a, mut b) = (1u32, 0u32);
        for _ in 0..100_000 {
            a = (a + 255) % 65521;
            b = (b + a) % 65521;
        }
        b << 16 | a
    }, "the long input that makes the 16-bit sums wrap");
}

#[test]
fn a_fixed_block_and_a_copy_that_overlaps_itself_are_read() {
    let abc = [0x78, 0xda, 0x4b, 0x4c, 0x4a, 0x4e, 0x84, 0x21, 0x00, 0x1d, 0xe0, 0x04, 0x99];
    assert_eq!(inflate::zlib(&abc, 12).unwrap(), b"abcabcabcabc");
    let a100 = [0x78, 0xda, 0x4b, 0x4c, 0xa4, 0x3d, 0x00, 0x00, 0x7a, 0x47, 0x25, 0xe5];
    assert_eq!(inflate::zlib(&a100, 100).unwrap(), vec![b'a'; 100], "a distance of 1 copies a byte that was just written");
    let empty = [0x78, 0xda, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
    assert_eq!(inflate::zlib(&empty, 0).unwrap(), Vec::<u8>::new());
    assert!(inflate::zlib(&abc, 11).is_err(), "more than the expected size");
    assert!(inflate::zlib(&abc, 13).is_err(), "less than the expected size");
}

#[test]
fn a_distance_that_reaches_before_the_start_is_an_error() {
    // a fixed block: the literal 'a', then a copy of length 3 at distance 5 (only one byte exists)
    let bits: Vec<u8> = {
        let mut w = BitWriter::default();
        w.bits(1, 1); // final
        w.bits(1, 2); // fixed
        w.huff_literal(b'a');
        w.huff_length_code_257(); // length 3
        w.fixed_dist(4, 5, 1); // distance code 4 = 5..6, one extra bit 0 = 5
        w.huff_end();
        w.finish()
    };
    let mut z = vec![0x78, 0x01];
    z.extend(&bits);
    z.extend(inflate::adler32(b"aaaa").to_be_bytes());
    assert!(matches!(inflate::zlib(&z, 4), Err(Error::Inflate(_))));
}

#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl BitWriter {
    fn bits(&mut self, v: u32, count: u32) {
        for i in 0..count {
            self.acc |= ((v >> i) & 1) << self.n;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    /// A Huffman code is sent from its most significant bit.
    fn code(&mut self, code: u32, len: u32) {
        for i in (0..len).rev() {
            self.bits((code >> i) & 1, 1);
        }
    }
    fn huff_literal(&mut self, b: u8) {
        self.code(0x30 + b as u32, 8);
    }
    fn huff_length_code_257(&mut self) {
        self.code(1, 7); // 257: length 3
    }
    fn fixed_dist(&mut self, code: u32, _distance: u32, extra: u32) {
        self.code(code, 5);
        self.bits(extra.saturating_sub(1), 1);
    }
    fn huff_end(&mut self) {
        self.code(0, 7);
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

// ---- the dynamic blocks and the far distances --------------------------------------------------------------------------------------------------------

#[test]
fn a_copy_from_the_far_end_of_the_window_is_read() {
    // 300 bytes, 24,800 zeros, the same 300 bytes: the copy is at a distance of 25,100 (distance code 29 with 13 extra bits)
    let x: Vec<u8> = (0..300u64).map(|j| (((j * j * 31 + j * 7) & 0xFFFF_FFFF) >> 3) as u8).collect();
    let mut want = x.clone();
    want.extend(vec![0u8; 24800]);
    want.extend(&x);
    let z = include_bytes!("../tests/fixtures/far_distance_zlib9.bin");
    assert_eq!(inflate::zlib(z, want.len()).unwrap(), want);
}

/// The header of a dynamic block: final, type 2, `hlit` and `hdist` and `hclen` as the stream stores them, then the code-length code lengths in their order.
fn dynamic_head(w: &mut BitWriter, hlit: u32, hdist: u32, cl_lengths: &[u32]) {
    w.bits(1, 1);
    w.bits(2, 2);
    w.bits(hlit, 5);
    w.bits(hdist, 5);
    w.bits(cl_lengths.len() as u32 - 4, 4);
    for l in cl_lengths {
        w.bits(*l, 3);
    }
}

fn zlib_of(bits: BitWriter, data: &[u8]) -> Vec<u8> {
    let mut z = vec![0x78, 0x01];
    z.extend(bits.finish());
    z.extend(inflate::adler32(data).to_be_bytes());
    z
}

#[test]
fn a_code_length_code_that_is_over_subscribed_or_incomplete_is_refused() {
    // order of the code-length symbols: 16, 17, 18, 0: four codes of length 1 do not fit
    let mut w = BitWriter::default();
    dynamic_head(&mut w, 0, 0, &[1, 1, 1, 1]);
    assert!(matches!(inflate::zlib(&zlib_of(w, b"a"), 1), Err(Error::Inflate(why)) if why.contains("over-subscribed")));
    // lengths 1 and 2 leave a quarter of the codes unused
    let mut w = BitWriter::default();
    dynamic_head(&mut w, 0, 0, &[1, 0, 2, 0]);
    assert!(matches!(inflate::zlib(&zlib_of(w, b"a"), 1), Err(Error::Inflate(why)) if why.contains("incomplete")));
}

#[test]
fn a_repeat_with_nothing_before_it_and_too_many_codes_are_refused() {
    // symbols 16 and 0 of length 1: '0' is the code of 0 and '1' the code of 16; the first length symbol is 16
    let mut w = BitWriter::default();
    dynamic_head(&mut w, 0, 0, &[1, 0, 0, 1]);
    w.bits(1, 1);
    w.bits(0, 2);
    assert!(matches!(inflate::zlib(&zlib_of(w, b"a"), 1), Err(Error::Inflate(why)) if why.contains("nothing before")));
    let mut w = BitWriter::default();
    dynamic_head(&mut w, 31, 0, &[1, 0, 0, 1]); // 288 length codes: two of them do not exist
    assert!(matches!(inflate::zlib(&zlib_of(w, b"a"), 1), Err(Error::Inflate(why)) if why.contains("too many")));
    let mut w = BitWriter::default();
    dynamic_head(&mut w, 0, 31, &[1, 0, 0, 1]); // 32 distance codes: two of them do not exist
    assert!(matches!(inflate::zlib(&zlib_of(w, b"a"), 1), Err(Error::Inflate(why)) if why.contains("too many")));
}

#[test]
fn a_dynamic_block_with_codes_of_all_lengths_up_to_fifteen_is_read() {
    // the literals 97 to 111 get the code lengths 1 to 15 and the end-of-block code a second length of 15: a complete code that uses every length
    let mut lit = [0u32; 257];
    for (i, l) in (1..=15u32).enumerate() {
        lit[97 + i] = l;
    }
    lit[256] = 15;
    let mut w = BitWriter::default();
    // the code-length alphabet: the 16 symbols 0 to 15, each of length 4 (the symbols 16, 17 and 18 unused); their lengths come in the order 16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
    let mut cl = [4u32; 19];
    cl[0] = 0;
    cl[1] = 0;
    cl[2] = 0;
    dynamic_head(&mut w, 0, 0, &cl);
    for l in lit {
        w.code(l, 4); // each symbol 0..=15 has the 4-bit code of its own value
    }
    w.code(0, 4); // the one distance code: no length
    // the code of the literal 111: the first of the two codes of length 15 (fourteen 1s and a 0); then the end of block: fifteen 1s
    for _ in 0..14 {
        w.bits(1, 1);
    }
    w.bits(0, 1);
    for _ in 0..15 {
        w.bits(1, 1);
    }
    assert_eq!(inflate::zlib(&zlib_of(w, &[111]), 1).unwrap(), vec![111]);
}

#[test]
fn a_deflate_bomb_is_stopped_at_the_size_the_header_says_and_not_after_it_has_grown() {
    // 'a', then 400,000 copies of 258 bytes at distance 1 (103 MB if it were let go), for a stream that may give 1,000 bytes
    let mut w = BitWriter::default();
    w.bits(1, 1);
    w.bits(1, 2);
    w.huff_literal(b'a');
    for _ in 0..400_000 {
        w.code(0xC5, 8); // the length code 285: 258
        w.code(0, 5); // the distance code 0: 1
    }
    w.huff_end();
    let bomb = zlib_of(w, b"");
    let t = std::time::Instant::now();
    assert!(matches!(inflate::zlib(&bomb, 1000), Err(Error::Inflate(_))));
    assert!(t.elapsed() < std::time::Duration::from_millis(250), "the bomb was let grow: {:?}", t.elapsed());
}
