import json
import os
import subprocess
import time
from pathlib import Path

from lico_measure.ime import _focus
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Client, Xvfb, private_dir
from lico_measure.xpixel import PixelReader

from tests.account_helpers import account
from tests.test_claude_adapter import claude_host, lines_of, new_claude, records, status_of
from tests.test_codex_adapter import wait_for


def ui(client, until=None, timeout=15.0):
    """The client's own model of what it shows (asked for until it satisfies `until`)."""
    end = time.monotonic() + timeout
    while True:
        client.send("ui")
        state = client.expect("ui")
        if until is None or until(state):
            return state
        assert time.monotonic() < end, [(r['session'][:5], r['label'], r['state_text'], r['approvals']) for r in state['rows']]
        time.sleep(0.3)


def window_id(display):
    env = dict(os.environ, DISPLAY=display)
    return subprocess.run(["xdotool", "search", "--name", "Lico candidate"], env=env, capture_output=True, text=True).stdout.split()[-1]


def xdo(display, *args):
    subprocess.run(["xdotool", *args], env=dict(os.environ, DISPLAY=display), check=True)


def click(display, x, y, button=1):
    xdo(display, "mousemove", "--window", window_id(display), str(int(x)), str(int(y)), "click", str(button))


def responses(received):
    out = []
    for line in lines_of(received):
        if line.startswith("{") and json.loads(line).get("type") == "control_response":
            out.append(json.loads(line)["response"])
    return out


def start_client(host, display):
    client = Client("a2", host.socket, display, extra_args=["--ui"], extra_env={"LICO_NO_AUTO_OPEN": "1"})   # no browser in these tests
    time.sleep(0.8)
    _focus(display)                                  # no window manager here: the keyboard goes where the focus is put
    time.sleep(0.3)
    return client


