//! Base64 of our own (RFC 4648, the standard alphabet, with `=` padding), for the picture block of a message to Claude (plan M02-P04, S05a).
//!
//! No crate is added for it. The output is built once, at its final size, so a 3 MiB picture costs one 4 MiB string and no copy.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The base64 text of `data`: 4 characters for each 3 bytes, the last group padded with `=`.
pub fn encode(data: &[u8]) -> String {
    let mut out = Vec::with_capacity(data.len().div_ceil(3) * 4);
    for group in data.chunks(3) {
        let (a, b, c) = (group[0] as u32, *group.get(1).unwrap_or(&0) as u32, *group.get(2).unwrap_or(&0) as u32);
        let bits = (a << 16) | (b << 8) | c;
        out.push(ALPHABET[(bits >> 18) as usize & 63]);
        out.push(ALPHABET[(bits >> 12) as usize & 63]);
        out.push(if group.len() > 1 { ALPHABET[(bits >> 6) as usize & 63] } else { b'=' });
        out.push(if group.len() > 2 { ALPHABET[bits as usize & 63] } else { b'=' });
    }
    String::from_utf8(out).expect("the alphabet is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vectors_of_rfc_4648() {
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(encode(plain.as_bytes()), coded, "{plain:?}");
        }
    }

    #[test]
    fn every_byte_value_and_every_length_modulo_three() {
        let all: Vec<u8> = (0..=255u8).collect();
        let coded = encode(&all);
        assert_eq!(coded.len(), 344);
        assert!(coded.starts_with("AAECAwQFBgcICQoL") && coded.ends_with("/P3+/w=="), "{coded}");
        let tail = |n: usize| encode(&all[..n]).chars().rev().take(2).collect::<String>();
        assert_eq!((tail(253).as_str(), tail(254).as_str(), tail(255).as_str()), ("==", "=0", "+3")); // 1, 2 and 0 bytes in the last group
    }
}
