"""Korean composition through a real input method, in a private session.

A private session bus and input-method daemon run for one virtual display; they
share nothing with any desktop session. Keys are sent with XTEST, so the input
method sees ordinary key events.
"""

from __future__ import annotations

import os
import queue
import secrets
import shutil
import subprocess
import time

from .procscope import tree_pids
from .rig import REPO, Client, Host, Xvfb, _client_home, private_dir, session_rgb

# 2-set keyboard: d-k-s-s-u-d spells 안녕, g-k-t-p-e-y spells 하세요, g-k-s-r-m-f spells 한글
SENTENCE_KEYS = ["d", "k", "s", "s", "u", "d", "g", "k", "t", "p", "d", "y", "space",
                 "g", "k", "s", "r", "m", "f", "space"]
SENTENCE_TEXT = "안녕하세요 한글 "


class InputMethod:
    """dbus session bus + ibus-daemon with the Hangul engine, bound to one display."""

    def __init__(self, display: str) -> None:
        home = _client_home()
        # the daemon's socket lives under its cache directory; keep that path short (Unix socket limit)
        root = REPO / "private" / "k"
        root.mkdir(mode=0o700, parents=True, exist_ok=True)
        for old in root.iterdir():                  # stale sockets of private daemons that a killed run left (not those of runs in parallel with this one)
            if time.time() - old.stat().st_mtime > 86400:
                shutil.rmtree(old, ignore_errors=True)
        while True:                                 # a name of two characters: the Qt client cannot reach a daemon whose socket path is much longer than the old one
            token = secrets.token_hex(1)
            cache = root / token
            try:
                cache.mkdir(mode=0o700)
                break
            except FileExistsError:
                continue
        self.cache = cache
        self.env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8", "DISPLAY": display,
            "HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"), "XDG_CACHE_HOME": str(cache),
            "XDG_DATA_HOME": str(home / "data"), "XDG_RUNTIME_DIR": str(home / "run"),
        }
        address = f"unix:abstract=lico-ime-{os.getpid()}-{secrets.token_hex(4)}"
        # a bus with no service directories: nothing is activated on it, only the processes we start
        config = home / "run" / f"private-bus-{token}.conf"
        self.config = config
        config.write_text(
            f"<busconfig><type>session</type><listen>{address}</listen><auth>EXTERNAL</auth>"
            "<policy context='default'><allow send_destination='*' eavesdrop='true'/>"
            "<allow eavesdrop='true'/><allow own='*'/></policy></busconfig>")
        self.bus = subprocess.Popen(["dbus-daemon", f"--config-file={config}", "--nofork", "--print-address"],
                                    stdout=subprocess.PIPE, text=True, env=self.env)
        self.bus.stdout.readline()
        self.env["DBUS_SESSION_BUS_ADDRESS"] = address
        self.daemon = subprocess.Popen(["ibus-daemon", "--xim", "--panel", "disable"], env=self.env,
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + 20
        while True:
            done = subprocess.run(["ibus", "engine", "hangul"], env=self.env, capture_output=True)
            if done.returncode == 0:
                break
            if time.monotonic() > deadline:
                self.stop()
                raise RuntimeError("the Hangul engine did not start")
            time.sleep(0.3)
        self.client_env = {"DBUS_SESSION_BUS_ADDRESS": address, "GTK_IM_MODULE": "ibus",
                           "QT_IM_MODULE": "ibus", "XMODIFIERS": "@im=ibus"}

    def stop(self) -> None:
        for proc in (self.daemon, self.bus):
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
        shutil.rmtree(self.cache, ignore_errors=True)
        self.config.unlink(missing_ok=True)


def _focus(display: str) -> None:
    env = dict(os.environ, DISPLAY=display)
    ids = subprocess.run(["xdotool", "search", "--name", "Lico"], env=env,
                         capture_output=True, text=True).stdout.split()
    if not ids:
        raise RuntimeError("no candidate window to focus")
    subprocess.run(["xdotool", "windowfocus", "--sync", ids[-1]], env=env, check=True)


def _analyse(events: list[dict]) -> dict:
    committed, preedit, grows, trace = "", "", True, []
    ime = 0
    for event in events:
        kind, text = event["kind"], event["text"]
        if kind == "preedit":
            ime += 1
            preedit = text
        elif kind == "text":
            ime += 1
            if not text.startswith(committed):
                grows = False          # something already committed was removed or rewritten
            committed = text
        trace.append((kind, text))
    return {"committed_only_grows": grows, "final_text": committed, "ime_events": ime, "trace": trace[-40:]}


class CompositionRig:
    """One virtual display with its input method, for several runs one after the other (each run has its own host and client)."""

    def __enter__(self) -> "CompositionRig":
        self.used = False
        self.xvfb = Xvfb()
        try:
            self.method = InputMethod(self.xvfb.display)
        except BaseException:
            self.xvfb.stop()
            raise
        return self

    def __exit__(self, *exc) -> None:
        self.method.stop()
        self.xvfb.stop()

    def fresh_engine(self) -> None:
        """The Hangul engine keeps its Latin/Hangul mode for all input contexts: switching away and back starts it again in Latin mode, as a first run finds it."""
        if not self.used:
            self.used = True
            return
        for name in ("xkb:us::eng", "hangul"):
            subprocess.run(["ibus", "engine", name], env=self.method.env, capture_output=True, check=True, timeout=10)


def _focus_when_mapped(display: str, timeout: float = 10.0) -> None:
    end = time.monotonic() + timeout
    while True:
        try:
            return _focus(display)
        except RuntimeError:
            if time.monotonic() > end:
                raise
            time.sleep(0.1)


def _collect(client, quiet: float, limit: float) -> list[dict]:
    """The client's `ime` events, until none has come for `quiet` seconds (other lines, such as output, do not count) or `limit` seconds have passed."""
    events, end = [], time.monotonic() + limit
    last = time.monotonic()
    while time.monotonic() < end and time.monotonic() - last < quiet:
        try:
            message = client.lines.get(timeout=0.05)
        except queue.Empty:
            continue
        if message.get("ev") == "ime":
            events.append(message)
            last = time.monotonic()
    return events


def composition_run(candidate: str, delay_ms: int = 80, producing: int = 0, rig: CompositionRig | None = None) -> dict:
    """Type the sentence through the input method into the candidate's input line. With `rig` the display and the input method are the rig's."""
    if rig is None:
        with CompositionRig() as own:
            return composition_run(candidate, delay_ms, producing, own)
    with private_dir() as directory:
        host = None
        client = None
        try:
            rig.fresh_engine()
            host = Host(directory, sessions=1, producing=producing, history=500)
            client = Client(candidate, host.socket, rig.xvfb.display, extra_env=rig.method.client_env)
            sid = host.sessions[0]
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(0.3)                                         # the toolkit makes its window and its input context after `ready`; none of the clients says when it is done
            _focus_when_mapped(rig.xvfb.display)
            time.sleep(0.5)
            _collect(client, quiet=0.2, limit=1.0)
            env = dict(os.environ, DISPLAY=rig.xvfb.display)
            # the engine starts in Latin mode; its trigger key switches to Hangul
            subprocess.run(["xdotool", "key", "shift+space"], env=env, check=True)
            _collect(client, quiet=0.3, limit=1.0)                  # what the switch said is not part of the sentence
            subprocess.run(["xdotool", "key", "--delay", str(delay_ms), *SENTENCE_KEYS], env=env, check=True)
            events = _collect(client, quiet=0.4, limit=4.0)
            client.send("text")
            events += _collect(client, quiet=0.25, limit=2.0)
            return _analyse(events)
        finally:
            if client:
                client.kill()
            if host:
                host.stop()
