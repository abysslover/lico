//! The host side of the helper process (plan M02-P04, S03; XF-07): `run_job` starts `lico xjob <job> [args]` as a child in a process group of
//! its own, reads its one answer line from standard output, and returns it as a value or says why not.
//!
//! The host holds no X connection; a job that needs one is a child that ends. The properties, each pinned by a test:
//! - one thread: the function polls the two pipes itself, so it starts no thread and leaves none, whatever the path;
//! - the answer is read AS it is written (a large answer cannot block the job on a full pipe) and is never held beyond `max_output` bytes;
//! - a job that does not end by the deadline, or answers too much, is ended with SIGKILL to its whole process group (the job and anything it
//!   started), and the child is REAPED before the function returns: no zombie, no process left, no descriptor left open;
//! - `Running` reaps on every path out of the function, a panic included;
//! - the function prints nothing and keeps no state; the caller (S07 for the agent tool, S04 for the clipboard) shows the `JobError`.
//!
//! Limit: a grandchild that leaves the process group on purpose (`setsid`) is not reached by the group kill; a job of ours does not do that.

#![allow(dead_code)] // the callers are S04 (the clipboard) and S07 (the agent tool `capture`); the first of them removes this line

use serde_json::{json, Value};
use std::ffi::c_ulong;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// The most of a job's standard error that is kept, for the detail of an error.
pub const STDERR_KEPT: usize = 4096;

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: i32) -> i32;
}

const POLLIN: i16 = 1;
const SIGKILL: i32 = 9;

/// What a job may take: the bytes of its answer and the time it may run.
#[derive(Clone, Copy, Debug)]
pub struct JobLimits {
    pub max_output: usize,
    pub deadline: Duration,
}

impl JobLimits {
    /// A capture: an answer line of a few hundred bytes, 64 KiB at most; a 1080p screen takes well under a second, 30 s is the stop.
    pub fn capture() -> JobLimits {
        JobLimits { max_output: 64 * 1024, deadline: Duration::from_secs(30) }
    }
}

/// Why a job gave no answer: a machine-readable `reason` and a `detail` a person can read.
/// The reasons of the helper itself: `timeout`, `spawn_failed`, `output_too_large`, `bad_answer`, `killed`; any other reason is the job's own
/// (`{"ok":0,"error":R,"detail":D}` with exit 1 or 2), passed through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobError {
    pub reason: String,
    pub detail: String,
}

impl JobError {
    fn new(reason: &str, detail: impl Into<String>) -> JobError {
        JobError { reason: reason.to_string(), detail: detail.into() }
    }
    pub fn to_json(&self) -> Value {
        json!({"ok": 0, "error": self.reason, "detail": self.detail})
    }
}

/// The child and its process group; ends and reaps them when it goes away, unless that was done already.
struct Running {
    child: Child,
    pgid: i32,
    reaped: bool,
}

impl Running {
    /// SIGKILL to the whole group, then the wait that reaps the child.
    fn end(&mut self) {
        if !self.reaped {
            unsafe { kill(-self.pgid, SIGKILL) };
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.end();
    }
}

enum Stop {
    Eof,
    Deadline,
    TooLarge,
}

/// Reads both pipes until both are closed, the deadline passes or the answer is over the limit. Standard output is read while the job runs.
fn pump(out: &mut impl Read, err: &mut impl Read, ofd: i32, efd: i32, o: &mut Vec<u8>, e: &mut Vec<u8>, limits: &JobLimits, deadline: Instant) -> Stop {
    let (mut o_open, mut e_open) = (true, true);
    let mut buf = [0u8; 8192];
    loop {
        if !o_open && !e_open {
            return Stop::Eof;
        }
        let now = Instant::now();
        if now >= deadline {
            return Stop::Deadline;
        }
        let wait_ms = ((deadline - now).as_micros().div_ceil(1000)).min(i32::MAX as u128) as i32; // rounded up, so it does not spin below a millisecond
        let mut fds: Vec<PollFd> = Vec::with_capacity(2);
        if o_open {
            fds.push(PollFd { fd: ofd, events: POLLIN, revents: 0 });
        }
        if e_open {
            fds.push(PollFd { fd: efd, events: POLLIN, revents: 0 });
        }
        let n = unsafe { poll(fds.as_mut_ptr(), fds.len() as c_ulong, wait_ms) };
        if n <= 0 {
            continue; // the deadline (re-read at the top) or a signal
        }
        for f in &fds {
            if f.revents == 0 {
                continue;
            }
            if f.fd == ofd {
                // never more than one byte over the limit is held
                let room = (limits.max_output + 1 - o.len()).min(buf.len());
                match out.read(&mut buf[..room]) {
                    Ok(0) => o_open = false,
                    Ok(k) => {
                        o.extend_from_slice(&buf[..k]);
                        if o.len() > limits.max_output {
                            return Stop::TooLarge;
                        }
                    }
                    Err(x) if x.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => o_open = false,
                }
            } else {
                match err.read(&mut buf) {
                    Ok(0) => e_open = false,
                    Ok(k) => e.extend_from_slice(&buf[..k.min(STDERR_KEPT.saturating_sub(e.len()))]), // the rest is read and dropped
                    Err(x) if x.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => e_open = false,
                }
            }
        }
    }
}

/// The standard error as a short line of printable text for a detail.
fn stderr_text(e: &[u8]) -> String {
    let t: String = String::from_utf8_lossy(e).chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let t = t.trim();
    if t.is_empty() {
        String::new()
    } else {
        format!("; stderr: {t}")
    }
}

/// Exactly one JSON line (a single line end after it is allowed).
fn one_line(o: &[u8]) -> Result<Value, &'static str> {
    let body = o.strip_suffix(b"\n").unwrap_or(o);
    if body.is_empty() {
        return Err("no answer line");
    }
    if body.contains(&b'\n') {
        return Err("more than one line");
    }
    serde_json::from_slice(body).map_err(|_| "the line is not JSON")
}

