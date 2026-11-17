//! The PNG container (chunks, header, palette), the filters and the conversion to pixels.

use crate::inflate;
use crate::{Error, Format, Header, Image, Limits, MAX_CHUNKS};

pub(crate) const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

const fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[n] = c;
        n += 1;
    }
    t
}

static CRC_TABLE: [u32; 256] = crc_table();

pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c = CRC_TABLE[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

/// The next chunk at `pos` (CRC checked) and the position after it.
fn chunk_at(png: &[u8], pos: usize) -> Result<(Chunk<'_>, usize), Error> {
    let head = png.get(pos..pos + 8).ok_or(Error::Truncated)?;
    let n = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
    if n > 0x7fff_ffff {
        return Err(Error::Chunk("a chunk longer than 2 GiB"));
    }
    let end = pos.checked_add(12).and_then(|p| p.checked_add(n)).ok_or(Error::Truncated)?;
    let whole = png.get(pos..end).ok_or(Error::Truncated)?;
    let kind = [head[4], head[5], head[6], head[7]];
    let data = &whole[8..8 + n];
    let crc = u32::from_be_bytes([whole[8 + n], whole[9 + n], whole[10 + n], whole[11 + n]]);
    if crc32(&whole[4..8 + n]) != crc {
        return Err(Error::Crc);
    }
    Ok((Chunk { kind, data }, end))
}

fn channels(colour: u8) -> usize {
    match colour {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        _ => 4,
    }
}

fn read_header(png: &[u8]) -> Result<(Header, usize), Error> {
    if png.len() < 8 || png[..8] != SIGNATURE {
        return Err(Error::NotAnImage);
    }
    let (c, next) = chunk_at(png, 8)?;
    if &c.kind != b"IHDR" || c.data.len() != 13 {
        return Err(Error::Chunk("the first chunk is not a header of 13 bytes"));
    }
    let d = c.data;
    let (width, height) = (u32::from_be_bytes([d[0], d[1], d[2], d[3]]), u32::from_be_bytes([d[4], d[5], d[6], d[7]]));
    let (depth, colour, compression, filter, interlace) = (d[8], d[9], d[10], d[11], d[12]);
    let ok = match colour {
        0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
        2 | 4 | 6 => matches!(depth, 8 | 16),
        3 => matches!(depth, 1 | 2 | 4 | 8),
        _ => false,
    };
    if !ok {
        return Err(Error::Chunk("a colour type and a bit depth that do not go together"));
    }
    if compression != 0 || filter != 0 {
        return Err(Error::Chunk("a compression or filter method that does not exist"));
    }
    if interlace > 1 {
        return Err(Error::Chunk("an interlace method that does not exist"));
    }
    if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff {
        return Err(Error::Dimensions);
    }
    if interlace == 1 {
        return Err(Error::Unsupported("interlaced (Adam7) images"));
    }
    if depth < 8 {
        return Err(Error::Unsupported("bit depths below 8"));
    }
    Ok((Header { width, height, colour, depth, format: Format::Png }, next))
}

pub(crate) fn header(png: &[u8]) -> Result<Header, Error> {
    read_header(png).map(|(h, _)| h)
}

pub(crate) fn decode(png: &[u8], limits: &Limits) -> Result<Image, Error> {
    if png.len() > limits.max_input {
        return Err(Error::TooBig);
    }
    let (h, mut pos) = read_header(png)?;
    let pixels = h.width as u64 * h.height as u64;
    if pixels > limits.max_pixels as u64 {
        return Err(Error::TooBig); // before anything is allocated
    }
    let (w, ht) = (h.width as usize, h.height as usize);
    let (ch, depth) = (channels(h.colour), h.depth as usize);
    let row_bytes = w * ch * depth / 8;
    let expected = ht * (1 + row_bytes);
    let bpp = (ch * depth / 8).max(1);
    let mut palette: Option<Vec<u8>> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_idat = false;
    let mut chunks = 1;
    loop {
        let (c, next) = chunk_at(png, pos)?;
        chunks += 1;
        if chunks > MAX_CHUNKS {
            return Err(Error::Chunk("too many chunks"));
        }
        pos = next;
        match &c.kind {
            b"IHDR" => return Err(Error::Chunk("a second header")),
            b"PLTE" => {
                if c.data.is_empty() || c.data.len() % 3 != 0 || c.data.len() > 768 {
                    return Err(Error::Chunk("a palette that is not 1 to 256 whole entries"));
                }
                if palette.is_none() {
                    palette = Some(c.data.to_vec());
                }
            }
            b"IDAT" => {
                seen_idat = true;
                idat.extend_from_slice(c.data);
            }
            b"IEND" => {
                if pos != png.len() {
                    return Err(Error::Chunk("bytes after IEND"));
                }
                break;
            }
            k if k[0] & 0x20 == 0 => return Err(Error::Unsupported("a critical chunk this decoder does not know")),
            _ => {}
        }
    }
    if !seen_idat {
        return Err(Error::Chunk("no image data"));
    }
    if h.colour == 3 && palette.is_none() {
        return Err(Error::Chunk("a palette image without a palette"));
    }
    let mut raw = inflate::zlib(&idat, expected)?;
    drop(idat);
    unfilter(&mut raw, ht, row_bytes, bpp)?;
    let mut out: Vec<u32> = Vec::with_capacity(w * ht);
    for y in 0..ht {
        let row = &raw[y * (1 + row_bytes) + 1..(y + 1) * (1 + row_bytes)];
        convert_row(row, h.colour, h.depth, palette.as_deref(), &mut out)?;
    }
    Ok(Image { width: h.width, height: h.height, pixels: out })
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a32, b32, c32) = (a as i32, b as i32, c as i32);
    let p = a32 + b32 - c32;
    let (pa, pb, pc) = ((p - a32).abs(), (p - b32).abs(), (p - c32).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undoes the filter of every row in place (each row: its filter byte, then `row_bytes`).
fn unfilter(raw: &mut [u8], rows: usize, row_bytes: usize, bpp: usize) -> Result<(), Error> {
    let stride = 1 + row_bytes;
    for y in 0..rows {
        let (before, rest) = raw.split_at_mut(y * stride);
        let cur = &mut rest[..stride];
        let prev: Option<&[u8]> = (y > 0).then(|| &before[(y - 1) * stride + 1..]);
        let filter = cur[0];
        let line = &mut cur[1..];
        match filter {
            0 => {}
            1 => {
                for i in bpp..row_bytes {
                    line[i] = line[i].wrapping_add(line[i - bpp]);
                }
            }
            2 => {
                if let Some(p) = prev {
                    for i in 0..row_bytes {
                        line[i] = line[i].wrapping_add(p[i]);
                    }
                }
            }
            3 => {
                for i in 0..row_bytes {
                    let a = if i >= bpp { line[i - bpp] as u16 } else { 0 };
                    let b = prev.map_or(0, |p| p[i] as u16);
                    line[i] = line[i].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                for i in 0..row_bytes {
                    let a = if i >= bpp { line[i - bpp] } else { 0 };
                    let b = prev.map_or(0, |p| p[i]);
                    let c = if i >= bpp { prev.map_or(0, |p| p[i - bpp]) } else { 0 };
                    line[i] = line[i].wrapping_add(paeth(a, b, c));
                }
            }
            _ => return Err(Error::Chunk("a filter type that does not exist")),
        }
    }
    Ok(())
}

fn convert_row(row: &[u8], colour: u8, depth: u8, palette: Option<&[u8]>, out: &mut Vec<u32>) -> Result<(), Error> {
    let step = if depth == 16 { 2 } else { 1 }; // 16 bits: the high byte of each sample
    let px = |r: u8, g: u8, b: u8, a: u8| (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32;
    match colour {
        0 => out.extend(row.chunks_exact(step).map(|s| px(s[0], s[0], s[0], 255))),
        2 => out.extend(row.chunks_exact(3 * step).map(|s| px(s[0], s[step], s[2 * step], 255))),
        4 => out.extend(row.chunks_exact(2 * step).map(|s| px(s[0], s[0], s[0], s[step]))),
        6 => out.extend(row.chunks_exact(4 * step).map(|s| px(s[0], s[step], s[2 * step], s[3 * step]))),
        _ => {
            let p = palette.ok_or(Error::Chunk("a palette image without a palette"))?;
            for &i in row {
                let at = i as usize * 3;
                let e = p.get(at..at + 3).ok_or(Error::Chunk("an index past the palette"))?;
                out.push(px(e[0], e[1], e[2], 255));
            }
        }
    }
    Ok(())
}
