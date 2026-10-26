"""The selection of tests by changed file (src/lico_measure/testmap.py), the receipts and the budget table - on small trees written
for the purpose, and on the real map. No process of the product runs here."""
import argparse
import re
import subprocess
import sys
import time
import tomllib
from pathlib import Path

import pytest

from lico_measure import receipts, suite, testmap

HOSTD = "src/host/hostd/src/"
TMAP = {
    "areas": {"tests/test_window.py": ["gui", "host"], "tests/test_host.py": ["host"], "tests/test_cred.py": ["cred"], "tests/test_logic.py": [],
              "tests/test_user.py": []},
    "rust": {"modules": {"core": ["host"], "gui": ["gui"], "keyring": ["cred"], "unified": ["*"]}, "everything": ["src/host/hostd/Cargo.lock"]},
    "acceptance": {"triggers": ["src/host/hostd/Cargo.lock", "tests/test_window.py"]},
    "sources": [{"glob": "src/adapters/**", "tests": ["tests/test_host.py"], "reason": "started by path"},
                {"glob": "tests/tables/names.json", "tests": ["tests/test_host.py"], "cargo": ["core::"], "reason": "read by a test and by include_str!"}],
    "inert": [{"glob": "**/*.md", "reason": "documents"}],
}
TREE = {
    "tests/test_window.py": "from lico_measure import tool\n",
    "tests/test_host.py": "from tests.test_logic import helper\nFIXTURE = 'turn.json'\n",
    "tests/test_cred.py": "x = 1\n",
    "tests/test_logic.py": "from lico_measure import pure\ndef helper(): pass\n",
    "tests/test_user.py": "from tests.test_host import FIXTURE\n",
    "tests/fixtures/turn.json": "{}\n",
    "tests/fixtures/orphan.json": "{}\n",
    "src/lico_measure/__init__.py": "",
    "src/lico_measure/tool.py": "from . import base\n",
    "src/lico_measure/base.py": "",
    "src/lico_measure/pure.py": "",
    "src/lico_measure/unused.py": "",
    "src/adapters/sim.py": "",
    "tests/tables/names.json": "[]\n",
    HOSTD + "unified.rs": "mod core;\nmod gui;\nmod keyring;\n",
    HOSTD + "core.rs": "use crate::keyring_free;\n",
    HOSTD + "gui/mod.rs": "mod ui;\nuse crate::core::Thing;\n",
    HOSTD + "gui/ui.rs": "use super::*;\n",
    HOSTD + "keyring.rs": "fn k() {}\n",
    "src/host/hostd/Cargo.lock": "",
    "README.md": "x\n",
    "notes.txt": "x\n",
}
EVERY = sorted(f for f in TREE if f.startswith("tests/test_"))


@pytest.fixture()
def tree(tmp_path):
    for name, text in TREE.items():
        (tmp_path / name).parent.mkdir(parents=True, exist_ok=True)
        (tmp_path / name).write_text(text)
    return tmp_path


def chosen(tree, *paths):
    result = testmap.select(list(paths), tmap=TMAP, files=sorted(TREE), root=tree)
    return result, result.tests(EVERY)


def test_an_unmapped_file_selects_everything_and_so_does_a_python_file_no_test_imports(tree):
    for path in ("notes.txt", "src/lico_measure/unused.py", "tests/fixtures/orphan.json"):
        result, tests = chosen(tree, path)
        assert result.everything and tests == EVERY and "" in result.cargo, path
        print(f"{path} -> everything ({len(tests)} of {len(EVERY)} test files, all unit tests)")


def test_a_changed_test_file_selects_itself_and_the_tests_that_import_it(tree):
    assert chosen(tree, "tests/test_cred.py")[1] == ["tests/test_cred.py"]
    assert chosen(tree, "tests/test_host.py")[1] == ["tests/test_host.py", "tests/test_user.py"]
    print("tests/test_host.py -> tests/test_host.py, tests/test_user.py (its importer)")


