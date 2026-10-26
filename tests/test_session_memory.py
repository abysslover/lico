"""M02-P02-S06: the memory a row shows is what the agent and everything it started hold of their own. A simulator session holds a known amount in
the agent, in a child and in that child's child; the host's figure, the row, the hint line under the mouse and the total line carry it."""
import re

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol import v1

from tests.test_activity import begin
from tests.test_claude_adapter import new_claude, status_of
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args

OWN, CHILD, GRANDCHILD = 60, 40, 20                  # MiB the agent, its child and its grandchild touch and keep
HELD_KB = (OWN + CHILD + GRANDCHILD) << 10
SLACK_KB = 24 << 10                                  # three Python interpreters hold a few MiB of their own besides what they were told to hold
FILE_KB = 8 << 10                                    # what an interpreter maps of files (itself, libc, its modules) and counts in RSS only: measured 40 MiB or more


def test_a_session_shows_what_its_agent_its_child_and_its_grandchild_hold_in_the_row_the_hint_and_the_total_line():
    with private_dir() as directory:
        args, _ = claude_args(directory)
        made = {}

        def session(w):
            made["sid"], made["generation"] = new_claude(Conn(w.socket), name="memo")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=session) as w:
            conn, sid, generation = Conn(w.socket), made["sid"], made["generation"]
            idle = wait_for(lambda: status_of(conn, sid).get("memory"))                 # the window asks every second; each ask names the agent for the next look
            assert idle["processes"] == 1 and 0 < idle["private_kb"] <= idle["rss_kb"] < HELD_KB, idle
            assert idle["rss_kb"] - idle["private_kb"] >= FILE_KB, idle                  # the interpreter's own libraries and files are in RSS and not in the figure: it is not VmRSS
            begin(conn, sid, generation, f"hold:{OWN}:{CHILD}:{GRANDCHILD}")
            held = wait_for(lambda: (m := status_of(conn, sid).get("memory")) and m["processes"] == 3 and m["private_kb"] - idle["private_kb"] >= HELD_KB * 0.9 and m)
            gained = held["private_kb"] - idle["private_kb"]
            assert HELD_KB <= gained <= HELD_KB + SLACK_KB, (gained >> 10, HELD_KB >> 10)      # every page was touched: nothing counts less than it holds
            assert held["rss_kb"] - held["private_kb"] >= FILE_KB, held                  # three interpreters, each with its mapped files (the slack above would hide them)
            reply = conn.call(op="status")
            assert v1.validate_response("status", reply) == [] and reply["host_memory"]["processes"] == 1 and reply["host_memory"]["private_kb"] > 0

            expected = held["private_kb"] >> 10                                         # MiB; the row rounds a figure above 100 to the nearest ten
            near = lambda text: (m := re.search(r"(\d+)M", text)) and abs(int(m.group(1)) - expected) <= 15        # the nearest ten, and a wobble of the figure between two reads
            wait_for(lambda: re.search(r"\t\d+M", w.ui()["list_text"]) and near(w.ui()["list_text"].split("\t")[-1]))
            total = wait_for(lambda: near(w.ui()["memory"]["total"]) and w.ui()["memory"]["total"])
            assert total.startswith("mem sessions ") and " host " in total and " window " in total, total

            top, bottom, _ = next(r for r in w.ui()["layout"]["rows"] if r[2] == sid)
            w.move(100, (top + bottom) / 2)                                             # the pointer over the row: the hint line tells what the figure is
            state = w.ui()
            assert state["memory"]["hover"] == sid and " own (RSS " in state["hint"] and "3 processes" in state["hint"] and "detached" in state["hint"] and "shared between processes" in state["hint"], state["hint"]
            w.move(w.ui()["layout"]["list_w"] + 200, 300)
            assert w.ui()["memory"]["hover"] is None and " own (RSS " not in w.ui()["hint"]

            assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1
            assert "memory" not in status_of(conn, sid), "a session that is closed holds nothing"
