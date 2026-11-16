//! The processes a host starts for its sessions: the one thread that spawns every provider (so the parent-death signal follows a long-lived thread), the list of provider
//! processes kept outside every session lock, and the stop that reaches each group. Std only; moved out of `core` unchanged (M16-P07: `core` is held at its size).

use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::sync::{mpsc, Mutex};
use std::thread;

type SpawnJob = (Command, mpsc::Sender<std::io::Result<Child>>);
static SPAWNER: std::sync::OnceLock<Mutex<mpsc::Sender<SpawnJob>>> = std::sync::OnceLock::new();

/// Agents are all spawned from one long-lived thread: the parent-death signal follows the
/// thread that created the child, so an agent created from a request thread would be killed
/// when that thread (a closed connection) ends.
pub fn init_spawner() {
    let (tx, rx) = mpsc::channel::<SpawnJob>();
    thread::spawn(move || {
        for (mut command, reply) in rx {
            let _ = reply.send(command.spawn());
        }
    });
    let _ = SPAWNER.set(Mutex::new(tx));
}

pub fn spawn_provider_process(mut command: Command) -> std::io::Result<Child> {
    unsafe {
        command.pre_exec(|| {
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG, SIGKILL: no provider outlives its host
            setsid(); // its own process group, so a stop reaches the helpers it starts too
            Ok(())
        });
    }
    let child = spawn_agent(command)?;
    note_provider(child.id());
    Ok(child)
}

/// The provider processes of this host with the start time of each, kept OUTSIDE every session lock: the stop reaches a provider even when its
/// session is stuck behind a lock that is held for long or poisoned.
static PROVIDERS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

/// (start time, process group) of a process; the fields are counted after the last `)` because the name may hold spaces and brackets.
fn stat_start_group(pid: u32) -> Option<(String, u32)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.rsplit_once(") ")?.1.split(' ').collect();
    Some((fields.get(19)?.to_string(), fields.get(2)?.parse().ok()?))
}

pub(crate) fn note_provider(pid: u32) {
    let mut list = PROVIDERS.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|(p, start)| stat_start_group(*p).map_or(false, |(s, _)| s == *start)); // the ones that ended are forgotten
    if let Some((start, _)) = stat_start_group(pid) {
        list.push((pid, start));
    }
}

/// Kill the group of every provider that is still the process that was noted (a pid that was reused is not signalled) and still leads its group.
pub(crate) fn end_listed_groups() {
    let list = PROVIDERS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for (pid, start) in list {
        if stat_start_group(pid).map_or(false, |(s, group)| s == start && group == pid) {
            unsafe { kill(-(pid as i32), 9) };
        }
    }
}

pub(crate) fn spawn_agent(command: Command) -> std::io::Result<Child> {
    let (reply, wait) = mpsc::channel();
    SPAWNER
        .get()
        .expect("spawner")
        .lock()
        .unwrap()
        .send((command, reply))
        .map_err(|_| std::io::Error::other("spawner stopped"))?;
    wait.recv().map_err(|_| std::io::Error::other("spawner stopped"))?
}

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
    fn setsid() -> i32;
}