def test_a_python_source_selects_the_tests_that_import_it_directly_or_through_other_modules(tree):
    assert chosen(tree, "src/lico_measure/tool.py")[1] == ["tests/test_window.py"]
    assert chosen(tree, "src/lico_measure/base.py")[1] == ["tests/test_window.py"]                  # tool imports base
    assert chosen(tree, "src/lico_measure/pure.py")[1] == ["tests/test_host.py", "tests/test_logic.py", "tests/test_user.py"]     # through test_logic


def test_a_rust_module_selects_its_unit_tests_those_of_its_users_and_the_tests_that_start_what_runs_it(tree):
    result, tests = chosen(tree, HOSTD + "gui/ui.rs")
    assert (result.cargo, tests) == ({"gui::"}, ["tests/test_window.py"])                           # nothing uses gui
    result, tests = chosen(tree, HOSTD + "keyring.rs")
    assert (result.cargo, tests) == ({"keyring::"}, ["tests/test_cred.py"])
    result, tests = chosen(tree, HOSTD + "core.rs")
    assert result.cargo == {"core::", "gui::"} and tests == ["tests/test_host.py", "tests/test_window.py"]     # gui uses core
    assert "tests/test_logic.py" not in tests and not result.everything
    print("gui/ui.rs -> cargo gui::; tests/test_window.py | core.rs -> cargo core:: gui::; tests/test_host.py, tests/test_window.py")


def test_a_rust_module_the_map_does_not_know_selects_everything(tree):
    (tree / (HOSTD + "newmod.rs")).write_text("")
    assert chosen(tree, HOSTD + "newmod.rs")[0].everything


def test_a_build_input_selects_every_test_that_starts_the_product_and_makes_the_acceptance_run_due(tree):
    result, tests = chosen(tree, "src/host/hostd/Cargo.lock")
    assert tests == ["tests/test_cred.py", "tests/test_host.py", "tests/test_window.py"] and "" in result.cargo
    assert result.acceptance == ["src/host/hostd/Cargo.lock"] and not result.everything
    assert chosen(tree, "tests/test_window.py")[0].acceptance == ["tests/test_window.py"]


def test_a_file_read_by_path_selects_the_tests_the_map_names_and_a_fixture_the_tests_that_name_it(tree):
    assert chosen(tree, "src/adapters/sim.py")[1] == ["tests/test_host.py", "tests/test_user.py"]
    assert chosen(tree, "tests/fixtures/turn.json")[1] == ["tests/test_host.py", "tests/test_user.py"]


def test_a_table_read_by_a_rust_unit_test_selects_that_modules_tests_too(tree):
    result, tests = chosen(tree, "tests/tables/names.json")
    assert tests == ["tests/test_host.py", "tests/test_user.py"] and result.cargo == {"core::"}      # a change to the table reaches both readers
    assert not chosen(tree, "src/adapters/sim.py")[0].cargo


def test_an_inert_file_selects_nothing(tree):
    result, tests = chosen(tree, "README.md")
    assert tests == [] and not result.cargo and not result.everything


def test_a_new_test_file_the_map_does_not_list_runs_for_every_rust_change(tree):
    (tree / "tests/test_new.py").write_text("")
    files = sorted(TREE) + ["tests/test_new.py"]
    result = testmap.select([HOSTD + "keyring.rs"], tmap=TMAP, files=files, root=tree)
    assert result.tests(EVERY + ["tests/test_new.py"]) == ["tests/test_cred.py", "tests/test_new.py"]


def git(directory, *args):
    return subprocess.run(["git", "-C", str(directory), "-c", "user.name=t", "-c", "user.email=t@t", *args], capture_output=True, text=True, check=True).stdout


