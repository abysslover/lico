"""M24-P11e in the window: a file name in the conversation that is really there is underlined under the pointer and a click opens it in the file viewer (at its line when the name carries
one); a folder opens as a list; a name that is not there is no link; a click elsewhere keeps the meaning it had. A private Xvfb and the simulated agent, which says the message again."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, records, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def test_a_name_that_is_there_is_underlined_under_the_pointer_and_a_click_opens_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        notes, sub, gone, blob = root / "notes.txt", root / "sub", root / "gone.txt", root / "blob.bin"
        notes.write_text("".join(f"row {n}\n" for n in range(1, 61)))
        sub.mkdir()
        (sub / "inner.txt").write_text("inner\n")
        blob.write_bytes(bytes([0, 1, 2, 3, 255, 254] * 20))
        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, object] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            sid, gen = new_claude(conn, account=acc, mode="auto", name="links")
            # (the simulated agent shows a message up to some 200 characters: one message for the three names and one for the fourth)
            for n, text in enumerate([f"see {notes}:12 and {sub} and {gone} please", f"and {blob} please"], 1):
                assert conn.call(op="input", session=sid, generation=gen, request_id=f"q{n}", text=text)["ok"] == 1
                wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and records(log_dir, sid).count("[turn completed]") >= n)
            made.update(sid=sid)

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w:
            sid = str(made["sid"])
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u)
            tab0 = u0["tabs"]["items"][0]
            w.click((tab0["rect"][0] + tab0["close_rect"][0]) / 2.0, (tab0["rect"][1] + tab0["rect"][3]) / 2.0)
            u = wait_for(lambda: (x := w.ui())["selected"] == sid and len(x["links"]["names"]) >= 8 and x)
            assert u["links"]["armed"] and u["links"]["hover"] is None, "a name is found in the text, and nothing is hovered while the pointer is elsewhere"

            def last(path: Path) -> dict:
                found = [n for n in w.ui()["links"]["names"] if n["name"] == str(path)]
                assert len(found) == 2, f"the message and the agent's reply both name {path}: {found}"
                return found[-1]

            def middle(n: dict) -> tuple[float, float]:
                r = n["rect"]
                return r[0] + 4.0, (r[1] + r[3]) / 2.0

            assert last(notes)["line"] == 12 and last(gone)["line"] is None

            # a name that is not there is no link, though it is found in the text
            w.move(*middle(last(gone)))
            assert w.ui()["links"]["hover"] is None, "a name of a file that does not exist is not underlined"

            # the file: the pointer over it underlines it, from the name's first glyph to its last, one pixel row under the text
            w.move(*middle(last(notes)))
            hover = wait_for(lambda: (x := w.ui()["links"]["hover"]) and x)
            assert (hover["name"], hover["kind"], hover["line"]) == (str(notes), "file", 12), hover
            rect, under = last(notes)["rect"], hover["underline"][0]
            assert abs(under[0] - rect[0]) <= 1 and abs(under[2] - rect[2]) <= 1 and rect[1] < under[1] < rect[3], (under, rect)

            # the folder: a link as well
            w.move(*middle(last(sub)))
            hover = wait_for(lambda: (x := w.ui()["links"]["hover"]) and x)
            assert (hover["name"], hover["kind"]) == (str(sub), "folder"), hover

            # a click on the file opens it in the viewer, from its line 12
            w.click(*middle(last(notes)))

            def viewer(mode: str):
                return lambda: (p := w.ui()["panel"]) and p.get("open") and p["files"] and p["files"]["mode"] == mode and p["files"]

            files = wait_for(viewer("preview"))
            assert files["preview"]["path"] == str(notes.resolve()) and files["preview"]["lines_total"] == 60 and files["preview"]["top"] == 11, files["preview"]
            assert w.ui()["links"]["hover"] is None, "the hover goes with the conversation"

            # back to the conversation, and a click on the folder opens its list
            w.key("Escape")
            u = wait_for(lambda: (x := w.ui()) and not (x["panel"] or {}).get("open") and len(x["links"]["names"]) >= 8 and x)
            w.click(*middle(last(sub)))
            files = wait_for(viewer("dir"))
            assert files["cwd"] == str(sub.resolve()) and [e["name"] for e in files["entries"]] == ["inner.txt"], files
            w.key("Escape")
            wait_for(lambda: (x := w.ui()) and not (x["panel"] or {}).get("open") and len(x["links"]["names"]) >= 8)

            # a file that is not text opens in the viewer too, which says so; nothing is handed to a program of the system
            w.click(*middle(last(blob)))
            files = wait_for(viewer("preview"))
            assert files["preview"]["path"] == str(blob.resolve()) and files["preview"]["binary"] is True, files["preview"]
            w.key("Escape")
            wait_for(lambda: (x := w.ui()) and not (x["panel"] or {}).get("open") and len(x["links"]["names"]) >= 8)

            # a click on the same line away from the name is the block's, as before
            rect = last(notes)["rect"]
            beside = (rect[2] + 12.0, (rect[1] + rect[3]) / 2.0)  # a space after the name, on the same row of the same line
            w.move(*beside)
            assert w.ui()["links"]["hover"] is None, "the pointer is not over a name"
            w.click(*beside)
            after = wait_for(lambda: (x := w.ui()) and x["selection"] and x)
            assert not (after["panel"] or {}).get("open"), "no viewer for a click that is not on a name"
