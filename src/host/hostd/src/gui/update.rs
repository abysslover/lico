//! The window brings the host to its own build and keeps the sessions (M02-P01-S04).
//!
//! A host of another build than the window is replaced by the window itself, with or without sessions: wait for what is running (a turn up to a
//! minute, a sign-in up to five), stop the old host over its own socket (never by process name), start one of this build, and resume the provider
//! sessions that were alive under a new generation. What the person reads is a header line: what was done, and anything that was not restored.
//! The work is done on a thread of its own, so a waiting turn never keeps the window from showing.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub(super) enum Update {
    /// Progress: the work goes on.
    Line(String),
    /// The last word when nothing was replaced and nothing more will happen.
    Stopped(String),
    /// A host of this build answers on the socket now: connect again; the line says what was done.
    Replaced(String),
}

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn getpid() -> i32;
    fn getpgrp() -> i32;
}

const STATUS: &str = "{\"op\":\"status\"}";

/// One request on a connection of its own; the reply, or None when the host does not answer in time.
pub(super) fn call(socket: &str, request: &str, timeout: Duration) -> Option<Value> {
    let mut conn = UnixStream::connect(socket).ok()?;
    conn.set_read_timeout(Some(timeout)).ok()?;
    conn.write_all(format!("{request}\n").as_bytes()).ok()?;
    for line in BufReader::new(conn).lines().map_while(Result::ok) {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if v.get("ok").is_some() {
                return Some(v);
            }
        }
    }
    None
}

fn status_of(socket: &str) -> Option<Value> {
    call(socket, STATUS, Duration::from_secs(3)).filter(|v| v["ok"] == json!(1))
}

/// The host's status when it runs another build than this window (an old host without a build id counts as another one), else None.
pub(super) fn stale(socket: &str) -> Option<Value> {
    status_of(socket).filter(|s| s["build"].as_str() != Some(crate::core::build_id()))
}

/// How long a running turn and a sign-in are waited for. The environment shortens them for the tests only.
fn limits() -> (Duration, Duration) {
    let millis = |name: &str, default: u64| Duration::from_millis(std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default));
    (millis("LICO_UPDATE_TURN_WAIT_MS", 60_000), millis("LICO_UPDATE_LOGIN_WAIT_MS", 300_000))
}

fn provider_sessions(status: &Value) -> impl Iterator<Item = &Value> {
    status["sessions"].as_array().into_iter().flatten().filter(|s| s["provider"].is_string())
}

/// A session that is in the middle of something: a turn is running or an approval is waiting for an answer.
fn in_the_middle(entry: &Value) -> bool {
    entry["state"] == json!("running") && (entry["activity"] == json!("working") || entry["approvals"].as_array().map_or(false, |a| !a.is_empty()))
}

fn gone(socket: &str, within: Duration) -> bool {
    let from = Instant::now();
    loop {
        if UnixStream::connect(socket).is_err() {
            return true;
        }
        if from.elapsed() >= within {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// How the old host came to an end.
enum Stopped {
    /// This window ended it, so this window resumes the sessions.
    Here,
    /// Another window (or the host itself) got there first: nothing to resume from here.
    Elsewhere,
    Failed(String),
}

/// Ended by the pid its own status named, after a fresh answer of the host confirmed that the number is still its.
fn end_by_pid(socket: &str, pid: u64) -> bool {
    status_of(socket).map_or(false, |s| s["pid"].as_u64() == Some(pid)) && unsafe { kill(pid as i32, 15) } == 0
}

fn stop(socket: &str, status: &Value) -> Stopped {
    let build = status["build"].as_str().map(str::to_string);
    let pid = status["pid"].as_u64();
    let request = |with_build: bool| match (&build, with_build) {
        (Some(b), true) => json!({"op": "host.stop", "force": true, "build": b}).to_string(),
        _ => json!({"op": "host.stop", "force": true}).to_string(),
    };
    let mut reply = call(socket, &request(true), Duration::from_secs(5));
    if reply.as_ref().map_or(false, |r| r["error"] == json!("bad_request")) && build.is_some() {
        // a host that knows host.stop but not the build field: asked again without it, once the status confirms it is still the host that was seen
        let again = status_of(socket);
        if again.as_ref().map(|s| (s["pid"].as_u64(), s["build"].as_str().map(str::to_string))) != Some((pid, build.clone())) {
            return Stopped::Elsewhere;
        }
        reply = call(socket, &request(false), Duration::from_secs(5));
    }
    match reply {
        Some(r) if r["ok"] == json!(1) => {
            if gone(socket, Duration::from_secs(3)) {
                Stopped::Here
            } else {
                Stopped::Failed(format!("its process {} did not end", pid.map_or("?".to_string(), |p| p.to_string())))
            }
        }
        Some(r) if matches!(r["error"].as_str(), Some("build_changed" | "stopping")) => Stopped::Elsewhere,
        other => {
            // host.stop is not understood (a host older than that call), or the host does not answer: it is ended once by the pid it reported
            if other.is_none() && UnixStream::connect(socket).is_err() {
                return Stopped::Elsewhere; // it is gone already
            }
            match pid {
                Some(pid) if end_by_pid(socket, pid) => {
                    if gone(socket, Duration::from_secs(3)) {
                        Stopped::Here
                    } else {
                        Stopped::Failed(format!("its process {pid} did not end"))
                    }
                }
                Some(pid) => Stopped::Failed(format!("it did not answer host.stop and its process {pid} could not be ended")),
                None => Stopped::Failed("it did not answer host.stop and did not name its process".to_string()),
            }
        }
    }
}

/// A child of the old host as it was before the stop: the start time tells a pid that is reused later from the same process, and `leader` says
/// whether it led its own process group (a provider does: its helpers are in that group).
struct Child {
    pid: u64,
    start: String,
    leader: bool,
}

/// The direct children of a process, read from /proc.
fn children_of(parent: u64) -> Vec<Child> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u64>().ok()) else { continue };
        if let Some(stat) = stat_of(pid) {
            if stat.ppid == parent {
                found.push(Child { pid, leader: stat.pgrp == pid, start: stat.start });
            }
        }
    }
    found
}

