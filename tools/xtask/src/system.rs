//! The short named list of system tests (`tests/system_tests.toml`, M16-P06): a few tests that need a real process boundary (the host binary, its socket, its registry and
//! log) and that no component test has. Each is on the list with the reason, run alone, and measured against its own budget (at most 512 MiB and 60 s); no test is
//! moved onto the list to make it pass, and a test off the list has the budget of its component.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

use crate::{runner, toml};

pub const MAX_MIB: u64 = 512;
pub const MAX_SECONDS: u64 = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// The test's path as the test runner names it (`--exact`).
    pub path: String,
    pub why: String,
    pub mib: u64,
    pub seconds: u64,
    pub measured_mib: Option<String>,
    pub measured_seconds: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub passed: bool,
    pub wall_s: f64,
    pub peak_mib: f64,
    pub problem: Option<String>,
}

pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let v = toml::parse(text)?;
    let text_of = |t: &Value, k: &str| t[k].as_str().unwrap_or("").to_string();
    Ok(v["test"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| Entry {
                    name: text_of(t, "name"),
                    path: text_of(t, "path"),
                    why: text_of(t, "why"),
                    mib: t["mib"].as_u64().unwrap_or(0),
                    seconds: t["seconds"].as_u64().unwrap_or(0),
                    measured_mib: t["measured_mib"].as_str().map(str::to_string),
                    measured_seconds: t["measured_seconds"].as_str().map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default())
}

/// What is wrong with the list itself.
pub fn problems(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    if entries.is_empty() {
        out.push("the list has no system test".to_string());
    }
    for (i, e) in entries.iter().enumerate() {
        if e.name.is_empty() || e.path.is_empty() {
            out.push(format!("entry {}: a name and a path are needed", i + 1));
        }
        if e.why.trim().is_empty() {
            out.push(format!("{}: the list says why a test is on it (why)", e.name));
        }
        if e.mib == 0 || e.seconds == 0 {
            out.push(format!("{}: the budget must be positive", e.name));
        }
        if e.mib > MAX_MIB {
            out.push(format!("{}: a system test may take at most {MAX_MIB} MiB", e.name));
        }
        if e.seconds > MAX_SECONDS {
            out.push(format!("{}: a system test may take at most {MAX_SECONDS} seconds", e.name));
        }
        if entries[..i].iter().any(|o| o.name == e.name || o.path == e.path) {
            out.push(format!("{}: named twice", e.name));
        }
    }
    out
}

/// Runs one test alone (`--ignored --exact PATH --nocapture`) from the compiled test binary `exe`, measured.
pub fn run_one(exe: &Path, entry: &Entry, env: &[(String, String)], cwd: &Path) -> Outcome {
    let mut command = Command::new(exe);
    command.args(["--ignored", "--exact", &entry.path, "--nocapture"]).current_dir(cwd);
    for (k, v) in env {
        command.env(k, v);
    }
    match runner::measured(command) {
        Err(e) => Outcome { passed: false, wall_s: 0.0, peak_mib: 0.0, problem: Some(format!("{}: could not be run: {e}", entry.name)) },
        Ok((text, wall_s, peak_mib)) => {
            let passed = text.lines().any(|l| l.starts_with("test result: ok. 1 passed"));
            let mut why = Vec::new();
            if !passed {
                why.push(format!("{}: did not pass:\n{}", entry.name, text.lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")));
            }
            if peak_mib > entry.mib as f64 {
                why.push(format!("{}: the peak memory, {peak_mib:.1} MiB, is over its budget of {} MiB", entry.name, entry.mib));
            }
            if wall_s > entry.seconds as f64 {
                why.push(format!("{}: it took {wall_s:.1} seconds, over its budget of {} seconds", entry.name, entry.seconds));
            }
            Outcome { passed, wall_s, peak_mib, problem: (!why.is_empty()).then(|| why.join("; ")) }
        }
    }
}

/// Puts what a passing run measured next to its test; a failed run changes nothing.
pub fn apply(entries: &mut [Entry], outcomes: &[Outcome]) {
    for (e, o) in entries.iter_mut().zip(outcomes) {
        if o.passed {
            e.measured_mib = Some(format!("{:.1}", o.peak_mib));
            e.measured_seconds = Some(format!("{:.1}", o.wall_s));
        }
    }
}

pub fn render(entries: &[Entry]) -> String {
    let mut out = String::from("# The short named list of system tests (M16-P06). Each needs a real process boundary that no component test has; the budget of every one is at most 512 MiB and 60 s.\n# `cargo xtask system --record` writes the measured peak and time (of the last passing run) next to each test.\n");
    for e in entries {
        out.push_str(&format!("\n[[test]]\nname = \"{}\"\npath = \"{}\"\nwhy = \"{}\"\nmib = {}\nseconds = {}\n", e.name, e.path, e.why.replace('"', "'"), e.mib, e.seconds));
        if let (Some(m), Some(s)) = (&e.measured_mib, &e.measured_seconds) {
            out.push_str(&format!("measured_mib = \"{m}\"\nmeasured_seconds = \"{s}\"\n"));
        }
    }
    out
}
