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
fn the_failing_tests_of_a_binary_are_named_with_their_panic_and_the_two_lines_after_it() {
    let text = "running 3 tests\ntest a ... ok\ntest b ... FAILED\ntest c ... ok\n\nfailures:\n\n---- b stdout ----\n\nthread 'b' (7) panicked at tests/x.rs:9:5:\nassertion `left == right` failed\n  left: 1\n right: 2\nnote: run with RUST_BACKTRACE=1\n\ntest result: FAILED. 2 passed; 1 failed";
    let said = runner::failures_of(text);
    assert!(said.contains("test b ... FAILED") && said.contains("panicked at tests/x.rs:9:5") && said.contains("left == right") && said.contains("  left: 1"), "{said}");
    assert!(!said.contains("test a ... ok") && !said.contains("RUST_BACKTRACE"), "{said}");
    assert_eq!(runner::failures_of("test result: ok. 3 passed").len(), 0);
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
fn the_acceptance_takes_a_scratch_folder_given_as_a_relative_path() {
    let repo = repo_with_script("accept-relative", "mkdir -p \"$1\" && printf 'bytes' > \"$1/lico\"\n");
    let relative = Path::new("acceptance-relative-scratch");
    let _ = fs::remove_dir_all(relative);
    let result = accept::run_acceptance(&repo, "build.sh", "lico", relative);
    let _ = fs::remove_dir_all(relative);
    assert_eq!(result.unwrap(), sha256::hex(b"bytes"));
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

#[test]
fn a_changed_dev_dependency_invalidates_the_receipt_of_the_component_that_tests_with_it_and_needs_no_listing() {
    let root = workspace("devdep");
    component(&root, "a", &[], &[], "pub fn f() {}\n");
    component(&root, "b", &[], &[], "pub fn g() {}\n");
    let manifest = root.join("components/b/Cargo.toml");
    let text = fs::read_to_string(&manifest).unwrap();
    fs::write(&manifest, text.replace("[dependencies]\n", "[dependencies]\n\n[dev-dependencies]\na = { path = \"../a\" }\nserde_json = \"=1.0.154\"\n")).unwrap();
    assert_eq!(problems_text(&root), "", "a dev-dependency is not a dependency of the component: it is not listed and not checked against `depends`");
    let run = runner::Run { passed: 3, failed: 0, ignored: 0, wall_s: 0.5, peak_mib: 12.0, compile_s: 1.0 };
    components::record(&root, "a", "rustc 1", &run).unwrap();
    components::record(&root, "b", "rustc 1", &run).unwrap();
    assert_eq!(components::verify(&root, "rustc 1"), Vec::<String>::new());
    fs::write(root.join("components/a/src/lib.rs"), "pub fn f() { /* changed */ }\n").unwrap();
    let text = components::verify(&root, "rustc 1").join("\n");
    assert!(text.contains("a: src/lib.rs changed"), "{text}");
    assert!(text.contains("b: a dev-dependency changed"), "the tests of b run against a: {text}");
    // a component without dev-dependencies keeps the hash it had (the receipts already recorded stay valid)
    let before = components::hash(&root, "a", "rustc 1").unwrap().hash;
    assert_eq!(components::hash(&root, "a", "rustc 1").unwrap().hash, before);
}

// ---- the short named list of system tests (M16-P06) -----------------------------------------------------------------------------

use lico_xtask::system;

fn list_text(body: &str) -> String {
    format!("[[test]]\nname = \"a\"\npath = \"m::a\"\nwhy = \"needs the real host\"\nmib = 512\nseconds = 60\n{body}")
}

#[test]
fn the_list_of_system_tests_is_read_and_its_problems_are_named() {
    let entries = system::parse(&format!("{}\n[[test]]\nname = \"b\"\npath = \"m::b\"\nwhy = \"another\"\nmib = 256\nseconds = 30\nmeasured_mib = \"81.3\"\nmeasured_seconds = \"4.2\"\n", list_text(""))).unwrap();
    assert_eq!(entries.iter().map(|e| (e.name.as_str(), e.mib, e.seconds)).collect::<Vec<_>>(), [("a", 512, 60), ("b", 256, 30)]);
    assert_eq!(entries[1].measured_mib.as_deref(), Some("81.3"));
    assert!(system::problems(&entries).is_empty());
    for (text, word) in [
        (list_text("").replace("why = \"needs the real host\"\n", ""), "why"),
        (list_text("").replace("mib = 512", "mib = 600"), "512 MiB"),
        (list_text("").replace("seconds = 60", "seconds = 61"), "60 seconds"),
        (list_text("").replace("mib = 512", "mib = 0"), "budget"),
        (format!("{0}\n{0}", list_text("")), "twice"),
        (String::new(), "no system test"),
    ] {
        let found = system::parse(&text).map(|e| system::problems(&e).join("; ")).unwrap_or_else(|e| e);
        assert!(found.contains(word), "{word}: {found}");
    }
}

#[test]
fn a_system_test_is_run_alone_with_its_time_and_memory_against_its_own_budget() {
    let dir = scratch("system");
    let script = dir.join("fake_test_binary");
    fs::write(&script, "#!/bin/sh\ncase \"$*\" in *--exact*m::a*--ignored*|*--ignored*--exact*m::a*) ;; *) echo \"wrong arguments: $*\"; exit 2;; esac\nif [ \"$LICO_BIN\" != /the/binary ]; then echo no binary; exit 3; fi\nsleep \"$NAP\"\necho 'test result: ok. 1 passed; 0 failed; 1 ignored'\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let entry = |seconds: u64| system::parse(&list_text("").replace("seconds = 60", &format!("seconds = {seconds}"))).unwrap().remove(0);
    let env = |nap: &str| vec![("LICO_BIN".to_string(), "/the/binary".to_string()), ("NAP".to_string(), nap.to_string())];
    let ok = system::run_one(&script, &entry(60), &env("0.3"), &dir);
    assert!(ok.passed && ok.problem.is_none() && ok.wall_s >= 0.3 && ok.peak_mib >= 0.0, "{ok:?}");
    let slow = system::run_one(&script, &entry(1), &env("1.5"), &dir);
    assert!(slow.passed && slow.problem.as_deref().is_some_and(|p| p.contains("over its budget of 1 seconds")), "{slow:?}");
    let wrong = system::run_one(&script, &system::parse(&list_text("").replace("m::a", "m::other")).unwrap().remove(0), &env("0"), &dir);
    assert!(!wrong.passed && wrong.problem.as_deref().is_some_and(|p| p.contains("did not pass")), "{wrong:?}");
    let nothing = system::run_one(&dir.join("no_such_binary"), &entry(60), &env("0"), &dir);
    assert!(!nothing.passed && nothing.problem.is_some(), "{nothing:?}");
}

#[test]
fn recording_writes_what_was_measured_next_to_each_test_and_nothing_else_changes() {
    let mut entries = system::parse(&list_text("")).unwrap();
    let results = vec![system::Outcome { passed: true, wall_s: 4.234, peak_mib: 81.26, problem: None }];
    system::apply(&mut entries, &results);
    let text = system::render(&entries);
    let again = system::parse(&text).unwrap();
    assert_eq!((again[0].measured_mib.as_deref(), again[0].measured_seconds.as_deref()), (Some("81.3"), Some("4.2")));
    assert_eq!((again[0].name.as_str(), again[0].path.as_str(), again[0].why.as_str(), again[0].mib, again[0].seconds), ("a", "m::a", "needs the real host", 512, 60));
    // a failed run records nothing
    let before = again.clone();
    let mut kept = again;
    system::apply(&mut kept, &[system::Outcome { passed: false, wall_s: 9.0, peak_mib: 9.0, problem: Some("x".into()) }]);
    assert_eq!(kept[0].measured_mib, before[0].measured_mib);
}

#[test]
fn the_adapter_of_the_local_kind_and_the_port_stay_outside_the_cycle_of_the_hubs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/host/hostd/src/unified.rs");
    let g = graph::analyse(&root).expect("the program's source is read");
    let biggest = g.sccs.iter().map(Vec::len).max().unwrap();
    assert_eq!(biggest, 48, "the cycle of the hubs is the one measured in M15-P01: a new kind of session must not make it larger");
    let in_cycle = |m: &str| g.sccs.iter().any(|c| c.len() > 1 && c.iter().any(|x| x == m));
    assert!(!in_cycle("local") && !in_cycle("driver"), "the adapter and the port are not part of the cycle");
    let importers = |m: &str| g.prod.keys().filter(|(_, to)| to == m).map(|(from, _)| from.as_str()).collect::<Vec<_>>();
    assert_eq!(importers("local"), ["unified"], "nothing but the root names the adapter");
    assert!(g.prod.keys().all(|(from, to)| from != "driver" || to == "driver"), "the port depends on nothing of the program");
    for hub in ["core", "session", "folders", "transfer", "handoff", "codex"] {
        assert!(g.prod.keys().all(|(from, to)| !(to == "local" && from == hub)), "{hub} must not name the adapter");
    }
}

#[test]
fn the_browser_adapter_stays_outside_the_cycle_of_the_hubs_and_only_the_root_names_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/host/hostd/src/unified.rs");
    let g = graph::analyse(&root).expect("the program's source is read");
    assert_eq!(g.sccs.iter().map(Vec::len).max().unwrap(), 48, "M17-P06 must not make the cycle of the hubs larger (48 before and after)");
    let in_cycle = |m: &str| g.sccs.iter().any(|c| c.len() > 1 && c.iter().any(|x| x == m));
    assert!(!in_cycle("browser"), "the adapter is not part of the cycle");
    let importers = |m: &str| g.prod.keys().filter(|(_, to)| to == m).map(|(from, _)| from.as_str()).collect::<Vec<_>>();
    assert_eq!(importers("browser"), ["unified"], "nothing but the root names the adapter");
    for hub in ["core", "ipc", "http", "cli", "session", "folders", "transfer", "handoff", "codex", "admin"] {
        assert!(g.prod.keys().all(|(from, to)| !(to == "browser" && from == hub)), "{hub} must not name the adapter");
    }
}
