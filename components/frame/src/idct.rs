//! The inverse DCT of one 8x8 block: the accurate integer method of the Independent JPEG Group (`jidctint`, 13 bits of constants, 2 extra bits between the passes), so that the pixels are the
//! same as libjpeg's. Arithmetic in `i64`: a damaged stream can give coefficients far over the range of a real one, and the result then is a wrong picture, never an overflow.

const CONST_BITS: u32 = 13;
const PASS1_BITS: u32 = 2;
const FIX_0_298631336: i64 = 2446;
const FIX_0_390180644: i64 = 3196;
const FIX_0_541196100: i64 = 4433;
const FIX_0_765366865: i64 = 6270;
const FIX_0_899976223: i64 = 7373;
const FIX_1_175875602: i64 = 9633;
const FIX_1_501321110: i64 = 12299;
const FIX_1_847759065: i64 = 15137;
const FIX_1_961570560: i64 = 16069;
const FIX_2_053119869: i64 = 16819;
const FIX_2_562915447: i64 = 20995;
const FIX_3_072711026: i64 = 25172;

fn descale(x: i64, n: u32) -> i64 {
    (x + (1i64 << (n - 1))) >> n
}

fn sample(x: i64) -> u8 {
    (x + 128).clamp(0, 255) as u8
}

/// The even and odd parts of one line of eight values (the same arithmetic for the columns and the rows): the eight results, before the final shift.
fn line(i: [i64; 8]) -> [i64; 8] {
    let (z2, z3) = (i[2], i[6]);
    let z1 = (z2 + z3) * FIX_0_541196100;
    let tmp2 = z1 + z3 * -FIX_1_847759065;
    let tmp3 = z1 + z2 * FIX_0_765366865;
    let tmp0 = (i[0] + i[4]) << CONST_BITS;
    let tmp1 = (i[0] - i[4]) << CONST_BITS;
    let (tmp10, tmp13, tmp11, tmp12) = (tmp0 + tmp3, tmp0 - tmp3, tmp1 + tmp2, tmp1 - tmp2);

    let (mut t0, mut t1, mut t2, mut t3) = (i[7], i[5], i[3], i[1]);
    let (mut z1, mut z2, mut z3, mut z4) = (t0 + t3, t1 + t2, t0 + t2, t1 + t3);
    let z5 = (z3 + z4) * FIX_1_175875602;
    t0 *= FIX_0_298631336;
    t1 *= FIX_2_053119869;
    t2 *= FIX_3_072711026;
    t3 *= FIX_1_501321110;
    z1 *= -FIX_0_899976223;
    z2 *= -FIX_2_562915447;
    z3 = z3 * -FIX_1_961570560 + z5;
    z4 = z4 * -FIX_0_390180644 + z5;
    t0 += z1 + z3;
    t1 += z2 + z4;
    t2 += z2 + z3;
    t3 += z1 + z4;
    [tmp10 + t3, tmp11 + t2, tmp12 + t1, tmp13 + t0, tmp13 - t0, tmp12 - t1, tmp11 - t2, tmp10 - t3]
}

/// Writes the 8x8 block of the (dequantised, natural order) coefficients `coef` as samples into `out`, whose rows are `stride` apart. `dc_only`: no coefficient but the first is not zero.
pub(crate) fn block(coef: &[i32; 64], dc_only: bool, out: &mut [u8], stride: usize) {
    if dc_only {
        // the general arithmetic with every other term zero, in one step
        let v = sample(descale((coef[0] as i64) << PASS1_BITS, PASS1_BITS + 3));
        for row in 0..8 {
            out[row * stride..row * stride + 8].fill(v);
        }
        return;
    }
    let mut ws = [0i64; 64];
    for col in 0..8 {
        let c = |row: usize| coef[row * 8 + col] as i64;
        if (1..8).all(|row| c(row) == 0) {
            let dc = c(0) << PASS1_BITS;
            for row in 0..8 {
                ws[row * 8 + col] = dc;
            }
            continue;
        }
        let r = line([c(0), c(1), c(2), c(3), c(4), c(5), c(6), c(7)]);
        for row in 0..8 {
            ws[row * 8 + col] = descale(r[row], CONST_BITS - PASS1_BITS);
        }
    }
    for row in 0..8 {
        let w = &ws[row * 8..row * 8 + 8];
        let dst = &mut out[row * stride..row * stride + 8];
        if w[1..].iter().all(|v| *v == 0) {
            dst.fill(sample(descale(w[0], PASS1_BITS + 3)));
            continue;
        }
        let r = line([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]]);
        for x in 0..8 {
            dst[x] = sample(descale(r[x], CONST_BITS + PASS1_BITS + 3));
        }
    }
}
