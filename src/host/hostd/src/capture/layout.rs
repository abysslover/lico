//! How the X server lays out the pixels of a `GetImage` reply, and the conversion of one row of it to RGB.
//!
//! Only what the setup of the connection says is trusted: the depth, the bits a pixel takes in the pixmap format of that depth, the padding of a
//! row, the byte order of the image data and the colour masks of the root visual. A layout that is not 24 or 32 bit TrueColor with three 8-bit
//! masks on byte boundaries is refused (`unsupported_screen`) rather than guessed.

/// The facts of the setup that decide the layout (plain numbers, so every layout can be tested without a server).
#[derive(Clone, Copy, Debug)]
pub struct Parts {
    pub depth: u8,
    pub bits_per_pixel: u8,
    pub scanline_pad: u8,
    pub msb_first: bool,
    pub true_color: bool,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    bytes_pp: usize,
    pad_bits: usize,
    msb_first: bool,
    shift: [u32; 3],
}

fn shift_of(mask: u32) -> Option<u32> {
    [0u32, 8, 16, 24].into_iter().find(|s| mask == 0xFF << s)
}

impl Layout {
    pub fn new(p: &Parts) -> Result<Layout, String> {
        if !p.true_color {
            return Err("the root visual is not TrueColor".into());
        }
        if p.depth != 24 && p.depth != 32 {
            return Err(format!("depth {} (only 24 and 32 are handled)", p.depth));
        }
        if p.bits_per_pixel != 24 && p.bits_per_pixel != 32 {
            return Err(format!("{} bits a pixel (only 24 and 32 are handled)", p.bits_per_pixel));
        }
        if !matches!(p.scanline_pad, 8 | 16 | 32 | 64) {
            return Err(format!("scanline pad {}", p.scanline_pad));
        }
        let (Some(r), Some(g), Some(b)) = (shift_of(p.red_mask), shift_of(p.green_mask), shift_of(p.blue_mask)) else {
            return Err("the colour masks are not three 8-bit fields on byte boundaries".into());
        };
        if r == g || g == b || r == b {
            return Err("two colour masks overlap".into());
        }
        let bytes_pp = p.bits_per_pixel as usize / 8;
        if [r, g, b].iter().any(|s| *s as usize + 8 > bytes_pp * 8) {
            return Err("a colour mask lies outside the pixel".into());
        }
        Ok(Layout { bytes_pp, pad_bits: p.scanline_pad as usize, msb_first: p.msb_first, shift: [r, g, b] })
    }

    /// The bytes one row of `width` pixels takes in the reply, padding included.
    pub fn stride(&self, width: usize) -> usize {
        (width * self.bytes_pp * 8).div_ceil(self.pad_bits) * self.pad_bits / 8
    }

    /// One row of the reply (`stride(width)` bytes or at least `width` pixels' worth) as `3 * width` bytes of R, G, B.
    pub fn row_to_rgb(&self, src: &[u8], width: usize, out: &mut [u8]) {
        let n = self.bytes_pp;
        for x in 0..width {
            let px = &src[x * n..x * n + n];
            let mut v: u32 = 0;
            if self.msb_first {
                for &b in px {
                    v = v << 8 | b as u32;
                }
            } else {
                for &b in px.iter().rev() {
                    v = v << 8 | b as u32;
                }
            }
            out[x * 3] = (v >> self.shift[0]) as u8;
            out[x * 3 + 1] = (v >> self.shift[1]) as u8;
            out[x * 3 + 2] = (v >> self.shift[2]) as u8;
        }
    }
}
