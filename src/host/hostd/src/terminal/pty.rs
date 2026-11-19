//! The pty and the shell on its other side (M23-P02): the one file of the terminal that calls the system, through `extern "C"` as `procs.rs` does (no `libc` crate). A
//! master that never blocks (a reader polls it, a write waits a bounded time), a size that can be set and read, and a spawn that makes the shell the leader of a new session
//! with the pty as its controlling terminal. The numbers below are those of Linux on x86-64 and arm64 (the test `the_size_round_trips` fails on a platform where they are not).

use std::ffi::{c_char, CStr};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const O_RDWR: i32 = 2;
const O_NOCTTY: i32 = 0o400;
const O_NONBLOCK: i32 = 0o4000;
const O_CLOEXEC: i32 = 0o2000000;
const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
#[cfg(test)]
const TIOCGWINSZ: u64 = 0x5413;
const TIOCSWINSZ: u64 = 0x5414;
const TIOCSCTTY: u64 = 0x540E;
const POLLIN: i16 = 1;
const POLLOUT: i16 = 4;

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
#[derive(Default)]
struct WinSize {
    rows: u16,
    cols: u16,
    xpixel: u16,
    ypixel: u16,
}

extern "C" {
    fn posix_openpt(flags: i32) -> i32;
    fn grantpt(fd: i32) -> i32;
    fn unlockpt(fd: i32) -> i32;
    fn ptsname_r(fd: i32, buf: *mut c_char, len: usize) -> i32;
    fn ioctl(fd: i32, request: u64, ...) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
    fn poll(fds: *mut PollFd, count: u64, timeout_ms: i32) -> i32;
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
    fn setsid() -> i32;
    fn tcgetpgrp(fd: i32) -> i32;
    fn signal(signum: i32, handler: usize) -> usize;
}

/// The signals a shell is started with at their default action, whatever the host was started with (a host run under `nohup` ignores the hang-up, and what is ignored on
/// entry stays ignored through `exec`: the shell could not trap it, and the programs it starts would outlive their terminal). Hang-up, interrupt, quit, broken pipe, terminate,
/// child, and the three that stop a job.
const DEFAULT_SIGNALS: [i32; 9] = [1, 2, 3, 13, 15, 17, 20, 21, 22];

/// What a read of the master found.
#[derive(Debug, PartialEq)]
pub enum ReadResult {
    Data(usize),
    /// Nothing now.
    Wait,
    /// Every holder of the other side has gone (or the pty broke): no more output will come.
    End,
}

#[derive(Debug, PartialEq)]
pub enum WriteError {
    /// The pty took nothing for the whole time that was given.
    Timeout,
    /// The other side is gone.
    Closed,
}

/// The master side: it never blocks, and the writes of several threads do not interleave.
pub struct Master {
    file: File,
    writing: Mutex<()>,
}

/// A pty: its master and the slave to hand to the shell.
pub fn open_pair(cols: u16, rows: u16) -> io::Result<(Master, File)> {
    // SAFETY: plain system calls on a descriptor this function owns; `File` closes it on every early return.
    unsafe {
        let fd = posix_openpt(O_RDWR | O_NOCTTY | O_CLOEXEC);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = File::from_raw_fd(fd);
        if grantpt(fd) != 0 || unlockpt(fd) != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut name = [0 as c_char; 128];
        let rc = ptsname_r(fd, name.as_mut_ptr(), name.len());
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        let path = CStr::from_ptr(name.as_ptr()).to_str().map_err(|_| io::Error::other("the name of the pty is not text"))?.to_string();
        let flags = fcntl(fd, F_GETFL, 0);
        if flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
        // not the controlling terminal of this process, whatever session it leads; the shell's own spawn makes it the shell's
        let slave = OpenOptions::new().read(true).write(true).custom_flags(O_NOCTTY).open(path)?;
        let master = Master { file, writing: Mutex::new(()) };
        master.set_size(cols, rows)?;
        Ok((master, slave))
    }
}

