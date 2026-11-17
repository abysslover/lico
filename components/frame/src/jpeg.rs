//! The JPEG container (markers, tables, the frame and scan headers), the entropy-coded scan, and the conversion to pixels. Baseline and extended-sequential Huffman JPEG at 8 bits, grey or
//! three components (YCbCr), one scan with all the components, restart intervals, any sampling factors that divide each other (chroma is repeated, not interpolated).

use crate::huffman::{Bits, Table};
use crate::idct;
use crate::{Error, Format, Header, Image, Limits};

/// The natural position (row * 8 + column) of the coefficient at each place of the zigzag order.
const ZIGZAG: [u8; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60,
    61, 54, 47, 55, 62, 63,
];

/// A coefficient is clamped to this after the multiplication with its quantiser: a real one is under 2^11, and the clamp keeps the arithmetic of a damaged stream in range.
const COEF_LIMIT: i64 = 1 << 24;

struct Component {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
}

struct Frame {
    width: usize,
    height: usize,
    comps: Vec<Component>,
}

struct Scan {
    /// The index in the frame of each component of the scan, in the order of the scan, and its DC and AC table.
    comps: Vec<(usize, usize, usize)>,
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    quant: [Option<[u16; 64]>; 4],
    dc: [Option<Table>; 4],
    ac: [Option<Table>; 4],
    restart: usize,
    frame: Option<Frame>,
}

fn be16(b: &[u8]) -> usize {
    (b[0] as usize) << 8 | b[1] as usize
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8]) -> Parser<'a> {
        Parser { data, pos: 2, quant: [None, None, None, None], dc: [None, None, None, None], ac: [None, None, None, None], restart: 0, frame: None }
    }

    /// The marker at `pos` (any number of fill bytes `FF` before it), `pos` after it.
    fn marker(&mut self) -> Result<u8, Error> {
        match self.data.get(self.pos) {
            None => return Err(Error::Truncated),
            Some(0xFF) => {}
            Some(_) => return Err(Error::Marker("a marker was expected")),
        }
        while self.data.get(self.pos) == Some(&0xFF) {
            self.pos += 1;
        }
        let m = *self.data.get(self.pos).ok_or(Error::Truncated)?;
        if m == 0 {
            return Err(Error::Marker("a marker was expected"));
        }
        self.pos += 1;
        Ok(m)
    }

    /// The body of the segment at `pos` (its length counts the two bytes of the length), `pos` after it.
    fn segment(&mut self) -> Result<&'a [u8], Error> {
        let head = self.data.get(self.pos..self.pos + 2).ok_or(Error::Truncated)?;
        let n = be16(head);
        if n < 2 {
            return Err(Error::Marker("a segment shorter than its length"));
        }
        let body = self.data.get(self.pos + 2..self.pos + n).ok_or(Error::Truncated)?;
        self.pos += n;
        Ok(body)
    }

    /// Reads markers until the frame header (`stop_at_frame`) or until the header of the scan; returns the scan, when that was where it stopped.
    fn run(&mut self, stop_at_frame: bool) -> Result<Option<Scan>, Error> {
        loop {
            let m = self.marker()?;
            match m {
                0xD8 => return Err(Error::Marker("a second start of image")),
                0xD9 => return Err(Error::Marker("the image ends before its data")),
                0x01 | 0xD0..=0xD7 => return Err(Error::Marker("a marker that does not belong here")),
                0xC0 | 0xC1 => {
                    let body = self.segment()?;
                    if self.frame.is_some() {
                        return Err(Error::Marker("a second frame header"));
                    }
                    self.frame = Some(parse_frame(body)?);
                    if stop_at_frame {
                        return Ok(None);
                    }
                }
                0xC2 => return Err(Error::Unsupported("progressive JPEG")),
                0xC9..=0xCB | 0xCD..=0xCF | 0xCC => return Err(Error::Unsupported("arithmetic-coded JPEG")),
                0xC3 | 0xC5..=0xC7 => return Err(Error::Unsupported("lossless or hierarchical JPEG")),
                0xDC => return Err(Error::Unsupported("a JPEG whose height comes later (DNL)")),
                0xC4 => {
                    let body = self.segment()?;
                    self.tables_of_codes(body)?;
                }
                0xDB => {
                    let body = self.segment()?;
                    self.tables_of_quantisers(body)?;
                }
                0xDD => {
                    let body = self.segment()?;
                    if body.len() != 2 {
                        return Err(Error::Marker("a restart interval of the wrong length"));
                    }
                    self.restart = be16(body);
                }
                0xDA => {
                    let body = self.segment()?;
                    let frame = self.frame.as_ref().ok_or(Error::Marker("a scan before the frame header"))?;
                    return parse_scan(body, frame).map(Some);
                }
                // application data, comments and the markers reserved for extensions: skipped by their length
                _ => {
                    self.segment()?;
                }
            }
        }
    }

    fn tables_of_codes(&mut self, mut body: &[u8]) -> Result<(), Error> {
        while !body.is_empty() {
            if body.len() < 17 {
                return Err(Error::Marker("a table of Huffman codes is cut short"));
            }
            let (class, id) = (body[0] >> 4, (body[0] & 15) as usize);
            if class > 1 || id > 3 {
                return Err(Error::Marker("a table of Huffman codes of an unknown kind"));
            }
            let mut counts = [0u8; 16];
            counts.copy_from_slice(&body[1..17]);
            let n: usize = counts.iter().map(|c| *c as usize).sum();
            let symbols = body.get(17..17 + n).ok_or(Error::Marker("a table of Huffman codes is cut short"))?;
            let table = Table::build(&counts, symbols)?;
            if class == 0 {
                self.dc[id] = Some(table);
            } else {
                self.ac[id] = Some(table);
            }
            body = &body[17 + n..];
        }
        Ok(())
    }

    fn tables_of_quantisers(&mut self, mut body: &[u8]) -> Result<(), Error> {
        while !body.is_empty() {
            let (precision, id) = (body[0] >> 4, (body[0] & 15) as usize);
            if precision > 1 || id > 3 {
                return Err(Error::Marker("a table of quantisers of an unknown kind"));
            }
            let size = if precision == 0 { 64 } else { 128 };
            let values = body.get(1..1 + size).ok_or(Error::Marker("a table of quantisers is cut short"))?;
            let mut table = [0u16; 64];
            for (k, q) in table.iter_mut().enumerate() {
                *q = if precision == 0 { values[k] as u16 } else { be16(&values[2 * k..]) as u16 };
            }
            self.quant[id] = Some(table);
            body = &body[1 + size..];
        }
        Ok(())
    }
}

