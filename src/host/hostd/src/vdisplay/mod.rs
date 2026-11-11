//! Per-session private virtual X11 displays (`Xvfb`, plan M02-P05 S02; CU-01, CU-04, MB-03).
//!
//! Each session gets an isolated `Xvfb` (`1280x800x24`, `-nolisten tcp -noreset`) on demand when
//! `computer` targets `"virtual"` or when `lico display` asks for one. Nothing runs until asked;
//! a virtual display stops on `session.close`, `session.purge`, `host.stop`, `display.stop`, after
//! the idle timeout (`DEFAULT_IDLE_SECS`), or with `SIGKILL` if the host process dies abruptly
//! (`PR_SET_PDEATHSIG`). Its memory (`RssAnon + RssShmem` and `VmRSS`) is added to the owning
//! session's `memory` figure (`MB-03`), never to the host or GUI.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::c_ulong;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use crate::core::{self, Reply, Shared};
use crate::memory::{self, Figure};
use crate::session::{obj, State};

pub const GEOMETRY: &str = "1280x800x24";
pub const DEFAULT_IDLE_SECS: u64 = 600;
const START_TIMEOUT_MS: i32 = 5000;

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn pipe(fds: *mut i32) -> i32;
    fn close(fd: i32) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    fn fcntl(fd: i32, cmd: i32, arg: i32) -> i32;
    fn setsid() -> i32;
    fn prctl(option: i32, arg2: c_ulong, arg3: c_ulong, arg4: c_ulong, arg5: c_ulong) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: i32) -> i32;
}

struct Entry {
    display: String,
    child: Child,
    pgid: i32,
    last_used: Instant,
}

fn table() -> MutexGuard<'static, HashMap<String, Entry>> {
    static TABLE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    TABLE.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner())
}

fn clamp_idle_secs(raw: Option<&str>) -> u64 {
    raw.and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| s >= 1)
        .map_or(DEFAULT_IDLE_SECS, |s| s.min(DEFAULT_IDLE_SECS))
}

fn idle_limit() -> Duration {
    Duration::from_secs(clamp_idle_secs(std::env::var("LICO_DISPLAY_IDLE_S").ok().as_deref()))
}

fn xvfb_bin() -> String {
    std::env::var("LICO_XVFB_BIN").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "Xvfb".to_string())
}

fn end_entry(mut e: Entry) {
    // SIGTERM lets Xvfb unlink /tmp/.X<n>-lock cleanly; SIGKILL reaps anything left in the group.
    unsafe { kill(-e.pgid, 15) };
    let deadline = Instant::now() + Duration::from_millis(120);
    let mut reaped = false;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = e.child.try_wait() {
            reaped = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    unsafe { kill(-e.pgid, 9) };
    if !reaped {
        let _ = e.child.wait();
    }
}

type SpawnJob = (Command, std::sync::mpsc::Sender<std::io::Result<Child>>);

/// Spawn `Xvfb` from one immortal keeper thread: on Linux `PR_SET_PDEATHSIG` tracks the creator
/// thread, so spawning from a short-lived HTTP request thread would `SIGKILL` `Xvfb` when the
/// request returns. Lazy (`OnceLock`) so hosts that never open a virtual display pay zero threads.
fn spawn_on_keeper(cmd: Command) -> std::io::Result<Child> {
    static KEEPER: OnceLock<Mutex<std::sync::mpsc::Sender<SpawnJob>>> = OnceLock::new();
    let tx = KEEPER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<SpawnJob>();
        std::thread::Builder::new()
            .name("lico-vd-keeper".into())
            .spawn(move || {
                for (mut c, reply) in rx {
                    let _ = reply.send(c.spawn());
                }
            })
            .expect("vdisplay keeper");
        Mutex::new(tx)
    });
    let (reply, wait) = std::sync::mpsc::channel();
    tx.lock()
        .unwrap_or_else(|e| e.into_inner())
        .send((cmd, reply))
        .map_err(|_| std::io::Error::other("spawner stopped"))?;
    wait.recv().map_err(|_| std::io::Error::other("spawner stopped"))?
}