struct Stat {
    ppid: u64,
    pgrp: u64,
    start: String,
}

/// The parent, the process group and the start time of a process; the fields are counted after the last `)` because the name may hold spaces and brackets.
fn stat_of(pid: u64) -> Option<Stat> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(") ")?.1;
    let fields: Vec<&str> = rest.split(' ').collect();
    if fields.first() == Some(&"Z") {
        return None; // gone but not yet reaped
    }
    Some(Stat { ppid: fields.get(1)?.parse().ok()?, pgrp: fields.get(2)?.parse().ok()?, start: fields.get(19)?.to_string() })
}

/// What the old host left running: the children that are still the same process, and every process in a group that one of them led. The old
/// build's providers die with their host (parent-death signal) and leave their helpers, in the dead leader's group, behind; a group id cannot be
/// reused while a member lives, so the scan names exactly those helpers. Never a group that no child led, never the window's own.
fn left_of(children: &[Child]) -> Vec<u64> {
    let own = unsafe { getpid() } as u64;
    let own_group = unsafe { getpgrp() } as u64;
    let leaders: Vec<u64> = children.iter().filter(|c| c.leader && c.pid > 1 && c.pid != own_group).map(|c| c.pid).collect();
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u64>().ok()) else { continue };
        let Some(stat) = stat_of(pid) else { continue };
        if pid <= 1 || pid == own {
            continue;
        }
        let child = children.iter().find(|c| c.pid == pid);
        // a child is the same process only with the same start time; a member of a led group is a helper
        if child.map_or(leaders.contains(&stat.pgrp), |c| c.start == stat.start) {
            found.push(pid);
        }
    }
    found
}

