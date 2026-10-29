//! The shrink of the agent copy: a box filter of exact integer arithmetic, row by row.
//!
//! A destination pixel is the area-weighted mean of the source pixels it covers, rounded to nearest (halves up). With a source of `sw x sh` and
//! a destination of `dw x dh`, a source column is `dw` units wide and a destination column `sw` units wide, so every overlap is a whole number
//! of units; the same holds for rows. The weight of one source pixel in one destination pixel is (horizontal overlap) x (vertical overlap), the
//! weights of a destination pixel add up to `sw * sh`, and one colour stays exactly that colour. The memory is two accumulator rows of the
//! destination's width; it does not depend on the height.

use std::io;

/// The size of the agent copy: at most `max` pixels on the long side, the other side in proportion (rounded to nearest, at least 1). A picture whose
/// long side is not over `max` keeps its size.
pub fn agent_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    let (w64, h64, m) = (w as u64, h as u64, max as u64);
    if w.max(h) <= max {
        return (w, h);
    }
    let part = |side: u64, long: u64| ((side * m * 2 + long) / (2 * long)).max(1) as u32;
    if w >= h {
        (max, part(h64, w64))
    } else {
        (part(w64, h64), max)
    }
}

pub struct Shrinker {
    sw: usize,
    sh: usize,
    dw: usize,
    dh: usize,
    /// Source rows taken so far.
    taken: usize,
    /// The destination row being filled.
    row: usize,
    /// The weighted sum of the colours of the current source row per destination column (units of one source pixel times dw).
    hsum: Vec<u64>,
    acc: Vec<u64>,
    out: Vec<u8>,
}

impl Shrinker {
    pub fn new(sw: usize, sh: usize, dw: usize, dh: usize) -> Shrinker {
        assert!(sw > 0 && sh > 0 && dw > 0 && dh > 0 && dw <= sw && dh <= sh, "a shrink makes a picture smaller");
        Shrinker { sw, sh, dw, dh, taken: 0, row: 0, hsum: vec![0; dw * 3], acc: vec![0; dw * 3], out: vec![0; dw * 3] }
    }

    /// The bytes the shrinker holds.
    pub fn buffer_bytes(&self) -> usize {
        self.hsum.capacity() * 8 + self.acc.capacity() * 8 + self.out.capacity()
    }

    /// The next source row (`3 * sw` bytes of R, G, B). `emit` gets each destination row that this row completes (none, one or, never more than
    /// one because the destination is not larger than the source).
    pub fn push_row(&mut self, src: &[u8], emit: &mut dyn FnMut(&[u8]) -> io::Result<()>) -> io::Result<()> {
        assert_eq!(src.len(), self.sw * 3, "a source row is 3 bytes a pixel");
        assert!(self.taken < self.sh, "more rows than the height");
        let (sw, dw, sh, dh) = (self.sw as u64, self.dw as u64, self.sh as u64, self.dh as u64);
        // horizontal: the weighted sum of this row per destination column
        self.hsum.iter_mut().for_each(|v| *v = 0);
        let (mut i, mut bound) = (0usize, sw);
        for x in 0..self.sw {
            let (mut pos, end) = (x as u64 * dw, (x as u64 + 1) * dw);
            loop {
                let take = end.min(bound) - pos;
                for c in 0..3 {
                    self.hsum[i * 3 + c] += take * src[x * 3 + c] as u64;
                }
                pos += take;
                if pos == bound {
                    i += 1;
                    bound += sw;
                }
                if pos == end {
                    break;
                }
            }
        }
        // vertical: this source row covers [taken * dh, (taken + 1) * dh) of the units in which a destination row is sh units tall
        let (mut pos, end) = (self.taken as u64 * dh, (self.taken as u64 + 1) * dh);
        loop {
            let bound = (self.row as u64 + 1) * sh;
            let take = end.min(bound) - pos;
            for (a, h) in self.acc.iter_mut().zip(&self.hsum) {
                *a += take * h;
            }
            pos += take;
            if pos == bound {
                let area = sw * sh;
                for (o, a) in self.out.iter_mut().zip(&self.acc) {
                    *o = ((2 * a + area) / (2 * area)) as u8;
                }
                self.acc.iter_mut().for_each(|v| *v = 0);
                self.row += 1;
                emit(&self.out)?;
            }
            if pos == end {
                break;
            }
        }
        self.taken += 1;
        Ok(())
    }

    /// True once every destination row has been given out.
    pub fn done(&self) -> bool {
        self.row == self.dh
    }
}
