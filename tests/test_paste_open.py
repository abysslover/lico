import json
import os
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.ime import _focus
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Client, Xvfb, private_dir

from tests.test_account_helpers import account
from tests.test_codex_adapter import wait_for
from tests.test_first_view import ui, xdo
from tests.test_login import URL, home_of
from tests.test_login_window import panel_host, panel_of


def own(display, text, selection="clipboard"):
    """Hold a selection from outside the window, as another program would (it stays until the display ends)."""
    subprocess.run(["xclip", "-selection", selection, "-i"], input=text.encode(), env={"DISPLAY": display, "PATH": "/usr/bin:/bin"},
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    time.sleep(0.3)


def window(host, display, directory, path=None):
    """The primary client with a fake xdg-open in front on its path that records what it was asked to open."""
    fake = Path(directory) / "fakebin"
    fake.mkdir(exist_ok=True)
    script = fake / "xdg-open"
    script.write_text('#!/bin/sh\necho "$1" >> "$XDG_CALLS"\n')
    script.chmod(0o755)
    calls = Path(directory) / "xdg_calls"
    env = {"PATH": path if path is not None else f"{fake}:{os.environ['PATH']}", "XDG_CALLS": str(calls)}
    client = Client("a2", host.socket, display, extra_args=["--ui"], extra_env=env)
    time.sleep(0.8)
    _focus(display)
    time.sleep(0.3)
    return client, calls


def opened(calls):
    return calls.read_text().split() if calls.exists() else []


def sign_in_panel(display):
    xdo(display, "key", "ctrl+a")
    time.sleep(0.5)
    xdo(display, "key", "1")
    time.sleep(0.3)
    xdo(display, "key", "s")


def test_ctrl_v_pastes_a_sign_in_code_that_is_then_accepted_like_a_typed_one_and_never_echoed():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log = panel_host(directory)
            conn = Conn(host.socket)
            work = account(conn, "Work")
            client, calls = window(host, xvfb.display, directory)

            class Recording:
                def __init__(self, queue):
                    self.queue, self.seen = queue, []

                def __getattr__(self, name):
                    return getattr(self.queue, name)

                def get(self, *args, **kwargs):
                    message = self.queue.get(*args, **kwargs)
                    self.seen.append(message)
                    return message

            client.lines = Recording(client.lines)
            sign_in_panel(xvfb.display)
            panel_of(client, lambda p: p["text_entry"])
            own(xvfb.display, "CANARYCODE9")
            xdo(xvfb.display, "key", "ctrl+v")
            ui(client, lambda u: u["panel"]["input_len"] == 11)
            assert not [m for m in client.lines.seen if m.get("ev") == "ime" and m.get("text")]      # a paste is not echoed to standard output
            xdo(xvfb.display, "key", "Return")                                                    # accepted only by the person's own Enter
            state = ui(client, lambda u: u["panel"] is not None and "Signed in." in u["panel"]["body"])
            assert state["panel"]["input"] == "" and "CANARYCODE9" not in json.dumps(state)
            assert (home_of(directory, work) / "home" / "received_code").read_text() == "CANARYCODE9"
            assert not [line for line in opened(calls) if line != URL]                              # the browser was asked for the address only
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def label_entry(w):
    w.key("ctrl+n")
    w.key("1")
    w.key("n")
    assert w.ui()["panel"]["text_entry"]


def test_the_other_paste_keys_and_the_refusals_leave_the_line_alone_and_never_submit():
    with ProductWindow() as w:
        display = w.xvfb.display
        label_entry(w)
        began = time.monotonic()
        w.key("ctrl+v")                                                       # nobody owns the clipboard: a notice, no hang
        panel = w.ui()["panel"]
        assert "Nothing to paste." in panel["body"] and panel["input"] == "" and time.monotonic() - began < 4
        own(display, "hello\nworld")
        w.key("ctrl+shift+v")
        assert w.ui()["panel"]["input"] == "hello"                           # a newline in the middle ends the paste
        w.key("ctrl+u")
        own(display, "trail\n")
        w.key("ctrl+v")
        panel = w.ui()["panel"]
        assert panel["input"] == "trail" and panel["text_entry"]             # a trailing newline does not submit
        w.key("ctrl+u")
        own(display, "prim", "primary")
        w.key("shift+Insert")
        assert w.ui()["panel"]["input"] == "prim"                            # the text selected elsewhere
        w.key("ctrl+u")
        own(display, "x" * 9216)
        w.key("ctrl+v")
        panel = w.ui()["panel"]
        assert panel["input"] == "" and "Paste refused: more than 8 KiB." in panel["body"]   # refused, not cut
        own(display, "Work account")
        w.key("ctrl+v")
        assert w.ui()["panel"]["input"] == "Work account"                    # the label line is not secret: visible text
        w.key("BackSpace")
        assert w.ui()["panel"]["input"] == "Work accoun"


def test_the_browser_opens_by_itself_once_per_attempt_again_on_request_and_a_missing_opener_says_so():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log = panel_host(directory)
            conn = Conn(host.socket)
            account(conn, "Work")
            client, calls = window(host, xvfb.display, directory)
            sign_in_panel(xvfb.display)
            wait_for(lambda: len(opened(calls)) == 1)
            time.sleep(2.5)                                                   # polls and redraws go on: it does not open again
            assert opened(calls) == [URL]
            assert any("Opening the browser" in line for line in panel_of(client, lambda p: URL in p["body"])["body"])
            xdo(xvfb.display, "key", "ctrl+o")                                # on request, once more
            wait_for(lambda: len(opened(calls)) == 2)
            xdo(xvfb.display, "key", "Escape")                                # ends the attempt
            time.sleep(0.5)
            sign_in_panel(xvfb.display)                                       # a new attempt opens it again
            wait_for(lambda: len(opened(calls)) == 3)
            assert all(line == URL for line in opened(calls)) and "CANARY" not in calls.read_text().replace("CANARYURLSTATE7", "")
            client.kill()
            client, calls = window(host, xvfb.display, directory, path="/nonexistent")
            sign_in_panel(xvfb.display)
            shown = panel_of(client, lambda p: URL in p["body"] and any("xdg-open is not available" in line for line in p["body"]))
            assert URL in shown["body"]                                        # the address stays on screen to copy
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