/// Ask what the old host left running to end, then force it after two seconds, so nothing of the old host meets the session that resumes.
/// Each process is signalled by its own pid; whatever could not be ended is returned, to be named in the line.
fn end_leftovers(children: &[Child]) -> Vec<u64> {
    for signal in [15, 9] {
        let running = left_of(children);
        if running.is_empty() {
            return running;
        }
        for pid in running {
            unsafe { kill(pid as i32, signal) };
        }
        let from = Instant::now();
        while !left_of(children).is_empty() && from.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    left_of(children) // what could not be ended: named in the line
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn why(error: &str) -> &str {
    match error {
        "bad_request" => "it has no conversation to resume yet",
        "stale_generation" => "it changed meanwhile",
        "unknown_session" => "the new host does not know it",
        other => other,
    }
}

/// The provider sessions that were running are resumed one by one, each with the generation the new host reports. Returns (resumed, failures).
fn resume(socket: &str, alive: &[String]) -> (usize, Vec<String>) {
    let mut resumed = 0;
    let mut failures = Vec::new();
    let Some(now) = status_of(socket) else { return (0, vec!["the new host does not answer".to_string()]) };
    for id in alive {
        let Some(entry) = provider_sessions(&now).find(|e| e["session"].as_str() == Some(id.as_str())) else {
            failures.push(why("unknown_session").to_string());
            continue;
        };
        match entry["state"].as_str() {
            Some("disconnected") => {
                let request = json!({"op": "session.resume", "session": id, "generation": entry["generation"]}).to_string();
                match call(socket, &request, Duration::from_secs(30)) {
                    Some(r) if r["ok"] == json!(1) => resumed += 1,
                    Some(r) => failures.push(why(r["error"].as_str().unwrap_or("unknown")).to_string()),
                    None => failures.push("the new host did not answer".to_string()),
                }
            }
            Some("running") => resumed += 1, // already back
            _ => failures.push(why("exited").to_string()),
        }
    }
    (resumed, failures)
}

/// Replace the host that `seen` describes. `tell` reports to the window (it is called from this thread).
pub(super) fn run(socket: &str, seen: Value, start_host: &dyn Fn() -> Result<String, String>, tell: &dyn Fn(Update)) {
    let ours = crate::core::build_id();
    let (turn_limit, login_limit) = limits();
    let began = Instant::now();
    let mut status = seen;
    let mut turn_since: Option<Instant> = None;
    let mut said = String::new();
    let mut say = |line: &str| {
        if said != line {
            said = line.to_string();
            tell(Update::Line(line.to_string()));
        }
    };
    let end = |line: &str| tell(Update::Stopped(line.to_string()));
    // 1. what is running is waited for: a turn for a limited time, a sign-in is never ended
    loop {
        let logins = status["logins_running"].as_u64().unwrap_or(0);
        if provider_sessions(&status).any(in_the_middle) {
            turn_since.get_or_insert_with(Instant::now);
        } else {
            turn_since = None;
        }
        if logins == 0 && turn_since.map_or(true, |t| t.elapsed() >= turn_limit) {
            break;
        }
        if logins > 0 && began.elapsed() >= login_limit {
            end("The host runs an older build and was not replaced: a sign-in was still in progress. Finish or cancel it, close every Lico window and run: lico host stop");
            return;
        }
        say(if logins > 0 { "Updating the host after the sign-in in progress..." } else { "Updating the host after the running turn..." });
        std::thread::sleep(Duration::from_millis(500));
        match status_of(socket) {
            Some(s) if s["build"].as_str() == Some(ours) => {
                tell(Update::Replaced("The host was replaced by another window (new build).".into())); // somebody else was quicker
                return;
            }
            Some(s) => status = s,
            None => break, // the host is gone: nothing more to wait for
        }
    }
    // 2. the state at the moment of the stop: which sessions were alive, which were in the middle of something
    if let Some(s) = status_of(socket) {
        status = s;
    }
    let alive: Vec<String> = provider_sessions(&status).filter(|e| e["state"] == json!("running")).filter_map(|e| e["session"].as_str().map(str::to_string)).collect();
    let interrupted = provider_sessions(&status).filter(|e| in_the_middle(e)).count();
    let approvals: usize = provider_sessions(&status).filter_map(|e| e["approvals"].as_array().map(Vec::len)).sum();
    say("Restarting the host with this build...");
    let children = status["pid"].as_u64().map(children_of).unwrap_or_default();
    let stopped = stop(socket, &status);
    let stuck = if matches!(stopped, Stopped::Here) { end_leftovers(&children) } else { Vec::new() };
    if let Stopped::Failed(why) = &stopped {
        end(&format!("The host runs an older build and could not be ended: {why}. Close every Lico window, end that process and run `lico gui` again."));
        return;
    }
    // 3. a host of this build; the sessions are resumed by the window that stopped the old one
    if let Err(why) = bring_up(socket, start_host) {
        end(&format!("The old host was ended but the new one did not start: {why}. Run `lico gui` again; the sessions are kept."));
        return;
    }
    match stopped {
        Stopped::Here => {
            let (resumed, failures) = resume(socket, &alive);
            let mut line = format!("Restarted the host (new build); resumed {}", plural(resumed, "session"));
            if !failures.is_empty() {
                let mut reasons: Vec<&str> = failures.iter().map(String::as_str).collect();
                reasons.dedup();
                line.push_str(&format!("; {} could not be resumed ({})", failures.len(), reasons.join(", ")));
            }
            if interrupted > 0 {
                line.push_str(&format!("; {} {} interrupted", plural(interrupted, "turn"), if interrupted == 1 { "was" } else { "were" }));
            }
            if approvals > 0 {
                line.push_str(&format!("; {} waiting for an answer {} not kept", plural(approvals, "approval"), if approvals == 1 { "was" } else { "were" }));
            }
            for pid in stuck {
                line.push_str(&format!("; process {pid} of the old host could not be ended"));
            }
            line.push('.');
            tell(Update::Replaced(line));
        }
        Stopped::Elsewhere => tell(Update::Replaced("The host was replaced by another window (new build).".into())),
        Stopped::Failed(_) => {}
    }
}

/// A window that is already open when its host goes away: waits (it starts nothing) for a host to answer on the same socket, whatever its build.
pub(super) fn wait_for_host(socket: &str) -> bool {
    let from = Instant::now();
    while from.elapsed() < Duration::from_secs(15) {
        if status_of(socket).is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Until a host of this build answers on the socket: a host that is still ending is waited out, one that is missing is started.
fn bring_up(socket: &str, start_host: &dyn Fn() -> Result<String, String>) -> Result<(), String> {
    let from = Instant::now();
    loop {
        match status_of(socket) {
            Some(s) if s["build"].as_str() == Some(crate::core::build_id()) => return Ok(()),
            Some(_) if from.elapsed() < Duration::from_secs(10) => std::thread::sleep(Duration::from_millis(100)),
            Some(_) => return Err("the host on the socket is not of this build".to_string()),
            None if from.elapsed() < Duration::from_secs(20) => {
                start_host()?;
                std::thread::sleep(Duration::from_millis(100));
            }
            None => return Err("the new host does not answer".to_string()),
        }
    }
}