def test_session_list_shows_state_account_and_a_pending_approval_while_the_session_is_hidden():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            a, b = account(conn, "Work", "claude"), account(conn, "Personal", "claude")
            s1, g1 = new_claude(conn, account=a, mode="default")
            s2, g2 = new_claude(conn, account=b, mode="default")
            s3, g3 = new_claude(conn, account=a, mode="default")
            conn.call(op="input", session=s1, generation=g1, request_id="a", text="hello")
            wait_for(lambda: "[turn completed]" in records(log, s1))
            conn.call(op="input", session=s2, generation=g2, request_id="b", text="slow:60")
            conn.call(op="input", session=s3, generation=g3, request_id="c", text="fixture")
            wait_for(lambda: status_of(conn, s3).get("approvals"))
            client = start_client(host, xvfb.display)
            client.send(f"show {s1}")
            client.expect("state", "subscribed")
            state = ui(client, lambda u: len(u["rows"]) == 3 and u["rows"][0]["label"] == "Work" and u["rows"][2]["approvals"])
            rows = {r["session"]: r for r in state["rows"]}
            assert [r["session"] for r in state["rows"]] == [s1, s2, s3]                # the host's order, whatever the state does
            assert rows[s1]["state_text"] == "idle" and rows[s2]["state_text"] == "working"
            assert rows[s3]["state_text"] == "waiting for approval" and rows[s3]["approvals"][0][1] == "run: ls -la"
            assert rows[s1]["label"] == rows[s3]["label"] == "Work" and rows[s2]["label"] == "Personal" and rows[s1]["model"] == "simulator"
            assert state["header"] == "claude | Work | simulator | idle | mode: asks for approval" and state["selected"] == s1
            # the hidden sessions have no view and no subscriber: the list comes from the host's status alone
            assert status_of(conn, s3)["subscribers"] == 0 and status_of(conn, s2)["subscribers"] == 0
            # a click on the row of the hidden session selects it, and its approval is offered
            row = [r for r in state["layout"]["rows"] if r[2] == s3][0]       # the list is grouped by provider and account, so its order is not the host's
            click(xvfb.display, 100, (row[0] + row[1]) / 2)
            state = ui(client, lambda u: u["selected"] == s3 and u["approval_bar"])
            assert state["approval_bar"].endswith("run: ls -la") and state["header"].startswith("claude | Work | simulator | waiting for approval")
            deny = state["layout"]["deny"]
            click(xvfb.display, (deny[0] + deny[2]) / 2, (deny[1] + deny[3]) / 2)
            wait_for(lambda: responses(received))
            time.sleep(1.2)
            first = responses(received)
            assert len(first) == 1 and first[0]["request_id"] == "req-7" and first[0]["response"]["behavior"] == "deny"   # one answer, for that request
            state = ui(client, lambda u: u["approval_bar"] and "Write" in u["approval_bar"])                                 # the next request is offered next
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_older_history_is_read_on_demand_without_replaying_the_transcript():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            conn.call(op="input", session=sid, generation=generation, request_id="many", text="lines:3000")
            wait_for(lambda: "[turn completed]" in records(log, sid), timeout=60)
            client = start_client(host, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            state = ui(client, lambda u: u["lines_buffered"] > 0)
            assert state["history_requests"] == 0 and state["lines_buffered"] <= 100        # a screen, not the transcript
            for _ in range(25):                                                              # the wheel, 3 lines a notch
                click(xvfb.display, 600, 300, 4)
            state = ui(client, lambda u: u["history_requests"] >= 1 and u["history_lines"] >= 100 and u["scroll"] > 0)
            for _ in range(60):
                click(xvfb.display, 600, 300, 4)
            state = ui(client, lambda u: u["history_requests"] >= 3)
            assert state["history_lines"] <= 700 and state["lines_buffered"] < 800          # what was scrolled to, never all 3000 records
            client.send("lines")
            shown = client.expect("lines")["lines"]
            first_seq = state["first_seq"]
            assert first_seq < 2900 and shown[0].startswith(f"line {first_seq - 2:05d}"), (first_seq, shown[:2])   # the older text matches its sequence number (the echo of the input and the start note come first)
            # back at the bottom the live end is there again
            xdo(xvfb.display, "key", "End")
            state = ui(client, lambda u: u["scroll"] == 0)
            client.send("lines")
            tail = client.expect("lines")["lines"][-2:]
            assert tail[0].startswith("line 02999") and tail[1] == "[turn completed]"      # the real end of the transcript, fetched again
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_only_selected_text_reaches_the_clipboard_and_only_on_a_user_action():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = reader = None
        try:
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            conn.call(op="input", session=sid, generation=generation, request_id="l", text="lines:30")
            wait_for(lambda: "[turn completed]" in records(log, sid))
            client = start_client(host, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            reader = PixelReader(xvfb.display)
            state = ui(client, lambda u: len(u["layout"]["lines"]) >= 10)
            assert reader.selection_owner("CLIPBOARD") == 0 and reader.selection_owner("PRIMARY") == 0
            lines = state["layout"]["lines"]
            y0, y1 = (lines[5][0] + lines[5][1]) / 2, (lines[7][0] + lines[7][1]) / 2
            wid = window_id(xvfb.display)
            xdo(xvfb.display, "mousemove", "--window", wid, "500", str(int(y0)), "mousedown", "1", "mousemove", "--window", wid, "500", str(int(y1)), "mouseup", "1")
            state = ui(client, lambda u: u["selection"])
            assert state["selection"].count("\n") == 2 and all(part.startswith("line ") for part in state["selection"].split("\n"))
            assert reader.selection_owner("CLIPBOARD") == 0                                  # selecting is not copying
            assert state["copied"] == 0
            xdo(xvfb.display, "key", "ctrl+shift+c")                                          # the user's own chord
            wait_for(lambda: reader.selection_owner("CLIPBOARD") != 0)
            assert reader.selection_text("CLIPBOARD") == state["selection"]                   # exactly the selected lines, nothing else
            assert ui(client)["copied"] == 1 and reader.selection_owner("PRIMARY") == 0
            # with nothing selected the chord copies nothing, and the clipboard keeps what it had
            xdo(xvfb.display, "key", "Escape")
            ui(client, lambda u: u["selection"] is None)
            xdo(xvfb.display, "key", "ctrl+shift+c")
            time.sleep(0.5)
            assert ui(client)["copied"] == 1 and reader.selection_text("CLIPBOARD") == state["selection"]
        finally:
            if reader:
                reader.close()
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_a_flood_of_lines_does_not_keep_the_client_at_full_cpu_and_the_text_stays_current():
    from lico_measure.p02 import start
    from lico_measure.procscope import cpu_seconds, tree_pids

    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            # a session producing 100 agent-shaped lines a second, shown in the first view
            host, log = start(directory, extra=["--text", "--payload", "110"], sessions=1, producing=1, rate=100, history=0)
            sid = host.sessions[0]
            client = start_client(host, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(2.0)
            pids = tree_pids(client.proc.pid)
            before, began = cpu_seconds(pids, strict=True), time.monotonic()
            time.sleep(6.0)                                                         # a mean over six seconds of a steady flood
            percent = (cpu_seconds(pids, strict=True) - before) / (time.monotonic() - began) * 100
            assert percent < 55, f"{percent:.0f}% of a core while 100 lines a second arrive"
            # capped is not stale: the lines shown are within a second of the host's newest
            newest = Conn(host.socket).call(op="status")["sessions"][0]["seq"]
            client.send("lines")
            shown = client.expect("lines")["lines"]
            last_shown = int(shown[-1][:6])
            assert newest - last_shown <= 120, (newest, last_shown)
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
