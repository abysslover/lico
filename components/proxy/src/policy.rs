//! The address policy as data, and the host rules: what a destination may be, decided before anything is connected.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Public,
    Loopback,
    Private,
    LinkLocal,
    Shared,
    Unspecified,
    Multicast,
    Broadcast,
    Documentation,
    Benchmark,
    Reserved,
}

/// What a connection may reach: only public addresses; with `allow_loopback` loopback as well; with `only_loopback` (0.2.0) loopback and NOTHING else, not even public (the
/// guard of a tab the person opted into this machine's own services: such a tab must never load a public page). Nothing else is ever allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    pub allow_loopback: bool,
    pub only_loopback: bool,
}

impl Policy {
    pub fn allows(&self, class: Class) -> bool {
        if self.only_loopback {
            return class == Class::Loopback;
        }
        class == Class::Public || (self.allow_loopback && class == Class::Loopback)
    }
}

fn classify_v4(a: Ipv4Addr) -> Class {
    let o = a.octets();
    match (o[0], o[1], o[2], o[3]) {
        (0, ..) => Class::Unspecified,
        (10, ..) => Class::Private,
        (100, 64..=127, ..) => Class::Shared,
        (127, ..) => Class::Loopback,
        (169, 254, ..) => Class::LinkLocal,
        (172, 16..=31, ..) => Class::Private,
        (192, 0, 0, _) => Class::Reserved,
        (192, 0, 2, _) => Class::Documentation,
        (192, 88, 99, _) => Class::Reserved,
        (192, 168, ..) => Class::Private,
        (198, 18..=19, ..) => Class::Benchmark,
        (198, 51, 100, _) => Class::Documentation,
        (203, 0, 113, _) => Class::Documentation,
        (255, 255, 255, 255) => Class::Broadcast,
        (224..=239, ..) => Class::Multicast,
        (240..=255, ..) => Class::Reserved,
        _ => Class::Public,
    }
}

fn classify_v6(a: Ipv6Addr) -> Class {
    let s = a.segments();
    if a == Ipv6Addr::UNSPECIFIED {
        return Class::Unspecified;
    }
    if a == Ipv6Addr::LOCALHOST {
        return Class::Loopback;
    }
    if let Some(v4) = a.to_ipv4_mapped() {
        return classify_v4(v4);
    }
    let embedded = |hi: u16, lo: u16| classify_v4(Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8));
    if s[0] == 0x64 && s[1] == 0xff9b {
        return if s[2..6] == [0, 0, 0, 0] { embedded(s[6], s[7]) } else if s[2] == 1 { Class::Reserved } else { Class::Reserved };
    }
    if s[0] == 0x2002 {
        return embedded(s[1], s[2]);
    }
    if s[..6] == [0, 0, 0, 0, 0, 0] {
        return Class::Reserved; // IPv4-compatible (deprecated) and the rest of ::/96
    }
    if s[0] & 0xfe00 == 0xfc00 || s[0] & 0xffc0 == 0xfec0 {
        return Class::Private;
    }
    if s[0] & 0xffc0 == 0xfe80 {
        return Class::LinkLocal;
    }
    if s[0] & 0xff00 == 0xff00 {
        return Class::Multicast;
    }
    if s[0] == 0x2001 && s[1] == 0x0db8 {
        return Class::Documentation;
    }
    if s[0] == 0x2001 && (s[1] == 0 || (s[1] == 2 && s[2] == 0)) {
        return Class::Reserved;
    }
    if s[0] == 0x0100 && s[1..4] == [0, 0, 0] {
        return Class::Reserved;
    }
    if s[0] & 0xe000 == 0x2000 {
        Class::Public
    } else {
        Class::Reserved
    }
}

/// The class of an address (the table of the SPEC); an IPv4 address inside an IPv6 one (mapped, NAT64, 6to4) is classified by the IPv4 address.
pub fn classify(ip: IpAddr) -> Class {
    match ip {
        IpAddr::V4(a) => classify_v4(a),
        IpAddr::V6(a) => classify_v6(a),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Host {
    Ip(IpAddr),
    Name(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostError {
    Empty,
    TooLong,
    NonAscii,
    /// A host made only of numbers (or `0x` numbers) that is not a strict dotted quad: the system lookup would read it as an address (`127.1`, `2130706433`, `0177.0.0.1`).
    Numeric,
    Invalid,
}

/// The longest host name (RFC 1035).
pub const MAX_HOST: usize = 253;

fn numeric_label(l: &str) -> bool {
    !l.is_empty() && (l.bytes().all(|b| b.is_ascii_digit()) || (l.len() > 2 && (l.starts_with("0x") || l.starts_with("0X")) && l[2..].bytes().all(|b| b.is_ascii_hexdigit())) || l == "0x" || l == "0X")
}

/// A host as the proxy accepts it, checked before any lookup. IPv6 literals come without their brackets.
pub fn check_host(host: &str) -> Result<Host, HostError> {
    if host.is_empty() {
        return Err(HostError::Empty);
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.len() > MAX_HOST {
        return Err(HostError::TooLong);
    }
    if !host.is_ascii() {
        return Err(HostError::NonAscii);
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(Host::Ip(ip));
    }
    if host.is_empty() {
        return Err(HostError::Empty);
    }
    if host.split('.').all(numeric_label) {
        return Err(HostError::Numeric);
    }
    for label in host.split('.') {
        let ok = !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(HostError::Invalid);
        }
    }
    Ok(Host::Name(host.to_ascii_lowercase()))
}