def test_changed_reads_staged_unstaged_new_and_committed_files_since_the_base(tmp_path):
    git(tmp_path, "init", "-q")
    for name in ("a.txt", "b.txt", "c.txt", "d.txt", "e.txt"):
        (tmp_path / name).write_text("1\n")
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-q", "-m", "base")
    base = git(tmp_path, "rev-parse", "HEAD").strip()
    (tmp_path / "a.txt").write_text("2\n")
    git(tmp_path, "commit", "-q", "-am", "committed after the base")
    (tmp_path / "b.txt").write_text("2\n")
    git(tmp_path, "add", "b.txt")                                                                  # staged
    (tmp_path / "c.txt").write_text("2\n")                                                         # unstaged
    (tmp_path / "new.txt").write_text("2\n")                                                       # new
    git(tmp_path, "mv", "d.txt", "moved.txt")                                                      # both sides of a rename
    assert testmap.changed(None, root=tmp_path) == ["b.txt", "c.txt", "d.txt", "moved.txt", "new.txt"]
    assert testmap.changed(base, root=tmp_path) == ["a.txt", "b.txt", "c.txt", "d.txt", "moved.txt", "new.txt"]
    print("changed(HEAD) =", testmap.changed(None, root=tmp_path), "| changed(BASE) adds the committed a.txt")


# --- the real map --------------------------------------------------------------------------------------------

REAL = testmap.load_map()
REAL_FILES = testmap.tracked()
REAL_TESTS = sorted(f for f in REAL_FILES if f.startswith("tests/test_") and f.count("/") == 1 and f.endswith(".py"))


def real(*paths):
    result = testmap.select(list(paths), tmap=REAL, files=REAL_FILES)
    return result, result.tests(REAL_TESTS)


def test_every_test_file_is_listed_with_what_it_starts_and_every_rust_module_with_where_it_runs():
    assert [f for f in REAL_TESTS if f not in REAL["areas"]] == []
    assert [f for f in REAL["areas"] if f not in REAL_TESTS] == [], "a listed test file that is gone"
    modules = {testmap.rust_module(f) for f in REAL_FILES if testmap.rust_module(f)}
    assert sorted(modules - set(REAL["rust"]["modules"])) == [] and sorted(set(REAL["rust"]["modules"]) - modules) == []
    kinds = {"gui", "host", "cred", "cli", "synth", "*"}
    assert all(set(a) <= kinds for a in REAL["areas"].values()) and all(set(a) <= kinds for a in REAL["rust"]["modules"].values())
    for entry in REAL["sources"]:
        assert all(t in REAL_TESTS for t in entry["tests"]) and entry["reason"]
        assert all(re.fullmatch(r"[a-z_]+::", c) and c[:-2] in REAL["rust"]["modules"] for c in entry.get("cargo", []))
    assert all(entry["reason"] for entry in REAL["inert"])


def test_every_test_file_holds_a_test_or_is_named_acceptance_only():
    # a helper module named test_*.py collects nothing (pytest rc 5), which the runner now counts as a failure: the cold run found
    # tests/test_account_helpers.py this way; a helper takes a name that is not test_*.py
    empty = [f for f in REAL_TESTS if f not in REAL["acceptance"]["only"] and not re.search(r"^\s*(?:async\s+)?def test_", (testmap.REPO / f).read_text(), re.M)]
    assert empty == [], f"collects no test: {empty}"


def test_in_the_real_tree_a_window_change_leaves_the_host_and_credential_tests_alone_and_a_core_change_does_not():
    result, tests = real(HOSTD + "gui/ui.rs")
    assert "tests/test_menu_bar.py" in tests and "tests/test_credential_store.py" not in tests and "tests/test_recovery.py" not in tests
    assert result.cargo == {"gui::"} and not result.everything
    result, tests = real(HOSTD + "core.rs")
    assert {"tests/test_menu_bar.py", "tests/test_recovery.py"} <= set(tests) and "tests/test_credential_store.py" not in tests
    assert {"core::", "gui::", "session::"} <= result.cargo
    result, tests = real("tests/test_menu_bar.py")
    assert tests == ["tests/test_menu_bar.py", "tests/test_size_ratchet.py"] and not result.cargo       # itself, and the guard that reads the whole tree
    result, tests = real("src/lico_measure/captures.py")
    assert "tests/test_menu_bar.py" in tests and "tests/test_recovery.py" not in tests
    assert real("a/file/nobody/declared.bin")[1] == REAL_TESTS
    print(f"real map: gui/ui.rs -> {len(real(HOSTD + 'gui/ui.rs')[1])} of {len(REAL_TESTS)} test files; core.rs -> {len(real(HOSTD + 'core.rs')[1])}; a test file -> 2 (itself and the size guard)")


