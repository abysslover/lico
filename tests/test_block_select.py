"""M24-P11c in the window: a click on a block of the conversation selects the whole block (not copied), a second click on it folds it to one line and a click on that line opens it again;
a command card keeps its own click. A private Xvfb and the simulated agent: nothing of the real machine is looked at."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, records, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args

QUESTION = ">> First line of a question\n>> second line of it\n>> third line of it"
REPLY = "echo: First line of a question\nsecond line of it\nthird line of it"
PILL = "▸ echo: First line of a question  / 3 lines"


def test_a_click_selects_a_whole_block_a_second_one_folds_it_and_a_third_opens_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, object] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            sid, gen = new_claude(conn, account=acc, mode="auto", name="blocks")
            assert conn.call(op="input", session=sid, generation=gen, request_id="q1", text="First line of a question\nsecond line of it\nthird line of it")["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and "[turn completed]" in records(log_dir, sid))
            made.update(sid=sid, gen=gen)

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u)
            tab0 = u0["tabs"]["items"][0]
            w.click((tab0["rect"][0] + tab0["close_rect"][0]) / 2.0, (tab0["rect"][1] + tab0["rect"][3]) / 2.0)
            u = wait_for(lambda: (u := w.ui())["selected"] == made["sid"] and len(u["shown_lines"]) >= 6 and u)
            lines = u["shown_lines"]
            assert lines[:3] == QUESTION.split("\n") and lines[3:6] == REPLY.split("\n"), lines
            assert len(u["layout"]["lines"]) == len(lines), "every line of the short conversation is on the screen"
            copied = u["copied"]

            def click_line(n: int, then):
                y0, y1, _raw = u["layout"]["lines"][n]
                w.click(u["layout"]["list_w"] + 300, (y0 + y1) / 2.0)
                return wait_for(lambda: (now := w.ui()) and then(now) and now)

            # a click on the middle line of the reply selects the reply, the whole of it, and copies nothing
            u = click_line(4, lambda s: s["selection"] == REPLY)
            first_raw, last_raw = u["blocks"]["sel"]
            assert last_raw - first_raw == 2 and u["blocks"]["folded"] == 0
            assert u["shown_lines"] == lines and u["copied"] == copied

            # a second click on it folds it to one line, which is the selection
            u = click_line(4, lambda s: s["shown_lines"][3:] == [PILL])
            assert u["shown_lines"][:3] == QUESTION.split("\n")
            assert u["blocks"] == {"folded": 1, "sel": [first_raw, first_raw]} and u["selection"] == PILL and u["copied"] == copied

            # a click on the folded line opens the block, and it is selected again, whole
            u = click_line(3, lambda s: s["shown_lines"] == lines)
            assert u["selection"] == REPLY and u["blocks"] == {"folded": 0, "sel": [first_raw, last_raw]}

            # a click on the message of the person selects it, all three lines, and the reply is no longer the selection
            u = click_line(1, lambda s: s["selection"] == QUESTION)
            assert u["shown_lines"] == lines and u["blocks"]["folded"] == 0
