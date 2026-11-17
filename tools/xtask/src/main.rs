//! `cargo xtask <command>`: the tools of the component rules (M15-P02). Commands:
//!   graph  [--root FILE] [--json OUT] [--cut]   the module graph of the crate (default root: src/host/hostd/src/unified.rs)
//!   check  [--root DIR]                         the manifests and the dependency graph of the components; non-zero when anything is wrong
//!   test   NAME [--root DIR] [--record]         the isolated run of a component against its budget; --record writes its RECEIPT.json when it passes
//!   verify [--root DIR]                         every receipt against the inputs as they are now, naming what changed
//!   hash   NAME [--root DIR]                    the hash of a component's inputs
//!   accept [--repo DIR] [--script PATH] [--artifact NAME] [--scratch DIR]   two clean clones build the same bytes

use lico_xtask::{accept, components, graph, runner, system};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn toolchain() -> String {
    Command::new("rustc").arg("-vV").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".to_string()));
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!("usage: cargo xtask graph|check|test|verify|hash|accept ...");
        return ExitCode::from(2);
    };
    match command {
        "graph" => {
            let file = arg(&args, "--root").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("src/host/hostd/src/unified.rs"));
            let g = match graph::analyse(&file) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            let big = g.sccs.first().map_or(0, Vec::len);
            let mut layers: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
            for l in g.layers.values() {
                *layers.entry(*l).or_default() += 1;
            }
            println!("{} modules, {} nodes, {} production edges, {} test-only edges, {} use sites", g.modules, g.nodes.len(), g.prod.len(), g.test.len(), g.prod.values().sum::<u32>() + g.test.values().sum::<u32>());
            println!("cycles: {} (the biggest has {big} modules), two-way pairs in them: {}", g.sccs.len(), g.mutual_pairs);
            println!("layers (modules per layer): {layers:?}");
            println!("uncertain: {} glob imports, {} files with macro_rules!", g.glob_uses, g.macro_files);
            if args.iter().any(|a| a == "--cut") {
                let cut = g.best_cut();
                println!("best cut found: {} edges (lower bound {}): {}", cut.len(), g.mutual_pairs, cut.iter().map(|(a, b)| format!("{a}->{b}")).collect::<Vec<_>>().join(" "));
            }
            if let Some(out) = arg(&args, "--json") {
                if let Err(e) = std::fs::write(&out, serde_json::to_string_pretty(&g.to_json()).unwrap_or_default() + "\n") {
                    eprintln!("{out}: {e}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        "check" => {
            let problems = components::check(&root);
            for p in &problems {
                eprintln!("{p}");
            }
            if problems.is_empty() {
                println!("the components are in order");
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        "verify" => {
            let problems = components::verify(&root, &toolchain());
            for p in &problems {
                eprintln!("{p}");
            }
            if problems.is_empty() {
                println!("every receipt matches its inputs");
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        "hash" => {
            let Some(name) = args.get(1) else { return ExitCode::from(2) };
            match components::hash(&root, name, &toolchain()) {
                Ok(h) => {
                    println!("{}", h.hash);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "test" => {
            let Some(name) = args.get(1) else { return ExitCode::from(2) };
            let all = match components::discover(&root) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            };
            let Some(c) = all.iter().find(|c| &c.name == name) else {
                eprintln!("no component named {name}");
                return ExitCode::FAILURE;
            };
            let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("target")).join("components").join(name);
            match runner::run(&root, c, &target) {
                Ok(run) => {
                    println!("{name}: {} passed, {} failed, {} ignored; tests {:.2} s, peak {:.1} MiB (budget {} MiB, {} s); compile {:.1} s", run.passed, run.failed, run.ignored, run.wall_s, run.peak_mib, c.budget_mib, c.budget_s, run.compile_s);
                    if let Some(why) = runner::over_budget(&run, c) {
                        eprintln!("{why}");
                        return ExitCode::FAILURE;
                    }
                    if run.failed > 0 {
                        return ExitCode::FAILURE;
                    }
                    if args.iter().any(|a| a == "--record") {
                        if let Err(e) = components::record(&root, name, &toolchain(), &run) {
                            eprintln!("{e}");
                            return ExitCode::FAILURE;
                        }
                        println!("{name}: receipt written");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        "system" => {
            let root = std::env::current_dir().unwrap_or_default();
            let list = root.join("tests/system_tests.toml");
            let mut entries = match std::fs::read_to_string(&list).map_err(|e| e.to_string()).and_then(|t| system::parse(&t)) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("tests/system_tests.toml: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let listed = system::problems(&entries);
            if !listed.is_empty() {
                eprintln!("{}", listed.join("\n"));
                return ExitCode::FAILURE;
            }
            let only = args.iter().skip(2).find(|a| !a.starts_with("--")).cloned();
            let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("private/build/system"));
            let binary = match std::env::var("LICO_BIN") {
                Ok(b) => PathBuf::from(b),
                Err(_) => {
                    let built = std::process::Command::new("cargo").args(["build", "--release", "--locked", "--offline", "--bin", "lico", "-p", "lico", "--features", "install_seam", "--target-dir"]).arg(&target).status();
                    if !matches!(built, Ok(s) if s.success()) {
                        eprintln!("the release binary did not build");
                        return ExitCode::FAILURE;
                    }
                    target.join("release/lico")
                }
            };
            let compiled = std::process::Command::new("cargo").args(["test", "--offline", "--no-run", "--message-format=json", "--bin", "lico", "-p", "lico", "--target-dir"]).arg(&target).output();
            let Some(compiled) = compiled.ok().filter(|o| o.status.success()) else {
                eprintln!("the tests of the program did not compile");
                return ExitCode::FAILURE;
            };
            let exe = String::from_utf8_lossy(&compiled.stdout).lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).find(|v| v["reason"] == "compiler-artifact" && v["profile"]["test"] == true && v["target"]["name"] == "lico" && v["executable"].is_string()).and_then(|v| v["executable"].as_str().map(PathBuf::from));
            let Some(exe) = exe else {
                eprintln!("the test binary of the program was not found");
                return ExitCode::FAILURE;
            };
            // the test double of the browser engine, built beside the host binary (the system tests of the browser adapter run it in place of the real engine)
            let fake_engine = match std::env::var("LICO_FAKE_ENGINE") {
                Ok(f) => PathBuf::from(f),
                Err(_) => {
                    let built = std::process::Command::new("cargo").args(["build", "--release", "--locked", "--offline", "--bin", "lico-fake-engine", "-p", "lico-fake-engine", "--target-dir"]).arg(&target).status();
                    if !matches!(built, Ok(s) if s.success()) {
                        eprintln!("the fake engine did not build");
                        return ExitCode::FAILURE;
                    }
                    target.join("release/lico-fake-engine")
                }
            };
            let env = vec![("LICO_BIN".to_string(), binary.to_string_lossy().into_owned()), ("LICO_FAKE_ENGINE".to_string(), fake_engine.to_string_lossy().into_owned())];
            let mut outcomes = Vec::new();
            let mut failed = false;
            for entry in entries.iter().filter(|e| only.as_ref().map_or(true, |o| &e.name == o)) {
                let out = system::run_one(&exe, entry, &env, &root.join("src/host/hostd"));
                println!("{}: {} in {:.1} s, peak {:.1} MiB (budget {} MiB, {} s)", entry.name, if out.passed { "passed" } else { "FAILED" }, out.wall_s, out.peak_mib, entry.mib, entry.seconds);
                if let Some(p) = &out.problem {
                    eprintln!("{p}");
                    failed = true;
                }
                outcomes.push(out);
            }
            if args.iter().any(|a| a == "--record") && !failed && only.is_none() {
                system::apply(&mut entries, &outcomes);
                if let Err(e) = std::fs::write(&list, system::render(&entries)) {
                    eprintln!("the list could not be written: {e}");
                    return ExitCode::FAILURE;
                }
            }
            if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        "accept" => {
            let repo = PathBuf::from(arg(&args, "--repo").unwrap_or_else(|| ".".to_string()));
            let script = arg(&args, "--script").unwrap_or_else(|| "src/host/hostd/build-release.sh".to_string());
            let artifact = arg(&args, "--artifact").unwrap_or_else(|| "lico".to_string());
            let scratch = PathBuf::from(arg(&args, "--scratch").unwrap_or_else(|| "private/acceptance".to_string()));
            match accept::run_acceptance(&repo, &script, &artifact, &scratch) {
                Ok(digest) => {
                    println!("two clean clones build the same {artifact}: {digest}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        other => {
            eprintln!("unknown command {other}");
            ExitCode::from(2)
        }
    }
}