/// Runs `bin xjob job args...` and returns its answer. The job gets no standard input; it runs in a process group of its own.
pub fn run_job(bin: &Path, job: &str, args: &[String], limits: &JobLimits) -> Result<Value, JobError> {
    let deadline = Instant::now() + limits.deadline;
    let mut command = Command::new(bin);
    command.arg("xjob").arg(job).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let child = command.spawn().map_err(|e| JobError::new("spawn_failed", format!("{}: {e}", bin.display())))?;
    let pgid = child.id() as i32; // process_group(0): the child leads a group of its own, with its own pid as the id
    let mut run = Running { child, pgid, reaped: false };
    let (Some(mut out), Some(mut err)) = (run.child.stdout.take(), run.child.stderr.take()) else {
        return Err(JobError::new("spawn_failed", "no pipes"));
    };
    let (mut o, mut e) = (Vec::new(), Vec::new());
    let (ofd, efd) = (out.as_raw_fd(), err.as_raw_fd());
    let stop = pump(&mut out, &mut err, ofd, efd, &mut o, &mut e, limits, deadline);
    let timeout = || JobError::new("timeout", format!("job {job} did not end within {} ms", limits.deadline.as_millis()));
    match stop {
        Stop::Deadline => {
            run.end();
            return Err(timeout());
        }
        Stop::TooLarge => {
            run.end();
            return Err(JobError::new("output_too_large", format!("job {job} answered more than {} bytes", limits.max_output)));
        }
        Stop::Eof => {}
    }
    // both pipes are closed: the job is ending; wait for it, still by the deadline
    let status = loop {
        match run.child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() >= deadline => {
                run.end();
                return Err(timeout());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(1)),
            Err(x) => {
                run.end();
                return Err(JobError::new("bad_answer", format!("job {job}: wait failed: {x}")));
            }
        }
    };
    run.reaped = true;
    // anything the job left running in its group (it closed the pipes and went on) goes with it; the group id cannot belong to anyone else
    // yet: process ids are handed out in turn and this one was released a moment ago
    unsafe { kill(-pgid, SIGKILL) };
    if let Some(sig) = status.signal() {
        return Err(JobError::new("killed", format!("job {job} was ended by signal {sig}{}", stderr_text(&e))));
    }
    let code = status.code().unwrap_or(-1);
    let line = one_line(&o);
    let bad = |why: &str| JobError::new("bad_answer", format!("job {job}: {why} (exit status {code}){}", stderr_text(&e)));
    match (code, line) {
        (0, Ok(v)) if v.get("ok") == Some(&json!(1)) => Ok(v),
        (1 | 2, Ok(v)) if v.get("ok") == Some(&json!(0)) && v.get("error").is_some_and(Value::is_string) => {
            let detail = v.get("detail").and_then(Value::as_str).unwrap_or("").to_string();
            Err(JobError::new(v["error"].as_str().unwrap_or(""), detail))
        }
        (_, Ok(_)) => Err(bad("the answer does not fit the exit status")),
        (_, Err(why)) => Err(bad(why)),
    }
}
