//! Standard base64, shared by the browser adapter (a frame travels to the window as text) and the window (which turns it back into bytes). A leaf: it names no other module.

/// Standard base64 (the alphabet with `+` and `/`, `=` padding at the end); anything else is `None`. A frame is at most the 4 MiB of one WebSocket message, so this never grows
/// past 3 MiB.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let body = match bytes.len() % 4 {
        0 => bytes.strip_suffix(b"==").or_else(|| bytes.strip_suffix(b"=")).unwrap_or(bytes),
        _ => return None,
    };
    let mut out = Vec::with_capacity(body.len() / 4 * 3 + 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in body {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (acc == 0).then_some(out)
}

/// Standard base64 with padding (a frame travels to the window as text).
pub fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

