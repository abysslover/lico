import subprocess
import time

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Xvfb, private_dir

from tests.account_helpers import account
from tests.test_claude_adapter import claude_host, new_claude
from tests.test_codex_adapter import wait_for
from tests.test_first_view import start_client, ui, window_id, xdo


def bottom_ink(display, rows=22):
    """Light pixels in the last `rows` pixel rows of the window: the permanent hint line lives there."""
    data = subprocess.run(["import", "-window", "root", "-depth", "8", "gray:-"], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, capture_output=True, check=True).stdout
    return sum(1 for y in range(800 - rows, 800) for x in range(1200) if data[y * 1200 + x] > 140)


def three_sessions(directory):
    host, log, received = claude_host(directory)
    conn = Conn(host.socket)
    a = account(conn, "Work", "claude")
    ids = [new_claude(conn, account=a)[0] for _ in range(3)]
    return host, conn, ids


def test_ctrl_n_and_ctrl_a_work_inside_a_session_view_and_a_permanent_hint_line_says_so():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, conn, ids = three_sessions(directory)
            client = start_client(host, xvfb.display)
            client.send(f"show {ids[0]}")
            client.expect("state", "subscribed")
            state = ui(client, lambda u: u["selected"] == ids[0] and len(u["rows"]) == 3)
            assert "Ctrl+N new session" in state["hint"] and "Ctrl+A accounts" in state["hint"] and "Ctrl+Tab next session" in state["hint"]      # in every view
            assert bottom_ink(xvfb.display) > 150                                  # and drawn: the hint is on the screen, not only in the model
            xdo(xvfb.display, "key", "ctrl+n")
            assert ui(client, lambda u: u["panel"] is not None)["panel"]["title"] == "New session"
            xdo(xvfb.display, "key", "Escape")
            ui(client, lambda u: u["panel"] is None)
            xdo(xvfb.display, "key", "ctrl+a")
            assert ui(client, lambda u: u["panel"] is not None)["panel"]["title"] == "Accounts"
            assert "Esc back out" in ui(client)["hint"]
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_ctrl_tab_alt_digits_and_escape_move_between_sessions_and_back_to_the_start_screen():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, conn, ids = three_sessions(directory)
            client = start_client(host, xvfb.display)
            state = ui(client, lambda u: len(u["rows"]) == 3)
            order = [r[2] for r in state["layout"]["rows"]]                         # the numbers of the list: its order
            assert sorted(order) == sorted(ids) and "1" in state["list_text"].splitlines()[1]
            assert state["selected"] == "" and "Ctrl+N  new session" in state["start_screen"]                 # nothing open: the start screen says what to do
            xdo(xvfb.display, "key", "alt+2")
            assert ui(client, lambda u: u["selected"] == order[1])["selected"] == order[1]                  # the number in the list
            xdo(xvfb.display, "key", "ctrl+Tab")
            assert ui(client, lambda u: u["selected"] == order[2])["selected"] == order[2]
            xdo(xvfb.display, "key", "ctrl+Tab")
            assert ui(client, lambda u: u["selected"] == order[0])["selected"] == order[0]                  # wraps
            xdo(xvfb.display, "key", "ctrl+shift+Tab")
            assert ui(client, lambda u: u["selected"] == order[2])["selected"] == order[2]
            # a draft stays with its session and Esc with text in the line keeps its meaning (it does not leave the session)
            xdo(xvfb.display, "type", "draft")
            ui(client, lambda u: u["input"] == "draft")
            xdo(xvfb.display, "key", "Escape")
            time.sleep(0.5)
            assert ui(client)["selected"] == order[2]
            xdo(xvfb.display, "key", "ctrl+Tab")
            ui(client, lambda u: u["selected"] == order[0])
            xdo(xvfb.display, "key", "ctrl+shift+Tab")
            assert ui(client, lambda u: u["selected"] == order[2] and u["input"] == "draft")["input"] == "draft"
            xdo(xvfb.display, "key", "ctrl+u")
            ui(client, lambda u: u["input"] == "")
            xdo(xvfb.display, "key", "Escape")                                      # empty line: back to the start screen, the session keeps running
            state = ui(client, lambda u: u["selected"] == "")
            assert "Ctrl+N  new session" in state["start_screen"]
            assert [s["state"] for s in conn.call(op="status")["sessions"]] == ["running"] * 3
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
