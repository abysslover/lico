//! The isolated test run of a component: compiled apart (its time is reported, its memory is not part of the budget), then each test binary run alone, with the tree of
//! processes it started (the process and its descendants, orphans included while they live) sampled from /proc for the peak resident memory, against the manifest's budget.

use crate::components::Component;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct Run {
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
    pub wall_s: f64,
    pub peak_mib: f64,
    pub compile_s: f64,
}

fn page_size() -> u64 {
    Command::new("getconf").arg("PAGESIZE").output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(4096)
}

/// `pid -> (parent, resident bytes)` for every process that can be read.
fn processes(page: u64) -> BTreeMap<u32, (u32, u64)> {
    let mut out = BTreeMap::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return out };
    for e in rd.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        let Some(after) = stat.rfind(')').map(|i| &stat[i + 1..]) else { continue };
        let ppid = after.split_whitespace().nth(1).and_then(|f| f.parse::<u32>().ok()).unwrap_or(0);
        let rss = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok().and_then(|s| s.split_whitespace().nth(1).and_then(|f| f.parse::<u64>().ok())).unwrap_or(0);
        out.insert(pid, (ppid, rss * page));
    }
    out
}

/// The sum of the resident sets of `root` and what it started, now.
fn tree_bytes(root: u32, page: u64, known: &mut BTreeSet<u32>) -> u64 {
    let all = processes(page);
    known.retain(|p| all.contains_key(p));
    known.insert(root);
    let mut grew = true;
    while grew {
        grew = false;
        for (pid, (ppid, _)) in &all {
            if !known.contains(pid) && known.contains(ppid) {
                known.insert(*pid);
                grew = true;
            }
        }
    }
    known.iter().filter_map(|p| all.get(p)).map(|(_, rss)| *rss).sum()
}

/// Runs `command` to its end and returns its stdout, the wall time and the peak of its process tree.
fn measured(mut command: Command) -> Result<(String, f64, f64), String> {
    let page = page_size();
    let start = Instant::now();
    let child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicU64::new(0));
    let sampler = {
        let (stop, peak) = (stop.clone(), peak.clone());
        std::thread::spawn(move || {
            let mut known = BTreeSet::new();
            while !stop.load(Ordering::SeqCst) {
                peak.fetch_max(tree_bytes(pid, page, &mut known), Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(40));
            }
        })
    };
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let wall = start.elapsed().as_secs_f64();
    stop.store(true, Ordering::SeqCst);
    let _ = sampler.join();
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    Ok((text, wall, peak.load(Ordering::SeqCst) as f64 / 1_048_576.0))
}

/// Compiles the tests of `component` (its own target directory `target`) and runs each test binary alone.
pub fn run(root: &Path, component: &Component, target: &Path) -> Result<Run, String> {
    let compile_start = Instant::now();
    let built = Command::new("cargo")
        .args(["test", "--offline", "--no-run", "--message-format=json", "-p", &component.name, "--target-dir"])
        .arg(target)
        .current_dir(root)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .map_err(|e| format!("cargo: {e}"))?;
    if !built.status.success() {
        let err = String::from_utf8_lossy(&built.stderr);
        return Err(format!("{}: the tests do not compile:\n{}", component.name, err.lines().rev().take(20).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")));
    }
    let compile_s = compile_start.elapsed().as_secs_f64();
    let mut binaries = Vec::new();
    for line in String::from_utf8_lossy(&built.stdout).lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v["reason"] == "compiler-artifact" && v["profile"]["test"] == true {
                if let Some(exe) = v["executable"].as_str() {
                    if v["package_id"].as_str().is_some_and(|id| id.contains(&component.name)) {
                        binaries.push(exe.to_string());
                    }
                }
            }
        }
    }
    let mut run = Run { compile_s, ..Run::default() };
    for exe in binaries {
        let mut command = Command::new(&exe);
        command.current_dir(&component.dir).env("CARGO_MANIFEST_DIR", &component.dir).env("CARGO_PKG_NAME", &component.name);
        let (text, wall, peak) = measured(command)?;
        run.wall_s += wall;
        run.peak_mib = run.peak_mib.max(peak);
        for line in text.lines().filter(|l| l.starts_with("test result:")) {
            let count = |word: &str| line.split(';').find(|p| p.trim_end().ends_with(word)).and_then(|p| p.split_whitespace().rev().nth(1).and_then(|n| n.parse::<u64>().ok())).unwrap_or(0);
            run.passed += count("passed");
            run.failed += count("failed");
            run.ignored += count("ignored");
        }
    }
    Ok(run)
}

/// What a run exceeded of the manifest's budget, in words; None when it is inside.
pub fn over_budget(run: &Run, component: &Component) -> Option<String> {
    let mut why = Vec::new();
    if run.peak_mib > component.budget_mib as f64 {
        why.push(format!("{}: the peak memory of its tests, {:.1} MiB, is over its budget of {} MiB", component.name, run.peak_mib, component.budget_mib));
    }
    if run.wall_s > component.budget_s as f64 {
        why.push(format!("{}: its tests took {:.1} seconds, over its budget of {} seconds", component.name, run.wall_s, component.budget_s));
    }
    (!why.is_empty()).then(|| why.join("; "))
}
