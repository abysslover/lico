"""015.1: a turn that is open shows that it is alive. The host says since when it works, when the provider last spoke, and whether it has been silent
for too long; the window shows a clock in the header, in the session list and as the last line, which changes while nothing arrives."""
import re
import time

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Xvfb, private_dir

from tests.test_claude_adapter import claude_host, new_claude, records, status_of
from tests.test_codex_adapter import wait_for
from tests.test_first_view import ui
from tests.test_frame_cost import client_on, differing_rows, pixels, show


def begin(conn, sid, generation, text, rid="t"):
    assert conn.call(op="input", session=sid, generation=generation, request_id=rid, text=text)["ok"] == 1


def completed(log, sid):
    return records(log, sid).count("[turn completed]")


def test_a_working_turn_says_since_when_it_works_and_when_the_provider_last_spoke():
    with private_dir() as directory:
        host, log, _ = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            idle = status_of(conn, sid)
            assert idle["activity"] == "idle" and "busy_since" not in idle and "last_event_at" not in idle and "stalled" not in idle
            before = int(time.time())
            begin(conn, sid, generation, "quiet:30")
            working = wait_for(lambda: (lambda e: e if e.get("activity") == "working" else None)(status_of(conn, sid)))
            assert before <= working["busy_since"] <= int(time.time()) and working["busy_since"] <= working["last_event_at"]
            assert working["stalled"] is False                                    # three seconds of quiet is thinking
            wait_for(lambda: completed(log, sid) == 1, timeout=15)
            done = status_of(conn, sid)
            assert done["activity"] == "idle" and "busy_since" not in done and "stalled" not in done
        finally:
            host.stop()


def test_a_provider_that_says_nothing_for_the_stall_time_is_reported_as_stalled_and_one_that_speaks_is_not():
    with private_dir() as directory:
        host, log, _ = claude_host(directory, env_overrides={"LICO_STALL_S": "1"})
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            begin(conn, sid, generation, "slow:40")                                # a line every 100 ms for four seconds
            wait_for(lambda: status_of(conn, sid).get("activity") == "working")
            end = time.monotonic() + 3
            while time.monotonic() < end:
                assert status_of(conn, sid).get("stalled") is False, "a provider that keeps writing is not stalled"
                time.sleep(0.1)
            wait_for(lambda: completed(log, sid) == 1, timeout=15)
            begin(conn, sid, generation, "quiet:50", "u")                          # five seconds of silence
            stalled = wait_for(lambda: (lambda e: e if e.get("stalled") else None)(status_of(conn, sid)), timeout=4)
            assert stalled["activity"] == "working" and stalled["last_event_at"] < int(time.time())
            wait_for(lambda: completed(log, sid) == 2, timeout=15)
            assert "stalled" not in status_of(conn, sid)
        finally:
            host.stop()


def test_a_session_that_waits_for_a_person_is_not_stalled():
    with private_dir() as directory:
        host, log, _ = claude_host(directory, env_overrides={"LICO_STALL_S": "1"})
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn, mode="default")
            begin(conn, sid, generation, "fixture")
            wait_for(lambda: status_of(conn, sid).get("approvals"))
            time.sleep(2.5)
            waiting = status_of(conn, sid)
            assert waiting["approvals"] and waiting["stalled"] is False            # the provider waits for an answer; that is not a hang
        finally:
            host.stop()


def changed_below_header(a, b, width=1200, list_w=300, header_rows=40):
    """The pixel rows under the header, right of the session list, that differ between two screens: the transcript's last line is there."""
    stride = width * 3
    return [y for y in range(header_rows, len(a) // stride) if a[y * stride + list_w * 3:(y + 1) * stride] != b[y * stride + list_w * 3:(y + 1) * stride]]


def clock(text):
    found = re.search(r"working (?:(\d+) m )?(\d+) s", text)
    return int(found.group(1) or 0) * 60 + int(found.group(2)) if found else None


def test_the_window_shows_a_clock_that_runs_while_nothing_arrives_and_it_goes_with_the_turn():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, _ = claude_host(directory)
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            begin(conn, sid, generation, "hello")
            wait_for(lambda: completed(log, sid) == 1)
            client = client_on(host, xvfb.display)
            show(client, sid, 3)
            state = ui(client)
            assert clock(state["header"]) is None and state["activity_line"] == ""     # nothing runs: no clock
            begin(conn, sid, generation, "quiet:100", "q")                          # ten seconds without a word
            first = ui(client, lambda u: clock(u["header"]) is not None)
            time.sleep(0.8)
            shot_a = pixels(xvfb.display)
            frames_a = first["frame"]["frames"]
            time.sleep(2.4)
            shot_b = pixels(xvfb.display)
            second = ui(client)
            assert clock(second["header"]) >= clock(first["header"]) + 2, (first["header"], second["header"])
            assert second["activity_line"].startswith("… working") and re.search(r"\(\d+ s\)", second["activity_line"]), second["activity_line"]
            assert re.search(r"working \d+s", second["list_text"]), second["list_text"]               # the session list entry too
            client.send("lines")
            shown = client.expect("lines")["lines"]
            assert not any("working" in line for line in shown), "the indicator is not a record of the transcript"
            assert differing_rows(shot_a, shot_b), "the screen did not change between two samples, with no new line"
            assert changed_below_header(shot_a, shot_b), "the last line of the transcript did not change: only the header did"
            frames = second["frame"]["frames"] - frames_a
            assert 1 <= frames <= 4 * 4, f"{frames} frames in about three seconds: an idle turn redraws at most four times a second"
            wait_for(lambda: completed(log, sid) == 2, timeout=15)
            gone = ui(client, lambda u: u["activity_line"] == "" and clock(u["header"]) is None)
            assert "idle" in gone["header"] and "working" not in gone["list_text"]
            time.sleep(1.5)                                                       # the frame that removes the line
            still_a = pixels(xvfb.display)
            time.sleep(2.2)
            assert not differing_rows(still_a, pixels(xvfb.display)), "nothing works and nothing arrives, yet the screen moves"
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_the_window_says_that_a_working_session_has_been_silent():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, _ = claude_host(directory, env_overrides={"LICO_STALL_S": "2"})
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            client = client_on(host, xvfb.display)
            show(client, sid, 0)
            begin(conn, sid, generation, "quiet:90")
            quiet = ui(client, lambda u: "no output for" in u["header"], timeout=15)
            assert re.search(r"no output for \d+ s", quiet["header"]) and "no output for" in quiet["activity_line"]
            assert re.search(r"silent \d+s", quiet["list_text"]), quiet["list_text"]
            speaking = ui(client, lambda u: "no output for" not in u["header"] and "idle" in u["header"], timeout=15)    # the turn ends with a line
            assert speaking["activity_line"] == ""
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
