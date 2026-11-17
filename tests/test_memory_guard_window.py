"""M19-P01: the memory guard in the window. A simulator session holds a known amount in its agent; its figure turns orange at 400 MB and red at 500 MB by the largest process, and a click
on a RED session asks once whether to end it and resume it (the conversation resumes; tool jobs that were running are lost). Pictures go to LICO_M19_SCREENS when it is set."""
import os
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_activity import begin
from tests.test_claude_adapter import new_claude, status_of
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args

SCREENS = Path(os.environ["LICO_M19_SCREENS"]) if os.environ.get("LICO_M19_SCREENS") else None


def shot(w, name):
    if SCREENS:
        SCREENS.mkdir(parents=True, exist_ok=True)
        w.png(SCREENS / name)


def holding(mib):
    """A window with one simulator session whose agent holds `mib` MiB; returns (window context, session id)."""
    directory_cm = private_dir()
    return directory_cm


def run(mib, check):
    with private_dir() as directory:
        args, _ = claude_args(directory)
        made = {}

        def session(w):
            made["sid"], made["generation"] = new_claude(Conn(w.socket), name="heavy")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=session) as w:
            conn, sid, generation = Conn(w.socket), made["sid"], made["generation"]
            begin(conn, sid, generation, f"hold:{mib}:0:0")
            figure = wait_for(lambda: (m := status_of(conn, sid).get("memory")) and m.get("max_rss_kb", 0) >= mib * 1024 * 0.95 and m)
            check(w, conn, sid, figure)


def centre(r):
    return (r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0


def tab_rect(w, sid):
    return next(t["rect"] for t in w.ui()["tabs"]["items"] if t["session"] == sid)


def settle(w):
    import time
    time.sleep(1.2)      # the window redraws after the model changed; let the picture catch up


def test_a_session_holding_450_mb_is_orange_and_a_click_asks_nothing():
    def check(w, conn, sid, figure):
        assert 400 * 1024 <= figure["max_rss_kb"] < 500 * 1024, figure
        w.click(*centre(tab_rect(w, sid)))
        wait_for(lambda: w.ui()["selected"] == sid)
        wait_for(lambda: "M" in w.ui()["memory"]["total"] and int(w.ui()["memory"]["total"].split("sessions ")[1].split("M")[0]) >= 400)
        settle(w)
        assert w.ui()["panel"] is None, "orange asks nothing"
        shot(w, "1_orange.png")
    run(450, check)


def test_a_click_on_a_red_session_asks_once_and_end_and_resume_gives_a_clean_process():
    def check(w, conn, sid, figure):
        assert figure["max_rss_kb"] >= 500 * 1024, figure
        w.click(*centre(tab_rect(w, sid)))
        panel = wait_for(lambda: w.ui()["panel"])
        text = str(panel)
        assert "limit 500" in text and "End it and continue" in text and "tool jobs that were running are lost" in text, text
        settle(w)
        shot(w, "2_red_question.png")
        w.key("n")                                         # cancel: the session opens as usual and stays as it is
        wait_for(lambda: w.ui()["panel"] is None)
        assert status_of(conn, sid)["state"] == "running" and status_of(conn, sid)["memory"]["max_rss_kb"] >= 500 * 1024
        w.click(*centre(tab_rect(w, sid)))
        import time
        time.sleep(0.8)
        assert w.ui()["panel"] is None, "once for each red episode"
        settle(w)
        shot(w, "3_red_after_cancel.png")
        # a new episode: leave red (end and resume below does it); here the person asks again through a second session click after the figure has dropped
        reply = conn.call(op="session.close", session=sid, generation=status_of(conn, sid)["generation"], by="guard")
        assert reply["ok"] == 1 and status_of(conn, sid)["closed_by"] == "guard", reply
        reply = conn.call(op="session.resume", session=sid, generation=reply["generation"])
        assert reply["ok"] == 1 and "closed_by" not in status_of(conn, sid), reply
        wait_for(lambda: (m := status_of(conn, sid).get("memory")) and m["max_rss_kb"] < 400 * 1024 and m)
    run(520, check)


def test_y_on_the_question_ends_the_session_and_resumes_it_to_a_clean_process():
    def check(w, conn, sid, figure):
        before = status_of(conn, sid)
        w.click(*centre(tab_rect(w, sid)))
        wait_for(lambda: w.ui()["panel"])
        w.key("y")
        wait_for(lambda: w.ui()["panel"] is None)
        after = wait_for(lambda: (s := status_of(conn, sid)) and s["state"] == "running" and s.get("memory") and s["memory"]["max_rss_kb"] < 400 * 1024 and s)
        assert after["generation"] > before["generation"] and "closed_by" not in after, (before["generation"], after)
        settle(w)
        shot(w, "4_after_end_and_resume.png")
    run(520, check)