fn spawn_xvfb() -> Result<Entry, &'static str> {
    let mut fds = [-1i32; 2];
    if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
        return Err("xvfb_unavailable");
    }
    let (rfd, wfd) = (fds[0], fds[1]);
    // F_SETFD = 2, FD_CLOEXEC = 1: Xvfb inherits wfd only, never rfd.
    unsafe {
        fcntl(rfd, 2, 1);
        fcntl(wfd, 2, 0);
    }
    let mut cmd = Command::new(xvfb_bin());
    cmd.args(["-displayfd", &wfd.to_string(), "-screen", "0", GEOMETRY, "-nolisten", "tcp", "-noreset"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(move || {
            setsid();
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG = 1, SIGKILL = 9
            Ok(())
        });
    }
    let child = match spawn_on_keeper(cmd) {
        Ok(c) => c,
        Err(_) => {
            unsafe {
                close(rfd);
                close(wfd);
            }
            return Err("xvfb_unavailable");
        }
    };
    unsafe { close(wfd) };
    let pgid = child.id() as i32;
    let mut entry = Entry { display: String::new(), child, pgid, last_used: Instant::now() };
    let mut raw = Vec::with_capacity(16);
    let deadline = Instant::now() + Duration::from_millis(START_TIMEOUT_MS as u64);
    while raw.len() < 32 && !raw.contains(&b'\n') {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let wait = (deadline - now).as_millis().min(START_TIMEOUT_MS as u128) as i32;
        let mut pfd = PollFd { fd: rfd, events: 1, revents: 0 };
        let rc = unsafe { poll(&mut pfd, 1, wait.max(1)) };
        if rc <= 0 {
            continue;
        }
        let mut byte = [0u8; 16];
        let n = unsafe { read(rfd, byte.as_mut_ptr(), byte.len()) };
        if n <= 0 {
            break;
        }
        raw.extend_from_slice(&byte[..n as usize]);
    }
    unsafe { close(rfd) };
    let text = String::from_utf8_lossy(&raw);
    let Some(num) = text.trim().parse::<u32>().ok() else {
        end_entry(entry);
        return Err("xvfb_unavailable");
    };
    if !matches!(entry.child.try_wait(), Ok(None)) {
        end_entry(entry);
        return Err("xvfb_unavailable");
    }
    entry.display = format!(":{num}");
    entry.last_used = Instant::now();
    Ok(entry)
}

/// Start or reuse the private virtual display of `session`, updating its idle clock.
pub fn ensure_for_session(session: &str) -> Result<String, &'static str> {
    let mut stale = None;
    {
        let mut map = table();
        if let Some(existing) = map.get_mut(session) {
            if matches!(existing.child.try_wait(), Ok(None)) {
                existing.last_used = Instant::now();
                return Ok(existing.display.clone());
            }
            stale = map.remove(session);
        }
    }
    if let Some(dead) = stale {
        end_entry(dead);
    }
    let fresh = spawn_xvfb()?;
    let disp = fresh.display.clone();
    let mut extra = None;
    let out = {
        let mut map = table();
        if let Some(existing) = map.get_mut(session) {
            if matches!(existing.child.try_wait(), Ok(None)) {
                existing.last_used = Instant::now();
                let winner = existing.display.clone();
                extra = Some(fresh);
                winner
            } else {
                extra = map.insert(session.to_string(), fresh);
                disp
            }
        } else {
            map.insert(session.to_string(), fresh);
            disp
        }
    };
    if let Some(loser) = extra {
        end_entry(loser);
    }
    Ok(out)
}

fn check_running(shared: &Shared, session: &str) -> Result<(), &'static str> {
    let found = core::find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    if s.purged {
        Err("unknown_session")
    } else if s.state == State::Exited {
        Err("session_exited")
    } else if s.state == State::Disconnected {
        Err("session_disconnected")
    } else {
        Ok(())
    }
}

/// Socket op `display.ensure`.
pub fn ensure(shared: &Shared, session: &str) -> Reply {
    check_running(shared, session)?;
    let disp = ensure_for_session(session)?;
    if let Err(code) = check_running(shared, session) {
        stop(session);
        return Err(code);
    }
    Ok(obj(json!({"session": session, "display": disp, "geometry": GEOMETRY})))
}

