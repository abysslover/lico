"""M24-P11f in the window: the status of a conversation stands at the bottom only. The first line of the conversation is the first row under a border of its own (a rule of two units in a
lighter tone and a margin of four under the tab bar: `layout.chat_top`), the strip above the path bar begins with what the top line said (provider, account, state: `telemetry.status_text`)
and keeps the folder's room, and the dump's `header` still carries the text of the status. A private Xvfb and the simulated agent."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, records, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def test_the_conversation_has_no_line_of_status_at_its_top_and_the_strip_at_the_bottom_says_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, object] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            sid, gen = new_claude(conn, account=acc, mode="auto", name="edge")
            assert conn.call(op="input", session=sid, generation=gen, request_id="q1", text="First question")["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and "[turn completed]" in records(log_dir, sid))
            made.update(sid=sid)

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w:
            sid = str(made["sid"])
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u)
            tab0 = u0["tabs"]["items"][0]
            w.click((tab0["rect"][0] + tab0["close_rect"][0]) / 2.0, (tab0["rect"][1] + tab0["rect"][3]) / 2.0)
            u = wait_for(lambda: (x := w.ui())["selected"] == sid and len(x["shown_lines"]) >= 2 and x["telemetry"]["status_text"] and x["layout"]["lines"] and x)

            # the first line of the conversation is the first row under the border: nothing of status stands between them
            chat_top = u["layout"]["chat_top"]
            assert abs(u["layout"]["lines"][0][0] - chat_top) < 0.5, (u["layout"]["lines"][0], chat_top)
            font = u["settings"]["font_px"]
            menu_h = u["menu"]["titles"][0]["rect"][3] - u["menu"]["titles"][0]["rect"][1]
            scale = menu_h / (font + 10)
            tabs_bottom = chat_top - 6 * scale
            assert abs(u["tabs"]["items"][0]["rect"][3] - tabs_bottom) <= 2 * scale, ("the tabs end where the border begins", u["tabs"]["items"][0]["rect"], tabs_bottom)
            # the border is drawn: the same two rows from the list's edge to the window's, a different ground under the margin
            data, width, _height = w.gray()
            x_a, x_b = int(u["layout"]["list_w"]) + 100, width - 100
            for y in (int(tabs_bottom), int(tabs_bottom + 2 * scale) - 1):
                assert data[y * width + x_a] == data[y * width + x_b], ("the rule at row", y)
                assert data[y * width + x_a] != data[int(chat_top) * width + x_a], ("the rule is not the chat's ground", y)
            assert data[(int(tabs_bottom) - 3) * width + x_a] != data[int(tabs_bottom) * width + x_a], "the rule is lighter than the bar over it"

            # the status is in the strip at the bottom, first; the dump's `header` still says it
            t = u["telemetry"]
            assert t["shown"] and t["status_text"].startswith("claude | Work | ") and "idle" in t["status_text"], t["status_text"]
            assert u["header"].startswith("claude | Work | "), u["header"]
            assert t["cwd_rect"] is not None, "the folder keeps its room in the strip"
            assert t["status_text"] not in "\n".join(u["shown_lines"]), "the status is not a line of the conversation"
