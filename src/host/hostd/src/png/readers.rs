//! The two readers the proofs judge the writer by. Nothing here uses the writer's own tables or check functions: reader 1 is written again from the
//! formats (inflate for stored and fixed blocks, the five unfilters, a bitwise CRC-32, a plain Adler-32); reader 2 is two programs outside this
//! crate: Python's `zlib` with a chunk parser that checks every CRC and the Adler-32 (the script `CHECK_PY`), and ImageMagick (`identify` for
//! the size, `convert FILE rgb:-` for the pixels). A missing outside program fails the test, it never skips.

use super::*;
use crate::handoff::Scratch;
use std::process::Command;

// ---- reader 1: the test's own decoder --------------------------------------------------------------------------------------

pub(crate) fn slow_crc(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

pub(crate) fn slow_adler(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct Rd<'a> {
    d: &'a [u8],
    pos: usize,
    bit: usize,
}

impl Rd<'_> {
    fn bit(&mut self) -> Result<u32, String> {
        let byte = *self.d.get(self.pos).ok_or("the stream ends inside a block")?;
        let b = (byte >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos += 1;
        }
        Ok(b as u32)
    }
    /// `n` bits, the lowest first.
    fn low(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }
    /// `n` bits of a Huffman code, the highest first.
    fn high(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }
    fn align(&mut self) {
        if self.bit > 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }
}

/// Inflate for stored and fixed blocks; gives the bytes and how many input bytes the stream took. A dynamic block is an `Err`.
pub(crate) fn inflate(d: &[u8]) -> Result<(Vec<u8>, usize), String> {
    // The bases and extra bits of the length and distance codes, worked out from the rule of RFC 1951 (not copied from the writer).
    let mut lens = Vec::new();
    let mut base = 3usize;
    for code in 0..28u32 {
        let extra = if code < 8 { 0 } else { (code - 4) / 4 };
        lens.push((base, extra));
        base += 1 << extra;
    }
    lens.push((258, 0));
    let mut dists = Vec::new();
    let mut base = 1usize;
    for code in 0..30u32 {
        let extra = if code < 4 { 0 } else { (code - 2) / 2 };
        dists.push((base, extra));
        base += 1 << extra;
    }
    let mut rd = Rd { d, pos: 0, bit: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = rd.bit()?;
        match rd.low(2)? {
            0 => {
                rd.align();
                let len = rd.low(16)?;
                let nlen = rd.low(16)?;
                if len != (!nlen & 0xFFFF) {
                    return Err("stored block: NLEN is not the complement of LEN".into());
                }
                let piece = d.get(rd.pos..rd.pos + len as usize).ok_or("stored block runs past the stream")?;
                out.extend_from_slice(piece);
                rd.pos += len as usize;
            }
            1 => loop {
                let mut v = rd.high(7)?;
                let sym = if v <= 0b001_0111 {
                    256 + v
                } else {
                    v = (v << 1) | rd.bit()?;
                    if (0x30..=0xBF).contains(&v) {
                        v - 0x30
                    } else if (0xC0..=0xC7).contains(&v) {
                        280 + v - 0xC0
                    } else {
                        v = (v << 1) | rd.bit()?;
                        if !(0x190..=0x1FF).contains(&v) {
                            return Err("bad literal code".into());
                        }
                        144 + v - 0x190
                    }
                };
                match sym {
                    0..=255 => out.push(sym as u8),
                    256 => break,
                    257..=285 => {
                        let (lb, le) = lens[sym as usize - 257];
                        let len = lb + rd.low(le)? as usize;
                        let dc = rd.high(5)? as usize;
                        let &(db, de) = dists.get(dc).ok_or("distance code 30 or 31")?;
                        let dist = db + rd.low(de)? as usize;
                        if dist > out.len() {
                            return Err(format!("distance {dist} reaches before the start ({} bytes so far)", out.len()));
                        }
                        for _ in 0..len {
                            out.push(out[out.len() - dist]);
                        }
                    }
                    _ => return Err("literal/length symbol 286 or 287".into()),
                }
            },
            t => return Err(format!("block type {t} (dynamic codes are not written by the writer)")),
        }
        if last == 1 {
            break;
        }
    }
    rd.align();
    Ok((out, rd.pos))
}

