//! zlib and deflate (RFC 1950 and 1951), bounded: the caller says how many bytes the stream must give, and the stream may not give one more or one less. Written for a stream
//! that a program outside our control wrote: every table is checked, every distance is checked against what exists, and nothing indexes past what was checked.

use crate::Error;

const LENGTH_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LENGTH_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const CLEN_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn bad(why: &'static str) -> Error {
    Error::Inflate(why)
}

pub(crate) fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        // 5552 bytes at most before the sums can overflow 32 bits
        for x in chunk {
            a += *x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    count: u32,
}

impl<'a> Bits<'a> {
    fn bits(&mut self, need: u32) -> Result<u32, Error> {
        while self.count < need {
            let byte = *self.data.get(self.pos).ok_or_else(|| bad("the stream ends early"))?;
            self.pos += 1;
            self.buf |= (byte as u32) << self.count;
            self.count += 8;
        }
        let v = self.buf & ((1u32 << need) - 1);
        self.buf = if need >= 32 { 0 } else { self.buf >> need };
        self.count -= need;
        Ok(v)
    }
}

struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// The canonical code of these lengths. An over-subscribed code is an error; an incomplete one only when it has a single code (a distance tree of one symbol).
    fn new(lengths: &[u8], fixed_code: bool) -> Result<Huffman, Error> {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        let used = lengths.len() as u32 - count[0] as u32;
        let mut left: i32 = 1;
        for len in 1..16 {
            left <<= 1;
            left -= count[len] as i32;
            if left < 0 {
                return Err(bad("an over-subscribed code"));
            }
        }
        if left > 0 && !fixed_code && used > 1 {
            return Err(bad("an incomplete code"));
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { count, symbol })
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16, Error> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.bits(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return self.symbol.get((index + (code - first)) as usize).copied().ok_or_else(|| bad("a code that is not in the table"));
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(bad("a code that is not in the table"))
    }
}

fn fixed() -> Result<(Huffman, Huffman), Error> {
    let mut l = [0u8; 288];
    for (i, x) in l.iter_mut().enumerate() {
        *x = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Ok((Huffman::new(&l, true)?, Huffman::new(&[5u8; 30], true)?)) // the fixed distance code has 30 of its 32 codes, as the standard says
}

fn dynamic(bits: &mut Bits) -> Result<(Huffman, Huffman), Error> {
    let hlit = bits.bits(5)? as usize + 257;
    let hdist = bits.bits(5)? as usize + 1;
    let hclen = bits.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(bad("too many length or distance codes"));
    }
    let mut cl = [0u8; 19];
    for &i in CLEN_ORDER.iter().take(hclen) {
        cl[i] = bits.bits(3)? as u8;
    }
    let clen = Huffman::new(&cl, false)?;
    let mut lengths = [0u8; 286 + 30];
    let mut n = 0;
    while n < hlit + hdist {
        let sym = clen.decode(bits)?;
        if sym < 16 {
            lengths[n] = sym as u8;
            n += 1;
        } else {
            let (value, repeat) = match sym {
                16 => {
                    if n == 0 {
                        return Err(bad("a repeat with nothing before it"));
                    }
                    (lengths[n - 1], 3 + bits.bits(2)? as usize)
                }
                17 => (0, 3 + bits.bits(3)? as usize),
                _ => (0, 11 + bits.bits(7)? as usize),
            };
            if n + repeat > hlit + hdist {
                return Err(bad("a repeat past the end of the lengths"));
            }
            for _ in 0..repeat {
                lengths[n] = value;
                n += 1;
            }
        }
    }
    if lengths[256] == 0 {
        return Err(bad("no end-of-block code"));
    }
    Ok((Huffman::new(&lengths[..hlit], false)?, Huffman::new(&lengths[hlit..hlit + hdist], false)?))
}

fn codes(bits: &mut Bits, out: &mut Vec<u8>, expected: usize, lit: &Huffman, dist: &Huffman) -> Result<(), Error> {
    loop {
        let sym = lit.decode(bits)? as usize;
        if sym < 256 {
            if out.len() >= expected {
                return Err(bad("more data than the header says"));
            }
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let s = sym - 257;
            if s >= 29 {
                return Err(bad("a length code that does not exist"));
            }
            let len = LENGTH_BASE[s] as usize + bits.bits(LENGTH_EXTRA[s] as u32)? as usize;
            let d = dist.decode(bits)? as usize;
            if d >= 30 {
                return Err(bad("a distance code that does not exist"));
            }
            let distance = DIST_BASE[d] as usize + bits.bits(DIST_EXTRA[d] as u32)? as usize;
            if distance > out.len() {
                return Err(bad("a distance that reaches before the start"));
            }
            if out.len() + len > expected {
                return Err(bad("more data than the header says"));
            }
            let start = out.len() - distance;
            for i in 0..len {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
}

/// The inflated bytes of a zlib stream, which must be exactly `expected` bytes long and end with its Adler-32.
pub(crate) fn zlib(data: &[u8], expected: usize) -> Result<Vec<u8>, Error> {
    if data.len() < 2 {
        return Err(bad("the stream ends at once"));
    }
    let (cmf, flg) = (data[0] as u32, data[1] as u32);
    if cmf & 0x0f != 8 || cmf >> 4 > 7 || (cmf * 256 + flg) % 31 != 0 || flg & 0x20 != 0 {
        return Err(bad("not a zlib header of the deflate method without a dictionary"));
    }
    // a deflate stream cannot give more than 1032 bytes for each byte it takes: do not reserve more than that for what is given
    let mut out: Vec<u8> = Vec::with_capacity(expected.min(data.len().saturating_mul(1032).saturating_add(1024)));
    let mut bits = Bits { data, pos: 2, buf: 0, count: 0 };
    loop {
        let last = bits.bits(1)?;
        match bits.bits(2)? {
            0 => {
                bits.buf = 0;
                bits.count = 0; // the rest of the byte is dropped
                let head = data.get(bits.pos..bits.pos + 4).ok_or_else(|| bad("the stream ends early"))?;
                let (len, nlen) = (u16::from_le_bytes([head[0], head[1]]), u16::from_le_bytes([head[2], head[3]]));
                if len != !nlen {
                    return Err(bad("a stored block whose length does not match its complement"));
                }
                bits.pos += 4;
                let body = data.get(bits.pos..bits.pos + len as usize).ok_or_else(|| bad("the stream ends early"))?;
                if out.len() + body.len() > expected {
                    return Err(bad("more data than the header says"));
                }
                out.extend_from_slice(body);
                bits.pos += len as usize;
            }
            1 => {
                let (l, d) = fixed()?;
                codes(&mut bits, &mut out, expected, &l, &d)?;
            }
            2 => {
                let (l, d) = dynamic(&mut bits)?;
                codes(&mut bits, &mut out, expected, &l, &d)?;
            }
            _ => return Err(bad("a block of the reserved type")),
        }
        if last == 1 {
            break;
        }
    }
    bits.buf = 0;
    bits.count = 0;
    let tail = data.get(bits.pos..).unwrap_or(&[]);
    if tail.len() < 4 {
        return Err(bad("the checksum is missing"));
    }
    if tail.len() > 4 {
        return Err(bad("bytes after the checksum"));
    }
    if out.len() != expected {
        return Err(bad("less data than the header says"));
    }
    if u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]) != adler32(&out) {
        return Err(Error::Adler);
    }
    Ok(out)
}
