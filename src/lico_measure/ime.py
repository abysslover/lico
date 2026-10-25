"""Korean composition through a real input method, in a private session.

A private session bus and input-method daemon run for one virtual display; they
share nothing with any desktop session. Keys are sent with XTEST, so the input
method sees ordinary key events.
"""

from __future__ import annotations

import os
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
        cache = REPO / "private" / "k"
        shutil.rmtree(cache, ignore_errors=True)   # stale sockets of earlier private daemons
        cache.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8", "DISPLAY": display,
            "HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"), "XDG_CACHE_HOME": str(cache),
            "XDG_DATA_HOME": str(home / "data"), "XDG_RUNTIME_DIR": str(home / "run"),
        }
        address = f"unix:abstract=lico-ime-{os.getpid()}-{secrets.token_hex(4)}"
        # a bus with no service directories: nothing is activated on it, only the processes we start
        config = home / "run" / "private-bus.conf"
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


def composition_run(candidate: str, delay_ms: int = 80, producing: int = 0) -> dict:
    """Type the sentence through the input method into the candidate's input line."""
    with private_dir() as directory:
        xvfb = Xvfb()
        host = None
        method = None
        client = None
        try:
            host = Host(directory, sessions=1, producing=producing, history=500)
            method = InputMethod(xvfb.display)
            client = Client(candidate, host.socket, xvfb.display, extra_env=method.client_env)
            sid = host.sessions[0]
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(1.0)
            _focus(xvfb.display)
            time.sleep(0.5)
            while not client.lines.empty():
                client.lines.get_nowait()
            env = dict(os.environ, DISPLAY=xvfb.display)
            # the engine starts in Latin mode; its trigger key switches to Hangul
            subprocess.run(["xdotool", "key", "shift+space"], env=env, check=True)
            time.sleep(0.5)
            while not client.lines.empty():
                client.lines.get_nowait()
            subprocess.run(["xdotool", "key", "--delay", str(delay_ms), *SENTENCE_KEYS], env=env, check=True)
            time.sleep(1.0)
            client.send("text")
            time.sleep(0.5)
            events = []
            while not client.lines.empty():
                message = client.lines.get_nowait()
                if message.get("ev") == "ime":
                    events.append(message)
            return _analyse(events)
        finally:
            if client:
                client.kill()
            if method:
                method.stop()
            if host:
                host.stop()
            xvfb.stop()
