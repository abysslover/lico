"""A product window (`lico gui`) on a private virtual display, looked at through its pixels.

Everything started here is stopped here and nothing else is touched: the host is found through the window's own socket and ended by its own
pid, never by name; the virtual display is stopped on every path.
"""

from __future__ import annotations

import json
import os
import queue
import shutil
import signal
import subprocess
import threading
import time
from pathlib import Path

from .p02 import LICO
from .rig import REPO, Xvfb, private_dir

TEXT_X = 284          # the main area starts right of the session list
GRAY_THRESHOLD = 140  # text is light on a near-black background


class ProductWindow:
    def __init__(self, stdin_open: bool = True, ime: bool = False, host_binary: Path | None = None, host_sessions: int = 0,
                 path_prefix: str | None = None, before_window=None, env_extra: dict | None = None, host_args: list | None = None) -> None:
        # path_prefix: a directory put first on PATH of the host and the window (a stand-in for a provider program);
        # before_window(self): called once the first host is up and before the window starts; env_extra: more environment for both;
        # host_args: more arguments for the first host
        self.host_binary, self.host_sessions, self.old_host = host_binary, host_sessions, None
        self.path_prefix, self.before_window, self.env_extra, self.host_args = path_prefix, before_window, env_extra or {}, host_args or []
        self.stdin_open = stdin_open
        self.ime = ime

    def __enter__(self) -> "ProductWindow":
        self._dir = private_dir()
        self.directory = self._dir.__enter__()
        self.xvfb = Xvfb()
        self.runtime = REPO / "private" / "rt" / Path(self.directory).name[-6:]   # a short path: a socket's path has a length limit
        self.runtime.mkdir(mode=0o700, parents=True, exist_ok=True)
        for name in ("home", "state"):
            (Path(self.directory) / name).mkdir(mode=0o700)
        self.env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": str(Path(self.directory) / "home"), "XDG_RUNTIME_DIR": str(self.runtime),
                    "XDG_STATE_HOME": str(Path(self.directory) / "state"), "DISPLAY": self.xvfb.display, **self.env_extra}
        if self.path_prefix:
            self.env["PATH"] = f"{self.path_prefix}:{self.env['PATH']}"
        self.socket = str(self.runtime / "lico" / "host.sock")
        self.old_pid = None
        if self.host_binary:              # a host started from another build, at the socket the window will look for
            (self.runtime / "lico").mkdir(mode=0o700, exist_ok=True)
            self.old_host = subprocess.Popen([str(self.host_binary), "host", "--socket", self.socket, "--state-dir", str(Path(self.directory) / "state" / "lico"),
                                              "--sessions", str(self.host_sessions), "--producing", "0", *self.host_args], env=self.env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            self.old_pid = json.loads(self.old_host.stdout.readline())["pid"]
        if self.before_window:
            self.before_window(self)
        self.method = None
        if self.ime:                      # a private ibus with the Hangul engine, bound to this display
            from .ime import InputMethod

            self.method = InputMethod(self.xvfb.display)
            self.env = {**self.env, **self.method.client_env}
        self.proc = subprocess.Popen([str(LICO), "gui"], env=self.env, stdin=subprocess.PIPE if self.stdin_open else subprocess.DEVNULL,
                                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.events: queue.Queue = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        self._wait_ready()
        return self

    def _read(self) -> None:
        for line in self.proc.stdout:
            try:
                self.events.put(json.loads(line))
            except ValueError:
                pass

    def _wait_ready(self) -> None:
        end = time.monotonic() + 20
        while time.monotonic() < end:
            if self.proc.poll() is not None:
                return                    # it ended by itself: the caller's checks say so
            try:
                if self.events.get(timeout=0.5).get("ev") == "ready":
                    break
            except queue.Empty:
                pass
        if self.stdin_open:
            self._first_frame()
        else:
            time.sleep(0.8)
        if self.proc.poll() is not None:
            return
        from .ime import _focus      # no window manager here: the keyboard goes where the focus is put

        _focus(self.xvfb.display)
        self.settle(self.frames(), 0.4)

    def alive(self) -> bool:
        return self.proc.poll() is None

    def frames(self) -> int:
        """How many frames the window has drawn so far (its own counter in the dump); 0 when its standard input is closed and it cannot be asked."""
        return self.ui()["frame"]["frames"] if self.stdin_open else 0

    def _first_frame(self, timeout: float = 5.0) -> None:
        end = time.monotonic() + timeout
        while time.monotonic() < end and self.alive() and self.frames() < 1:
            time.sleep(0.03)

    def settle(self, since: int, upto: float) -> bool:
        """Wait until the window has drawn after `since` frames and then drawn nothing for a moment (longer with an input method, whose answer comes
        from another process). `upto` is the bound and was the fixed pause that stood here: a key that changes nothing still costs it."""
        if not self.stdin_open:
            time.sleep(upto)
            return False
        quiet = 0.3 if self.ime else 0.15
        end = time.monotonic() + upto
        last, changed = since, None
        while time.monotonic() < end:
            try:
                frames = self.frames()
            except (OSError, TimeoutError):
                return True              # the window ended (a quit key): nothing more to wait for
            if frames != last:
                last, changed = frames, time.monotonic()
            elif changed is not None and time.monotonic() - changed >= quiet:
                return True
            time.sleep(0.03)
        return last != since

    def key(self, *keys: str) -> None:
        since = self.frames()
        subprocess.run(["xdotool", "key", *keys], env=self.env, check=True)
        self.settle(since, 0.9)

    def type_text(self, text: str) -> None:
        since = self.frames()
        subprocess.run(["xdotool", "type", "--delay", "40", text], env=self.env, check=True)
        self.settle(since, 0.9)

    def hold(self, key: str, seconds: float) -> None:
        since = self.frames()
        subprocess.run(["xdotool", "keydown", key], env=self.env, check=True)
        time.sleep(seconds)
        subprocess.run(["xdotool", "keyup", key], env=self.env, check=True)
        self.settle(since, 0.6)

    def bright_total(self) -> int:
        data, width, height = self.gray()
        return sum(1 for y in range(height) for x in range(TEXT_X, width) if data[y * width + x] > GRAY_THRESHOLD)

    def click(self, x: float, y: float, button: int = 1) -> None:
        wid = subprocess.run(["xdotool", "search", "--name", "^Lico$"], env=self.env, capture_output=True, text=True).stdout.split()[-1]
        since = self.frames()
        subprocess.run(["xdotool", "mousemove", "--window", wid, str(int(x)), str(int(y)), "click", str(button)], env=self.env, check=True)
        self.settle(since, 0.7)

    def ui(self) -> dict:
        self.proc.stdin.write("ui\n")
        self.proc.stdin.flush()
        end = time.monotonic() + 10
        while time.monotonic() < end:
            try:
                event = self.events.get(timeout=0.5)
            except queue.Empty:
                if self.proc.poll() is not None:
                    raise TimeoutError("the window ended before it answered")
                continue
            if event.get("ev") == "ui":
                return event
        raise TimeoutError("no ui dump")

    def gray(self) -> tuple[bytes, int, int]:
        out = subprocess.run(["import", "-window", "root", "-depth", "8", "gray:-"], env=self.env, capture_output=True, check=True).stdout
        return out, 1200, 800

    def png(self, path: Path) -> None:
        subprocess.run(["import", "-window", "root", str(path)], env=self.env, check=True)

    def bright(self, x0: int, y0: int, x1: int, y1: int, threshold: int = GRAY_THRESHOLD) -> int:
        data, width, _ = self.gray()
        return sum(1 for y in range(y0, y1) for x in range(x0, x1) if data[y * width + x] > threshold)

    def text_rows(self) -> int:
        """How many 18-pixel text rows of the main area carry text."""
        data, width, height = self.gray()
        rows = 0
        for band in range(0, height, 18):
            lit = sum(1 for y in range(band, min(band + 18, height)) for x in range(TEXT_X, width) if data[y * width + x] > GRAY_THRESHOLD)
            rows += lit > 15
        return rows

    def corner_is_background(self) -> bool:
        data, width, _ = self.gray()
        bar = data[5 * width + 1100]                  # the menu bar's own colour, far from its titles
        return all(data[y * width + x] == bar for y, x in ((2, 2), (2, 30), (13, 2), (13, 30), (8, 2), (8, 30)))     # no solid squares of the measurement markers

    def __exit__(self, *exc) -> None:
        host_pid = None
        try:
            done = subprocess.run([str(LICO), "--socket", self.socket, "status"], env=self.env, capture_output=True, text=True, timeout=5)
            host_pid = json.loads(done.stdout).get("pid")
        except (OSError, ValueError, subprocess.TimeoutExpired):
            pass
        if self.alive():
            self.proc.terminate()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        if host_pid and Path(f"/proc/{host_pid}").exists():
            try:
                os.kill(host_pid, signal.SIGTERM)      # the host this window started, by its pid, found through its own socket
            except OSError:
                pass
        if self.old_host and self.old_host.poll() is None:
            self.old_host.terminate()          # the one this window started, by its handle
        if self.method:
            self.method.stop()
        self.xvfb.stop()
        shutil.rmtree(self.runtime, ignore_errors=True)
        self._dir.__exit__(None, None, None)


def capture_three(out: Path) -> dict:
    """Start, after Ctrl+N and after Ctrl+A, as PNG files plus what the window says it shows."""
    out.mkdir(parents=True, exist_ok=True)
    report = {}
    with ProductWindow() as w:
        w.png(out / "1_start.png")
        report["start"] = {"panel": w.ui()["panel"], "text_rows": w.text_rows()}
        w.key("ctrl+n")
        w.png(out / "2_after_ctrl_n.png")
        report["after_ctrl_n"] = {"panel": w.ui()["panel"], "text_rows": w.text_rows()}
        w.key("Escape")
        w.key("ctrl+a")
        w.png(out / "3_after_ctrl_a.png")
        report["after_ctrl_a"] = {"panel": w.ui()["panel"], "text_rows": w.text_rows()}
    (out / "captures.json").write_text(json.dumps(report, indent=1))
    return report


def capture_backspace(out: Path) -> dict:
    """The account label line with `ab` typed, and after one Backspace."""
    out.mkdir(parents=True, exist_ok=True)
    with ProductWindow() as w:
        for keys in (("ctrl+n",), ("1",), ("n",)):
            w.key(*keys)
        w.type_text("ab")
        w.png(out / "4_typed_ab.png")
        before = w.ui()["panel"]["input"]
        w.key("BackSpace")
        w.png(out / "5_after_backspace.png")
        after = w.ui()["panel"]["input"]
    return {"typed": before, "after_backspace": after}


def capture_paste(out: Path) -> dict:
    """The account label line after a paste from the clipboard."""
    out.mkdir(parents=True, exist_ok=True)
    with ProductWindow() as w:
        for keys in (("ctrl+n",), ("1",), ("n",)):
            w.key(*keys)
        subprocess.run(["xclip", "-selection", "clipboard", "-i"], input=b"Work account", env={"DISPLAY": w.xvfb.display, "PATH": "/usr/bin:/bin"},
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
        time.sleep(0.3)
        w.key("ctrl+v")
        w.png(out / "7_label_after_paste.png")
        return {"input": w.ui()["panel"]["input"]}
