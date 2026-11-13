"""M11-P02 in the window: the Chats tab marks a chat that another program is writing, the `+ New chat` dialog says so and refuses Resume (New works), and an empty Resume
says how many chats of the folder are under other accounts and homes. A private Xvfb, synthetic homes, mocked providers: no real terminal or home is looked at."""

from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_the_window_marks_a_live_chat_refuses_its_resume_allows_new_and_hints_where_the_other_chats_are():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        c_args, c_rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            work, _empty = account(conn, "ClaudeWork", "claude"), account(conn, "EmptyOne", "claude")
            home = Path(w.directory) / "state" / "lico" / "accounts" / work / "claude"
            live = _seed_claude_jsonl(home, "p", "cl-live", str(proj), "chat that another terminal writes", "answer", int(time.time()))
            os.utime(live, (time.time(), time.time()))
            _seed_claude_jsonl(home, "p", "cl-closed", str(proj), "chat that is closed", "answer", 1_760_000_100)
            _seed_claude_jsonl(Path(w.directory) / "home" / ".claude", "p", "cl-native", str(proj), "chat of the default home", "answer", 1_760_000_050)
            opened = conn.call(op="session.create", kind="claude", account=work, mode="auto", cwd=str(proj), name="Here")
            assert opened["ok"] == 1, opened

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u = wait_for(lambda: (x := w.ui()) and len(x["tabs"]["items"]) == 1 and x["sidebar"]["shown"] is True and x)
            tab = u["tabs"]["items"][0]
            w.click((tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0)
            wait_for(lambda: (x := w.ui()) and x["sidebar"]["cwd"] == str(proj) and x["sidebar"]["parent"] and x)
            wait_for(lambda: w.ui()["sidebar"]["native_prompt"])
            _key(w, "l")  # the default home has a chat: the first-run question comes; not now
            wait_for(lambda: w.ui()["sidebar"]["native_prompt"] is None)

            # the Chats tab marks the chat that is being written, not the closed one
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            listed = wait_for(lambda: (s := w.ui()["sidebar"]) and s["tab"] == "chats" and s["chats_loading"] is False and s["chats_total"] >= 2 and s)
            by_id = {e["provider_session"]: e for e in listed["chats_entries"]}
            assert by_id["cl-live"]["running_elsewhere"] is True and by_id["cl-closed"]["running_elsewhere"] is False, by_id
            w.click(*_centre(listed["dir_tab_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["tab"] == "directories")

            # the dialog says so, refuses Resume, and New is allowed
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["last_line"].startswith("Last Session") and u)
            assert nc["selected"] == 0 and nc["accounts"][0]["label"] == "ClaudeWork", nc
            assert "running in another terminal; it cannot be resumed from here" in nc["last_line"] and "chat that another terminal writes" in nc["last_line"], nc["last_line"]
            before_tabs, before_starts = len(w.ui()["tabs"]["items"]), len(argv_of(c_rec))
            _key(w, "Return")
            refused = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["last_line"].startswith("This chat is running in another terminal") and u)
            assert refused["open"] and len(w.ui()["tabs"]["items"]) == before_tabs and len(argv_of(c_rec)) == before_starts, "nothing was started"
            _key(w, "Right")
            wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["resume"] is False and u["last_line"].startswith("New starts"))
            _key(w, "Return")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None and len(w.ui()["tabs"]["items"]) == before_tabs + 1)
            wait_for(lambda: len(argv_of(c_rec)) == before_starts + 1)
            assert "--resume" not in argv_of(c_rec)[-1], argv_of(c_rec)[-1]

            # an account without a chat here: Resume would open a new one, and the hint says where the other chats are
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["new_chat"])
            _key(w, "Down")
            hint = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["selected"] == 1 and u["last_line"].startswith("No earlier chat") and u)
            assert "3 chats of this folder exist under other accounts or homes (" in hint["last_line"] and "ClaudeWork: 2" in hint["last_line"] and "Native (~/.claude): 1" in hint["last_line"], hint["last_line"]
            assert "chat of the default home" not in hint["last_line"], "counts only, no title"