pub(crate) fn unzlib(z: &[u8]) -> Result<Vec<u8>, String> {
    if z.len() < 6 || z[0] & 0x0F != 8 || z[0] >> 4 > 7 || (z[0] as u32 * 256 + z[1] as u32) % 31 != 0 || z[1] & 0x20 != 0 {
        return Err("bad zlib header".into());
    }
    let (out, used) = inflate(&z[2..])?;
    let tail = &z[2 + used..];
    if tail.len() != 4 {
        return Err(format!("{} bytes after the last block, expected the 4 of the Adler-32", tail.len()));
    }
    if u32::from_be_bytes(tail.try_into().unwrap()) != slow_adler(&out) {
        return Err("Adler-32 differs".into());
    }
    Ok(out)
}

pub(crate) fn paeth_spec(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (a as i32, b as i32, c as i32);
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    (if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }) as u8
}

pub(crate) struct Decoded {
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) pix: Vec<u8>,
    /// How often each filter type was used.
    pub(crate) filters: [usize; 5],
    pub(crate) idat_chunks: usize,
    pub(crate) biggest_idat: usize,
}

pub(crate) fn read_png(d: &[u8]) -> Result<Decoded, String> {
    if d.get(..8) != Some(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10][..]) {
        return Err("signature".into());
    }
    let mut pos = 8;
    let (mut kinds, mut idat, mut sizes) = (Vec::<[u8; 4]>::new(), Vec::new(), Vec::new());
    let mut ihdr = Vec::new();
    while pos < d.len() {
        let head = d.get(pos..pos + 8).ok_or("a chunk header is cut off")?;
        let n = u32::from_be_bytes(head[..4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = head[4..8].try_into().unwrap();
        let body = d.get(pos + 8..pos + 8 + n).ok_or("a chunk is cut off")?;
        let crc = d.get(pos + 8 + n..pos + 12 + n).ok_or("a chunk's CRC is cut off")?;
        if u32::from_be_bytes(crc.try_into().unwrap()) != slow_crc(&d[pos + 4..pos + 8 + n]) {
            return Err(format!("CRC of {}", String::from_utf8_lossy(&kind)));
        }
        match &kind {
            b"IHDR" => ihdr = body.to_vec(),
            b"IDAT" => {
                if kinds.last() != Some(b"IHDR") && kinds.last() != Some(b"IDAT") {
                    return Err("IDAT chunks are not in a row".into());
                }
                idat.extend_from_slice(body);
                sizes.push(n);
            }
            b"IEND" if n != 0 => return Err("IEND has data".into()),
            _ => {}
        }
        kinds.push(kind);
        pos += 12 + n;
    }
    if kinds.first() != Some(b"IHDR") || kinds.last() != Some(b"IEND") || kinds.iter().filter(|k| *k == b"IEND").count() != 1 {
        return Err("chunk order".into());
    }
    if ihdr.len() != 13 || ihdr[8..] != [8, 2, 0, 0, 0] {
        return Err("IHDR is not 8-bit RGB without interlace".into());
    }
    let (w, h) = (u32::from_be_bytes(ihdr[..4].try_into().unwrap()), u32::from_be_bytes(ihdr[4..8].try_into().unwrap()));
    let raw = unzlib(&idat)?;
    let rl = w as usize * 3;
    if raw.len() != h as usize * (rl + 1) {
        return Err(format!("{} bytes of image data, expected {}", raw.len(), h as usize * (rl + 1)));
    }
    let mut pix = vec![0u8; h as usize * rl];
    let mut filters = [0usize; 5];
    for y in 0..h as usize {
        let ft = raw[y * (rl + 1)] as usize;
        *filters.get_mut(ft).ok_or("filter type over 4")? += 1;
        for i in 0..rl {
            let x = raw[y * (rl + 1) + 1 + i];
            let a = if i >= 3 { pix[y * rl + i - 3] } else { 0 };
            let b = if y > 0 { pix[(y - 1) * rl + i] } else { 0 };
            let c = if y > 0 && i >= 3 { pix[(y - 1) * rl + i - 3] } else { 0 };
            pix[y * rl + i] = x.wrapping_add(match ft {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                _ => paeth_spec(a, b, c),
            });
        }
    }
    Ok(Decoded { w, h, pix, filters, idat_chunks: sizes.len(), biggest_idat: sizes.iter().copied().max().unwrap_or(0) })
}

// ---- reader 2: programs outside this crate ---------------------------------------------------------------------------------

/// Python's zlib and a chunk parser: every CRC, the order of the chunks, IHDR, the size of every IDAT, the end of the zlib stream (its Adler-32
/// is checked by zlib itself) and the length of the data. Prints `ok` or `bad: why`.
pub(crate) const CHECK_PY: &str = r#"import struct, sys, zlib
def fail(why):
    print("bad: " + why)
    sys.exit(1)
data = open(sys.argv[1], "rb").read()
w, h, biggest = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
if data[:8] != b"\x89PNG\r\n\x1a\n":
    fail("signature")
pos, kinds, idat = 8, [], b""
while pos < len(data):
    if pos + 8 > len(data):
        fail("header cut off")
    n = struct.unpack(">I", data[pos:pos + 4])[0]
    kind = data[pos + 4:pos + 8]
    if pos + 12 + n > len(data):
        fail("chunk cut off")
    body = data[pos + 8:pos + 8 + n]
    if struct.unpack(">I", data[pos + 8 + n:pos + 12 + n])[0] != zlib.crc32(kind + body):
        fail("crc of " + kind.decode("latin1"))
    if kind == b"IHDR" and body != struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0):
        fail("IHDR")
    if kind == b"IDAT":
        if n > biggest or n == 0 or kinds[-1:] not in ([b"IHDR"], [b"IDAT"]):
            fail("IDAT size or place")
        idat += body
    kinds.append(kind)
    pos += 12 + n
