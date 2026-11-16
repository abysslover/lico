//! The pure part: what a confinement asks for, checked, and turned into rules for a given Landlock ABI. No system call here; the one question it asks of the world (does a path
//! exist, and is it a file or a directory) is a function the caller passes, so that every rule and every refusal is tested without a kernel.

use std::collections::HashSet;
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// The first Landlock ABI with TCP port rules. Without them the child could connect anywhere on the machine, so a kernel with a lower ABI is refused, not run with less.
pub const MIN_ABI: u32 = 4;
/// The rules of a real confinement are a dozen (system directories, the program's directory, one scratch directory); more is a mistake or a leak.
pub const MAX_PATHS: usize = 64;
/// The port of a CDP endpoint and one or two ports of fixtures are all a browser tab needs.
pub const MAX_PORTS: usize = 16;
/// A program like an engine needs a token and a few switches; the environment of a user is hundreds of entries and is exactly what must not be passed.
pub const MAX_ENV: usize = 32;
/// The same reason, in bytes.
pub const MAX_ENV_BYTES: usize = 16 * 1024;

// The access rights of the kernel interface (include/uapi/linux/landlock.h).
const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
const FS_REFER: u64 = 1 << 13; // ABI 2
const FS_TRUNCATE: u64 = 1 << 14; // ABI 3
const FS_IOCTL_DEV: u64 = 1 << 15; // ABI 5
const FS_V1_ALL: u64 = (1 << 13) - 1; // the thirteen rights of ABI 1
const NET_BIND_TCP: u64 = 1 << 0; // ABI 4
const NET_CONNECT_TCP: u64 = 1 << 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    File,
    Dir,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    /// `RLIMIT_AS`: may not suit an engine that reserves virtual memory (a measurement says).
    pub address_space: Option<u64>,
    /// `RLIMIT_DATA`.
    pub data: Option<u64>,
    /// `RLIMIT_CPU`, in seconds.
    pub cpu_seconds: Option<u64>,
    /// `RLIMIT_NOFILE`.
    pub open_files: Option<u64>,
}

#[derive(Clone)]
pub struct Confinement {
    /// Read and execute: the program, its libraries.
    pub read_exec: Vec<PathBuf>,
    /// Read only: a data file, a font directory.
    pub read: Vec<PathBuf>,
    /// Read, write, create, remove (but not execute): one scratch directory.
    pub read_write: Vec<PathBuf>,
    pub connect_ports: Vec<u16>,
    pub bind_ports: Vec<u16>,
    /// The whole environment of the child: nothing else is passed, `PATH` included.
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub limits: Limits,
    pub min_abi: u32,
}

impl Default for Confinement {
    fn default() -> Confinement {
        Confinement { read_exec: Vec::new(), read: Vec::new(), read_write: Vec::new(), connect_ports: Vec::new(), bind_ports: Vec::new(), env: Vec::new(), cwd: PathBuf::from("/"), limits: Limits::default(), min_abi: MIN_ABI }
    }
}

