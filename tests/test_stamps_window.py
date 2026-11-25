"""M24-P11a in the window: for the session it shows, the window asks the host (`stamps`) when each line it holds was written, once a second at most and one request at a time, and keeps
the answer (the state dump's `times`). `M24-P11b`: on a window wide enough the transcript keeps a gutter at the right (`gutter`, in pixels) and draws the time of the first line of each
block in it (`drawn`: top, bottom and text of each). A private Xvfb and the simulated agent: nothing of the real machine is looked at."""

from __future__ import annotations

import re
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, records, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def test_the_window_learns_when_the_lines_it_holds_were_written_and_follows_new_ones_without_polling():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, object] = {}
        began = int(time.time())

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            sid, gen = new_claude(conn, account=acc, mode="auto", name="when")
            assert conn.call(op="input", session=sid, generation=gen, request_id="q1", text="First question")["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and "[turn completed]" in records(log_dir, sid))
            made.update(sid=sid, gen=gen)

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w:
            sid, gen = str(made["sid"]), int(made["gen"])
            conn = Conn(w.socket)
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u)
            tab0 = u0["tabs"]["items"][0]
            w.click((tab0["rect"][0] + tab0["close_rect"][0]) / 2.0, (tab0["rect"][1] + tab0["rect"][3]) / 2.0)
            wait_for(lambda: (u := w.ui())["selected"] == sid and len(u["shown_lines"]) >= 2 and u)

            # the lines the view holds are covered, with times from the time of the test
            def covered():
                u = w.ui()
                t = u["times"]
                last = status_of(conn, sid)["seq"]
                return u if t["session"] == sid and t["pending"] is False and t["answered"] >= 1 and t["lo"] <= u["first_seq"] and t["hi"] == last + 1 else None
            u = wait_for(covered, timeout=15)
            t = u["times"]
            assert t["runs"] >= 1 and t["last_run"][0] <= t["hi"] - 1
            assert began - 2 <= t["first"] <= t["newest"] <= int(time.time()) + 1, (t, began)
            assert t["asked"] == t["answered"], "one request at a time, and every one answered"

            # the gutter is kept, and the times of the blocks on the screen stand in it: the local time to the second, one row per block, in the order of the lines
            def drawn_times():
                now = w.ui()["times"]
                return now if now["gutter"] > 0 and now["drawn"] else None
            shown = wait_for(drawn_times, timeout=15)
            texts = [row[2] for row in shown["drawn"]]
            assert all(re.fullmatch(r"\d{4}-\d\d-\d\d \d\d:\d\d:\d\d", text) for text in texts), texts
            seconds = [time.mktime(time.strptime(text, "%Y-%m-%d %H:%M:%S")) for text in texts]
            assert began - 2 <= min(seconds) <= max(seconds) <= time.time() + 2, (texts, began)
            assert seconds == sorted(seconds) and [row[0] for row in shown["drawn"]] == sorted(row[0] for row in shown["drawn"])
            assert len(texts) >= 2 and shown["gutter"] >= 21 * 8, "the first reply and the person's line are blocks of their own"

            # nothing is missing now: the window does not ask again by itself
            asked = t["asked"]
            time.sleep(3.5)
            assert w.ui()["times"]["asked"] == asked, "no polling while the view is complete"

            # a new line is timed: one more request, for the lines after the known ones
            time.sleep(1.1)                                                        # so that the new lines fall in another second than the old
            assert conn.call(op="input", session=sid, generation=gen, request_id="q2", text="Second question")["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and records(log_dir, sid).count("[turn completed]") >= 2)

            def followed():
                now = w.ui()["times"]
                return now if now["hi"] == status_of(conn, sid)["seq"] + 1 and now["pending"] is False else None
            after = wait_for(followed, timeout=15)
            assert after["asked"] > asked and after["asked"] == after["answered"]
            assert after["lo"] == t["lo"] and after["newest"] > t["newest"], "the old range stays and the new lines have a later time"
            assert after["runs"] > t["runs"]
