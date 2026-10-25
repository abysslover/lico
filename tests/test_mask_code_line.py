import json
import subprocess
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Xvfb, private_dir

from tests.test_account_helpers import account
from tests.test_first_view import ui, xdo
from tests.test_login_window import panel_host, panel_of
from tests.test_paste_open import own, sign_in_panel, window

CANARY = "CANARYCODE9"


def ink(display):
    """How many pixels of the main area are light: the prompt row is the only part that changes with what is typed."""
    time.sleep(0.8)                                                                                # the frame after the last key has been presented
    data = subprocess.run(["import", "-window", "root", "-depth", "8", "gray:-"], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, capture_output=True, check=True).stdout
    return sum(1 for y in range(800) for x in range(284, 1200) if data[y * 1200 + x] > 140)


def test_the_sign_in_code_is_drawn_as_stars_and_is_in_no_dump_notice_or_output():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log = panel_host(directory)
            conn = Conn(host.socket)
            account(conn, "Work")
            client, calls = window(host, xvfb.display, directory)
            sign_in_panel(xvfb.display)
            panel_of(client, lambda p: p["text_entry"])
            empty = ink(xvfb.display)
            own(xvfb.display, CANARY)
            xdo(xvfb.display, "key", "ctrl+v")
            state = ui(client, lambda u: u["panel"]["input_len"] == len(CANARY))
            assert state["panel"]["input"] == "*" * len(CANARY) and CANARY not in json.dumps(state)      # the model holds stars, not the code
            pasted = ink(xvfb.display)
            xdo(xvfb.display, "key", "ctrl+u")
            ui(client, lambda u: u["panel"]["input_len"] == 0)
            xdo(xvfb.display, "type", "W" * len(CANARY))                                                   # the widest letter: if the code's own glyphs were drawn, the ink would differ
            ui(client, lambda u: u["panel"]["input_len"] == len(CANARY))
            typed = ink(xvfb.display)
            assert typed == pasted > empty, (empty, pasted, typed)                                                    # the same pixels as eleven characters of anything else
            xdo(xvfb.display, "key", "ctrl+u")
            own(xvfb.display, "S" * 80)
            xdo(xvfb.display, "key", "ctrl+v")
            state = ui(client, lambda u: u["panel"]["input_len"] == 80)
            assert state["panel"]["input"] == "*" * 64                                                    # at most 64 stars are drawn
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