def test_the_rust_scan_reads_grouped_imports_nested_groups_aliases_and_ignores_comments(tmp_path):
    source = """
use crate::{a, b::{c, d}, e as f, g::h::{self, i}};
use crate::{
    j,
    // k is only talked about
    l as m,
};
use crate::n::o;
pub(in crate::p) fn q() {}
/* crate::r */
"""
    names, unreadable = testmap.crate_names(source)
    assert names == {"a", "b", "e", "g", "j", "l", "n", "p"} and unreadable == []
    sources = {"core.rs": "use crate::{session, gui::Thing};\n", "session.rs": "", "gui/mod.rs": "", "host.rs": "use crate::{accounts, core};\n", "accounts.rs": ""}
    for path, text in sources.items():
        (tmp_path / HOSTD / path).parent.mkdir(parents=True, exist_ok=True)
        (tmp_path / HOSTD / path).write_text(text)
    uses = testmap.rust_uses([HOSTD + n for n in sources], tmp_path)
    assert uses["core"] == {"session", "gui"} and uses["host"] == {"accounts", "core"}


def test_in_the_real_tree_host_and_credcmd_use_the_modules_their_grouped_imports_name():
    uses = testmap.rust_uses(REAL_FILES)
    assert {"accounts", "core", "http", "ipc", "registry", "session"} <= uses["host"]           # use crate::{accounts, core, http, ipc, registry, session};
    assert uses["credcmd"] == {"creds", "keyring"}                                                # use crate::{creds, keyring};
    assert "host" in testmap.rust_dependents(uses, "http") and "credcmd" in testmap.rust_dependents(uses, "keyring")
    print("host uses", sorted(uses["host"]), "| credcmd uses", sorted(uses["credcmd"]))


def test_a_crate_path_the_scan_cannot_read_makes_its_module_use_everything_and_says_so(tmp_path):
    for name, text in TREE.items():
        (tmp_path / name).parent.mkdir(parents=True, exist_ok=True)
        (tmp_path / name).write_text(text)
    (tmp_path / HOSTD / "gui/mod.rs").write_text("mod ui;\nuse crate::*;\n")                    # a glob of the crate root: no module is named
    unreadable = []
    uses = testmap.rust_uses(sorted(TREE), tmp_path, unreadable)
    assert uses["gui"] == {"core", "keyring", "unified"} and [m for m, _ in unreadable] == ["gui"]
    result = testmap.select([HOSTD + "keyring.rs"], tmap=TMAP, files=sorted(TREE), root=tmp_path)
    assert "gui::" in result.cargo and any("gui" in w and "crate::" in w for w in result.warnings)
    assert any(line.startswith("# warning") for line in testmap.render(result, EVERY).splitlines())
    print(result.warnings[0])


LEGACY_FILES = ["tests/test_korean_ime.py", "tests/test_view_lifecycle.py", "tests/test_client_integration.py", "tests/test_unicode_clients.py",
                "tests/test_protocol_conformance.py"]


def collected(*options):
    done = subprocess.run([sys.executable, "-m", "pytest", "--collect-only", "-q", "-p", "no:cacheprovider", *options, *LEGACY_FILES],
                          cwd=testmap.REPO, capture_output=True, text=True)
    return [line for line in done.stdout.splitlines() if "::" in line]


