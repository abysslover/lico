//! Tests of the tools themselves (M15-P02), written before the tools: a graph with a cycle is refused, a changed file invalidates a receipt, an over-budget run is reported,
//! a clone build that differs is refused. The synthetic workspaces live under the target directory of the test run (CARGO_TARGET_TMPDIR), never in /tmp.

use lico_xtask::{accept, components, graph, runner, sha256, toml};
use std::fs;
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn workspace(name: &str) -> PathBuf {
    let root = scratch(name);
    fs::write(root.join("Cargo.toml"), "[workspace]\nresolver = \"2\"\nmembers = [\"components/*\"]\n").unwrap();
    fs::write(root.join("Cargo.lock"), "# a lock\nversion = 4\n").unwrap();
    root
}

/// A component with `cargo_deps` as path dependencies and `listed` as its manifest's `depends`.
fn component(root: &Path, name: &str, cargo_deps: &[&str], listed: &[&str], source: &str) {
    let dir = root.join("components").join(name);
    fs::create_dir_all(dir.join("src")).unwrap();
    let deps: String = cargo_deps.iter().map(|d| format!("{d} = {{ path = \"../{d}\", version = \"0.1.0\" }}\n")).collect();
    let list = listed.iter().map(|d| format!("\"{d}\"")).collect::<Vec<_>>().join(", ");
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{deps}\n[package.metadata.lico]\npurpose = \"a test component\"\nspec = \"SPEC.md\"\ndepends = [{list}]\nexternal = []\nbudget = {{ mib = 256, seconds = 30 }}\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("SPEC.md"), "# spec\n").unwrap();
    fs::write(dir.join("src/lib.rs"), source).unwrap();
}

fn problems_text(root: &Path) -> String {
    components::check(root).join("\n")
}

