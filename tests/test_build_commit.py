"""The commit the program carries (src/host/hostd/build.rs): the environment variable `LICO_COMMIT` when it is set, else the commit of the checkout (with
`-dirty` after it when the package has changes), else `unknown`. The script is compiled alone and run in throw-away repositories, the way cargo runs it."""

import os
import re
import shutil
import subprocess
from pathlib import Path

import pytest

from lico_measure.rig import REPO, private_dir

BUILD_RS = REPO / "src" / "host" / "hostd" / "build.rs"
GIT_ENV = {"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.invalid",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.invalid"}


@pytest.fixture(scope="module")
def script():
    with private_dir() as directory:
        built = Path(directory) / "build_script"
        done = subprocess.run(["rustc", "--edition", "2021", "-o", str(built), str(BUILD_RS)], capture_output=True, text=True)
        assert done.returncode == 0, done.stderr
        yield built


def git(folder, *args, env=None):
    done = subprocess.run(["git", *args], cwd=folder, env={**os.environ, **GIT_ENV, **(env or {})}, capture_output=True, text=True)
    assert done.returncode == 0, done.stderr
    return done.stdout.strip()


def package(directory, name="repo"):
    """A repository with one commit; the package (the folder with Cargo.toml and src) is a folder below its top, as in the real tree."""
    top = Path(directory) / name
    (top / "pkg" / "src").mkdir(parents=True)
    (top / "pkg" / "src" / "main.rs").write_text("fn main() {}\n")
    (top / "pkg" / "Cargo.toml").write_text("[package]\n")
    (top / "other.txt").write_text("not part of the package\n")
    git(top, "init", "-q", "-b", "trunk")
    git(top, "add", "-A")
    git(top, "commit", "-q", "-m", "first")
    return top


def run(script, folder, commit=None, path=None, extra=None):
    """Run the script in `folder` as cargo does; the result is what it asked cargo to do."""
    env = {"PATH": path if path is not None else os.environ["PATH"], "CARGO_MANIFEST_DIR": str(folder), "OUT_DIR": str(folder), **GIT_ENV, **(extra or {})}
    if commit is not None:
        env["LICO_COMMIT"] = commit
    done = subprocess.run([str(script)], cwd=folder, env=env, capture_output=True, text=True)
    assert done.returncode == 0, done.stderr
    lines = done.stdout.splitlines()
    baked = [l.split("=", 2)[2] for l in lines if l.startswith("cargo:rustc-env=LICO_BUILT_COMMIT=")]
    assert len(baked) == 1, done.stdout
    return {"commit": baked[0], "rerun": [l.split("=", 1)[1] for l in lines if l.startswith("cargo:rerun-if-changed=")],
            "env": [l.split("=", 1)[1] for l in lines if l.startswith("cargo:rerun-if-env-changed=")], "warnings": [l for l in lines if l.startswith("cargo:warning=")]}


def test_the_commit_of_a_clean_checkout_is_the_seven_digit_commit(script):
    with private_dir() as directory:
        top = package(directory)
        want = git(top, "rev-parse", "--short=7", "HEAD")
        assert run(script, top / "pkg")["commit"] == want and re.fullmatch(r"[0-9a-f]{7}", want)


def test_a_changed_or_new_file_in_the_package_adds_dirty_and_a_change_outside_it_does_not(script):
    with private_dir() as directory:
        top = package(directory)
        pkg = top / "pkg"
        want = git(top, "rev-parse", "--short=7", "HEAD")
        (top / "other.txt").write_text("changed outside the package\n")
        (top / "stray.txt").write_text("new, outside\n")
        assert run(script, pkg)["commit"] == want                                  # tests and notes change all the time; the program does not
        (pkg / "src" / "main.rs").write_text("fn main() { }\n")
        assert run(script, pkg)["commit"] == f"{want}-dirty"
        git(top, "checkout", "--", "pkg/src/main.rs")
        assert run(script, pkg)["commit"] == want
        (pkg / "src" / "new.rs").write_text("\n")                                  # a file git does not know yet is a change too
        assert run(script, pkg)["commit"] == f"{want}-dirty"


def test_a_checkout_that_cannot_say_what_changed_is_not_called_clean(script):
    with private_dir() as directory:
        top = package(directory)
        want = git(top, "rev-parse", "--short=7", "HEAD")
        (top / ".git" / "index").write_bytes(b"not an index")                       # the commit is still readable, the state of the tree is not
        assert run(script, top / "pkg")["commit"] == f"{want}-dirty"


def test_the_environment_variable_wins_is_cut_to_seven_digits_and_a_bad_value_is_said_and_ignored(script):
    with private_dir() as directory:
        top = package(directory)
        pkg = top / "pkg"
        want = git(top, "rev-parse", "--short=7", "HEAD")
        assert run(script, pkg, commit="ABCDEF0123456789abcdef0123456789abcdef01")["commit"] == "abcdef0"
        assert run(script, pkg, commit="abcdef0-dirty")["commit"] == "abcdef0-dirty"
        assert run(script, pkg, commit="1234567")["commit"] == "1234567"
        for bad in ["not a commit", "123456", "v1.2.3", "abcdef0; rm -rf", "abcdef0-dirty-dirty", "g123456"]:
            found = run(script, pkg, commit=bad)
            assert found["commit"] == want and any("LICO_COMMIT" in w for w in found["warnings"]), (bad, found)
        assert run(script, pkg, commit="")["commit"] == want                         # an empty variable is the same as none


def test_without_a_checkout_or_without_git_the_commit_is_unknown(script):
    with private_dir() as directory:
        loose = Path(directory) / "loose" / "pkg"
        (loose / "src").mkdir(parents=True)
        assert run(script, loose, extra={"GIT_CEILING_DIRECTORIES": str(Path(directory))})["commit"] == "unknown"      # an exported tree
        top = package(directory)
        nowhere = Path(directory) / "empty"
        nowhere.mkdir()
        assert run(script, top / "pkg", path=str(nowhere))["commit"] == "unknown"     # a machine without git
        assert run(script, top / "pkg", commit="abcdef0", path=str(nowhere))["commit"] == "abcdef0"


def test_cargo_is_told_to_build_again_when_the_commit_or_the_variable_changes_in_a_worktree_too(script):
    with private_dir() as directory:
        top = package(directory)
        side = Path(directory) / "side"
        git(top, "worktree", "add", "-q", "-b", "feature", str(side))
        pkg = side / "pkg"
        found = run(script, pkg)
        assert "LICO_COMMIT" in found["env"]
        git_dir, common = Path(git(pkg, "rev-parse", "--absolute-git-dir")), (pkg / git(pkg, "rev-parse", "--git-common-dir")).resolve()
        wanted = {"the HEAD of this worktree": git_dir / "HEAD", "the branch it is on": common / "refs" / "heads" / "feature", "the packed refs": common / "packed-refs"}
        watched = {(pkg / p).resolve() for p in found["rerun"]}
        for what, path in wanted.items():
            assert path.resolve() in watched, f"{what} ({path}) is not watched: {found['rerun']}"
        assert any((pkg / p).resolve() == (pkg / "src").resolve() for p in found["rerun"])      # the dirty mark is looked at again when a source changes
        # the commit really is the worktree's, and moves with it
        before = run(script, pkg)["commit"]
        (pkg / "src" / "main.rs").write_text("fn main() { }\n")
        git(pkg, "commit", "-q", "-a", "-m", "second")
        after = run(script, pkg)["commit"]
        assert after == git(pkg, "rev-parse", "--short=7", "HEAD") and after != before