def test_the_retired_prototype_clients_and_hosts_run_only_behind_the_legacy_marker():
    daily = collected()
    legacy = collected("-o", "addopts=", "-m", "legacy_candidates")
    ids = lambda rows: sorted(row.split("[")[1].rstrip("]") for row in rows if "[" in row)
    assert daily and legacy
    retired = {"a", "b", "c", "c2", "d", "rust_std", "rust_serde"}                                      # clients a b c c2 d; hosts rust_std rust_serde c
    assert set(ids(legacy)) <= retired
    assert not set(ids(daily)) & retired, ids(daily)                                                   # the daily run has none of them
    assert {"a2"} <= set(ids(daily)) and {"oracle", "hostd"} <= set(ids(daily))                        # the product under its old name, the reference host and the product host stay
    assert {"a", "b", "c", "c2", "d", "rust_std", "rust_serde"} == set(ids(legacy))                    # nothing is deleted: the marker selects them
    assert not any("[a2]" in row or "[oracle]" in row or "[hostd]" in row for row in legacy)
    print(f"daily rows of the five files: {len(daily)}; behind the marker: {len(legacy)} ({', '.join(sorted(set(ids(legacy))))})")


def test_the_cargo_job_hands_several_module_filters_to_the_test_binary_not_to_cargo(tmp_path):
    # `cargo test --bin lico a:: b::` is refused ("unexpected argument"); libtest takes any number of filters after `--` (a core.rs change found this)
    argv = suite.cargo_job(["core::", "gui::", "session::"], tmp_path).argv
    assert argv[:4] == ["cargo", "test", "--bin", "lico"] and argv[4:] == ["--", "core::", "gui::", "session::"]
    assert suite.cargo_job([], tmp_path).argv == ["cargo", "test", "--bin", "lico"]               # no filter: every unit test
    print("cargo argv for a core.rs change:", " ".join(argv))


# --- receipts --------------------------------------------------------------------------------------------------

def receipt_key(tree, name, binaries):
    files = sorted(TREE)
    graph = testmap.python_graph(files, tree)
    return receipts.key(name, graph, TMAP, files, {}, binaries, root=tree)


def test_a_receipt_names_what_the_result_depends_on_and_nothing_else(tree):
    old = receipt_key(tree, "tests/test_host.py", {"lico": "aaa"})
    assert receipt_key(tree, "tests/test_host.py", {"lico": "aaa"}) == old
    assert receipt_key(tree, "tests/test_host.py", {"lico": "bbb"}) != old                           # the binary
    (tree / "tests/fixtures/turn.json").write_text("{ }\n")
    assert receipt_key(tree, "tests/test_host.py", {"lico": "aaa"}) != old                           # a fixture it names
    old = receipt_key(tree, "tests/test_host.py", {"lico": "aaa"})
    (tree / "tests/test_logic.py").write_text(TREE["tests/test_logic.py"] + "# edited\n")
    assert receipt_key(tree, "tests/test_host.py", {"lico": "aaa"}) != old                           # a test file it imports
    old = receipt_key(tree, "tests/test_logic.py", {"lico": "aaa"})
    (tree / "src/lico_measure/pure.py").write_text("x = 1\n")
    assert receipt_key(tree, "tests/test_logic.py", {"lico": "aaa"}) != old                          # a source it imports
    old = receipt_key(tree, "tests/test_logic.py", {"lico": "aaa"})
    assert receipt_key(tree, "tests/test_logic.py", {"lico": "bbb"}) == old                          # pure Python never starts the binary
    (tree / "src/adapters/sim.py").write_text("x = 1\n")
    assert receipt_key(tree, "tests/test_logic.py", {}) == old                                       # a file it does not read
    host = receipt_key(tree, "tests/test_host.py", {})
    (tree / "src/adapters/sim.py").write_text("x = 2\n")
    assert receipt_key(tree, "tests/test_host.py", {}) != host                                       # one it reads by path


