"""Process rig: private X server, synthetic host and a candidate client."""

from __future__ import annotations

import json
import os
import queue
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
FONT_PRIMARY = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_FALLBACK = "/usr/share/fonts/truetype/nanum/NanumSquare_acR.ttf"


def candidate_command(name: str, sock: str) -> list[str]:
    build = Path(os.environ.get("LICO_BUILD_ROOT", REPO / "private" / "build"))
    if name == "a":
        return [str(build / "a_native" / "release" / "lico-candidate-a"), "--socket", sock,
                "--font", FONT_PRIMARY, "--font2", FONT_FALLBACK]
    if name == "a2":
        return [str(build / "a2_cosmic" / "release" / "lico-candidate-a2"), "--socket", sock]
    if name in ("b", "b2", "b3"):
        return [str(build / "b_gtk" / "candidate-b"), "--socket", sock]
    if name == "d":
        return [str(build / "d_webkit" / "candidate-d"), "--socket", sock]
    if name in ("c", "c2"):
        return [str(build / "c_qt" / "candidate-c"), "--socket", sock]
    raise ValueError(name)


# environment that distinguishes variants built from the same binary
CANDIDATE_ENV = {
    "b2": {"GDK_DEBUG": "gl-disable", "__GLX_VENDOR_LIBRARY_NAME": "none"},
    # experiment only: an empty libGL stub first on the library path keeps the GL driver stack unmapped
    "b3": {"GDK_DEBUG": "gl-disable", "LD_LIBRARY_PATH": str(REPO / "private" / "build" / "stubs")},
    "d": {"WEBKIT_DISABLE_COMPOSITING_MODE": "1", "WEBKIT_DISABLE_DMABUF_RENDERER": "1"},
    "c": {"QT_QPA_PLATFORM": "xcb", "QT_ACCESSIBILITY": "0"},
    "c2": {"QT_QPA_PLATFORM": "xcb", "QT_ACCESSIBILITY": "0", "QT_XCB_GL_INTEGRATION": "none"},
}


def _client_home() -> Path:
    """A private, persistent home for client processes (font caches survive between runs)."""
    home = REPO / "private" / "scratch" / "clienthome"
    for sub in ("", "config", "cache", "data", "run"):
        (home / sub).mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(home / "run", 0o700)
    return home


def session_rgb(session_id: str) -> int:
    return int(session_id[1:7], 16)


class Xvfb:
    """A private virtual X server (no compositor, software rendering)."""

    def __init__(self, geometry: str = "1200x800x24") -> None:
        read_end, write_end = os.pipe()
        self.proc = subprocess.Popen(
            ["Xvfb", "-displayfd", str(write_end), "-screen", "0", geometry, "-nolisten", "tcp"],
            pass_fds=(write_end,), stderr=subprocess.DEVNULL,
        )
        os.close(write_end)
        with os.fdopen(read_end) as handle:
            self.display = ":" + handle.readline().strip()

    def stop(self) -> None:
        self.proc.kill()
        self.proc.wait()


HOST_BUILD = {"rust_std": "host_rust_std/release/lico-host", "rust_serde": "host_rust_serde/release/lico-host",
              "c": "host_c/lico-host-c", "hostd": "host_hostd/release/lico-host"}


def _host_command(impl: str) -> list[str]:
    if impl == "oracle":
        return [sys.executable, "-B", "-m", "lico_host"]
    build = Path(os.environ.get("LICO_BUILD_ROOT", REPO / "private" / "build"))
    return [str(build / HOST_BUILD[impl])]


def host_impls() -> list[str]:
    """The reference oracle plus every conformance candidate that has been built."""
    build = Path(os.environ.get("LICO_BUILD_ROOT", REPO / "private" / "build"))
    return ["oracle"] + [name for name, rel in HOST_BUILD.items() if (build / rel).exists()]


# When set, synthetic sessions produce UTF-8 text lines shaped like agent output (mixed scripts,
# ~110 bytes) and every transcript of 100 records or more becomes `history` lines long.
TEXT_WORKLOAD: dict | None = None


# When set, every measured host gets this many accounts (provider "claude") before the measurement starts.
ACCOUNTS = 0


def effective_history(history: int) -> int:
    return TEXT_WORKLOAD["history"] if TEXT_WORKLOAD and history >= 100 else history


