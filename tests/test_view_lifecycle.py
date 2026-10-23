import time

import pytest

from lico_measure.rig import (
    Client, Host, HostConn, Xvfb, private_dir, session_rgb,
)
from lico_measure.xpixel import PixelReader

CANDIDATES = ["a", "b"]


@pytest.fixture
def rig():
    with private_dir() as directory:
        xvfb, host = Xvfb(), None
        try:
            host = Host(directory, sessions=3, producing=1)
            yield xvfb, host
        finally:
            if host:
                host.stop()
            xvfb.stop()


def _wait_pixel(reader, x, y, want, timeout=5.0, equal=True):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if (reader.pixel(x, y) == want) == equal:
            return True
        time.sleep(0.02)
    return False


@pytest.mark.parametrize("name", CANDIDATES)
def test_hidden_view_releases_presentation(rig, name):
    xvfb, host = rig
    producer = host.sessions[0]
    client = Client(name, host.socket, xvfb.display)
    reader = PixelReader(xvfb.display)
    conn = HostConn(host.socket)
    try:
        client.send(f"show {producer}")
        client.expect("state", "subscribed")
        assert _wait_pixel(reader, 8, 8, session_rgb(producer)), "view never shown on screen"
        deadline = time.monotonic() + 10
        while client.state()["line_buffer_lines"] < 40 and time.monotonic() < deadline:
            time.sleep(0.1)
        visible = client.state()
        assert visible["view_resident"] and visible["subscribed"] and visible["line_buffer_lines"] == 40

        client.send("hide")
        hidden = client.expect("state", "hidden")
        assert not hidden["view_resident"] and not hidden["subscribed"]
        assert hidden["line_buffer_lines"] == 0
        assert hidden["cursor"] >= visible["cursor"]          # metadata and cursor survive
        assert _wait_pixel(reader, 8, 8, session_rgb(producer), equal=False), "stale view still shown"
        # the host no longer feeds the hidden view, yet keeps producing
        before = {s["session"]: s for s in conn.call(op="status")["sessions"]}[producer]
        time.sleep(1.0)
        after = {s["session"]: s for s in conn.call(op="status")["sessions"]}[producer]
        assert after["subscribers"] == 0
        assert after["seq"] > before["seq"]
        assert client.state()["cursor"] == hidden["cursor"]    # nothing delivered while hidden
    finally:
        client.quit()
        reader.close()
        conn.close()


@pytest.mark.parametrize("name", CANDIDATES)
def test_gui_exit_preserves_output_cursor(rig, name):
    xvfb, host = rig
    producer = host.sessions[0]
    conn = HostConn(host.socket)
    first = Client(name, host.socket, xvfb.display)
    try:
        first.send(f"show {producer}")
        first.expect("state", "subscribed")
        time.sleep(1.0)
        cursor = first.state()["cursor"]
        assert cursor >= 0
        first.kill()                                           # GUI crash, not a polite exit
        time.sleep(2.0)
        status = {s["session"]: s for s in conn.call(op="status")["sessions"]}[producer]
        assert status["subscribers"] == 0 and status["seq"] >= cursor + 150   # host kept producing

        # protocol-level replay: exactly cursor+1, cursor+2, ... with nothing skipped
        replay = HostConn(host.socket)
        reply = replay.call(op="subscribe", session=producer, after=cursor)
        assert reply["gap"] is False and reply["from"] == cursor + 1
        seqs = [replay.next_event()["seq"] for _ in range(50)]
        assert seqs == list(range(cursor + 1, cursor + 51))
        replay.close()

        second = Client(name, host.socket, xvfb.display)
        try:
            second.send(f"show {producer} {cursor}")
            second.expect("state", "subscribed")
            time.sleep(1.0)
            state = second.state()
            assert state["gaps"] == 0 and state["dups"] == 0
            assert state["cursor"] >= status["seq"]            # caught up from the cursor
        finally:
            second.quit()
    finally:
        first.kill() if first.proc.poll() is None else None
        conn.close()
