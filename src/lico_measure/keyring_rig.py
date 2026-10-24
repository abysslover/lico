"""A throwaway Secret Service for tests: a private session bus and a private gnome-keyring daemon.

Nothing here touches a desktop session: the bus has no service directories and an abstract
address of its own, the daemon runs with its own home, data and runtime directories, and its
login keyring is created with a canary password. The environment given to clients names only
this bus, so a client can never reach the owner's keyring through it.
"""

from __future__ import annotations

import os
import secrets
import shutil
import subprocess
import time
from pathlib import Path

from .procscope import tree_pids
from .rig import REPO

CANARY_PASSWORD = "canary-login-password-6d41"


class PrivateKeyring:
    """`daemon=False` leaves a bus with no Secret Service; `unlock=False` a daemon with no keyring yet."""

    def __init__(self, daemon: bool = True, unlock: bool = True) -> None:
        base = REPO / "private" / "kr"
        base.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.root = base / secrets.token_hex(3)      # kept short: the daemon's control socket lives under it
        for name in ("home", "data", "run", "cache"):
            (self.root / name).mkdir(parents=True, mode=0o700)
        address = f"unix:abstract=lico-kr-{os.getpid()}-{secrets.token_hex(4)}"
        self.env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": str(self.root / "home"),
                    "XDG_DATA_HOME": str(self.root / "data"), "XDG_RUNTIME_DIR": str(self.root / "run"),
                    "XDG_CACHE_HOME": str(self.root / "cache"), "DBUS_SESSION_BUS_ADDRESS": address}
        config = self.root / "run" / "bus.conf"
        config.write_text(f"<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth>"
                          "<policy context='default'><allow send_destination='*' eavesdrop='true'/>"
                          "<allow eavesdrop='true'/><allow own='*'/></policy></busconfig>")
        self.procs: list[subprocess.Popen] = []
        bus = subprocess.Popen(["dbus-daemon", f"--config-file={config}", "--nofork", "--print-address"],
                               stdout=subprocess.PIPE, text=True, env=self.env)
        self.procs.append(bus)
        bus.stdout.readline()
        if daemon:
            args = ["gnome-keyring-daemon", "--foreground", "--components=secrets"] + (["--unlock"] if unlock else [])
            keyring = subprocess.Popen(args, env=self.env, stdin=subprocess.PIPE if unlock else subprocess.DEVNULL,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, text=True)
            self.procs.append(keyring)
            if unlock:
                keyring.stdin.write(CANARY_PASSWORD + "\n")
                keyring.stdin.close()
            self._wait_for_service(expect_default=unlock)

    def _busctl(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["busctl", "--user", *args], env=self.env, capture_output=True, text=True, timeout=20)

    def _wait_for_service(self, expect_default: bool) -> None:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            done = self._busctl("call", "org.freedesktop.secrets", "/org/freedesktop/secrets",
                                "org.freedesktop.Secret.Service", "ReadAlias", "s", "default")
            if done.returncode == 0 and (not expect_default or '"/"' not in done.stdout):
                return
            time.sleep(0.2)
        self.stop()
        raise RuntimeError("the private Secret Service did not start")

    def lock(self) -> None:
        """Lock the default collection, as a screen lock or a timeout would."""
        done = self._busctl("call", "org.freedesktop.secrets", "/org/freedesktop/secrets", "org.freedesktop.Secret.Service",
                            "Lock", "ao", "1", "/org/freedesktop/secrets/collection/login")
        if done.returncode != 0:
            raise RuntimeError("could not lock the private keyring: " + done.stderr.strip())

    def pids(self) -> list[int]:
        return [p.pid for p in self.procs if p.poll() is None]

    def stop(self) -> None:
        for proc in reversed(self.procs):
            try:
                pids = tree_pids(proc.pid)
            except Exception:
                pids = [proc.pid]
            for pid in reversed(pids):
                try:
                    os.kill(pid, 9)
                except OSError:
                    pass
            proc.wait()
        shutil.rmtree(self.root, ignore_errors=True)

    def __enter__(self) -> "PrivateKeyring":
        return self

    def __exit__(self, *exc) -> None:
        self.stop()
