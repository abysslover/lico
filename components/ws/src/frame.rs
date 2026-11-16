//! The pure part: frames in and out (RFC 6455 section 5), with no socket. `Decoder` takes bytes as they come and gives whole messages; every rule of the RFC that a peer
//! can break is an error, and a cap is checked from the length in the header, before the payload is buffered.

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Continuation = 0,
    Text = 1,
    Binary = 2,
    Close = 8,
    Ping = 9,
    Pong = 10,
}

/// Which end the decoder reads for: a client reads unmasked frames from a server, a server reads masked frames from a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Client,
    Server,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close { code: Option<u16>, reason: String },
}

/// One frame as bytes: FIN as given, no reserved bits, a mask only if one is given.
pub fn encode_frame(fin: bool, op: Op, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(if fin { 0x80 } else { 0 } | op as u8);
    let bit = if mask.is_some() { 0x80 } else { 0 };
    match payload.len() {
        n if n <= 125 => out.push(bit | n as u8),
        n if n <= 0xFFFF => {
            out.push(bit | 126);
            out.extend((n as u16).to_be_bytes());
        }
        n => {
            out.push(bit | 127);
            out.extend((n as u64).to_be_bytes());
        }
    }
    match mask {
        Some(m) => {
            out.extend(m);
            out.extend(payload.iter().enumerate().map(|(i, b)| b ^ m[i % 4]));
        }
        None => out.extend(payload),
    }
    out
}

pub struct Decoder {
    role: Role,
    max_message: usize,
    buf: Vec<u8>,
    open: Option<(bool, Vec<u8>)>, // a message that is not finished: is it text, and what has come
    failed: bool,
}

impl Decoder {
    pub fn new(role: Role, max_message: usize) -> Decoder {
        Decoder { role, max_message, buf: Vec::new(), open: None, failed: false }
    }

    /// The messages that these bytes finish, in order; the first error ends the stream (the same decoder answers `Closed` from then on).
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Result<Incoming, Error>> {
        if self.failed {
            return vec![Err(Error::Closed)];
        }
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            match self.next_frame() {
                Ok(Some(incoming)) => {
                    if let Some(i) = incoming {
                        out.push(Ok(i));
                    }
                }
                Ok(None) => return out,
                Err(e) => {
                    self.failed = true;
                    self.buf = Vec::new();
                    out.push(Err(e));
                    return out;
                }
            }
        }
    }

    /// `Ok(None)`: a frame is not whole yet. `Ok(Some(None))`: a frame was taken and gave no message (a fragment). `Ok(Some(Some(m)))`: a message.
    fn next_frame(&mut self) -> Result<Option<Option<Incoming>>, Error> {
        let b = &self.buf;
        if b.len() < 2 {
            return Ok(None);
        }
        let (fin, rsv, opcode) = (b[0] & 0x80 != 0, b[0] & 0x70, b[0] & 0x0f);
        let (masked, len7) = (b[1] & 0x80 != 0, (b[1] & 0x7f) as usize);
        if rsv != 0 {
            return Err(Error::Protocol("reserved bits are set"));
        }
        let op = match opcode {
            0 => Op::Continuation,
            1 => Op::Text,
            2 => Op::Binary,
            8 => Op::Close,
            9 => Op::Ping,
            10 => Op::Pong,
            _ => return Err(Error::Protocol("an opcode that does not exist")),
        };
        match (self.role, masked) {
            (Role::Server, false) => return Err(Error::Protocol("an unmasked frame from a client")),
            (Role::Client, true) => return Err(Error::Protocol("a masked frame from a server")),
            _ => {}
        }
        let control = opcode >= 8;
        if control && !fin {
            return Err(Error::Protocol("a fragmented control frame"));
        }
        if control && len7 > 125 {
            return Err(Error::Protocol("a control frame over 125 bytes"));
        }
        let (ext, len) = match len7 {
            126 => {
                if b.len() < 4 {
                    return Ok(None);
                }
                (2, u16::from_be_bytes([b[2], b[3]]) as u64)
            }
            127 => {
                if b.len() < 10 {
                    return Ok(None);
                }
                let n = u64::from_be_bytes([b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9]]);
                if n >> 63 != 0 {
                    return Err(Error::Protocol("a length with the top bit set"));
                }
                (8, n)
            }
            n => (0, n as u64),
        };
        if len > self.max_message as u64 {
            return Err(Error::TooBig); // from the header: the payload is never buffered
        }
        let len = len as usize;
        let head = 2 + ext + if masked { 4 } else { 0 };
        if b.len() < head + len {
            return Ok(None);
        }
        let mut payload = b[head..head + len].to_vec();
        if masked {
            let m = [b[head - 4], b[head - 3], b[head - 2], b[head - 1]];
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= m[i % 4];
            }
        }
        self.buf.drain(..head + len);
        match op {
            Op::Ping => Ok(Some(Some(Incoming::Ping(payload)))),
            Op::Pong => Ok(Some(Some(Incoming::Pong(payload)))),
            Op::Close => close_of(payload).map(|c| Some(Some(c))),
            Op::Text | Op::Binary => {
                if self.open.is_some() {
                    return Err(Error::Protocol("a new message while one is open"));
                }
                if fin {
                    finish(op == Op::Text, payload).map(|m| Some(Some(m)))
                } else {
                    self.open = Some((op == Op::Text, payload));
                    Ok(Some(None))
                }
            }
            Op::Continuation => {
                let Some((text, mut so_far)) = self.open.take() else {
                    return Err(Error::Protocol("a continuation with nothing to continue"));
                };
                if so_far.len() + payload.len() > self.max_message {
                    return Err(Error::TooBig);
                }
                so_far.extend(payload);
                if fin {
                    finish(text, so_far).map(|m| Some(Some(m)))
                } else {
                    self.open = Some((text, so_far));
                    Ok(Some(None))
                }
            }
        }
    }
}

fn finish(text: bool, payload: Vec<u8>) -> Result<Incoming, Error> {
    if text {
        String::from_utf8(payload).map(Incoming::Text).map_err(|_| Error::Protocol("text that is not UTF-8"))
    } else {
        Ok(Incoming::Binary(payload))
    }
}

fn close_of(payload: Vec<u8>) -> Result<Incoming, Error> {
    match payload.len() {
        0 => Ok(Incoming::Close { code: None, reason: String::new() }),
        1 => Err(Error::Protocol("a close frame with one byte")),
        _ => {
            let code = u16::from_be_bytes([payload[0], payload[1]]);
            if !matches!(code, 1000..=1003 | 1007..=1014 | 3000..=4999) {
                return Err(Error::Protocol("a close code that is not allowed on the wire"));
            }
            let reason = String::from_utf8(payload[2..].to_vec()).map_err(|_| Error::Protocol("a close reason that is not UTF-8"))?;
            Ok(Incoming::Close { code: Some(code), reason })
        }
    }
}