fn parse_frame(body: &[u8]) -> Result<Frame, Error> {
    if body.len() < 6 {
        return Err(Error::Marker("a frame header that is cut short"));
    }
    if body[0] != 8 {
        return Err(Error::Unsupported("a sample precision other than 8 bits"));
    }
    let (height, width, n) = (be16(&body[1..3]), be16(&body[3..5]), body[5] as usize);
    if width == 0 || height == 0 {
        return Err(Error::Dimensions);
    }
    if n != 1 && n != 3 {
        return Err(Error::Unsupported("a JPEG that is not grey or of three components"));
    }
    if body.len() != 6 + 3 * n {
        return Err(Error::Marker("a frame header of the wrong length"));
    }
    let mut comps = Vec::with_capacity(n);
    for c in body[6..].chunks(3) {
        let (h, v, tq) = ((c[1] >> 4) as usize, (c[1] & 15) as usize, c[2] as usize);
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) || tq > 3 {
            return Err(Error::Marker("a sampling factor or a table out of range"));
        }
        if comps.iter().any(|o: &Component| o.id == c[0]) {
            return Err(Error::Marker("two components with one number"));
        }
        comps.push(Component { id: c[0], h, v, tq });
    }
    if n == 1 {
        // one component alone is scanned block by block, whatever factors it names
        comps[0].h = 1;
        comps[0].v = 1;
    } else {
        let blocks: usize = comps.iter().map(|c| c.h * c.v).sum();
        if blocks > 10 {
            return Err(Error::Marker("more than ten blocks in a unit of the image"));
        }
        let (hmax, vmax) = (comps.iter().map(|c| c.h).max().unwrap_or(1), comps.iter().map(|c| c.v).max().unwrap_or(1));
        if comps.iter().any(|c| hmax % c.h != 0 || vmax % c.v != 0) {
            return Err(Error::Unsupported("sampling factors that do not divide each other"));
        }
    }
    Ok(Frame { width, height, comps })
}

fn parse_scan(body: &[u8], frame: &Frame) -> Result<Scan, Error> {
    let n = *body.first().ok_or(Error::Marker("a scan header that is cut short"))? as usize;
    if body.len() != 1 + 2 * n + 3 || n == 0 {
        return Err(Error::Marker("a scan header of the wrong length"));
    }
    if n != frame.comps.len() {
        return Err(Error::Unsupported("a scan of only some of the components (more than one scan)"));
    }
    let mut comps = Vec::with_capacity(n);
    for c in body[1..1 + 2 * n].chunks(2) {
        let index = frame.comps.iter().position(|f| f.id == c[0]).ok_or(Error::Marker("a scan of a component that the frame does not have"))?;
        if comps.iter().any(|(i, _, _)| *i == index) {
            return Err(Error::Marker("a component twice in a scan"));
        }
        let (td, ta) = ((c[1] >> 4) as usize, (c[1] & 15) as usize);
        if td > 3 || ta > 3 {
            return Err(Error::Marker("a table of Huffman codes out of range"));
        }
        comps.push((index, td, ta));
    }
    let tail = &body[1 + 2 * n..];
    if tail[0] != 0 || tail[1] != 63 || tail[2] != 0 {
        return Err(Error::Unsupported("a scan that is not the whole sequential one"));
    }
    Ok(Scan { comps })
}