def output_kind() -> str:
    return "agent-shaped UTF-8 text lines" if TEXT_WORKLOAD else "synthetic bytes shown as hex"


class Host:
    def __init__(self, directory: str, sessions: int, producing: int, rate: float = 100.0,
                 history: int = 5000, retain: int = 2048, impl: str | None = None,
                 extra_args: list[str] | None = None, env_overrides: dict | None = None) -> None:
        impl = impl or os.environ.get("LICO_HOST_IMPL") or ("hostd" if "hostd" in host_impls() else "oracle")
        self.socket = os.path.join(directory, "host.sock")
        self.directory = directory
        env = dict(os.environ, PYTHONPATH=str(REPO / "src"), PYTHONDONTWRITEBYTECODE="1")
        extra_args = list(extra_args or [])
        if TEXT_WORKLOAD:
            extra_args += ["--text", "--payload", str(TEXT_WORKLOAD["payload"])]
            history = effective_history(history)
            retain = max(retain, TEXT_WORKLOAD["history"] + 2000)   # the durable log really keeps the long transcripts
        self.proc = subprocess.Popen(
            [*_host_command(impl), "--socket", self.socket, "--sessions", str(sessions),
             "--producing", str(producing), "--rate", str(rate), "--history", str(history),
             "--retain", str(retain), *extra_args],
            stdout=subprocess.PIPE, stderr=open(os.path.join(directory, "host.err"), "wb"), text=True,
            env={k: v for k, v in {**env, **(env_overrides or {})}.items() if k != "PYTHONHOME"},
        )
        self.ready_line = self.proc.stdout.readline()
        ready = json.loads(self.ready_line)
        self.pid = ready["pid"]
        self.sessions: list[str] = ready["sessions"]

    def stop(self) -> None:
        self.proc.kill()
        self.proc.wait()


class HostConn:
    """A plain protocol client (no GUI) for status and replay checks."""

    def __init__(self, path: str) -> None:
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(path)
        self.reader = self.sock.makefile("r")

    def call(self, **request) -> dict:
        self.sock.sendall((json.dumps(request) + "\n").encode())
        return json.loads(self.reader.readline())

    def next_event(self) -> dict:
        return json.loads(self.reader.readline())

    def close(self) -> None:
        self.sock.close()


class Client:
    """A candidate GUI process driven over stdin; its JSON lines are queued."""

    def __init__(self, name: str, host_socket: str, display: str, wait_ready: bool = True,
                 extra_env: dict | None = None) -> None:
        home = _client_home()
        # an explicit environment: no inherited session bus, themes, input-method or user settings
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": os.environ.get("LANG", "C.UTF-8"),
               "DISPLAY": display, "HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
               "XDG_CACHE_HOME": str(home / "cache"), "XDG_DATA_HOME": str(home / "data"),
               "XDG_RUNTIME_DIR": str(home / "run"), "GDK_BACKEND": "x11", "NO_AT_BRIDGE": "1",
               **CANDIDATE_ENV.get(name, {}), **(extra_env or {})}
        self.proc = subprocess.Popen(
            candidate_command(name, host_socket), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            text=True, env=env,
        )
        self.lines: queue.Queue[dict] = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        if wait_ready:
            self.expect("ready")

    def _read(self) -> None:
        for line in self.proc.stdout:
            try:
                self.lines.put(json.loads(line))
            except ValueError:
                pass

    def send(self, command: str) -> None:
        self.proc.stdin.write(command + "\n")
        self.proc.stdin.flush()

    def expect(self, ev: str, why: str | None = None, timeout: float = 20.0) -> dict:
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"no {ev}/{why} from client")
            message = self.lines.get(timeout=remaining)
            if message.get("ev") == ev and (why is None or message.get("why") == why):
                return message

    def state(self) -> dict:
        self.send("status")
        return self.expect("state", "status")

    def kill(self) -> None:
        self.proc.kill()
        self.proc.wait()

    def quit(self) -> None:
        if self.proc.poll() is None:
            self.send("quit")
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.kill()


def private_dir() -> tempfile.TemporaryDirectory:
    """A 0700 run directory under the git-ignored private scratch area."""
    base = REPO / "private" / "scratch" / "runs"
    base.mkdir(parents=True, exist_ok=True)
    return tempfile.TemporaryDirectory(dir=base, prefix="run-")