/// Current `(display, memory)` of `session`'s virtual display, if one is running.
pub fn info(session: &str) -> Option<(String, Figure)> {
    let mut dead = None;
    let found = {
        let mut map = table();
        if let Some(e) = map.get_mut(session) {
            if matches!(e.child.try_wait(), Ok(None)) {
                let pid = e.child.id();
                let (private_kb, rss_kb) = std::fs::read_to_string(format!("/proc/{pid}/status")).map_or((0, 0), |t| memory::split(&t));
                Some((e.display.clone(), Figure { private_kb, rss_kb, processes: 1 }))
            } else {
                dead = map.remove(session);
                None
            }
        } else {
            None
        }
    };
    if let Some(d) = dead {
        end_entry(d);
    }
    found
}

/// Socket op `display.status`.
pub fn status_op(shared: &Shared, session: &str) -> Reply {
    let found = core::find(shared, session).ok_or("unknown_session")?;
    if found.lock().unwrap().purged {
        return Err("unknown_session");
    }
    match info(session) {
        Some((disp, fig)) => Ok(obj(json!({"session": session, "running": true, "display": disp, "geometry": GEOMETRY, "memory": memory::json_of(&fig)}))),
        None => Ok(obj(json!({"session": session, "running": false, "display": Value::Null}))),
    }
}

/// Stop the virtual display of `session`, if any. True when one was running.
pub fn stop(session: &str) -> bool {
    let removed = table().remove(session);
    match removed {
        Some(e) => {
            end_entry(e);
            true
        }
        None => false,
    }
}

/// Socket op `display.stop`.
pub fn stop_op(shared: &Shared, session: &str) -> Reply {
    let found = core::find(shared, session).ok_or("unknown_session")?;
    if found.lock().unwrap().purged {
        return Err("unknown_session");
    }
    Ok(obj(json!({"session": session, "stopped": stop(session)})))
}

/// Reap virtual displays that exited or have been idle for `idle_limit()`.
pub fn sweep_idle() {
    let limit = idle_limit();
    let mut doomed = Vec::new();
    {
        let mut map = table();
        let keys: Vec<String> = map
            .iter_mut()
            .filter_map(|(k, e)| {
                let dead = !matches!(e.child.try_wait(), Ok(None));
                (dead || e.last_used.elapsed() >= limit).then(|| k.clone())
            })
            .collect();
        for k in keys {
            if let Some(e) = map.remove(&k) {
                doomed.push(e);
            }
        }
    }
    for e in doomed {
        end_entry(e);
    }
}

/// Stop every virtual display (called on `host.stop`).
pub fn stop_all() {
    let all: Vec<Entry> = table().drain().map(|(_, e)| e).collect();
    for e in all {
        end_entry(e);
    }
}

#[cfg(test)]
pub fn pid_of(session: &str) -> Option<u32> {
    table().get(session).map(|e| e.child.id())
}