fn check_size(frame: &Frame, limits: &Limits) -> Result<(), Error> {
    if frame.width.checked_mul(frame.height).map_or(true, |p| p > limits.max_pixels) {
        return Err(Error::TooBig);
    }
    Ok(())
}

fn check_start(jpeg: &[u8]) -> Result<(), Error> {
    if !jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(Error::NotAnImage);
    }
    Ok(())
}

/// The size and kind of a JPEG, from its frame header.
pub(crate) fn header(jpeg: &[u8]) -> Result<Header, Error> {
    check_start(jpeg)?;
    let mut p = Parser::new(jpeg);
    p.run(true)?;
    let frame = p.frame.ok_or(Error::Marker("no frame header"))?;
    Ok(Header { width: frame.width as u32, height: frame.height as u32, colour: if frame.comps.len() == 1 { 0 } else { 2 }, depth: 8, format: Format::Jpeg })
}

pub(crate) fn decode(jpeg: &[u8], limits: &Limits) -> Result<Image, Error> {
    if jpeg.len() > limits.max_input {
        return Err(Error::TooBig);
    }
    check_start(jpeg)?;
    let mut p = Parser::new(jpeg);
    let scan = p.run(false)?.ok_or(Error::Marker("no scan"))?;
    let frame = p.frame.as_ref().ok_or(Error::Marker("no frame header"))?;
    check_size(frame, limits)?;
    decode_scan(&p, frame, &scan)
}

/// What one component needs to decode its blocks.
struct Plane<'t> {
    /// Index in the frame.
    index: usize,
    h: usize,
    v: usize,
    quant: &'t [u16; 64],
    dc: &'t Table,
    ac: &'t Table,
    stride: usize,
    samples: Vec<u8>,
}

fn decode_scan(p: &Parser<'_>, frame: &Frame, scan: &Scan) -> Result<Image, Error> {
    let (hmax, vmax) = (frame.comps.iter().map(|c| c.h).max().unwrap_or(1), frame.comps.iter().map(|c| c.v).max().unwrap_or(1));
    let (mcu_w, mcu_h) = (8 * hmax, 8 * vmax);
    let (mcus_x, mcus_y) = (frame.width.div_ceil(mcu_w), frame.height.div_ceil(mcu_h));

    // the planes of one row of units of the image, in the order of the scan
    let mut planes = Vec::with_capacity(scan.comps.len());
    for (index, td, ta) in &scan.comps {
        let c = &frame.comps[*index];
        let quant = p.quant[c.tq].as_ref().ok_or(Error::Marker("a table of quantisers is missing"))?;
        let dc = p.dc[*td].as_ref().ok_or(Error::Marker("a table of Huffman codes is missing"))?;
        let ac = p.ac[*ta].as_ref().ok_or(Error::Marker("a table of Huffman codes is missing"))?;
        let stride = mcus_x * c.h * 8;
        planes.push(Plane { index: *index, h: c.h, v: c.v, quant, dc, ac, stride, samples: vec![0u8; stride * c.v * 8] });
    }
    let mut pixels = vec![0u32; frame.width * frame.height];

    let mut bits = Bits::new(p.data, p.pos);
    let mut predictions = [0i32; 3];
    let mut coef = [0i32; 64];
    let (mut unit, mut next_restart) = (0usize, 0u8);
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            if p.restart > 0 && unit > 0 && unit % p.restart == 0 {
                match bits.find_marker() {
                    Some(m) if m == 0xD0 + next_restart => {}
                    Some(_) => return Err(Error::Marker("a restart marker that is not the next one")),
                    None => return Err(Error::Truncated),
                }
                bits.skip_marker();
                next_restart = (next_restart + 1) & 7;
                predictions = [0; 3];
            }
            unit += 1;
            for plane in planes.iter_mut() {
                for by in 0..plane.v {
                    for bx in 0..plane.h {
                        let dc_only = block(&mut bits, plane, &mut predictions[plane.index], &mut coef)?;
                        let at = by * 8 * plane.stride + (mx * plane.h + bx) * 8;
                        idct::block(&coef, dc_only, &mut plane.samples[at..], plane.stride);
                    }
                }
            }
            if bits.overrun() {
                return Err(Error::Truncated);
            }
        }
        convert_rows(frame, &planes, (hmax, vmax), my * mcu_h, &mut pixels);
    }
    // after the last unit: the end of the image and nothing more
    match bits.find_marker() {
        Some(0xD9) => {}
        Some(0xDA) => return Err(Error::Unsupported("more than one scan")),
        Some(_) => return Err(Error::Marker("something other than the end of the image after the scan")),
        None => return Err(Error::Truncated),
    }
    bits.skip_marker();
    if bits.position() != p.data.len() {
        return Err(Error::Marker("bytes after the end of the image"));
    }
    Ok(Image { width: frame.width as u32, height: frame.height as u32, pixels })
}