#[test]
fn sha256_gives_the_published_digests() {
    assert_eq!(sha256::hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(sha256::hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    let long = vec![b'a'; 1_000_000];
    assert_eq!(sha256::hex(&long), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
}

#[test]
fn the_toml_reader_reads_a_manifest_and_a_lock_file() {
    let v = toml::parse("# c\n[package]\nname = \"x\"\nversion = \"0.1.0\"\n[package.metadata.lico]\ndepends = [\"a\",\n  \"b\"]\nbudget = { mib = 256, seconds = 30 }\nflag = true\n[[pkg]]\nname = \"p\"\n[[pkg]]\nname = \"q\"\n").unwrap();
    assert_eq!(v["package"]["name"], "x");
    assert_eq!(v["package"]["metadata"]["lico"]["depends"][1], "b");
    assert_eq!(v["package"]["metadata"]["lico"]["budget"]["mib"], 256);
    assert_eq!(v["package"]["metadata"]["lico"]["flag"], true);
    assert_eq!(v["pkg"][1]["name"], "q");
}

#[test]
fn a_good_workspace_passes_the_check() {
    let root = workspace("good");
    component(&root, "a", &[], &[], "pub fn f() {}\n");
    component(&root, "b", &["a"], &["a"], "pub fn g() {}\n");
    assert_eq!(components::check(&root), Vec::<String>::new());
}

#[test]
fn a_cycle_between_components_is_refused_and_named() {
    let root = workspace("cycle");
    component(&root, "a", &["b"], &["b"], "");
    component(&root, "b", &["a"], &["a"], "");
    let text = problems_text(&root);
    assert!(text.contains("cycle") && text.contains('a') && text.contains('b'), "{text}");
}

#[test]
fn a_dependency_that_the_manifest_does_not_list_is_refused_and_so_is_a_listed_one_that_is_not_used() {
    let root = workspace("unlisted");
    component(&root, "a", &[], &[], "");
    component(&root, "b", &["a"], &[], "");
    assert!(problems_text(&root).contains("b depends on a in Cargo.toml but the manifest does not list it"), "{}", problems_text(&root));
    let root = workspace("unused");
    component(&root, "a", &[], &[], "");
    component(&root, "b", &[], &["a"], "");
    assert!(problems_text(&root).contains("b lists a but does not depend on it"), "{}", problems_text(&root));
}

#[test]
fn a_manifest_without_its_fields_or_its_spec_is_refused() {
    let root = workspace("fields");
    component(&root, "a", &[], &[], "");
    let manifest = root.join("components/a/Cargo.toml");
    let text = fs::read_to_string(&manifest).unwrap().replace("purpose = \"a test component\"\n", "");
    fs::write(&manifest, text).unwrap();
    assert!(problems_text(&root).contains("a: the manifest has no purpose"), "{}", problems_text(&root));
    let root = workspace("nospec");
    component(&root, "a", &[], &[], "");
    fs::remove_file(root.join("components/a/SPEC.md")).unwrap();
    assert!(problems_text(&root).contains("a: SPEC.md is missing"), "{}", problems_text(&root));
}

#[test]
fn a_changed_file_invalidates_the_receipt_and_so_does_a_change_below() {
    let root = workspace("receipt");
    component(&root, "a", &[], &[], "pub fn f() {}\n");
    component(&root, "b", &["a"], &["a"], "pub fn g() {}\n");
    let run = runner::Run { passed: 3, failed: 0, ignored: 0, wall_s: 0.5, peak_mib: 12.0, compile_s: 1.0 };
    components::record(&root, "a", "rustc 1", &run).unwrap();
    components::record(&root, "b", "rustc 1", &run).unwrap();
    assert_eq!(components::verify(&root, "rustc 1"), Vec::<String>::new());
    // a build output and the receipt itself are not inputs
    fs::create_dir_all(root.join("components/a/target")).unwrap();
    fs::write(root.join("components/a/target/junk"), "x").unwrap();
    assert_eq!(components::verify(&root, "rustc 1"), Vec::<String>::new());
    // a changed source file of `a` invalidates `a` (naming the file) and `b` (a dependency changed)
    fs::write(root.join("components/a/src/lib.rs"), "pub fn f() { /* changed */ }\n").unwrap();
    let text = components::verify(&root, "rustc 1").join("\n");
    assert!(text.contains("a: src/lib.rs changed"), "{text}");
    assert!(text.contains("b: a dependency changed"), "{text}");
    fs::write(root.join("components/a/src/lib.rs"), "pub fn f() {}\n").unwrap();
    assert_eq!(components::verify(&root, "rustc 1"), Vec::<String>::new());
    // another toolchain invalidates both
    let text = components::verify(&root, "rustc 2").join("\n");
    assert!(text.contains("a: the toolchain changed") && text.contains("b: the toolchain changed"), "{text}");
    // a failed result is never valid
    let failed = runner::Run { failed: 1, ..run };
    components::record(&root, "a", "rustc 1", &failed).unwrap();
    assert!(components::verify(&root, "rustc 1").join("\n").contains("a: the recorded run has a failure"));
}

#[test]
fn an_over_budget_run_is_reported_with_the_figure_that_broke_it() {
    let root = workspace("budget");
    component(
        &root,
        "hog",
        &[],
        &[],
        "#[test]\nfn holds_memory() {\n    let mut v = vec![0u8; 96 << 20];\n    for i in (0..v.len()).step_by(4096) { v[i] = 1; }\n    std::thread::sleep(std::time::Duration::from_millis(800));\n    assert_eq!(v[0], 1);\n}\n",
    );
    component(&root, "slow", &[], &[], "#[test]\nfn takes_time() {\n    std::thread::sleep(std::time::Duration::from_millis(2500));\n}\n");
    component(&root, "fine", &[], &[], "#[test]\nfn is_small() {\n    assert_eq!(1 + 1, 2);\n}\n");
    let all = components::discover(&root).unwrap();
    let find = |n: &str| all.iter().find(|c| c.name == n).unwrap().clone();
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("budget-target");
    let mut hog = find("hog");
    hog.budget_mib = 32;
    let run = runner::run(&root, &hog, &target).unwrap();
    assert_eq!((run.passed, run.failed), (1, 0));
    assert!(run.peak_mib >= 90.0, "peak {}", run.peak_mib);
    let text = runner::over_budget(&run, &hog).unwrap();
    assert!(text.contains("MiB") && text.contains("32"), "{text}");
    let mut slow = find("slow");
    slow.budget_s = 1;
    let run = runner::run(&root, &slow, &target).unwrap();
    assert!(runner::over_budget(&run, &slow).unwrap().contains("seconds"));
    let fine = find("fine");
    let run = runner::run(&root, &fine, &target).unwrap();
    assert_eq!((run.passed, run.failed), (1, 0));
    assert!(run.compile_s >= 0.0 && runner::over_budget(&run, &fine).is_none(), "{run:?}");
}

#[test]
fn the_graph_tool_reads_modules_edges_test_only_uses_cycles_and_layers_from_the_text() {
    let root = scratch("graph");
    let src = root.join("src");
    fs::create_dir_all(src.join("d")).unwrap();
    fs::write(src.join("unified.rs"), "mod a;\nmod b;\nmod c;\nmod d;\nfn main() { a::f(); }\n").unwrap();
    fs::write(src.join("a.rs"), "// crate::d::x is a comment\nconst S: &str = \"crate::d::y\";\nuse crate::{b::g, c};\npub fn f() { crate::b::g(); let _ = c::h(); }\n").unwrap();
    fs::write(src.join("b.rs"), "pub fn g() { crate::a::f(); }\n").unwrap();
    fs::write(src.join("c.rs"), "pub fn h() -> u8 { 1 }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { crate::a::f(); assert_eq!(h(), 1); }\n}\n").unwrap();
    fs::write(src.join("d/mod.rs"), "pub mod inner;\npub fn x() {}\n").unwrap();
    fs::write(src.join("d/inner.rs"), "pub fn y() { super::x(); }\n").unwrap();
    let g = graph::analyse(&src.join("unified.rs")).unwrap();
    assert_eq!(g.modules, 6, "unified, a, b, c, d, d::inner");
    assert_eq!(g.nodes, ["a", "b", "c", "d", "unified"]);
    let edge = |m: &std::collections::BTreeMap<(String, String), u32>, a: &str, b: &str| m.contains_key(&(a.to_string(), b.to_string()));
    assert!(edge(&g.prod, "a", "b") && edge(&g.prod, "b", "a") && edge(&g.prod, "a", "c"), "{:?}", g.prod);
    assert!(!edge(&g.prod, "a", "d"), "a comment and a string make no edge: {:?}", g.prod);
    assert!(edge(&g.test, "c", "a") && !edge(&g.prod, "c", "a"), "a use inside a #[cfg(test)] module counts apart");
    assert!(edge(&g.prod, "unified", "a"), "the root dispatches by a bare child name");
    assert_eq!(g.sccs, vec![vec!["a".to_string(), "b".to_string()]]);
    assert_eq!(g.mutual_pairs, 1);
    assert_eq!((g.layers["c"], g.layers["d"], g.layers["a"], g.layers["b"], g.layers["unified"]), (0, 0, 1, 1, 2), "c and d depend on nothing; the cycle of a and b uses c; the root uses all");
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").args(args).current_dir(dir).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").status().unwrap().success();
    assert!(ok, "git {args:?}");
}

fn repo_with_script(name: &str, script: &str) -> PathBuf {
    let repo = scratch(name);
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("build.sh"), script).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "x"]);
    repo
}