impl Master {
    pub fn set_size(&self, cols: u16, rows: u16) -> io::Result<()> {
        let ws = WinSize { rows, cols, xpixel: 0, ypixel: 0 };
        // SAFETY: `ws` lives through the call and has the layout the request wants.
        if unsafe { ioctl(self.file.as_raw_fd(), TIOCSWINSZ, &ws as *const WinSize) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// The process group in the foreground of the pty (`tcgetpgrp`: the group the terminal's keys, Ctrl+C among them, go to; its leader's pid), or none when the pty has none (the
    /// shell has ended) or the call fails. Read-only (M23-P04a).
    pub fn foreground_group(&self) -> Option<i32> {
        // SAFETY: a plain system call on a descriptor this struct owns; it reads a number and changes nothing.
        let group = unsafe { tcgetpgrp(self.file.as_raw_fd()) };
        (group > 0).then_some(group)
    }

    /// The size the pty has now, `(cols, rows)` (what the tests read back).
    #[cfg(test)]
    pub fn size(&self) -> io::Result<(u16, u16)> {
        let mut ws = WinSize::default();
        // SAFETY: `ws` lives through the call and has the layout the request wants.
        if unsafe { ioctl(self.file.as_raw_fd(), TIOCGWINSZ, &mut ws as *mut WinSize) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((ws.cols, ws.rows))
    }

    /// Waits up to `ms` for something to read (or for the other side to hang up); true when a read will not wait.
    pub fn wait_readable(&self, ms: i32) -> bool {
        self.poll(POLLIN, ms)
    }

    fn poll(&self, events: i16, ms: i32) -> bool {
        let mut fd = PollFd { fd: self.file.as_raw_fd(), events, revents: 0 };
        // SAFETY: one valid `PollFd` for the whole call.
        let n = unsafe { poll(&mut fd, 1, ms) };
        n > 0
    }

    pub fn read(&self, buf: &mut [u8]) -> ReadResult {
        match (&self.file).read(buf) {
            Ok(0) => ReadResult::End,
            Ok(n) => ReadResult::Data(n),
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => ReadResult::Wait,
            Err(_) => ReadResult::End, // EIO: every holder of the other side has gone; any other error ends the reading as well
        }
    }

    /// Writes all of `bytes`, waiting for room for at most `patience` in all.
    pub fn write(&self, bytes: &[u8], patience: Duration) -> Result<(), WriteError> {
        let _one_at_a_time = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + patience;
        let mut done = 0;
        while done < bytes.len() {
            match (&self.file).write(&bytes[done..]) {
                Ok(0) => return Err(WriteError::Closed),
                Ok(n) => done += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(WriteError::Timeout);
                    }
                    self.poll(POLLOUT, left.as_millis().clamp(1, 1000) as i32);
                }
                Err(_) => return Err(WriteError::Closed),
            }
        }
        Ok(())
    }
}

/// Starts `command` with the pty's slave as its input, output and error, as the leader of a new session whose controlling terminal is the pty, and with the parent-death
/// signal (no shell outlives its host). It is spawned by the host's one spawner thread, as every provider is (the parent-death signal follows that thread).
pub fn spawn(mut command: Command, slave: File) -> io::Result<Child> {
    command.stdin(Stdio::from(slave.try_clone()?)).stdout(Stdio::from(slave.try_clone()?)).stderr(Stdio::from(slave));
    // SAFETY: the closure runs between `fork` and `exec` and only calls system calls that are safe there (`signal` is `sigaction` underneath).
    unsafe {
        command.pre_exec(|| {
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG, SIGKILL
            for sig in DEFAULT_SIGNALS {
                signal(sig, 0); // SIG_DFL
            }
            if setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            if ioctl(0, TIOCSCTTY, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = crate::procs::spawn_agent(command)?;
    crate::procs::note_provider(child.id());
    Ok(child)
}
