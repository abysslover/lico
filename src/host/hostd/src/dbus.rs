//! A minimal D-Bus client over a Unix socket (EXTERNAL authentication, little-endian messages):
//! just enough to talk to a Secret Service. It has no dependency and no background thread.

use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::Duration;

extern "C" {
    fn getuid() -> u32;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    U32(u32),
    Str(String),
    Path(String),
    Sig(String),
    Variant(Box<Value>),
    Array(String, Vec<Value>), // element signature, elements
    Struct(Vec<Value>),
    DictEntry(Box<Value>, Box<Value>),
}

impl Value {
    pub fn sig(&self) -> String {
        match self {
            Value::Byte(_) => "y".into(),
            Value::Bool(_) => "b".into(),
            Value::U32(_) => "u".into(),
            Value::Str(_) => "s".into(),
            Value::Path(_) => "o".into(),
            Value::Sig(_) => "g".into(),
            Value::Variant(_) => "v".into(),
            Value::Array(element, _) => format!("a{element}"),
            Value::Struct(items) => format!("({})", items.iter().map(Value::sig).collect::<String>()),
            Value::DictEntry(k, v) => format!("{{{}{}}}", k.sig(), v.sig()),
        }
    }

    pub fn string(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Path(s) => Some(s),
            _ => None,
        }
    }

    pub fn items(&self) -> Option<&[Value]> {
        match self {
            Value::Struct(items) | Value::Array(_, items) => Some(items),
            _ => None,
        }
    }

    pub fn bytes(&self) -> Option<Vec<u8>> {
        match self {
            Value::Array(_, items) => items.iter().map(|v| if let Value::Byte(b) = v { Some(*b) } else { None }).collect(),
            _ => None,
        }
    }
}

pub fn byte_array(bytes: &[u8]) -> Value {
    Value::Array("y".into(), bytes.iter().map(|b| Value::Byte(*b)).collect())
}

fn alignment(first: char) -> usize {
    match first {
        'y' | 'g' | 'v' => 1,
        'b' | 'u' | 's' | 'o' | 'a' => 4,
        _ => 8, // structs and dictionary entries
    }
}

fn pad(buf: &mut Vec<u8>, to: usize) {
    while buf.len() % to != 0 {
        buf.push(0);
    }
}