def test_a_receipt_skips_a_green_unchanged_file_and_never_one_that_failed(tmp_path, monkeypatch):
    monkeypatch.setattr(receipts, "DIRECTORY", tmp_path / "receipts")
    assert not receipts.valid("tests/a.py", "k1")                                                   # nothing recorded
    receipts.record("tests/a.py", "k1", 3, 1.5)
    assert receipts.valid("tests/a.py", "k1") and not receipts.valid("tests/a.py", "k2")            # another key: run
    receipts.forget("tests/a.py")                                                                  # it failed
    assert not receipts.valid("tests/a.py", "k1") and receipts.failed() == {"tests/a.py"}
    receipts.record("tests/a.py", "k1", 3, 1.5)                                                    # it passed again
    assert receipts.valid("tests/a.py", "k1") and receipts.failed() == set()
    receipts.mark_failed("tests/a.py", True)                                                       # a failure on record beats a matching key
    assert not receipts.valid("tests/a.py", "k1")


def test_a_selection_of_everything_ignores_receipts_because_a_file_nobody_declared_is_in_no_key():
    asked = argparse.Namespace(receipts=True, cold=False)
    use, why = suite.use_receipts(asked, testmap.Selection(everything=True))
    assert use is False and "default-deny" in why and "--cold" in why                              # run everything, and say why
    assert suite.use_receipts(asked, testmap.Selection(files={"tests/a.py"})) == (True, None)      # a narrow selection may skip a green file
    assert suite.use_receipts(asked, None) == (True, None)                                         # a plain run with --receipts
    assert suite.use_receipts(argparse.Namespace(receipts=False, cold=False), testmap.Selection(everything=True)) == (False, None)    # not asked: nothing to say
    assert suite.use_receipts(argparse.Namespace(receipts=True, cold=True), testmap.Selection()) == (False, None)
    print("--changed --receipts with an undeclared file:", why)


def test_pytest_collecting_nothing_passes_only_for_a_file_the_map_names_as_acceptance_only(tmp_path):
    job = suite.pytest_job("tests/test_x.py", tmp_path)
    job.rc = 5                                                                                      # pytest: no test was collected
    assert not suite.passed(job, set()) and not suite.passed(job, {"tests/test_other.py"})          # a mistyped marker must not print ok
    assert suite.passed(job, {"tests/test_x.py"})
    job.rc = 0
    assert suite.passed(job, set())
    job.rc = 1
    assert not suite.passed(job, {"tests/test_x.py"})
    assert not suite.passed(suite.cargo_job([], tmp_path), set())                                   # not a pytest job: 0 only
    only = REAL["acceptance"]["only"]
    assert all(f in REAL_TESTS for f in only)
    for f in only:                                                                                  # the daily run really collects nothing from them
        done = subprocess.run([sys.executable, "-m", "pytest", "--collect-only", "-q", "-p", "no:cacheprovider", f], cwd=testmap.REPO, capture_output=True, text=True)
        assert done.returncode == 5, (f, done.stdout[-300:])
    print("files the map names as acceptance-only (rc 5 is a pass for these and a failure for any other):", ", ".join(only) or "none")


def test_a_run_measures_with_one_tree_and_one_sampler_started_before_the_build(tmp_path, monkeypatch):
    # run() once built the Tree, the stop flag and the sampler twice: the first sampler thread went on, through the rebound names, sampling the second Tree too
    trees, at_build = [], []

    class Spy(suite.Tree):
        def __init__(self, root):
            super().__init__(root)
            self.samples = 0
            trees.append(self)

        def sample(self):
            self.samples += 1
            return super().sample()

    def fake_build(files):
        at_build.append(len(trees))
        return "build: skipped (test)", 0.0

    monkeypatch.setattr(suite, "Tree", Spy)
    monkeypatch.setattr(suite, "INTERVAL", 0.05)
    monkeypatch.setattr(suite, "ensure_build", fake_build)
    monkeypatch.setattr(suite, "LOGS", tmp_path / "logs")
    monkeypatch.setattr(receipts, "DIRECTORY", tmp_path / "receipts")
    small = tmp_path / "test_small.py"
    small.write_text("import time\n\ndef test_small():\n    time.sleep(0.4)\n")
    asked = argparse.Namespace(changed=None, paths=[str(small)], receipts=False, cold=False, workers=1, ledger=None)
    assert suite.run(asked) == 0
    assert len(trees) == 1 and at_build == [1], (len(trees), at_build)                              # one tree, there before the build
    settled = trees[0].samples
    assert settled >= 2
    time.sleep(0.3)
    assert trees[0].samples == settled, "a sampler thread is still running after the run"
    print(f"one Tree, started before the build, {settled} samples, stopped with the run")


