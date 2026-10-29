//! A PNG writer of our own: 8-bit RGB, no interlace, written row by row to any `Write`.
//!
//! No crate: the lock file has no deflate or PNG library and none is added (plan M02-P04, deviation D2). The picture goes out as it is
//! pushed: each row gets one of the five PNG filters (the one with the smallest sum of absolute values), the filtered bytes go through a
//! deflate of our own (LZ77 over a 32 KiB window with a hash chain, the fixed Huffman codes, and a stored block whenever a block would grow),
//! and the compressed bytes leave in IDAT chunks of at most `IDAT_MAX` bytes as soon as there are that many. The memory is a fixed set of
//! buffers whose size depends on the width and not on the height (`buffer_bytes`). There is no global state: two encoders can run side by side.
//!
//! Wrong use is an `Err` of kind `InvalidInput` and never a panic; a write error of the sink comes back as it is, and the encoder refuses
//! everything after it, so it never goes on to write a file that has a hole in it.

// The first caller is S02b (the screen read and `lico capture`); S02b removes this line.
#![allow(dead_code)]

use std::io::{self, Write};

#[cfg(test)]
mod readers;
#[cfg(test)]
mod tests;

/// The largest width or height a PNG may have.
pub const MAX_SIDE: u32 = 0x7FFF_FFFF;
/// The most compressed bytes in one IDAT chunk.
pub const IDAT_MAX: usize = 32768;
/// How far back a match may reach.
const WINDOW: usize = 32768;
/// The input of one deflate block: the most a stored block holds.
const BLOCK: usize = 65535;
/// How many earlier positions with the same hash are looked at for one match.
const MAX_CHAIN: usize = 48;
const HASH_BITS: u32 = 15;
const NIL: u32 = u32::MAX;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// A match of three bytes this far back costs about as much as the three bytes.
const TOO_FAR: usize = 4096;
/// Bytes of one pixel.
const BPP: usize = 3;
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 (the PNG chunk check), fed in pieces.
#[derive(Clone, Copy)]
pub struct Crc32(u32);