/// `lico display ...`: start/print, status, stop, or run a command with `DISPLAY` set to the session's virtual display.
pub fn cli(args: &[String], socket: &str) -> ! {
    let die = |code: i32, msg: &str| -> ! {
        eprintln!("{msg}");
        std::process::exit(code);
    };
    if socket.is_empty() {
        die(1, "lico display needs a host socket (--socket PATH or $LICO_SOCKET)");
    }
    let take_session = |words: &[String]| -> (String, Vec<String>) {
        let mut out = Vec::new();
        let mut session = std::env::var("LICO_SESSION").ok().filter(|s| !s.is_empty());
        let mut i = 0;
        while i < words.len() {
            if words[i] == "--session" {
                let Some(v) = words.get(i + 1) else { die(1, "--session needs a session id") };
                session = Some(v.clone());
                i += 2;
            } else {
                out.push(words[i].clone());
                i += 1;
            }
        }
        if session.is_none() && out.first().is_some_and(|w| !w.starts_with('-') && w != "status" && w != "stop" && w != "run") {
            session = Some(out.remove(0));
        }
        let Some(sid) = session else { die(1, "lico display needs --session ID (or $LICO_SESSION)") };
        (sid, out)
    };
    match args.first().map(String::as_str) {
        Some("status") => {
            let (sid, rest) = take_session(&args[1..]);
            if !rest.is_empty() {
                die(1, "usage: lico display status [--session ID]");
            }
            crate::cli::expect_ok_pub(crate::cli::call_once(socket, json!({"op": "display.status", "session": sid})));
            std::process::exit(0);
        }
        Some("stop") => {
            let (sid, rest) = take_session(&args[1..]);
            if !rest.is_empty() {
                die(1, "usage: lico display stop [--session ID]");
            }
            crate::cli::expect_ok_pub(crate::cli::call_once(socket, json!({"op": "display.stop", "session": sid})));
            std::process::exit(0);
        }
        Some("run") => {
            let mut session = std::env::var("LICO_SESSION").ok().filter(|s| !s.is_empty());
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--session" => {
                        let Some(v) = args.get(i + 1) else { die(1, "--session needs a session id") };
                        session = Some(v.clone());
                        i += 2;
                    }
                    "--" => {
                        i += 1;
                        break;
                    }
                    w if w.starts_with('-') => die(1, "usage: lico display run [--session ID] [--] CMD [ARGS...]"),
                    _ => break,
                }
            }
            let Some(sid) = session else { die(1, "lico display run needs --session ID (or $LICO_SESSION)") };
            if i >= args.len() {
                die(1, "usage: lico display run [--session ID] [--] CMD [ARGS...]");
            }
            let reply = crate::cli::call_once(socket, json!({"op": "display.ensure", "session": sid}));
            if reply["ok"] != json!(1) {
                println!("{reply}");
                std::process::exit(2);
            }
            let disp = reply["display"].as_str().unwrap_or_else(|| die(2, "host gave no display"));
            let status = Command::new(&args[i]).args(&args[i + 1..]).env("DISPLAY", disp).status().unwrap_or_else(|e| die(1, &format!("cannot run {}: {e}", args[i])));
            std::process::exit(status.code().unwrap_or(1));
        }
        _ => {
            let (sid, rest) = take_session(args);
            let as_json = rest.iter().any(|w| w == "--json");
            if rest.iter().any(|w| w != "--json") {
                die(1, "usage: lico display [--session ID] [--json]");
            }
            let reply = crate::cli::call_once(socket, json!({"op": "display.ensure", "session": sid}));
            if reply["ok"] != json!(1) {
                println!("{reply}");
                std::process::exit(2);
            }
            if as_json {
                println!("{reply}");
            } else {
                let disp = reply["display"].as_str().unwrap_or("");
                println!("export DISPLAY='{disp}'");
            }
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sessions_get_distinct_displays_survive_caller_thread_exit_and_leave_no_process_after_stop() {
        let a = format!("vd-test-a-{}", core::random_hex(4));
        let b = format!("vd-test-b-{}", core::random_hex(4));
        let a_clone = a.clone();
        let da = std::thread::spawn(move || ensure_for_session(&a_clone).expect("xvfb a")).join().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let db = std::thread::spawn({
            let b_clone = b.clone();
            move || ensure_for_session(&b_clone).expect("xvfb b")
        })
        .join()
        .unwrap();
        assert_ne!(da, db, "two sessions never share a virtual display even after their HTTP request threads exit");
        assert_eq!(ensure_for_session(&a).unwrap(), da, "a second call reuses the live Xvfb");
        let pid_a = pid_of(&a).expect("pid a");
        let (disp, fig) = info(&a).expect("info a");
        assert_eq!(disp, da);
        assert!(fig.private_kb > 0 && fig.rss_kb >= fig.private_kb && fig.processes == 1, "{fig:?}");
        assert!(stop(&a) && !stop(&a));
        assert!(info(&a).is_none());
        assert!(!std::path::Path::new(&format!("/proc/{pid_a}")).exists(), "Xvfb was reaped");
        assert!(stop(&b));
    }

    #[test]
    fn idle_secs_clamps_to_at_most_ten_minutes_and_ignores_zero_or_invalid() {
        assert_eq!(clamp_idle_secs(None), DEFAULT_IDLE_SECS);
        assert_eq!(clamp_idle_secs(Some("0")), DEFAULT_IDLE_SECS);
        assert_eq!(clamp_idle_secs(Some("bad")), DEFAULT_IDLE_SECS);
        assert_eq!(clamp_idle_secs(Some("120")), 120);
        assert_eq!(clamp_idle_secs(Some("86400")), DEFAULT_IDLE_SECS);
    }
}