if kinds[0] != b"IHDR" or kinds[-1] != b"IEND" or kinds.count(b"IEND") != 1 or n != 0:
    fail("chunk order")
d = zlib.decompressobj()
try:
    raw = d.decompress(idat)
except zlib.error as e:
    fail("zlib: " + str(e))
if not d.eof or d.unused_data:
    fail("zlib stream does not end where the data ends")
if len(raw) != h * (1 + 3 * w):
    fail("size of the image data")
print("ok")
"#;

pub(crate) fn run(cmd: &mut Command, what: &str) -> std::process::Output {
    cmd.output().unwrap_or_else(|e| panic!("{what} must be installed for these tests (they never skip): {e}"))
}

/// What the two outside programs say about `png`: Python's checker passes, `identify` gives the size, `convert` gives the pixels.
pub(crate) fn outside(png: &[u8], w: u32, h: u32) -> (bool, String, Vec<u8>, String) {
    let dir = Scratch::new("png");
    let (file, script) = (dir.0.join("t.png"), dir.0.join("check.py"));
    std::fs::write(&file, png).unwrap();
    std::fs::write(&script, CHECK_PY).unwrap();
    let py = run(Command::new("python3").arg("-I").arg(&script).arg(&file).arg(w.to_string()).arg(h.to_string()).arg(IDAT_MAX.to_string()), "python3");
    let py_ok = py.status.success() && py.stdout == b"ok\n";
    let id = run(Command::new("identify").arg("-format").arg("%w %h").arg(&file), "identify (ImageMagick)");
    let cv = run(Command::new("convert").arg(&file).arg("rgb:-"), "convert (ImageMagick)");
    let pixels = if cv.status.success() { cv.stdout } else { Vec::new() };
    let said = format!("python: {}{} | identify: {}", String::from_utf8_lossy(&py.stdout).trim(), String::from_utf8_lossy(&py.stderr).trim(), String::from_utf8_lossy(&id.stdout));
    (py_ok, String::from_utf8_lossy(&id.stdout).into_owned(), pixels, said)
}

/// Both readers on `png`, which was made from `pix`: the pixels must come back exactly. Returns the first reader's result for the callers' counts.
pub(crate) fn both_readers(png: &[u8], w: u32, h: u32, pix: &[u8], name: &str) -> Decoded {
    let dec = read_png(png).unwrap_or_else(|e| panic!("{name}: the test decoder refuses: {e}"));
    assert_eq!((dec.w, dec.h), (w, h), "{name}");
    assert!(dec.pix == pix, "{name}: the test decoder's pixels differ from the input");
    assert!(dec.biggest_idat <= IDAT_MAX, "{name}: an IDAT chunk of {} bytes", dec.biggest_idat);
    let (py_ok, size, pixels, said) = outside(png, w, h);
    assert!(py_ok, "{name}: the Python checker refuses: {said}");
    assert_eq!(size, format!("{w} {h}"), "{name}: identify");
    assert!(pixels.len() == pix.len() && pixels == pix, "{name}: ImageMagick's pixels differ from the input ({said})");
    dec
}