impl Crc32 {
    pub fn new() -> Crc32 {
        Crc32(!0)
    }
    pub fn update(&mut self, data: &[u8]) {
        let mut c = self.0;
        for &b in data {
            c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = c;
    }
    pub fn value(&self) -> u32 {
        !self.0
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(data);
    c.value()
}

/// Adler-32 (the zlib check), fed in pieces.
#[derive(Clone, Copy)]
pub struct Adler32 {
    a: u32,
    b: u32,
}

impl Adler32 {
    pub fn new() -> Adler32 {
        Adler32 { a: 1, b: 0 }
    }
    pub fn update(&mut self, data: &[u8]) {
        // 5552 is the most bytes after which the sums cannot yet overflow 32 bits; the modulo is taken at the end of every such piece.
        for piece in data.chunks(5552) {
            for &x in piece {
                self.a += x as u32;
                self.b += self.a;
            }
            self.a %= 65521;
            self.b %= 65521;
        }
    }
    pub fn value(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a = Adler32::new();
    a.update(data);
    a.value()
}

/// The bits of a deflate stream not yet a whole byte; the bytes go to the `Vec` given.
#[derive(Clone, Copy)]
struct Bits {
    acc: u64,
    n: u32,
}

impl Bits {
    /// `n` bits of `v`, the lowest first (the order of the extra bits and of the headers).
    fn put(&mut self, out: &mut Vec<u8>, v: u32, n: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += n;
        while self.n >= 8 {
            out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// A Huffman code of `n` bits: its highest bit goes first.
    fn code(&mut self, out: &mut Vec<u8>, c: u32, n: u32) {
        self.put(out, c.reverse_bits() >> (32 - n), n);
    }
    fn align(&mut self, out: &mut Vec<u8>) {
        if self.n > 0 {
            out.push(self.acc as u8);
            self.acc = 0;
            self.n = 0;
        }
    }
}

/// The fixed Huffman code of a literal or length symbol: the code and its length.
fn fixed_code(sym: u32) -> (u32, u32) {
    match sym {
        0..=143 => (0x30 + sym, 8),
        144..=255 => (0x190 + sym - 144, 9),
        256..=279 => (sym - 256, 7),
        _ => (0xC0 + sym - 280, 8),
    }
}

/// The index of the last entry of `base` that is at most `v`.
fn code_of(base: &[u16], v: usize) -> usize {
    base.partition_point(|&b| b as usize <= v) - 1
}

fn put_literal(bits: &mut Bits, out: &mut Vec<u8>, b: u8) {
    let (c, n) = fixed_code(b as u32);
    bits.code(out, c, n);
}

fn put_match(bits: &mut Bits, out: &mut Vec<u8>, len: usize, dist: usize) {
    let lc = code_of(&LEN_BASE, len);
    let (c, n) = fixed_code(257 + lc as u32);
    bits.code(out, c, n);
    bits.put(out, (len - LEN_BASE[lc] as usize) as u32, LEN_EXTRA[lc] as u32);
    let dc = code_of(&DIST_BASE, dist);
    bits.code(out, dc as u32, 5);
    bits.put(out, (dist - DIST_BASE[dc] as usize) as u32, DIST_EXTRA[dc] as u32);
}

fn hash(b: &[u8]) -> usize {
    let v = b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16;
    (v.wrapping_mul(0x1E35_A7BD) >> (32 - HASH_BITS)) as usize
}

/// The zlib stream: the input is collected in blocks of `BLOCK` bytes, each compressed against the 32 KiB before it.
struct Deflate {
    /// The window (up to `WINDOW` bytes of earlier input) followed by the input of the block being collected.
    buf: Vec<u8>,
    /// How much of `buf` is window.
    hist: usize,
    /// The newest position in `buf` for each hash, and for each position the one before it with the same hash.
    head: Vec<u32>,
    prev: Vec<u32>,
    /// The next position to put into the hash chains.
    ins: usize,
    /// The block compressed with fixed codes, kept apart until it is known to be smaller than a stored block.
    trial: Vec<u8>,
    bits: Bits,
    adler: Adler32,
    /// The compressed bytes that have not left yet.
    out: Vec<u8>,
}

impl Deflate {
    fn new() -> Deflate {
        let mut out = Vec::with_capacity(IDAT_MAX + BLOCK + 8);
        // CMF 0x78: deflate with a 32 KiB window; FLG 0x01: the lowest level, and the check bits that make the pair a multiple of 31.
        out.extend_from_slice(&[0x78, 0x01]);
        Deflate {
            buf: Vec::with_capacity(WINDOW + BLOCK),
            hist: 0,
            head: vec![NIL; 1 << HASH_BITS],
            prev: vec![NIL; WINDOW + BLOCK],
            ins: 0,
            trial: Vec::with_capacity(BLOCK / 8 * 9 + 64),
            bits: Bits { acc: 0, n: 0 },
            adler: Adler32::new(),
            out,
        }
    }

    fn buffer_bytes(&self) -> usize {
        self.buf.capacity() + 4 * (self.head.capacity() + self.prev.capacity()) + self.trial.capacity() + self.out.capacity()
    }

    fn feed(&mut self, mut data: &[u8]) {
        self.adler.update(data);
        while !data.is_empty() {
            let take = data.len().min(BLOCK - (self.buf.len() - self.hist));
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() - self.hist == BLOCK {
                self.block(false);
            }
        }
    }

    /// Ends the stream: the last block, the padding to a byte and the Adler-32.
    fn finish(&mut self) {
        self.block(true);
        self.bits.align(&mut self.out);
        let sum = self.adler.value().to_be_bytes();
        self.out.extend_from_slice(&sum);
    }

    fn insert_upto(&mut self, upto: usize, end: usize) {
        while self.ins < upto && self.ins + 2 < end {
            let h = hash(&self.buf[self.ins..]);
            self.prev[self.ins] = self.head[h];
            self.head[h] = self.ins as u32;
            self.ins += 1;
        }
    }

    /// The longest match for the input at `i` among the earlier positions of the chain, as (length, distance); length 0 for none.
    fn find(&self, i: usize, end: usize) -> (usize, usize) {
        let max = (end - i).min(MAX_MATCH);
        if max < MIN_MATCH {
            return (0, 0);
        }
        let mut cand = self.head[hash(&self.buf[i..])];
        let mut chain = MAX_CHAIN;
        let mut best = (0, 0);
        while cand != NIL && chain > 0 {
            let c = cand as usize;
            let dist = i - c;
            if dist > WINDOW {
                break;
            }
            let mut l = 0;
            while l < max && self.buf[c + l] == self.buf[i + l] {
                l += 1;
            }
            if l > best.0 {
                best = (l, dist);
                if l == max {
                    break;
                }
            }
            cand = self.prev[c];
            chain -= 1;
        }
        match best {
            (l, _) if l < MIN_MATCH => (0, 0),
            (MIN_MATCH, d) if d > TOO_FAR => (0, 0),
            b => b,
        }
    }

    /// Compresses the collected input as one block: with the fixed codes, or stored when that is not smaller.
    fn block(&mut self, last: bool) {
        let start = self.hist;
        let end = self.buf.len();
        let n = end - start;
        let mut bits = self.bits;
        let mut trial = std::mem::take(&mut self.trial);
        trial.clear();
        bits.put(&mut trial, last as u32, 1);
        bits.put(&mut trial, 1, 2);
        let mut i = start;
        while i < end {
            self.insert_upto(i, end);
            let (len, dist) = self.find(i, end);
            if len == 0 {
                put_literal(&mut bits, &mut trial, self.buf[i]);
                i += 1;
            } else {
                put_match(&mut bits, &mut trial, len, dist);
                i += len;
            }
        }
        self.insert_upto(end, end);
        let (c, k) = fixed_code(256);
        bits.code(&mut trial, c, k);
        if n > 0 && trial.len() + (bits.n > 0) as usize >= n + 5 {
            self.bits.put(&mut self.out, last as u32, 1);
            self.bits.put(&mut self.out, 0, 2);
            self.bits.align(&mut self.out);
            self.out.extend_from_slice(&(n as u16).to_le_bytes());
            self.out.extend_from_slice(&(!(n as u16)).to_le_bytes());
            self.out.extend_from_slice(&self.buf[start..end]);
        } else {
            self.out.extend_from_slice(&trial);
            self.bits = bits;
        }
        self.trial = trial;
        if !last {
            self.slide();
        }
    }

    /// Keeps the last `WINDOW` bytes of the input as the window of the next block.
    fn slide(&mut self) {
        let end = self.buf.len();
        let keep = end.min(WINDOW);
        let shift = end - keep;
        if shift > 0 {
            self.buf.copy_within(shift..end, 0);
            self.buf.truncate(keep);
            self.prev.copy_within(shift..end, 0);
            let shift32 = shift as u32;
            for e in self.prev[..keep].iter_mut().chain(self.head.iter_mut()) {
                *e = if *e != NIL && *e >= shift32 { *e - shift32 } else { NIL };
            }
            self.ins = self.ins.saturating_sub(shift);
        }
        self.hist = keep;
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// The row filtered with filter `kind` (0 None, 1 Sub, 2 Up, 3 Average, 4 Paeth) into `out` (after the filter byte); returns the sum of the
/// absolute values of the filtered bytes taken as signed, the number the choice of the filter goes by. `prev` is the row above (zeros above row 0).
fn filter_into(kind: u8, row: &[u8], prev: &[u8], out: &mut [u8]) -> u64 {
    out[0] = kind;
    let mut sum = 0u64;
    for i in 0..row.len() {
        let x = row[i];
        let (a, c) = if i >= BPP { (row[i - BPP], prev[i - BPP]) } else { (0, 0) };
        let b = prev[i];
        let f = match kind {
            0 => x,
            1 => x.wrapping_sub(a),
            2 => x.wrapping_sub(b),
            3 => x.wrapping_sub(((a as u16 + b as u16) / 2) as u8),
            _ => x.wrapping_sub(paeth(a, b, c)),
        };
        out[i + 1] = f;
        sum += if f < 128 { f as u64 } else { 256 - f as u64 };
    }
    sum
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, what.to_string())
}

/// One chunk, built whole in `scratch` and written with one call.
fn write_chunk<W: Write>(sink: &mut W, scratch: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> io::Result<()> {
    scratch.clear();
    scratch.extend_from_slice(&(data.len() as u32).to_be_bytes());
    scratch.extend_from_slice(kind);
    scratch.extend_from_slice(data);
    let crc = crc32(&scratch[4..]);
    scratch.extend_from_slice(&crc.to_be_bytes());
    sink.write_all(scratch)
}

/// Writes one PNG: `new`, then `push_row` once per row, then `finish`.
pub struct PngEncoder<W: Write> {
    sink: Option<W>,
    row_len: usize,
    height: u32,
    rows: u32,
    prev: Vec<u8>,
    /// The five filtered versions of the row being pushed, each with its filter byte in front.
    cand: [Vec<u8>; 5],
    z: Deflate,
    chunk: Vec<u8>,
    broken: bool,
}

impl<W: Write> PngEncoder<W> {
    /// Starts the file: the signature and the header. `Err` for a width or height of 0 or over `MAX_SIDE`.
    pub fn new(mut sink: W, width: u32, height: u32) -> io::Result<PngEncoder<W>> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return Err(bad("a PNG needs a width and a height from 1 to 2147483647"));
        }
        let row_len = width as usize * BPP;
        let mut chunk = Vec::with_capacity(IDAT_MAX + 12);
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        // 8 bits a sample, colour type 2 (RGB), deflate, filter method 0, no interlace.
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        sink.write_all(&SIGNATURE)?;
        write_chunk(&mut sink, &mut chunk, b"IHDR", &ihdr)?;
        Ok(PngEncoder {
            sink: Some(sink),
            row_len,
            height,
            rows: 0,
            prev: vec![0; row_len],
            cand: std::array::from_fn(|_| vec![0; row_len + 1]),
            z: Deflate::new(),
            chunk,
            broken: false,
        })
    }

    /// The bytes the encoder's own buffers hold (capacities); the same for any height.
    pub fn buffer_bytes(&self) -> usize {
        self.z.buffer_bytes() + self.prev.capacity() + self.cand.iter().map(|c| c.capacity()).sum::<usize>() + self.chunk.capacity()
    }

    fn live(&self) -> io::Result<()> {
        if self.sink.is_none() {
            return Err(bad("the PNG is already finished"));
        }
        if self.broken {
            return Err(bad("an earlier write failed; the file is not complete"));
        }
        Ok(())
    }

    /// Writes the full IDAT chunks of what is compressed so far (`all`: the rest too).
    fn drain(&mut self, all: bool) -> io::Result<()> {
        let Some(sink) = self.sink.as_mut() else { return Err(bad("the PNG is already finished")) };
        let whole = self.z.out.len() / IDAT_MAX * IDAT_MAX;
        let upto = if all { self.z.out.len() } else { whole };
        for piece in self.z.out[..upto].chunks(IDAT_MAX) {
            write_chunk(sink, &mut self.chunk, b"IDAT", piece)?;
        }
        self.z.out.drain(..upto);
        Ok(())
    }

    /// The next row: `3 * width` bytes, red, green, blue of each pixel from the left.
    pub fn push_row(&mut self, row: &[u8]) -> io::Result<()> {
        self.live()?;
        if self.rows >= self.height {
            return Err(bad("more rows than the height"));
        }
        if row.len() != self.row_len {
            return Err(bad("a row must be 3 bytes a pixel"));
        }
        let mut best = (u64::MAX, 0usize);
        for kind in 0..5u8 {
            let sum = filter_into(kind, row, &self.prev, &mut self.cand[kind as usize]);
            if sum < best.0 {
                best = (sum, kind as usize);
            }
        }
        self.z.feed(&self.cand[best.1]);
        self.prev.copy_from_slice(row);
        self.rows += 1;
        self.drain(false).inspect_err(|_| self.broken = true)
    }

    /// Ends the file and gives the sink back; `Err` when fewer rows than the height were pushed. A second call is an `Err`.
    pub fn finish(&mut self) -> io::Result<W> {
        self.live()?;
        if self.rows != self.height {
            return Err(bad("fewer rows than the height"));
        }
        self.z.finish();
        let done = self.drain(true).and_then(|()| {
            let sink = self.sink.as_mut().expect("checked by live");
            write_chunk(sink, &mut self.chunk, b"IEND", &[])?;
            sink.flush()
        });
        match done {
            Ok(()) => Ok(self.sink.take().expect("checked by live")),
            Err(e) => {
                self.broken = true;
                Err(e)
            }
        }
    }
}