#[test]
fn the_acceptance_passes_for_a_build_that_is_the_same_in_two_clones_and_refuses_one_that_is_not() {
    let same = repo_with_script("accept-same", "mkdir -p \"$1\" && printf 'the same bytes' > \"$1/lico\"\n");
    let digest = accept::run_acceptance(&same, "build.sh", "lico", &scratch("accept-same-out")).unwrap();
    assert_eq!(digest, sha256::hex(b"the same bytes"));
    let differs = repo_with_script("accept-differs", "mkdir -p \"$1\" && printf 'built in %s' \"$PWD\" > \"$1/lico\"\n");
    let err = accept::run_acceptance(&differs, "build.sh", "lico", &scratch("accept-differs-out")).unwrap_err();
    assert!(err.contains("differ") || err.contains("forbidden") || err.contains("where it was built"), "{err}");
}

#[test]
fn the_command_line_refuses_a_workspace_with_a_cycle_and_accepts_a_good_one() {
    let bad = workspace("cli-bad");
    component(&bad, "a", &["b"], &["b"], "");
    component(&bad, "b", &["a"], &["a"], "");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_xtask")).args(["check", "--root"]).arg(&bad).output().unwrap();
    assert!(!out.status.success(), "a cycle must give a non-zero exit");
    assert!(String::from_utf8_lossy(&out.stderr).contains("cycle"));
    let good = workspace("cli-good");
    component(&good, "a", &[], &[], "");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_xtask")).args(["check", "--root"]).arg(&good).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}
