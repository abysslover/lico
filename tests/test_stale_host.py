import shutil
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import HOSTD_DIR, LICO
from lico_measure.rig import private_dir

from tests.test_codex_adapter import wait_for


def fnv(path):
    h = 0xCBF29CE484222325
    for b in Path(path).read_bytes():
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def older_build(directory):
    """A copy of the program with one byte more: another build as far as the window can tell."""
    folder = Path(directory) / "older"
    folder.mkdir()
    shutil.copy2(LICO, folder / "lico")
    shutil.copy2(HOSTD_DIR / "lico-synth-agent", folder / "lico-synth-agent")
    with (folder / "lico").open("ab") as out:
        out.write(b"\0")
    return folder / "lico"


def status(w):
    import json
    done = subprocess.run([str(LICO), "--socket", w.socket, "status"], env=w.env, capture_output=True, text=True, timeout=10)
    return json.loads(done.stdout)


def alive(pid):
    return Path(f"/proc/{pid}").exists() and not Path(f"/proc/{pid}/stat").read_text().split(") ")[1].startswith("Z")


def test_an_idle_host_of_another_build_is_replaced_by_the_window():
    with private_dir() as directory:
        with ProductWindow(host_binary=older_build(directory)) as w:
            old = w.old_pid
            wait_for(lambda: status(w)["pid"] != old, timeout=15)
            assert not alive(old)                                             # asked over its socket, not killed by name
            assert status(w)["build"] == fnv(LICO)                            # the new one is this window's build
            wait_for(lambda: "Restarted the host (new build); resumed 0 sessions." in w.ui()["header"], timeout=10)


def test_a_host_of_another_build_with_a_session_is_replaced_too_and_the_stop_command_still_refuses_without_force():
    with private_dir() as directory:
        with ProductWindow(host_binary=older_build(directory), host_sessions=1) as w:
            old = w.old_pid
            wait_for(lambda: status(w)["pid"] != old, timeout=15)             # sessions do not keep an old host alive any more (S04)
            assert not alive(old) and status(w)["build"] == fnv(LICO)
            wait_for(lambda: "Restarted the host (new build)" in w.ui()["header"], timeout=10)
    with private_dir() as directory:
        with ProductWindow(host_binary=LICO, host_sessions=1) as w:           # the same build: the window leaves it alone; the stop command is the explicit way
            old = w.old_pid
            refused = subprocess.run([str(LICO), "--socket", w.socket, "host", "stop"], env=w.env, capture_output=True, text=True, timeout=10)
            assert refused.returncode == 2 and "busy" in refused.stdout and alive(old)       # sessions exist: nothing is ended
            forced = subprocess.run([str(LICO), "--socket", w.socket, "host", "stop", "--force"], env=w.env, capture_output=True, text=True, timeout=10)
            assert forced.returncode == 0
            wait_for(lambda: not alive(old), timeout=10)
