import hashlib
import json
import os
import shutil
import signal
import subprocess
import time
from pathlib import Path

import pytest

from lico_measure import notices
from lico_measure.p02 import HOSTD_DIR
from lico_measure.rig import REPO, Xvfb, private_dir

EXE = HOSTD_DIR / "lico"
ALLOWED_LIBRARIES = {"linux-vdso.so.1", "libgcc_s.so.1", "libm.so.6", "libc.so.6", "/lib64/ld-linux-x86-64.so.2"}


def alone(directory):
    """The program copied into an otherwise empty directory, and the plain environment it is run with."""
    folder = Path(directory) / "only-this"
    folder.mkdir()
    exe = folder / "lico"
    shutil.copy2(EXE, exe)
    home = Path(directory) / "home"
    for name in ("home", "state"):
        (Path(directory) / name).mkdir(mode=0o700)
    runtime = REPO / "private" / "rt" / Path(directory).name[-6:]          # a short path: a socket's path may not be longer than about 100 bytes
    runtime.mkdir(mode=0o700, parents=True, exist_ok=True)
    env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": str(home), "XDG_RUNTIME_DIR": str(runtime), "XDG_STATE_HOME": str(Path(directory) / "state")}
    return exe, env


def run(exe, env, *args, timeout=30):
    return subprocess.run([str(exe), *args], env=env, capture_output=True, text=True, timeout=timeout)


def test_the_single_executable_runs_every_role_alone_in_an_empty_directory():
    with private_dir() as directory:
        exe, env = alone(directory)
        assert sorted(p.name for p in exe.parent.iterdir()) == ["lico"]
        # it needs the C library and nothing else on disk; the display libraries are loaded by name when the client starts
        libraries = {line.split()[0] for line in subprocess.run(["ldd", str(exe)], capture_output=True, text=True).stdout.splitlines()}
        assert libraries <= ALLOWED_LIBRARIES, libraries
        assert run(exe, env, "version").stdout.strip() == "lico 0.0.0"
        text = run(exe, env, "licenses").stdout
        assert "Permission is hereby granted, free of charge" in text and "THIRD-PARTY NOTICES" in text and len(text) > 100_000   # its own and every crate's, inside the file
        status = run(exe, env, "cred", "--data-dir", str(Path(directory) / "creds"), "status")
        assert status.returncode == 3 and json.loads(status.stdout)["keyring"] == "unavailable"   # a role of the same file, and it fails closed
        # the client finds no host, starts one itself with a state directory of its own, and shows the first view
        xvfb = Xvfb()
        client = None
        try:
            display_env = dict(env, DISPLAY=xvfb.display)
            client = subprocess.Popen([str(exe), "gui"], env=display_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            ready = json.loads(client.stdout.readline())
            assert ready["ev"] == "ready"
            client.stdin.write("ui\n")
            client.stdin.flush()
            view = {}
            while view.get("ev") != "ui":                      # the client also reports input-method events
                view = json.loads(client.stdout.readline())
            assert view["rows"] == [] and view["header"] == "select a session"
            socket = Path(env["XDG_RUNTIME_DIR"]) / "lico" / "host.sock"
            assert socket.exists() and oct(socket.parent.stat().st_mode & 0o777) == "0o700"
            state = Path(directory) / "state" / "lico"
            assert state.is_dir() and oct(state.stat().st_mode & 0o777) == "0o700"
            pid = json.loads(run(exe, env, "--socket", str(socket), "status").stdout)["pid"]        # the command line answers from the same file
            # accounts and sessions are managed from the same command line, which finds the host by itself
            made = json.loads(run(exe, env, "account", "create", "--provider", "codex", "--label", "Work").stdout)
            account = made["account"]
            listed = json.loads(run(exe, env, "account", "list").stdout)
            assert [(a["account"], a["label"]) for a in listed["accounts"]] == [(account, "Work")]
            assert json.loads(run(exe, env, "account", "affected", account).stdout)["sessions"] == []
            assert run(exe, env, "account", "remove", account, "detach").returncode == 0
            assert json.loads(run(exe, env, "account", "list").stdout)["accounts"] == []
            # closing the window does not end the host: it was started in a session of its own
            client.stdin.write("quit\n")
            client.stdin.flush()
            client.wait(timeout=10)
            time.sleep(0.5)
            assert Path(f"/proc/{pid}").exists() and run(exe, env, "--socket", str(socket), "status").returncode == 0
            os.kill(pid, signal.SIGTERM)                       # the host this test started, by its own pid
            time.sleep(0.5)
            shutil.rmtree(env["XDG_RUNTIME_DIR"], ignore_errors=True)
        finally:
            if client and client.poll() is None:
                client.kill()
            xvfb.stop()


@pytest.mark.acceptance                                          # two clean release builds: a property of the build recipe, run by `pytest -m acceptance tests`
def test_a_clean_clone_builds_the_same_executable_twice_without_private_or_host_content():
    with private_dir() as directory:
        digests = []
        for number in (1, 2):
            clone = Path(directory) / f"clone-{number}-in-a-differently-named-place"
            subprocess.run(["git", "clone", "-q", "--no-hardlinks", str(REPO), str(clone)], check=True, capture_output=True)
            out = Path(directory) / f"out-{number}"
            env = {k: v for k, v in os.environ.items() if k != "CARGO_TARGET_DIR"}                  # its own target directory: never the shared one, whose binary other tests use
            env.update(CARGO_NET_OFFLINE="true", CARGO_HOME=str(REPO / "private" / "cargo-home"))
            subprocess.run(["sh", str(clone / "src" / "host" / "hostd" / "build-release.sh"), str(out)], check=True, capture_output=True, env=env, timeout=1500)
            digests.append(hashlib.sha256((out / "lico").read_bytes()).hexdigest())
            assert (out / "lico.sha256").read_text().split()[0] == digests[-1]
            content = (out / "lico").read_bytes()
            for forbidden in (str(REPO).encode(), str(clone).encode(), str(Path.home()).encode(), str(REPO.parent).encode(), b"/private/"):
                assert forbidden not in content, forbidden       # nothing of where it was built (only LICENSE and NOTICES are compiled in besides the sources)
            shutil.rmtree(clone, ignore_errors=True)
            shutil.rmtree(out / "target", ignore_errors=True)
        assert digests[0] == digests[1], digests                # two builds, two places, one file


def test_every_bundled_dependency_has_its_license_notice():
    assert notices.problems() == []                              # each crate listed, each license one the project accepts, each attributable
    committed = (notices.HOSTD / "NOTICES.txt").read_text()
    assert committed == notices.build(), "NOTICES.txt is out of date: run `python -m lico_measure notices`"
    packages = notices.locked_packages()
    assert len(packages) > 50 and all(f"{n} {v} | " in committed for n, v in packages)
    text = subprocess.run([str(EXE), "licenses"], capture_output=True, text=True).stdout
    assert text.endswith(committed) and (REPO / "LICENSE").read_text() in text    # what the program prints is what is committed, after its own license
