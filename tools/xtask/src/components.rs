//! Components (`components/<name>/`): the manifest check, the graph check and receipts.
//!
//! A component's `Cargo.toml` carries `[package.metadata.lico]`: `purpose` (one sentence), `spec` (the file, default `SPEC.md`), `depends` (the components it may use, which must
//! be exactly its path dependencies on other components), `external` (programs its tests need), `budget = { mib, seconds }` (default 256 MiB and 30 s). A receipt (`RECEIPT.json`
//! next to the component) records a hash of the component's inputs and the result of its last isolated run; `verify` says which input changed when it no longer matches.

use crate::{runner, sha256, toml};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Component {
    pub name: String,
    pub dir: PathBuf,
    pub version: String,
    pub purpose: String,
    pub spec: String,
    pub depends: Vec<String>,
    pub external: Vec<String>,
    pub budget_mib: u64,
    pub budget_s: u64,
    /// The path dependencies of `Cargo.toml` that point at other components.
    pub path_deps: Vec<String>,
    /// The other dependencies (crates from the registry), by name.
    pub crates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Hashed {
    pub hash: String,
    pub files: Vec<(String, String)>,
    pub deps: Vec<(String, String)>,
    pub lock: String,
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

pub fn discover(root: &Path) -> Result<Vec<Component>, String> {
    let dir = root.join("components");
    let mut names: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.join("Cargo.toml").is_file()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    let mut out = Vec::new();
    for path in names {
        let text = std::fs::read_to_string(path.join("Cargo.toml")).map_err(|e| format!("{}: {e}", path.display()))?;
        let v = toml::parse(&text).map_err(|e| format!("{}: Cargo.toml: {e}", path.display()))?;
        let name = v["package"]["name"].as_str().ok_or_else(|| format!("{}: Cargo.toml has no package name", path.display()))?.to_string();
        let lico = &v["package"]["metadata"]["lico"];
        let mut path_deps = Vec::new();
        let mut crates = Vec::new();
        if let Some(deps) = v["dependencies"].as_object() {
            for (dep, spec) in deps {
                match spec.get("path").and_then(Value::as_str) {
                    Some(p) if p.starts_with("../") => path_deps.push(dep.clone()),
                    Some(_) => {}
                    None => crates.push(dep.clone()),
                }
            }
        }
        out.push(Component {
            name,
            dir: path.clone(),
            version: v["package"]["version"].as_str().unwrap_or("").to_string(),
            purpose: lico["purpose"].as_str().unwrap_or("").to_string(),
            spec: lico["spec"].as_str().unwrap_or("SPEC.md").to_string(),
            depends: strings(&lico["depends"]),
            external: strings(&lico["external"]),
            budget_mib: lico["budget"]["mib"].as_u64().unwrap_or(256),
            budget_s: lico["budget"]["seconds"].as_u64().unwrap_or(30),
            path_deps,
            crates,
        });
    }
    Ok(out)
}

/// The problems of the manifests and of the dependency graph of the components; empty when there is none.
pub fn check(root: &Path) -> Vec<String> {
    let all = match discover(root) {
        Ok(a) => a,
        Err(e) => return vec![e],
    };
    let mut problems = Vec::new();
    let names: BTreeSet<&str> = all.iter().map(|c| c.name.as_str()).collect();
    if names.len() != all.len() {
        problems.push("two components have the same name".to_string());
    }
    for c in &all {
        if c.purpose.trim().is_empty() {
            problems.push(format!("{}: the manifest has no purpose", c.name));
        }
        if c.version.is_empty() {
            problems.push(format!("{}: the manifest has no version", c.name));
        }
        if !c.dir.join(&c.spec).is_file() {
            problems.push(format!("{}: {} is missing", c.name, c.spec));
        }
        if c.budget_mib == 0 || c.budget_s == 0 {
            problems.push(format!("{}: the budget must be positive", c.name));
        }
        for d in &c.path_deps {
            if !c.depends.contains(d) {
                problems.push(format!("{} depends on {} in Cargo.toml but the manifest does not list it", c.name, d));
            }
        }
        for d in &c.depends {
            if !names.contains(d.as_str()) {
                problems.push(format!("{} lists {} which is not a component", c.name, d));
            } else if !c.path_deps.contains(d) {
                problems.push(format!("{} lists {} but does not depend on it", c.name, d));
            }
        }
    }
    // cycles among the components (listed and real dependencies together)
    let edges: BTreeMap<&str, Vec<&str>> = all.iter().map(|c| (c.name.as_str(), c.depends.iter().chain(c.path_deps.iter()).map(String::as_str).filter(|d| names.contains(d)).collect())).collect();
    let mut state: BTreeMap<&str, u8> = BTreeMap::new();
    let mut found: Option<Vec<String>> = None;
    fn visit<'a>(n: &'a str, edges: &BTreeMap<&'a str, Vec<&'a str>>, state: &mut BTreeMap<&'a str, u8>, stack: &mut Vec<&'a str>, found: &mut Option<Vec<String>>) {
        if found.is_some() {
            return;
        }
        state.insert(n, 1);
        stack.push(n);
        for &m in edges.get(n).map(|v| v.as_slice()).unwrap_or(&[]) {
            match state.get(m).copied().unwrap_or(0) {
                0 => visit(m, edges, state, stack, found),
                1 => {
                    let at = stack.iter().position(|&x| x == m).unwrap_or(0);
                    let mut cycle: Vec<String> = stack[at..].iter().map(|s| s.to_string()).collect();
                    cycle.push(m.to_string());
                    *found = Some(cycle);
                    return;
                }
                _ => {}
            }
        }
        stack.pop();
        state.insert(n, 2);
    }
    for n in edges.keys() {
        if state.get(n).copied().unwrap_or(0) == 0 {
            visit(n, &edges, &mut state, &mut Vec::new(), &mut found);
        }
    }
    if let Some(cycle) = found {
        problems.push(format!("cycle among components: {}", cycle.join(" -> ")));
    }
    problems
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk(&p, base, out);
        } else if name != "RECEIPT.json" {
            out.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The lock file entries of the registry crates a component uses, with everything they use in turn: `name version checksum` lines, sorted, hashed.
fn lock_digest(root: &Path, crates: &[String]) -> String {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.lock")) else { return String::new() };
    let Ok(v) = toml::parse(&text) else { return String::new() };
    let packages = v["package"].as_array().cloned().unwrap_or_default();
    let by_name: BTreeMap<String, Vec<&Value>> = packages.iter().fold(BTreeMap::new(), |mut m, p| {
        m.entry(p["name"].as_str().unwrap_or("").to_string()).or_default().push(p);
        m
    });
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut todo: Vec<String> = crates.to_vec();
    let mut lines = BTreeSet::new();
    while let Some(n) = todo.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        for p in by_name.get(&n).into_iter().flatten() {
            lines.insert(format!("{} {} {}", n, p["version"].as_str().unwrap_or(""), p["checksum"].as_str().unwrap_or("")));
            for d in strings(&p["dependencies"]) {
                todo.push(d.split(' ').next().unwrap_or("").to_string());
            }
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    sha256::hex(lines.into_iter().collect::<Vec<_>>().join("\n").as_bytes())
}

pub fn hash(root: &Path, name: &str, toolchain: &str) -> Result<Hashed, String> {
    let all = discover(root)?;
    hash_in(root, &all, name, toolchain, &mut Vec::new())
}

fn hash_in(root: &Path, all: &[Component], name: &str, toolchain: &str, stack: &mut Vec<String>) -> Result<Hashed, String> {
    if stack.iter().any(|s| s == name) {
        return Err(format!("cycle among components: {} -> {name}", stack.join(" -> ")));
    }
    let c = all.iter().find(|c| c.name == name).ok_or_else(|| format!("no component named {name}"))?;
    stack.push(name.to_string());
    let mut rel = Vec::new();
    walk(&c.dir, &c.dir, &mut rel);
    rel.sort();
    let mut files = Vec::new();
    for r in rel {
        files.push((r.clone(), sha256::file_hex(&c.dir.join(&r)).map_err(|e| format!("{name}: {r}: {e}"))?));
    }
    let mut deps = Vec::new();
    let mut listed = c.depends.clone();
    listed.sort();
    for d in &listed {
        deps.push((d.clone(), hash_in(root, all, d, toolchain, stack)?.hash));
    }
    stack.pop();
    let lock = lock_digest(root, &c.crates);
    let mut text = String::from("files\n");
    for (p, h) in &files {
        text.push_str(&format!("{p} {h}\n"));
    }
    text.push_str("dependencies\n");
    for (d, h) in &deps {
        text.push_str(&format!("{d} {h}\n"));
    }
    text.push_str(&format!("lock\n{lock}\ntoolchain\n{toolchain}\n"));
    Ok(Hashed { hash: sha256::hex(text.as_bytes()), files, deps, lock })
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let z = secs / 86_400 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// Writes `RECEIPT.json` next to the component: the hash of its inputs, what they were, the result and the cost of the run.
pub fn record(root: &Path, name: &str, toolchain: &str, run: &runner::Run) -> Result<(), String> {
    let all = discover(root)?;
    let c = all.iter().find(|c| c.name == name).ok_or_else(|| format!("no component named {name}"))?;
    let h = hash_in(root, &all, name, toolchain, &mut Vec::new())?;
    let receipt = json!({
        "component": c.name,
        "version": c.version,
        "hash": h.hash,
        "inputs": {
            "files": h.files.iter().map(|(p, s)| json!([p, s])).collect::<Vec<_>>(),
            "dependencies": h.deps.iter().map(|(d, s)| json!({"name": d, "hash": s})).collect::<Vec<_>>(),
            "externals": c.external,
            "lock": h.lock,
            "toolchain": toolchain,
        },
        "result": {"passed": run.passed, "failed": run.failed, "ignored": run.ignored, "count": run.passed + run.failed + run.ignored},
        "cost": {"wall_s": (run.wall_s * 100.0).round() / 100.0, "peak_mib": (run.peak_mib * 10.0).round() / 10.0, "compile_s": (run.compile_s * 10.0).round() / 10.0, "budget": {"mib": c.budget_mib, "seconds": c.budget_s}},
        "tool": format!("lico-xtask {}", env!("CARGO_PKG_VERSION")),
        "date": today(),
    });
    let text = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())? + "\n";
    std::fs::write(c.dir.join("RECEIPT.json"), text).map_err(|e| format!("{name}: {e}"))
}

/// What no longer matches the receipts: for each component without a valid receipt, the input that changed.
pub fn verify(root: &Path, toolchain: &str) -> Vec<String> {
    let all = match discover(root) {
        Ok(a) => a,
        Err(e) => return vec![e],
    };
    let mut problems = Vec::new();
    for c in &all {
        let Ok(text) = std::fs::read_to_string(c.dir.join("RECEIPT.json")) else {
            problems.push(format!("{}: no receipt", c.name));
            continue;
        };
        let Ok(r) = serde_json::from_str::<Value>(&text) else {
            problems.push(format!("{}: the receipt cannot be read", c.name));
            continue;
        };
        let now = match hash_in(root, &all, &c.name, toolchain, &mut Vec::new()) {
            Ok(h) => h,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        if r["hash"].as_str() != Some(now.hash.as_str()) {
            let before: BTreeMap<String, String> = r["inputs"]["files"].as_array().map(|a| a.iter().filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_str()?.to_string()))).collect()).unwrap_or_default();
            let after: BTreeMap<&String, &String> = now.files.iter().map(|(p, h)| (p, h)).collect();
            let mut why = 0;
            for (p, h) in &after {
                match before.get(*p) {
                    None => problems.push(format!("{}: {p} was added", c.name)),
                    Some(b) if b != *h => problems.push(format!("{}: {p} changed", c.name)),
                    _ => continue,
                }
                why += 1;
            }
            for p in before.keys() {
                if !after.contains_key(p) {
                    problems.push(format!("{}: {p} was removed", c.name));
                    why += 1;
                }
            }
            let dep_before: BTreeMap<String, String> = r["inputs"]["dependencies"].as_array().map(|a| a.iter().filter_map(|d| Some((d["name"].as_str()?.to_string(), d["hash"].as_str()?.to_string()))).collect()).unwrap_or_default();
            if dep_before != now.deps.iter().cloned().collect::<BTreeMap<_, _>>() {
                problems.push(format!("{}: a dependency changed", c.name));
                why += 1;
            }
            if r["inputs"]["lock"].as_str().unwrap_or("") != now.lock {
                problems.push(format!("{}: the locked crates changed", c.name));
                why += 1;
            }
            if r["inputs"]["toolchain"].as_str() != Some(toolchain) {
                problems.push(format!("{}: the toolchain changed", c.name));
                why += 1;
            }
            if why == 0 {
                problems.push(format!("{}: the hash differs", c.name));
            }
        } else if r["result"]["failed"].as_u64().unwrap_or(0) > 0 {
            problems.push(format!("{}: the recorded run has a failure", c.name));
        }
    }
    problems
}
