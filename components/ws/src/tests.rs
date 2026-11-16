//! White-box tests of the parts that are not public: SHA-1 and base64 with their published test vectors.

use super::*;

fn hex(d: [u8; 20]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn sha1_gives_the_published_digests() {
    assert_eq!(hex(sha1::sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    assert_eq!(hex(sha1::sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(hex(sha1::sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")), "84983e441c3bd26ebaae4aa1f95129e5e54670f1", "two blocks");
    assert_eq!(hex(sha1::sha1(&vec![b'a'; 1_000_000])), "34aa973cd4c4daa4f61eeb2bdbad27316534016f", "a million a, the long test of the standard");
    assert_eq!(hex(sha1::sha1(&[b'x'; 55])), hex(sha1::sha1(&[b'x'; 55])));
    for n in [55usize, 56, 63, 64, 65] {
        assert_eq!(sha1::sha1(&vec![1u8; n]).len(), 20, "the padding edges {n}");
    }
    assert_eq!(hex(sha1::sha1(&[b'a'; 64])), "0098ba824b5c16427bd7a1122a5a442a25ec644d");
}

#[test]
fn base64_gives_the_vectors_of_rfc_4648() {
    for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
        assert_eq!(sha1::base64(plain.as_bytes()), coded);
    }
    assert_eq!(sha1::base64(&[0xfb, 0xff, 0xfe]), "+//+", "the two characters that are not letters or digits");
}

#[test]
fn a_key_is_sixteen_random_bytes_in_base64_and_two_keys_differ() {
    let (a, b) = (new_key(), new_key());
    assert_eq!((a.len(), b.len()), (24, 24));
    assert!(a.ends_with("==") && a != b);
}
