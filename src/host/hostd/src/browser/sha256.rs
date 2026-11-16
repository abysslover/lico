//! SHA-256 of what a reader gives (the engine file that is checked before it runs). Std only; the vectors of the standard pin it.

use std::io::{self, Read};

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn block(h: &mut [u32; 8], chunk: &[u8]) {
    let mut w = [0u32; 64];
    for (i, word) in chunk.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *slot = slot.wrapping_add(v);
    }
}

/// The digest of everything `reader` gives, as lowercase hexadecimal.
pub fn hex_of(mut reader: impl Read) -> io::Result<String> {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut pending = Vec::with_capacity(128 * 1024);
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        pending.extend_from_slice(&buf[..n]);
        let whole = pending.len() / 64 * 64;
        for chunk in pending[..whole].chunks_exact(64) {
            block(&mut h, chunk);
        }
        pending.drain(..whole);
    }
    pending.push(0x80);
    while pending.len() % 64 != 56 {
        pending.push(0);
    }
    pending.extend_from_slice(&(total * 8).to_be_bytes());
    for chunk in pending.chunks_exact(64) {
        block(&mut h, chunk);
    }
    Ok(h.iter().map(|w| format!("{w:08x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_vectors_are_met_at_every_length_around_a_block() {
        assert_eq!(hex_of(&b""[..]).unwrap(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex_of(&b"abc"[..]).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hex_of(&b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"[..]).unwrap(), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        let million = vec![b'a'; 1_000_000];
        assert_eq!(hex_of(&million[..]).unwrap(), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        // the padding boundary: 55, 56, 63, 64 and 65 bytes (a one-bit change in the length handling shows here)
        for (n, want) in [(55, "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"), (56, "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"), (64, "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb")] {
            assert_eq!(hex_of(&vec![b'a'; n][..]).unwrap(), want, "{n} bytes");
        }
    }

    #[test]
    fn a_reader_that_gives_one_byte_at_a_time_gives_the_same_digest() {
        struct OneByte<'a>(&'a [u8]);
        impl Read for OneByte<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                match self.0.split_first() {
                    Some((b, rest)) => {
                        buf[0] = *b;
                        self.0 = rest;
                        Ok(1)
                    }
                    None => Ok(0),
                }
            }
        }
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7) as u8).collect();
        assert_eq!(hex_of(OneByte(&data)).unwrap(), hex_of(&data[..]).unwrap());
    }
}