/// `Debug` shows the names of the environment variables and never a value (the token travels there).
impl fmt::Debug for Confinement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.env.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("Confinement")
            .field("read_exec", &self.read_exec)
            .field("read", &self.read)
            .field("read_write", &self.read_write)
            .field("connect_ports", &self.connect_ports)
            .field("bind_ports", &self.bind_ports)
            .field("env_names", &names)
            .field("cwd", &self.cwd)
            .field("limits", &self.limits)
            .field("min_abi", &self.min_abi)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The kernel has Landlock ABI `have` (0: none) and `need` was asked.
    Unsupported { have: u32, need: u32 },
    Path(&'static str),
    Port(&'static str),
    TooMany(&'static str),
    Env(&'static str),
    /// A system call failed: its name and the `errno`.
    Os(&'static str, i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported { have, need } => write!(f, "this kernel has Landlock ABI {have}, {need} is needed"),
            Error::Path(why) => write!(f, "path: {why}"),
            Error::Port(why) => write!(f, "port: {why}"),
            Error::TooMany(what) => write!(f, "too many {what}"),
            Error::Env(why) => write!(f, "environment: {why}"),
            Error::Os(call, errno) => write!(f, "{call} failed (errno {errno})"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathRule {
    pub path: PathBuf,
    pub access: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortRule {
    pub port: u16,
    pub access: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub handled_fs: u64,
    pub handled_net: u64,
    pub paths: Vec<PathRule>,
    pub ports: Vec<PortRule>,
}

/// The file-system rights this crate asks the kernel to handle (to deny unless a rule grants them): the ones of the ABI, as far as the crate knows them.
pub fn handled_fs(abi: u32) -> u64 {
    match abi {
        0 => 0,
        1 => FS_V1_ALL,
        2 => FS_V1_ALL | FS_REFER,
        3 | 4 => FS_V1_ALL | FS_REFER | FS_TRUNCATE,
        _ => FS_V1_ALL | FS_REFER | FS_TRUNCATE | FS_IOCTL_DEV,
    }
}

/// The network rights handled: TCP bind and connect from ABI 4.
pub fn handled_net(abi: u32) -> u64 {
    if abi >= 4 {
        NET_BIND_TCP | NET_CONNECT_TCP
    } else {
        0
    }
}

#[derive(Clone, Copy)]
enum Use {
    ReadExec,
    Read,
    ReadWrite,
}

fn rights(use_: Use, kind: PathKind, abi: u32) -> u64 {
    match (use_, kind) {
        (Use::ReadExec, PathKind::Dir) => FS_EXECUTE | FS_READ_FILE | FS_READ_DIR,
        (Use::ReadExec, PathKind::File) => FS_EXECUTE | FS_READ_FILE,
        (Use::Read, PathKind::Dir) => FS_READ_FILE | FS_READ_DIR,
        (Use::Read, PathKind::File) => FS_READ_FILE,
        (Use::ReadWrite, PathKind::Dir) => handled_fs(abi) & !FS_EXECUTE,
        (Use::ReadWrite, PathKind::File) => {
            let mut r = FS_WRITE_FILE | FS_READ_FILE;
            if abi >= 3 {
                r |= FS_TRUNCATE;
            }
            if abi >= 5 {
                r |= FS_IOCTL_DEV;
            }
            r
        }
    }
}

fn clean_absolute(p: &Path) -> Result<(), Error> {
    if p.as_os_str().as_bytes().contains(&0) {
        return Err(Error::Path("a path with a NUL"));
    }
    if !p.is_absolute() {
        return Err(Error::Path("a path that is not absolute"));
    }
    let mut normal = 0;
    for c in p.components() {
        match c {
            Component::RootDir => {}
            Component::Normal(_) => normal += 1,
            _ => return Err(Error::Path("a path with . or .. in it")),
        }
    }
    if normal == 0 {
        return Err(Error::Path("the root directory confines nothing"));
    }
    Ok(())
}

/// Checks everything and gives the rules for a kernel of this `abi`. `inspect` says what a path is (`None`: it does not exist).
pub fn plan(c: &Confinement, abi: u32, inspect: &dyn Fn(&Path) -> Option<PathKind>) -> Result<Plan, Error> {
    if abi < c.min_abi {
        return Err(Error::Unsupported { have: abi, need: c.min_abi });
    }
    let has_ports = !c.connect_ports.is_empty() || !c.bind_ports.is_empty();
    if has_ports && abi < MIN_ABI {
        return Err(Error::Unsupported { have: abi, need: MIN_ABI });
    }
    if c.read_exec.len() + c.read.len() + c.read_write.len() > MAX_PATHS {
        return Err(Error::TooMany("paths"));
    }
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for (list, use_) in [(&c.read_exec, Use::ReadExec), (&c.read, Use::Read), (&c.read_write, Use::ReadWrite)] {
        for p in list {
            clean_absolute(p)?;
            if !seen.insert(p.clone()) {
                return Err(Error::Path("a path listed twice"));
            }
            let kind = inspect(p).ok_or(Error::Path("a path that does not exist"))?;
            paths.push(PathRule { path: p.clone(), access: rights(use_, kind, abi) });
        }
    }
    let mut ports: Vec<PortRule> = Vec::new();
    for (list, access) in [(&c.connect_ports, NET_CONNECT_TCP), (&c.bind_ports, NET_BIND_TCP)] {
        let mut in_list = HashSet::new();
        for &port in list {
            if port == 0 {
                return Err(Error::Port("port 0 is not a port"));
            }
            if !in_list.insert(port) {
                return Err(Error::Port("a port listed twice"));
            }
            match ports.iter_mut().find(|r| r.port == port) {
                Some(r) => r.access |= access,
                None => ports.push(PortRule { port, access }),
            }
        }
    }
    if ports.len() > MAX_PORTS {
        return Err(Error::TooMany("ports"));
    }
    clean_absolute_or_root(&c.cwd)?;
    if inspect(&c.cwd) != Some(PathKind::Dir) {
        return Err(Error::Path("the working directory is not an existing directory"));
    }
    if c.env.len() > MAX_ENV {
        return Err(Error::TooMany("environment entries"));
    }
    let mut names = HashSet::new();
    let mut bytes = 0;
    for (name, value) in &c.env {
        let mut chars = name.chars();
        let ok = chars.next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_') && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !ok {
            return Err(Error::Env("a variable name that is not [A-Za-z_][A-Za-z0-9_]*"));
        }
        if value.contains('\0') {
            return Err(Error::Env("a value with a NUL"));
        }
        if !names.insert(name.as_str()) {
            return Err(Error::Env("a name given twice"));
        }
        bytes += name.len() + value.len() + 2;
    }
    if bytes > MAX_ENV_BYTES {
        return Err(Error::TooMany("environment bytes"));
    }
    Ok(Plan { handled_fs: handled_fs(abi), handled_net: handled_net(abi), paths, ports })
}

/// The working directory may be `/` (it only has to be absolute and exist).
fn clean_absolute_or_root(p: &Path) -> Result<(), Error> {
    if p == Path::new("/") {
        return Ok(());
    }
    clean_absolute(p)
}