fn marshal(buf: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Byte(b) => buf.push(*b),
        Value::Bool(b) => {
            pad(buf, 4);
            buf.extend_from_slice(&(*b as u32).to_le_bytes());
        }
        Value::U32(n) => {
            pad(buf, 4);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        Value::Str(s) | Value::Path(s) => {
            pad(buf, 4);
            buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
            buf.extend_from_slice(s.as_bytes());
            buf.push(0);
        }
        Value::Sig(s) => {
            buf.push(s.len() as u8);
            buf.extend_from_slice(s.as_bytes());
            buf.push(0);
        }
        Value::Variant(inner) => {
            marshal(buf, &Value::Sig(inner.sig()));
            marshal(buf, inner);
        }
        Value::Array(element, items) => {
            pad(buf, 4);
            let at = buf.len();
            buf.extend_from_slice(&[0; 4]);
            pad(buf, alignment(element.chars().next().unwrap_or('y')));
            let start = buf.len();
            for item in items {
                marshal(buf, item);
            }
            let length = (buf.len() - start) as u32;
            buf[at..at + 4].copy_from_slice(&length.to_le_bytes());
        }
        Value::Struct(items) => {
            pad(buf, 8);
            for item in items {
                marshal(buf, item);
            }
        }
        Value::DictEntry(k, v) => {
            pad(buf, 8);
            marshal(buf, k);
            marshal(buf, v);
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Reader<'_> {
    fn align(&mut self, to: usize) {
        while self.pos % to != 0 {
            self.pos += 1;
        }
    }

    fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.data.len()).ok_or(Error::Protocol("short message"))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        self.align(4);
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, Error> {
        let n = self.u32()? as usize;
        let text = String::from_utf8(self.take(n)?.to_vec()).map_err(|_| Error::Protocol("string is not UTF-8"))?;
        self.take(1)?;
        Ok(text)
    }

    fn signature(&mut self) -> Result<String, Error> {
        let n = self.take(1)?[0] as usize;
        let text = String::from_utf8(self.take(n)?.to_vec()).map_err(|_| Error::Protocol("signature is not UTF-8"))?;
        self.take(1)?;
        Ok(text)
    }

    /// Read one complete type from `sig` starting at `*at`.
    fn value(&mut self, sig: &[u8], at: &mut usize) -> Result<Value, Error> {
        self.depth += 1;
        let parsed = if self.depth > MAX_DEPTH { Err(Error::Protocol("nested too deeply")) } else { self.value_inner(sig, at) };
        self.depth -= 1;
        parsed
    }

    fn value_inner(&mut self, sig: &[u8], at: &mut usize) -> Result<Value, Error> {
        let c = *sig.get(*at).ok_or(Error::Protocol("bad signature"))? as char;
        *at += 1;
        Ok(match c {
            'y' => Value::Byte(self.take(1)?[0]),
            'b' => Value::Bool(self.u32()? != 0),
            'u' => Value::U32(self.u32()?),
            's' => Value::Str(self.string()?),
            'o' => Value::Path(self.string()?),
            'g' => Value::Sig(self.signature()?),
            'v' => {
                let inner = self.signature()?;
                let mut i = 0;
                Value::Variant(Box::new(self.value(inner.as_bytes(), &mut i)?))
            }
            'a' => {
                let start_of_element = *at;
                let end_of_element = complete_type_end(sig, start_of_element)?;
                let element = String::from_utf8_lossy(&sig[start_of_element..end_of_element]).to_string();
                *at = end_of_element;
                let length = self.u32()? as usize;
                self.align(alignment(element.chars().next().unwrap_or('y')));
                let end = self.pos + length;
                if end > self.data.len() || length > MAX_MESSAGE {
                    return Err(Error::Protocol("array too long"));
                }
                let mut items = Vec::new();
                while self.pos < end {
                    let mut i = 0;
                    items.push(self.value(element.as_bytes(), &mut i)?);
                }
                Value::Array(element, items)
            }
            '(' => {
                self.align(8);
                let mut items = Vec::new();
                while sig.get(*at) != Some(&b')') {
                    items.push(self.value(sig, at)?);
                }
                *at += 1;
                Value::Struct(items)
            }
            '{' => {
                self.align(8);
                let k = self.value(sig, at)?;
                let v = self.value(sig, at)?;
                if sig.get(*at) != Some(&b'}') {
                    return Err(Error::Protocol("bad dictionary entry"));
                }
                *at += 1;
                Value::DictEntry(Box::new(k), Box::new(v))
            }
            _ => return Err(Error::Protocol("unsupported type")),
        })
    }
}

fn complete_type_end(sig: &[u8], start: usize) -> Result<usize, Error> {
    let first = *sig.get(start).ok_or(Error::Protocol("bad signature"))?;
    match first {
        b'a' => complete_type_end(sig, start + 1),
        b'(' | b'{' => {
            let (open, close) = if first == b'(' { (b'(', b')') } else { (b'{', b'}') };
            let mut depth = 0;
            for (i, c) in sig.iter().enumerate().skip(start) {
                if *c == open {
                    depth += 1;
                } else if *c == close {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(i + 1);
                    }
                }
            }
            Err(Error::Protocol("unbalanced signature"))
        }
        _ => Ok(start + 1),
    }
}

const MAX_MESSAGE: usize = 1 << 20;
const MAX_DEPTH: usize = 16; // a hostile peer cannot overflow the stack with nested containers
const CALL_DEADLINE: Duration = Duration::from_secs(20); // a peer that streams unrelated messages cannot hold a call forever

#[derive(Debug)]
pub enum Error {
    /// The bus could not be reached.
    NoBus,
    /// The peer answered with a D-Bus error (its name).
    Remote(String),
    Protocol(&'static str),
}

pub struct Conn {
    stream: UnixStream,
    serial: u32,
}

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn connect_address(address: &str) -> Option<UnixStream> {
    for candidate in address.split(';') {
        let Some(options) = candidate.strip_prefix("unix:") else { continue };
        for option in options.split(',') {
            if let Some(path) = option.strip_prefix("path=") {
                if let Ok(stream) = UnixStream::connect(unescape(path)) {
                    return Some(stream);
                }
            } else if let Some(name) = option.strip_prefix("abstract=") {
                if let Ok(addr) = SocketAddr::from_abstract_name(unescape(name).as_bytes()) {
                    if let Ok(stream) = UnixStream::connect_addr(&addr) {
                        return Some(stream);
                    }
                }
            }
        }
    }
    None
}

impl Conn {
    /// Connect to the session bus named by `DBUS_SESSION_BUS_ADDRESS`, or else `$XDG_RUNTIME_DIR/bus`.
    pub fn session() -> Result<Conn, Error> {
        let stream = match std::env::var("DBUS_SESSION_BUS_ADDRESS") {
            Ok(address) if !address.is_empty() => connect_address(&address),
            _ => std::env::var("XDG_RUNTIME_DIR").ok().filter(|d| !d.is_empty()).and_then(|d| UnixStream::connect(format!("{d}/bus")).ok()),
        }
        .ok_or(Error::NoBus)?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(|_| Error::NoBus)?;
        stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(|_| Error::NoBus)?;
        let mut conn = Conn { stream, serial: 0 };
        conn.authenticate()?;
        conn.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "Hello", &[])?;
        Ok(conn)
    }