def test_a_run_a_guard_refuses_leaves_no_receipt_so_the_next_run_runs_the_file_again_and_is_refused_again(tmp_path, monkeypatch, capsys):
    # the receipt used to be written before the guards were judged: an exit 3 (budget, cold-run limit, memory bound) was followed by a run that skipped the file and passed
    monkeypatch.setattr(suite, "ensure_build", lambda files: ("build: skipped (test)", 0.0))
    monkeypatch.setattr(suite, "LOGS", tmp_path / "logs")
    monkeypatch.setattr(receipts, "DIRECTORY", tmp_path / "receipts")
    small = tmp_path / "test_small.py"
    small.write_text("def test_small():\n    assert True\n")
    asked = argparse.Namespace(changed=None, paths=[str(small)], receipts=True, cold=False, workers=1, ledger=None)
    table = suite.budget()
    monkeypatch.setattr(suite, "budget", lambda root=None: dict(table, peak_rss_mib=1))                   # every run is over a 1 MiB bound
    skipping = lambda: [l for l in capsys.readouterr().out.splitlines() if l.startswith("skipped (receipt) ")]
    assert suite.run(asked) == 3 and suite.run(asked) == 3
    assert skipping() == [], "the second run skipped the file the first one left a receipt for"
    monkeypatch.setattr(suite, "budget", lambda root=None: table)
    assert suite.run(asked) == 0 and suite.run(asked) == 0
    assert len(skipping()) == 1, "only the green run left a receipt"


# --- budget ----------------------------------------------------------------------------------------------------

def test_the_budget_names_the_tests_over_their_limit_by_the_longest_matching_entry():
    table = {"default_s": 15, "allow": [{"test": "tests/test_x.py", "seconds": 30, "reason": "r"}, {"test": "tests/test_x.py::test_big", "seconds": 60, "reason": "r"}]}
    rows = [{"nodeid": "tests/test_a.py::test_a", "wall_s": 15.0}, {"nodeid": "tests/test_a.py::test_b[c2]", "wall_s": 15.1},
            {"nodeid": "tests/test_x.py::test_small", "wall_s": 29.0}, {"nodeid": "tests/test_x.py::test_small2", "wall_s": 31.0},
            {"nodeid": "tests/test_x.py::test_big[a2]", "wall_s": 59.0}, {"nodeid": "tests/test_x.py::test_big[b]", "wall_s": 61.0}]
    found = suite.over_budget(rows, table)
    for seconds, nodeid, limit in found:
        print(f"over budget: {seconds:.1f} s > {limit} s  {nodeid}")                                # the output the report quotes
    assert [(n, limit) for _, n, limit in found] == [("tests/test_x.py::test_big[b]", 60), ("tests/test_x.py::test_small2", 30), ("tests/test_a.py::test_b[c2]", 15)]


def test_the_checked_in_budget_has_a_default_a_cold_run_limit_and_a_reason_for_every_exception():
    table = suite.budget()
    assert table["default_s"] == 15 and table["cold_run_s"] == 180 and table["peak_rss_mib"] <= 2048
    for entry in table.get("allow", []):
        assert entry["seconds"] > table["default_s"] and entry["reason"].strip() and (testmap.REPO / entry["test"].split("::")[0]).exists(), entry
