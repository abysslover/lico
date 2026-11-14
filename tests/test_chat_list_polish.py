"""M13-P03 in the window: the Chats tab says when a chat was last written where it can be read, shows the whole title of a chat that another program is writing (its `[elsewhere]` mark moves
to the second line), and does not repeat `Native (~/.claude)` on every row (the default home says nothing; another account or an ssh host is named). A private Xvfb, synthetic homes,
mocked providers: no real terminal or home is looked at. The running-elsewhere rule itself is the one of M11 and is not tested again here."""

from __future__ import annotations

import os
import re
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_running_elsewhere_window import _centre, _key

LIVE_TITLE = "Refactor the billing exporter so that the nightly job can be resumed"


def test_the_list_says_when_each_chat_was_written_shows_a_live_chats_title_and_names_no_native_home():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        c_args, _rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            work = account(conn, "ClaudeWork", "claude")
            home = Path(w.directory) / "state" / "lico" / "accounts" / work / "claude"
            now = int(time.time())
            live = _seed_claude_jsonl(home, "p", "cl-live", str(proj), LIVE_TITLE, "answer", now)
            os.utime(live, (time.time() - 90, time.time() - 90))  # written 90 s ago: another program is writing it
            _seed_claude_jsonl(home, "p", "cl-closed", str(proj), "Chat that is closed", "answer", now - 7200)
            native = Path(w.directory) / "home" / ".claude"
            for n, (title, age) in enumerate([("Fix the flaky test in the picture paste", 600), ("Explain the build id hash", 3 * 3600), ("Rename the sidebar groups", 2 * 86400)]):
                _seed_claude_jsonl(native, "p", f"cl-native{n}", str(proj), title, "answer", now - age)
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
            _key(w, "y")  # use the default home: its chats are listed
            wait_for(lambda: w.ui()["sidebar"]["native_prompt"] is None)
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            listed = wait_for(lambda: (s := w.ui()["sidebar"]) and s["tab"] == "chats" and s["chats_loading"] is False and s["chats_total"] >= 6 and s)
            by_id = {e["provider_session"]: e for e in listed["chats_entries"]}

            live = by_id["cl-live"]
            assert live["running_elsewhere"] is True and live["pill"] == "[elsewhere]" and live["pill_line"] == 2, live
            assert re.fullmatch(r"\[elsewhere\] written (\d+s|\d+m) ago(?: · ClaudeWork)?", live["sub"]), live["sub"]
            assert live["title_shown"].startswith("Refactor the billing exporter so") and len(live["title_shown"]) >= 30, "the mark no longer takes the title's room: " + live["title_shown"]

            for sid, age in (("cl-native0", r"10m ago"), ("cl-native1", r"3h ago"), ("cl-native2", r"2d ago")):
                sub = by_id[sid]["sub"]
                assert re.fullmatch(age + r" · #" + sid[:8], sub), sub
                assert "Native" not in sub and by_id[sid]["pill"] is None, by_id[sid]
            closed = by_id["cl-closed"]
            assert re.fullmatch(r"2h ago · ClaudeWork · #cl-close", closed["sub"]), closed["sub"]
            assert all("Native" not in e["sub"] for e in listed["chats_entries"]), [e["sub"] for e in listed["chats_entries"]]
            here = next(e for e in listed["chats_entries"] if e["open_session"])
            assert here["pill"] == "[open]" and here["pill_line"] == 1, here
