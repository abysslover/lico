//! `lico-sandbox` (SPEC.md): starts a program as a child under a Landlock confinement (files by path, TCP connect and bind by port), a cleared environment, resource limits, its
//! own session and death with its parent. It fails closed: if the kernel cannot give what was asked, the program is not started. Std only.

mod plan;
mod sys;

pub use plan::{handled_fs, handled_net, plan, Confinement, Error, Limits, PathKind, PathRule, Plan, PortRule, MAX_ENV, MAX_ENV_BYTES, MAX_PATHS, MAX_PORTS, MIN_ABI};

use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

/// The Landlock ABI of this kernel (`Unsupported` when there is none).
pub fn abi_version() -> Result<u32, Error> {
    sys::abi_version(MIN_ABI)
}

fn inspect(p: &Path) -> Option<PathKind> {
    let m = std::fs::metadata(p).ok()?;
    Some(if m.is_dir() { PathKind::Dir } else { PathKind::File })
}

/// Configures `cmd` to start confined: the environment is cleared and set to exactly `c.env`, the working directory is `c.cwd`, and between the fork and the program the child
/// makes the system calls of `sys::child`. Everything is checked and built here in the parent, so that a refusal is an `Err` and no process: the paths are opened and the
/// ruleset is made before the fork. The caller chooses the standard streams and spawns.
pub fn confine(cmd: &mut Command, c: &Confinement) -> Result<(), Error> {
    let abi = abi_version()?;
    let plan = plan(c, abi, &inspect)?;
    let program = Path::new(cmd.get_program());
    if !program.is_absolute() {
        return Err(Error::Path("the program is not an absolute path"));
    }
    if !c.read_exec.iter().any(|d| program.starts_with(d)) {
        return Err(Error::Path("the program is not under a read_exec path: it could not run"));
    }
    let ruleset = sys::build_ruleset(&plan)?;
    cmd.env_clear();
    for (name, value) in &c.env {
        cmd.env(name, value);
    }
    cmd.current_dir(&c.cwd);
    let limits = c.limits.clone();
    // the closure owns the ruleset descriptor, so that it lives as long as the command
    unsafe {
        cmd.pre_exec(move || sys::child(ruleset.as_raw_fd(), &limits));
    }
    Ok(())
}
