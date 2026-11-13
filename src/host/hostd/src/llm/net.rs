//! The listening TCP sockets of this machine, read from the process table (`/proc/net/tcp` and `tcp6`), and which of them belong to processes of the same user (the socket
//! inodes under `/proc/<pid>/fd`). The root of the table is a parameter, so that a test can give a table it wrote and never the real one.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

extern "C" {
    fn geteuid() -> u32;
}

/// How many process folders and how many descriptors of one process are looked at at the most.
const MAX_PROCESSES: usize = 32_768;
const MAX_FDS: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bind {
    Loopback,
    All,
    Address,
}

impl Bind {
    pub fn word(self) -> &'static str {
        match self {
            Bind::Loopback => "loopback",
            Bind::All => "all",
            Bind::Address => "address",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Listener {
    pub ip: IpAddr,
    pub port: u16,
    pub inode: u64,
    pub bind: Bind,
}

impl Listener {
    /// The address the probe connects to: this machine's own, never another. A bind to everything is reached through loopback; a bind to one interface is reached there.
    pub fn target(&self) -> SocketAddr {
        let ip = match (self.bind, self.ip) {
            (Bind::Address, ip) => ip,
            (_, IpAddr::V4(_)) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            (_, IpAddr::V6(_)) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        };
        SocketAddr::new(ip, self.port)
    }
}

fn v4(hex: &str) -> Option<Ipv4Addr> {
    // the kernel prints the address as the number that its four bytes make on a little-endian machine: 127.0.0.1 is 0100007F
    (hex.len() == 8).then(|| u32::from_str_radix(hex, 16).ok()).flatten().map(|n| Ipv4Addr::from(n.swap_bytes()))
}

fn v6(hex: &str) -> Option<Ipv6Addr> {
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0u8; 16];
    for word in 0..4 {
        let n = u32::from_str_radix(&hex[word * 8..word * 8 + 8], 16).ok()?;
        bytes[word * 4..word * 4 + 4].copy_from_slice(&n.swap_bytes().to_be_bytes());
    }
    Some(Ipv6Addr::from(bytes))
}

fn bind_of(ip: IpAddr) -> Bind {
    let (loopback, unspecified) = match ip {
        IpAddr::V4(a) => (a.is_loopback(), a.is_unspecified()),
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(m) => (m.is_loopback(), m.is_unspecified()),
            None => (a.is_loopback(), a.is_unspecified()),
        },
    };
    if loopback {
        Bind::Loopback
    } else if unspecified {
        Bind::All
    } else {
        Bind::Address
    }
}

/// The sockets in the LISTEN state (`0A`) of one table (`ipv6`: `tcp6`).
pub fn parse_table(text: &str, ipv6: bool) -> Vec<Listener> {
    let mut found = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 || fields[3] != "0A" {
            continue;
        }
        let Some((address, port)) = fields[1].rsplit_once(':') else { continue };
        let ip = if ipv6 { v6(address).map(IpAddr::V6) } else { v4(address).map(IpAddr::V4) };
        let (Some(ip), Ok(port), Ok(inode)) = (ip, u16::from_str_radix(port, 16), fields[9].parse::<u64>()) else { continue };
        found.push(Listener { ip, port, inode, bind: bind_of(ip) });
    }
    found
}

/// The listening sockets of the machine under `root`; `None` when neither table can be read (the answer then says it does not know).
pub fn listeners(root: &Path) -> Option<Vec<Listener>> {
    let (a, b) = (std::fs::read_to_string(root.join("net/tcp")), std::fs::read_to_string(root.join("net/tcp6")));
    if a.is_err() && b.is_err() {
        return None;
    }
    let mut all = parse_table(&a.unwrap_or_default(), false);
    all.extend(parse_table(&b.unwrap_or_default(), true));
    Some(all)
}

fn clean_name(raw: &str) -> String {
    raw.trim().chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(32).collect()
}

