"""The rigs close what they started: when their own start fails, and when pytest is told to stop (TERM). A failing run of these
tests removes what it looked for, by exact pid."""
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import pytest

from lico_measure import ime
from lico_measure.captures import LICO, ProductWindow
from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.rig import REPO


def running(pid):
    """Alive, not a zombie that nobody has collected yet."""
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return False
    return stat[stat.rindex(")") + 2] != "Z"


def kill_by_pid(pids):
    for pid in pids:
        if running(pid):
            os.kill(pid, signal.SIGKILL)


def test_a_window_that_fails_while_it_starts_stops_the_host_the_display_and_the_runtime_folder():
    seen = {}

    class Failing(ProductWindow):
        def settle(self, since, upto):       # the first statement once the host and the window are up
            done = subprocess.run([str(LICO), "--socket", self.socket, "status"], env=self.env, capture_output=True, text=True, timeout=5)
            seen["host"] = json.loads(done.stdout)["pid"]
            raise RuntimeError("failed while starting")

    window = Failing()
    try:
        with pytest.raises(RuntimeError, match="failed while starting"):
            with window:
                pass
        assert not running(seen["host"])
        assert not running(window.proc.pid) and window.xvfb.proc.poll() is not None
        assert not window.runtime.exists()
    finally:
        kill_by_pid([seen.get("host")] if seen else [])
        window.__exit__()


def test_a_keyring_that_fails_while_it_starts_stops_its_bus_and_daemon_and_removes_its_folder():
    seen = {}

    class Failing(PrivateKeyring):
        def _wait_for_service(self, expect_default):      # the bus and the daemon are up
            seen["pids"], seen["root"] = self.pids(), self.root
            raise RuntimeError("failed while starting")

    try:
        with pytest.raises(RuntimeError, match="failed while starting"):
            with Failing():
                pass
        assert len(seen["pids"]) == 2
        assert not [pid for pid in seen["pids"] if running(pid)]
        assert not seen["root"].exists()
    finally:
        kill_by_pid(seen.get("pids", []))
        if seen:
            subprocess.run(["rm", "-rf", str(seen["root"])])


def test_an_input_method_that_fails_while_it_starts_stops_its_bus_and_removes_its_folders(monkeypatch):
    started = []
    real = subprocess.Popen

    def popen(argv, *args, **kwargs):
        if argv[0] == "ibus-daemon":
            raise FileNotFoundError("ibus-daemon")         # not installed: the private bus is already up
        started.append(real(argv, *args, **kwargs))
        return started[-1]

    monkeypatch.setattr(subprocess, "Popen", popen)
    config = cache = None
    try:
        with pytest.raises(FileNotFoundError):
            with ime.CompositionRig():
                pass
        bus = next(p for p in started if p.args[0] == "dbus-daemon")
        config = Path(bus.args[1].split("=", 1)[1])
        cache = REPO / "private" / "k" / config.name.split("-")[-1].split(".")[0]      # named by the same two characters
        assert not [p for p in started if p.poll() is None]
        assert not config.exists() and not cache.exists()
    finally:
        kill_by_pid([p.pid for p in started])
        if config:
            config.unlink(missing_ok=True)
            subprocess.run(["rm", "-rf", str(cache)])


IN_A_RIG = '''
import os, time
from pathlib import Path

class Rig:
    def __enter__(self):
        Path(os.environ["MARK"], "entered").write_text("1")
        return self
    def __exit__(self, *exc):
        Path(os.environ["MARK"], "exited").write_text("1")

def test_waits():
    with Rig():
        time.sleep(60)
'''


def test_term_ends_pytest_the_way_ctrl_c_does_so_a_rig_is_closed(tmp_path):
    (tmp_path / "test_in_a_rig.py").write_text(IN_A_RIG)
    env = dict(os.environ, PYTHONPATH=str(REPO / "src"), MARK=str(tmp_path))
    proc = subprocess.Popen([sys.executable, "-m", "pytest", "-p", "lico_measure.ledger", "-p", "no:cacheprovider", "-q", "test_in_a_rig.py"],
                            cwd=tmp_path, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        end = time.monotonic() + 15
        while not (tmp_path / "entered").exists() and time.monotonic() < end:
            time.sleep(0.05)
        assert (tmp_path / "entered").exists()
        proc.send_signal(signal.SIGTERM)
        assert proc.wait(timeout=10) == 2            # pytest's "interrupted", as for Ctrl+C
        assert (tmp_path / "exited").exists()
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


IN_A_WINDOW = '''
import json, subprocess, time
from pathlib import Path
from lico_measure.captures import LICO, ProductWindow

def test_waits():
    with ProductWindow() as w:
        done = subprocess.run([str(LICO), "--socket", w.socket, "status"], env=w.env, capture_output=True, text=True, timeout=5)
        Path("up.json").write_text(json.dumps({"host": json.loads(done.stdout)["pid"], "window": w.proc.pid, "xvfb": w.xvfb.proc.pid, "runtime": str(w.runtime), "directory": w.directory}))
        time.sleep(60)
'''


def test_term_while_a_product_window_is_up_leaves_no_host_no_display_and_no_runtime_folder(tmp_path):
    (tmp_path / "test_in_a_window.py").write_text(IN_A_WINDOW)
    env = dict(os.environ, PYTHONPATH=str(REPO / "src"))
    proc = subprocess.Popen([sys.executable, "-m", "pytest", "-p", "lico_measure.ledger", "-p", "no:cacheprovider", "-q", "test_in_a_window.py"],
                            cwd=tmp_path, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    up = tmp_path / "up.json"
    seen = {}
    try:
        end = time.monotonic() + 20
        while not up.exists() and time.monotonic() < end:
            time.sleep(0.05)
        seen = json.loads(up.read_text())
        proc.send_signal(signal.SIGTERM)
        assert proc.wait(timeout=10) == 2
        left = [name for name in ("host", "window", "xvfb") if running(seen[name])]
        assert not left and not Path(seen["runtime"]).exists() and not Path(seen["directory"]).exists()
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        kill_by_pid([seen[name] for name in ("host", "window", "xvfb") if name in seen])
        if seen:
            subprocess.run(["rm", "-rf", seen["runtime"], seen["directory"]])
