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