    fn authenticate(&mut self) -> Result<(), Error> {
        let uid = unsafe { getuid() }.to_string();
        let hex: String = uid.bytes().map(|b| format!("{b:02x}")).collect();
        self.stream.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes()).map_err(|_| Error::NoBus)?;
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            if line.len() > 256 || self.stream.read(&mut byte).map_err(|_| Error::NoBus)? == 0 {
                return Err(Error::NoBus);
            }
            line.push(byte[0]);
        }
        if !line.starts_with(b"OK") {
            return Err(Error::Protocol("the bus refused the authentication"));
        }
        self.stream.write_all(b"BEGIN\r\n").map_err(|_| Error::NoBus)
    }

    /// Call a method and return the values of its reply.
    pub fn call(&mut self, destination: &str, path: &str, interface: &str, member: &str, body: &[Value]) -> Result<Vec<Value>, Error> {
        self.serial += 1;
        let serial = self.serial;
        let mut payload = Vec::new();
        for value in body {
            marshal(&mut payload, value);
        }
        let fields = |code: u8, value: Value| Value::Struct(vec![Value::Byte(code), Value::Variant(Box::new(value))]);
        let mut header_fields = vec![
            fields(1, Value::Path(path.into())),
            fields(2, Value::Str(interface.into())),
            fields(3, Value::Str(member.into())),
            fields(6, Value::Str(destination.into())),
        ];
        if !body.is_empty() {
            header_fields.push(fields(8, Value::Sig(body.iter().map(Value::sig).collect())));
        }
        let mut message = vec![b'l', 1, 0, 1];
        message.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        message.extend_from_slice(&serial.to_le_bytes());
        marshal(&mut message, &Value::Array("(yv)".into(), header_fields));
        pad(&mut message, 8);
        message.extend_from_slice(&payload);
        self.stream.write_all(&message).map_err(|_| Error::NoBus)?;
        for byte in payload.iter_mut() {
            *byte = 0; // the payload may hold a secret
        }
        let deadline = std::time::Instant::now() + CALL_DEADLINE;
        loop {
            if std::time::Instant::now() > deadline {
                return Err(Error::NoBus);
            }
            let (kind, reply_serial, error_name, signature, mut reply_body) = self.read_message()?;
            if reply_serial != Some(serial) || (kind != 2 && kind != 3) {
                continue; // a signal or a message for someone else
            }
            if kind == 3 {
                wipe(&mut reply_body);
                return Err(Error::Remote(error_name.unwrap_or_default()));
            }
            let mut reader = Reader { data: &reply_body, pos: 0, depth: 0 };
            let sig = signature.unwrap_or_default();
            let mut values = Vec::new();
            let mut at = 0;
            let parsed = (|| {
                while at < sig.len() {
                    values.push(reader.value(sig.as_bytes(), &mut at)?);
                }
                Ok(())
            })();
            wipe(&mut reply_body);
            parsed?;
            return Ok(values);
        }
    }

    #[allow(clippy::type_complexity)]
    fn read_message(&mut self) -> Result<(u8, Option<u32>, Option<String>, Option<String>, Vec<u8>), Error> {
        let mut fixed = [0u8; 16];
        self.stream.read_exact(&mut fixed).map_err(|_| Error::NoBus)?;
        if fixed[0] != b'l' {
            return Err(Error::Protocol("unsupported byte order"));
        }
        let body_len = u32::from_le_bytes(fixed[4..8].try_into().unwrap()) as usize;
        let fields_len = u32::from_le_bytes(fixed[12..16].try_into().unwrap()) as usize;
        if body_len > MAX_MESSAGE || fields_len > MAX_MESSAGE {
            return Err(Error::Protocol("message too large"));
        }
        let header_end = (16 + fields_len + 7) / 8 * 8;
        let mut rest = vec![0u8; header_end - 16 + body_len];
        self.stream.read_exact(&mut rest).map_err(|_| Error::NoBus)?;
        let mut whole = fixed.to_vec();
        whole.extend_from_slice(&rest);
        let mut reader = Reader { data: &whole, pos: 16, depth: 0 };
        let (mut reply_serial, mut error_name, mut signature) = (None, None, None);
        while reader.pos < 16 + fields_len {
            reader.align(8);
            let code = reader.take(1)?[0];
            let inner = reader.signature()?;
            let mut i = 0;
            let value = reader.value(inner.as_bytes(), &mut i)?;
            match (code, value) {
                (5, Value::U32(n)) => reply_serial = Some(n),
                (4, Value::Str(s)) => error_name = Some(s),
                (8, Value::Sig(s)) => signature = Some(s),
                _ => {}
            }
        }
        let body = whole[header_end..].to_vec();
        wipe(&mut whole);
        Ok((fixed[1], reply_serial, error_name, signature, body))
    }
}

/// Overwrite a buffer that may have held a secret (best effort; the compiler may not elide it).
pub fn wipe(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}
