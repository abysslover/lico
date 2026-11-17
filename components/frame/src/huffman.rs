//! The Huffman tables of a JPEG and the bit reader of its entropy-coded data.

use crate::Error;

/// A table of canonical Huffman codes (JPEG Annex C): the codes of up to 9 bits through a lookup of 512 entries, the longer ones (up to 16 bits) by comparing with the largest code of each length.
pub(crate) struct Table {
    /// For the first 9 bits of the stream: `(length << 8) | symbol`, or 0 when the code is longer than 9 bits (or is none).
    lookup: [u16; 512],
    /// `maxcode[l]`: the largest code of length `l` (1 to 16), -1 when there is none.
    maxcode: [i32; 17],
    /// The index of the symbol of a code of length `l` is `code + valoffset[l]`.
    valoffset: [i32; 17],
    symbols: Vec<u8>,
}

impl Table {
    /// `counts[l - 1]` is the number of codes of length `l`; `symbols` the symbols in the order of their codes. The codes must fit their lengths; the code space need not be filled.
    pub(crate) fn build(counts: &[u8; 16], symbols: &[u8]) -> Result<Table, Error> {
        let total: usize = counts.iter().map(|c| *c as usize).sum();
        if total > 256 || total != symbols.len() {
            return Err(Error::Marker("a table of Huffman codes whose lengths and symbols do not agree"));
        }
        let mut t = Table { lookup: [0; 512], maxcode: [-1; 17], valoffset: [0; 17], symbols: symbols.to_vec() };
        let mut code: u32 = 0;
        let mut k: usize = 0;
        for l in 1..=16usize {
            let n = counts[l - 1] as u32;
            t.valoffset[l] = k as i32 - code as i32;
            if n > 0 {
                if code + n > (1u32 << l) {
                    return Err(Error::Huffman("a table with more codes than their lengths allow"));
                }
                t.maxcode[l] = (code + n - 1) as i32;
                if l <= 9 {
                    for i in 0..n {
                        let first = ((code + i) << (9 - l)) as usize;
                        let entry = (l as u16) << 8 | symbols[k + i as usize] as u16;
                        for slot in &mut t.lookup[first..first + (1usize << (9 - l))] {
                            *slot = entry;
                        }
                    }
                }
            }
            code += n;
            k += n as usize;
            code <<= 1;
        }
        Ok(t)
    }
}

/// The bits of the entropy-coded data. A byte `FF 00` is the byte `FF`; `FF FF` is fill; any other `FF xx` is a marker, where the data end: past it zero bits are given, and the bits taken
/// from that padding are counted, so that `overrun` tells whether the data were cut short.
pub(crate) struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    acc: u64,
    nbits: u32,
    /// How many of the `nbits` low bits of `acc` are padding (they are the last ones: the real bits are taken first).
    pad: u32,
    ended: bool,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize) -> Bits<'a> {
        Bits { data, pos, acc: 0, nbits: 0, pad: 0, ended: false }
    }

    /// The byte after the last one taken (the marker, once the data have ended).
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// True when bits of the padding have been taken: the data ended before the blocks did.
    pub(crate) fn overrun(&self) -> bool {
        self.nbits < self.pad
    }

    fn next_byte(&mut self) -> Option<u8> {
        if self.ended {
            return None;
        }
        loop {
            let b = match self.data.get(self.pos) {
                Some(b) => *b,
                None => {
                    self.ended = true;
                    return None;
                }
            };
            if b != 0xFF {
                self.pos += 1;
                return Some(b);
            }
            match self.data.get(self.pos + 1) {
                Some(0x00) => {
                    self.pos += 2;
                    return Some(0xFF);
                }
                Some(0xFF) => self.pos += 1,
                _ => {
                    self.ended = true;
                    return None;
                }
            }
        }
    }

    fn fill(&mut self) {
        while self.nbits <= 56 {
            match self.next_byte() {
                Some(b) => self.acc = self.acc << 8 | b as u64,
                None => {
                    self.acc <<= 8;
                    self.pad += 8;
                }
            }
            self.nbits += 8;
        }
    }

    fn take(&mut self, n: u32) {
        self.nbits -= n;
    }

    /// The next `n` bits (1 to 16) as a number.
    pub(crate) fn receive(&mut self, n: u32) -> u32 {
        if self.nbits < n {
            self.fill();
        }
        let v = ((self.acc >> (self.nbits - n)) & ((1u64 << n) - 1)) as u32;
        self.take(n);
        v
    }

    /// The next symbol of `table`.
    pub(crate) fn symbol(&mut self, table: &Table) -> Result<u8, Error> {
        if self.nbits < 16 {
            self.fill();
        }
        let entry = table.lookup[((self.acc >> (self.nbits - 9)) & 0x1FF) as usize];
        if entry != 0 {
            self.take((entry >> 8) as u32);
            return Ok(entry as u8);
        }
        let word = ((self.acc >> (self.nbits - 16)) & 0xFFFF) as i32;
        for l in 10..=16usize {
            let code = word >> (16 - l);
            if code <= table.maxcode[l] {
                let symbol = table.symbols.get((code + table.valoffset[l]) as usize).copied().ok_or(Error::Huffman("a code outside its table"))?;
                self.take(l as u32);
                return Ok(symbol);
            }
        }
        Err(Error::Huffman("a code that no symbol has"))
    }

    /// Throws away the bits in hand and finds the next marker (skipping any bytes before it); returns it without taking it. `None` when there is no marker before the end of the data.
    pub(crate) fn find_marker(&mut self) -> Option<u8> {
        self.acc = 0;
        self.nbits = 0;
        self.pad = 0;
        self.ended = false;
        loop {
            let b = *self.data.get(self.pos)?;
            if b != 0xFF {
                self.pos += 1;
                continue;
            }
            match *self.data.get(self.pos + 1)? {
                0x00 => self.pos += 2,
                0xFF => self.pos += 1,
                m => return Some(m),
            }
        }
    }

    /// Takes the marker `find_marker` stopped at (two bytes) and starts the bits again after it.
    pub(crate) fn skip_marker(&mut self) {
        self.pos += 2;
        self.ended = false;
    }
}
