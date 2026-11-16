//! The system calls, declared directly (std only, as `procs.rs` of the program does for `prctl`). Everything that can allocate or fail in a way that needs a message happens in
//! the parent, before the fork; what runs in the child is `child`, which makes system calls and nothing else.

use crate::plan::{Limits, Plan};
use crate::Error;
use std::fs::OpenOptions;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;

extern "C" {
    fn syscall(num: i64, ...) -> i64;
    fn prctl(option: i32, a2: u64, a3: u64, a4: u64, a5: u64) -> i32;
    fn setrlimit(resource: i32, limit: *const Rlimit) -> i32;
    fn setsid() -> i32;
}

#[repr(C)]
struct Rlimit {
    cur: u64,
    max: u64,
}

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
}

#[repr(C, packed)]
struct PathBeneath {
    allowed_access: u64,
    parent_fd: i32,
}

#[repr(C)]
struct NetPort {
    allowed_access: u64,
    port: u64,
}

// The numbers are the same on every architecture (the "unified" system call table).
const SYS_CLOSE_RANGE: i64 = 436;
const SYS_LANDLOCK_CREATE_RULESET: i64 = 444;
const SYS_LANDLOCK_ADD_RULE: i64 = 445;
const SYS_LANDLOCK_RESTRICT_SELF: i64 = 446;
const LANDLOCK_CREATE_RULESET_VERSION: u64 = 1;
const LANDLOCK_RULE_PATH_BENEATH: u64 = 1;
const LANDLOCK_RULE_NET_PORT: u64 = 2;
const PR_SET_PDEATHSIG: i32 = 1;
const PR_SET_NO_NEW_PRIVS: i32 = 38;
const CLOSE_RANGE_CLOEXEC: u32 = 4;
const O_PATH: i32 = 0o10_000_000;
const O_CLOEXEC: i32 = 0o2_000_000;
const SIGKILL: u64 = 9;
const RLIMIT_CPU: i32 = 0;
const RLIMIT_DATA: i32 = 2;
const RLIMIT_NOFILE: i32 = 7;
const RLIMIT_AS: i32 = 9;

fn errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// The Landlock ABI of this kernel; `Unsupported { have: 0 }` when there is none (not built in, or disabled at boot).
pub fn abi_version(need: u32) -> Result<u32, Error> {
    let r = unsafe { syscall(SYS_LANDLOCK_CREATE_RULESET, std::ptr::null::<RulesetAttr>(), 0usize, LANDLOCK_CREATE_RULESET_VERSION) };
    if r >= 0 {
        return Ok(r as u32);
    }
    match errno() {
        38 | 95 => Err(Error::Unsupported { have: 0, need }), // ENOSYS, EOPNOTSUPP
        e => Err(Error::Os("landlock_create_ruleset", e)),
    }
}

/// The ruleset with every rule of the plan added, as a descriptor that closes on exec; made in the parent.
pub fn build_ruleset(plan: &Plan) -> Result<OwnedFd, Error> {
    let attr = RulesetAttr { handled_access_fs: plan.handled_fs, handled_access_net: plan.handled_net };
    let fd = unsafe { syscall(SYS_LANDLOCK_CREATE_RULESET, &attr as *const RulesetAttr, std::mem::size_of::<RulesetAttr>(), 0u64) };
    if fd < 0 {
        return Err(Error::Os("landlock_create_ruleset", errno()));
    }
    let ruleset = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    for rule in &plan.paths {
        let file = OpenOptions::new().read(true).custom_flags(O_PATH | O_CLOEXEC).open(&rule.path).map_err(|e| Error::Os("open", e.raw_os_error().unwrap_or(0)))?;
        let beneath = PathBeneath { allowed_access: rule.access, parent_fd: file.as_raw_fd() };
        let r = unsafe { syscall(SYS_LANDLOCK_ADD_RULE, ruleset.as_raw_fd(), LANDLOCK_RULE_PATH_BENEATH, &beneath as *const PathBeneath, 0u32) };
        if r != 0 {
            return Err(Error::Os("landlock_add_rule", errno()));
        }
    }
    for rule in &plan.ports {
        let net = NetPort { allowed_access: rule.access, port: rule.port as u64 };
        let r = unsafe { syscall(SYS_LANDLOCK_ADD_RULE, ruleset.as_raw_fd(), LANDLOCK_RULE_NET_PORT, &net as *const NetPort, 0u32) };
        if r != 0 {
            return Err(Error::Os("landlock_add_rule", errno()));
        }
    }
    Ok(ruleset)
}

fn limit(resource: i32, value: Option<u64>) -> io::Result<()> {
    if let Some(v) = value {
        let l = Rlimit { cur: v, max: v };
        if unsafe { setrlimit(resource, &l) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Between the fork and the program: system calls only, no allocation (an error is a raw `errno`). The order: no new privileges, the limits, the ruleset, a new session, death
/// with the parent, and every descriptor above 2 marked close-on-exec.
pub fn child(ruleset: i32, limits: &Limits) -> io::Result<()> {
    unsafe {
        if prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
        limit(RLIMIT_AS, limits.address_space)?;
        limit(RLIMIT_DATA, limits.data)?;
        limit(RLIMIT_CPU, limits.cpu_seconds)?;
        limit(RLIMIT_NOFILE, limits.open_files)?;
        if syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset, 0u32) != 0 {
            return Err(io::Error::last_os_error());
        }
        if setsid() == -1 {
            return Err(io::Error::last_os_error());
        }
        if prctl(PR_SET_PDEATHSIG, SIGKILL, 0, 0, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
        if syscall(SYS_CLOSE_RANGE, 3u32, u32::MAX, CLOSE_RANGE_CLOEXEC) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