/// The socket inodes held by processes of this user, with the name of the process (`comm`, nothing else of it): a process of another user has an fd folder that cannot be read,
/// and is skipped, which is how the default scope stays the user's own.
pub fn owners(root: &Path) -> HashMap<u64, String> {
    let me = unsafe { geteuid() };
    let mut found = HashMap::new();
    let Ok(dir) = std::fs::read_dir(root) else { return found };
    for entry in dir.flatten().take(MAX_PROCESSES) {
        if entry.file_name().to_str().map_or(true, |n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        if entry.metadata().map_or(true, |m| m.uid() != me) {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        let name = std::fs::read_to_string(entry.path().join("comm")).map(|c| clean_name(&c)).unwrap_or_default();
        for fd in fds.flatten().take(MAX_FDS) {
            let Ok(link) = std::fs::read_link(fd.path()) else { continue };
            if let Some(inode) = link.to_string_lossy().strip_prefix("socket:[").and_then(|r| r.strip_suffix(']')).and_then(|n| n.parse::<u64>().ok()) {
                found.entry(inode).or_insert_with(|| name.clone());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

    #[test]
    fn the_table_gives_listening_sockets_with_their_address_port_inode_and_bind() {
        let text = format!(
            "{HEADER}   0: 0100007F:2CAF 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 12345 1 0\n\
                         1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 22 1 0\n\
                         2: 0100007F:2CAF 0100007F:9999 01 00000000:00000000 00:00000000 00000000  1000        0 99 1 0\n\
                         3: 0B14000A:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 33 1 0\n\
                         bad line\n"
        );
        let got = parse_table(&text, false);
        assert_eq!(got.len(), 3, "the established socket is not a listener, the bad line is skipped");
        assert_eq!((got[0].ip, got[0].port, got[0].inode, got[0].bind), (IpAddr::V4(Ipv4Addr::LOCALHOST), 11439, 12345, Bind::Loopback));
        assert_eq!((got[1].port, got[1].bind), (22, Bind::All));
        assert_eq!((got[2].ip, got[2].port, got[2].bind), (IpAddr::V4(Ipv4Addr::new(10, 0, 20, 11)), 8080, Bind::Address));
    }

    #[test]
    fn the_ipv6_table_reads_its_four_words_and_knows_loopback_the_wildcard_and_the_mapped_forms() {
        let row = |addr: &str, port: &str, inode: u64| format!("   0: {addr}:{port} 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 {inode} 1 0\n");
        let text = format!(
            "{HEADER}{}{}{}{}",
            row("00000000000000000000000001000000", "2CAF", 1),
            row("00000000000000000000000000000000", "1F90", 2),
            row("0000000000000000FFFF00000100007F", "0050", 3),
            row("B80D0120000000000000000001000000", "0051", 4)
        );
        let got = parse_table(&text, true);
        assert_eq!(got.len(), 4);
        assert_eq!((got[0].ip, got[0].bind), (IpAddr::V6(Ipv6Addr::LOCALHOST), Bind::Loopback));
        assert_eq!((got[1].ip, got[1].bind), (IpAddr::V6(Ipv6Addr::UNSPECIFIED), Bind::All));
        assert_eq!(got[2].bind, Bind::Loopback, "::ffff:127.0.0.1");
        assert_eq!((got[3].ip, got[3].bind), (IpAddr::V6("2001:db8::1".parse().unwrap()), Bind::Address));
    }

    #[test]
    fn the_probe_address_is_always_the_machines_own() {
        let l = |ip: &str, bind| Listener { ip: ip.parse().unwrap(), port: 80, inode: 1, bind };
        assert_eq!(l("0.0.0.0", Bind::All).target(), "127.0.0.1:80".parse().unwrap());
        assert_eq!(l("127.0.0.2", Bind::Loopback).target(), "127.0.0.1:80".parse().unwrap());
        assert_eq!(l("::", Bind::All).target(), "[::1]:80".parse().unwrap());
        assert_eq!(l("10.0.20.11", Bind::Address).target(), "10.0.20.11:80".parse().unwrap(), "a bind to one interface is reached there");
    }

    #[test]
    fn the_owners_are_the_inodes_of_readable_fd_folders_and_a_folder_nobody_may_read_is_skipped() {
        let root = std::env::temp_dir().join(format!("lico-llm-net-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (pid, comm, inodes) in [("100", "llama-server", vec![11u64, 12]), ("200", "other-user", vec![33])] {
            let fd = root.join(pid).join("fd");
            std::fs::create_dir_all(&fd).unwrap();
            std::fs::write(root.join(pid).join("comm"), format!("{comm}\n")).unwrap();
            for (i, inode) in inodes.iter().enumerate() {
                std::os::unix::fs::symlink(format!("socket:[{inode}]"), fd.join(format!("{}", 3 + i))).unwrap();
            }
        }
        std::os::unix::fs::symlink("/dev/null", root.join("100/fd/9")).unwrap();
        std::fs::create_dir_all(root.join("net")).unwrap();
        std::fs::set_permissions(root.join("200/fd"), std::os::unix::fs::PermissionsExt::from_mode(0)).unwrap();
        let got = owners(&root);
        assert_eq!(got.get(&11).map(String::as_str), Some("llama-server"));
        assert_eq!(got.get(&12).map(String::as_str), Some("llama-server"));
        assert!(!got.contains_key(&33), "the fd folder of the other process could not be read");
        std::fs::set_permissions(root.join("200/fd"), std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        assert!(listeners(&root).is_none(), "no table: unknown, not empty");
        let _ = std::fs::remove_dir_all(&root);
    }
}
