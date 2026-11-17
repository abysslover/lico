//! `lico-frame` (SPEC.md): a bounded decoder for the frames of a browser tab, PNG and JPEG (told apart by their first bytes). PNG: the colour types 0, 2, 3, 4 and 6 at 8 bits (16 bits for
//! 0, 2, 4 and 6), no interlacing; every chunk CRC and the zlib checksum are checked. JPEG: baseline and extended-sequential Huffman, 8 bits, grey or YCbCr, one scan. The size of the output
//! is checked from the header before anything is allocated. Std only, no `unsafe`.

mod huffman;
mod idct;
mod inflate;
mod jpeg;
mod png;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_jpeg;

use std::fmt;

/// The largest image taken (PNG or JPEG): the cap of one frame of the tab (`lico-ws` 0.2.0 and `lico-browser-view`): the first engine's largest frame was 69,612 bytes, Moli's largest PNG 3.77 MB and its JPEG 132 KB.
pub const MAX_INPUT: usize = 4 * 1024 * 1024;
/// The most pixels of one image: the largest viewport of a tab, 1920x1200 (2.3 million pixels, 9.2 MB decoded); it bounds the memory of a decode.
pub const MAX_PIXELS: usize = 1920 * 1200;
/// (PNG only) A PNG of a frame has three chunks (header, data, end); 64 is room for the ancillary ones and stops a stream of empty chunks.
pub const MAX_CHUNKS: usize = 64;

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_input: usize,
    pub max_pixels: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_input: MAX_INPUT, max_pixels: MAX_PIXELS }
    }
}

/// The kind of an image, told by its first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    /// The PNG colour type: 0 grey, 2 RGB, 3 palette, 4 grey and alpha, 6 RGBA. A JPEG has 0 when it is grey and 2 when it has colour.
    pub colour: u8,
    pub depth: u8,
    pub format: Format,
}

/// The pixels as `0xAARRGGBB` (alpha `0xFF` for a type that has none), row after row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The first bytes are neither a PNG signature nor the start of a JPEG.
    NotAnImage,
    Truncated,
    /// (PNG) a chunk is wrong.
    Chunk(&'static str),
    Crc,
    Unsupported(&'static str),
    TooBig,
    Inflate(&'static str),
    Adler,
    Dimensions,
    /// (JPEG) the container is wrong: a marker, a segment, a table or the frame header.
    Marker(&'static str),
    /// (JPEG) the entropy-coded data are wrong: a code that no symbol has, a size over the limit, a coefficient past the block.
    Huffman(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotAnImage => write!(f, "not a PNG or a JPEG"),
            Error::Truncated => write!(f, "the image is cut short"),
            Error::Chunk(why) => write!(f, "chunk: {why}"),
            Error::Crc => write!(f, "a chunk has a wrong checksum"),
            Error::Unsupported(what) => write!(f, "not supported: {what}"),
            Error::TooBig => write!(f, "the image or the input is over its cap"),
            Error::Inflate(why) => write!(f, "the compressed data is wrong: {why}"),
            Error::Adler => write!(f, "the compressed data has a wrong checksum"),
            Error::Dimensions => write!(f, "the image has no width or height"),
            Error::Marker(why) => write!(f, "JPEG: {why}"),
            Error::Huffman(why) => write!(f, "JPEG data: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// The kind of the image, from its first bytes (a PNG signature, or the start of a JPEG: `FF D8 FF`); `None` for anything else.
pub fn format(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(&png::SIGNATURE) {
        Some(Format::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else {
        None
    }
}

/// The header of a PNG (its CRC checked) or the frame header of a JPEG, without reading the rest.
pub fn header(bytes: &[u8]) -> Result<Header, Error> {
    match format(bytes) {
        Some(Format::Png) => png::header(bytes),
        Some(Format::Jpeg) => jpeg::header(bytes),
        None => Err(Error::NotAnImage),
    }
}

/// Decodes a PNG or a JPEG within `limits`.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Image, Error> {
    match format(bytes) {
        Some(Format::Png) => png::decode(bytes, limits),
        Some(Format::Jpeg) => jpeg::decode(bytes, limits),
        None => Err(Error::NotAnImage),
    }
}
