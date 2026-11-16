//! `lico-frame` (SPEC.md): a bounded PNG decoder for the frames of a browser tab. The colour types 0, 2, 3, 4 and 6 at 8 bits (16 bits for 0, 2, 4 and 6), no interlacing; every
//! chunk CRC and the zlib checksum are checked; the size of the output is checked from the header before anything is allocated. Std only, no `unsafe`.

mod inflate;
mod png;
#[cfg(test)]
mod tests;

use std::fmt;

/// The largest PNG taken: the cap of one frame of the tab (`lico-ws` 0.2.0 and `lico-browser-view`): the largest frame the engine sent was 69,612 bytes.
pub const MAX_INPUT: usize = 4 * 1024 * 1024;
/// The most pixels of one image: the largest viewport of a tab, 1920x1200 (2.3 million pixels, 9.2 MB decoded); it bounds the memory of a decode.
pub const MAX_PIXELS: usize = 1920 * 1200;
/// A PNG of a frame has three chunks (header, data, end); 64 is room for the ancillary ones and stops a stream of empty chunks.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    /// The PNG colour type: 0 grey, 2 RGB, 3 palette, 4 grey and alpha, 6 RGBA.
    pub colour: u8,
    pub depth: u8,
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
    NotPng,
    Truncated,
    Chunk(&'static str),
    Crc,
    Unsupported(&'static str),
    TooBig,
    Inflate(&'static str),
    Adler,
    Dimensions,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotPng => write!(f, "not a PNG"),
            Error::Truncated => write!(f, "the PNG is cut short"),
            Error::Chunk(why) => write!(f, "chunk: {why}"),
            Error::Crc => write!(f, "a chunk has a wrong checksum"),
            Error::Unsupported(what) => write!(f, "not supported: {what}"),
            Error::TooBig => write!(f, "the image or the input is over its cap"),
            Error::Inflate(why) => write!(f, "the compressed data is wrong: {why}"),
            Error::Adler => write!(f, "the compressed data has a wrong checksum"),
            Error::Dimensions => write!(f, "the image has no width or height"),
        }
    }
}

impl std::error::Error for Error {}

/// The header of a PNG (its CRC checked), without reading the rest.
pub fn header(png: &[u8]) -> Result<Header, Error> {
    png::header(png)
}

/// Decodes a PNG within `limits`.
pub fn decode(png: &[u8], limits: &Limits) -> Result<Image, Error> {
    png::decode(png, limits)
}
