//! Memory per session. The figure a row shows is what the agent and everything it started hold of their own: `RssAnon` + `RssShmem` from
//! `/proc/<pid>/status`, summed over the agent and all its descendants (kB). `VmRSS` (it also counts libraries and mapped files, and moves with what
//! the kernel happens to keep cached) travels next to it for the hover text. PSS is not used: it moves with whatever other processes share. A program
//! a tool detached from the session (`setsid`) is not a descendant and is not counted.
//!
//! One look serves every session, and it follows the `children` files of the agents' own threads, so it costs what the sessions' trees hold, not what the
//! machine holds (the first version scanned all of `/proc`: 1380 processes here, 0.57 % of a core with a window attached; the scan is still the way on a
//! kernel without those files). A status call starts a look when the last one is older than `EVERY`, so nothing looks while nobody asks (a host without
//! a window costs nothing) and a window that asks every second costs one look in three. The look runs on its own short thread: a status call never
//! waits for it and shows what the last one found. A process that vanished or cannot be read counts for nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::session::{Session, State};

const EVERY: Duration = Duration::from_secs(3);

/// What one agent and its descendants hold, in kB.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Figure {
    pub private_kb: u64,
    pub rss_kb: u64,
    pub processes: u32,
}

struct Cache {
    at: Option<Instant>,
    busy: bool,
    asked: BTreeSet<u32>, // the agents the status calls since the last look named
    figures: BTreeMap<u32, Figure>,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { at: None, busy: false, asked: BTreeSet::new(), figures: BTreeMap::new() });

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// (private, rss) in kB from the text of a `/proc/<pid>/status` file; a key that is missing counts as 0.
pub fn split(status: &str) -> (u64, u64) {
    let kb = |key: &str| status.lines().find_map(|l| l.strip_prefix(key)?.split_whitespace().next()?.parse::<u64>().ok()).unwrap_or(0);
    (kb("RssAnon:") + kb("RssShmem:"), kb("VmRSS:"))
}

/// The figure of this very process (the host's, or the window's): one small read.
pub fn own() -> Figure {
    let (private_kb, rss_kb) = fs::read_to_string("/proc/self/status").map_or((0, 0), |t| split(&t));
    Figure { private_kb, rss_kb, processes: 1 }
}

pub fn json_of(f: &Figure) -> Value {
    json!({"private_kb": f.private_kb, "rss_kb": f.rss_kb, "processes": f.processes})
}

fn parent_of(pid: u32) -> Option<u32> {
    // the fields are counted after the last `)`: the name may hold spaces and brackets; then the state, then the parent
    fs::read_to_string(format!("/proc/{pid}/stat")).ok()?.rsplit_once(") ")?.1.split(' ').nth(1)?.parse().ok()
}

/// Every process and the ones it started, from one scan of `/proc`.
fn scan() -> HashMap<u32, Vec<u32>> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
        if let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) {
            if let Some(parent) = parent_of(pid) {
                children.entry(parent).or_default().push(pid);
            }
        }
    }
    children
}

/// The processes `pid` started, from the `children` file of each of its threads (a child is listed with the thread that started it and stays on a live
/// thread when that one ends).
fn children_of(pid: u32) -> Vec<u32> {
    let tasks = fs::read_dir(format!("/proc/{pid}/task")).into_iter().flatten().flatten();
    tasks.filter_map(|t| fs::read_to_string(t.path().join("children")).ok()).flat_map(|text| text.split_whitespace().filter_map(|p| p.parse().ok()).collect::<Vec<u32>>()).collect()
}

fn has_children_files() -> bool {
    let me = std::process::id();
    std::path::Path::new(&format!("/proc/{me}/task/{me}/children")).exists()
}

/// The figure of each agent in `roots` with all of its descendants; an agent that is gone is left out.
pub fn walk(roots: &[u32]) -> BTreeMap<u32, Figure> {
    walk_with(roots, has_children_files())
}

fn walk_with(roots: &[u32], files: bool) -> BTreeMap<u32, Figure> {
    let everyone = (!files).then(scan);
    let kids = |pid: u32| -> Vec<u32> { everyone.as_ref().map_or_else(|| children_of(pid), |all| all.get(&pid).cloned().unwrap_or_default()) };
    let mut found = BTreeMap::new();
    for &root in roots {
        let (mut figure, mut stack, mut seen) = (Figure::default(), vec![root], HashSet::new());
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            if let Ok(text) = fs::read_to_string(format!("/proc/{pid}/status")) {
                let (private, rss) = split(&text);
                (figure.private_kb, figure.rss_kb, figure.processes) = (figure.private_kb + private, figure.rss_kb + rss, figure.processes + 1);
            }
            stack.extend(kids(pid));
        }
        if figure.processes > 0 {
            found.insert(root, figure);
        }
    }
    found
}

fn refresh_if_stale() {
    let roots: Vec<u32> = {
        let mut c = cache();
        if c.busy || c.at.map_or(false, |t| t.elapsed() < EVERY) {
            return;
        }
        c.busy = true;
        std::mem::take(&mut c.asked).into_iter().collect()
    };
    let started = std::thread::Builder::new().name("memory".into()).spawn(move || {
        let figures = walk(&roots);
        let mut c = cache();
        (c.figures, c.at, c.busy) = (figures, Some(Instant::now()), false);
    });
    if started.is_err() {
        cache().busy = false;
    }
}

/// A status reply is complete: start a look if the last is old (it covers the agents the calls since then named), and give the host's own figure.
pub fn host_json() -> Value {
    refresh_if_stale();
    json_of(&own())
}

/// Put the figure of a running session into its status entry (when there is one yet) and name its agent for the next look.
pub fn annotate(entry: &mut Value, s: &Session) {
    if s.state != State::Running || s.agent_pid == 0 {
        return;
    }
    let figure = {
        let mut c = cache();
        c.asked.insert(s.agent_pid);
        c.figures.get(&s.agent_pid).copied()
    };
    if let Some(f) = figure {
        entry["memory"] = json_of(&f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_is_anonymous_plus_shared_memory_and_a_file_mapping_counts_for_rss_only() {
        let text = "Name:\tx\nVmRSS:\t  109000 kB\nRssAnon:\t   3000 kB\nRssFile:\t 106000 kB\nRssShmem:\t   500 kB\nThreads:\t4\n";
        assert_eq!(split(text), (3500, 109000));
        assert_eq!(split("Name:\tx\n"), (0, 0), "a kernel without the fields gives nothing, not a guess");
    }

    #[test]
    fn a_look_sums_the_agent_and_its_descendants_by_the_children_files_and_by_a_scan_and_leaves_a_gone_agent_blank() {
        let mut child = std::process::Command::new("sh").args(["-c", "sleep 5 & wait"]).spawn().unwrap();
        let root = child.id();
        for files in [true, false] {
            if files && !has_children_files() {
                continue;
            }
            let mut found = Figure::default();
            for _ in 0..40 {
                found = walk_with(&[root], files).get(&root).copied().unwrap_or_default();
                if found.processes >= 2 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            assert_eq!(found.processes, 2, "the shell and its background sleep (children files: {files}): {found:?}");
            assert!(found.rss_kb > 0 && found.private_kb <= found.rss_kb, "a shell and a sleep hold next to nothing of their own, only their libraries: {found:?}");
            assert!(walk_with(&[0x7fff_fff0], files).is_empty(), "a process that is not there is left out, not an error (children files: {files})");
        }
        let _ = (child.kill(), child.wait());
    }
}