/// Decodes one block into `coef` (dequantised, natural order). True when only the first coefficient is not zero.
fn block(bits: &mut Bits<'_>, plane: &Plane<'_>, prediction: &mut i32, coef: &mut [i32; 64]) -> Result<bool, Error> {
    *coef = [0; 64];
    let size = bits.symbol(plane.dc)? as u32;
    if size > 11 {
        return Err(Error::Huffman("a size of a DC difference over the limit of baseline"));
    }
    let diff = if size == 0 { 0 } else { extend(bits.receive(size), size) };
    *prediction = prediction.wrapping_add(diff);
    coef[0] = dequantise(*prediction, plane.quant[0]);
    let mut k = 1usize;
    let mut dc_only = true;
    while k < 64 {
        let rs = bits.symbol(plane.ac)?;
        let (run, size) = ((rs >> 4) as usize, (rs & 15) as u32);
        if size == 0 {
            if run == 15 {
                k += 16;
                continue;
            }
            break;
        }
        k += run;
        if k > 63 {
            return Err(Error::Huffman("a coefficient past the end of the block"));
        }
        if size > 10 {
            return Err(Error::Huffman("a size of a coefficient over the limit of baseline"));
        }
        coef[ZIGZAG[k] as usize] = dequantise(extend(bits.receive(size), size), plane.quant[k]);
        dc_only = false;
        k += 1;
    }
    Ok(dc_only)
}

fn dequantise(value: i32, quantiser: u16) -> i32 {
    (value as i64 * quantiser as i64).clamp(-COEF_LIMIT, COEF_LIMIT) as i32
}

/// The value of `size` bits `v` (JPEG F.2.2.1): the upper half are positive numbers, the lower half the negative ones.
fn extend(v: u32, size: u32) -> i32 {
    if v < 1 << (size - 1) {
        v as i32 - (1 << size) + 1
    } else {
        v as i32
    }
}

fn clamp8(x: i32) -> u32 {
    x.clamp(0, 255) as u32
}

/// Writes the rows of the planes (one row of units of the image, from line `y0`) into `pixels`: chroma repeated up to the size of the luma, then YCbCr to RGB as libjpeg 9 does (the constants of the JFIF text: 1.402, 0.344136286, 0.714136286 and 1.772, in 16 bits).
fn convert_rows(frame: &Frame, planes: &[Plane<'_>], (hmax, vmax): (usize, usize), y0: usize, pixels: &mut [u32]) {
    let width = frame.width;
    let rows = (8 * vmax).min(frame.height - y0);
    let mut line: [Vec<u8>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for r in 0..rows {
        for plane in planes {
            let (rx, ry) = (hmax / plane.h, vmax / plane.v);
            let row = &plane.samples[(r / ry) * plane.stride..][..plane.stride];
            let out = &mut line[plane.index];
            out.clear();
            if rx == 1 {
                out.extend_from_slice(&row[..width]);
            } else {
                out.extend((0..width).map(|x| row[x / rx]));
            }
        }
        let dst = &mut pixels[(y0 + r) * width..][..width];
        if frame.comps.len() == 1 {
            for (d, y) in dst.iter_mut().zip(&line[0]) {
                *d = 0xFF00_0000 | (*y as u32) * 0x0001_0101;
            }
        } else {
            for (x, d) in dst.iter_mut().enumerate() {
                let (y, cb, cr) = (line[0][x] as i32, line[1][x] as i32 - 128, line[2][x] as i32 - 128);
                let red = y + ((91881 * cr + 32768) >> 16);
                let green = y + ((-22553 * cb - 46802 * cr + 32768) >> 16);
                let blue = y + ((116130 * cb + 32768) >> 16);
                *d = 0xFF00_0000 | clamp8(red) << 16 | clamp8(green) << 8 | clamp8(blue);
            }
        }
    }
}
