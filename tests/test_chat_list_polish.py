"""M13-P03, as `M22-P04c` made it, in the window: a Chats card says when a chat was last written (a short time in a colour that follows its age), has the whole width for its title (no mark of
words takes room from it; a chat another program writes has no mark, the hint row says it), draws the provider as a picture and the state as a disc, ring or square, and never says `Native (~/.claude)`. A private Xvfb, synthetic homes,
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
            assert "native_prompt" not in w.ui()["sidebar"]  # the default home is on by default: its chats are listed and nothing is asked (M22-P03: the question is gone from the window)
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            listed = wait_for(lambda: (s := w.ui()["sidebar"]) and s["tab"] == "chats" and s["chats_loading"] is False and s["chats_total"] >= 6 and s)
            by_id = {e["provider_session"]: e for e in listed["chats_entries"]}

            live = by_id["cl-live"]
            assert live["running_elsewhere"] is True and live["state"] == "none" and live["icon"] == "claude", live  # M22-P04c: no word and no picture for a chat another program writes
            assert re.fullmatch(r"[12]m", live["age"]) and live["age_class"] == "fresh", live
            assert live["title_shown"].startswith("Refactor the billing") and live["title_line2"] and len(live["title_shown"] + live["title_line2"]) >= 40, "no mark takes the title's room; it goes on to the second line: " + repr(live)
            assert LIVE_TITLE.replace(" ", "").startswith((live["title_shown"] + live["title_line2"]).replace(" ", "").rstrip("\u2026")), live
            assert live["folder_shown"] == "", "a title on two lines leaves no room for the folder (the hint row and the path row have it)"

            for sid, age, klass in (("cl-native0", "10m", "fresh"), ("cl-native1", "3h", "today"), ("cl-native2", "2d", "week")):
                card = by_id[sid]
                assert (card["age"], card["age_class"], card["state"], card["memory"]) == (age, klass, "none", None), card
                assert card["icon"] == "claude" and (card["title_line2"] != "") != (card["folder_shown"] != ""), card  # the second line is the rest of a long title, or else the folder
                assert card["folder_shown"] == "" or card["folder_shown"].endswith("proj-alpha"), card
            closed = by_id["cl-closed"]
            assert (closed["age"], closed["age_class"], closed["state"]) == ("2h", "today", "none"), closed
            here = next(e for e in listed["chats_entries"] if e["open_session"])
            assert (here["state"], here["memory"], here["icon"]) == ("open", "ok", "claude"), here  # a disc, green while its memory is fine

            # no card has a letter or a word of state any more: only the title, the folder and the time are text
            assert all(not any(k in e for k in ("pill", "pill_line", "sub")) for e in listed["chats_entries"]), listed["chats_entries"][0]

            # the hint row says all of it for the card under the pointer
            w.move(*_centre(live["rect"]))
            hint = wait_for(lambda: (h := w.ui()["hint"]) and "billing exporter" in h and h)
            assert re.search(r"\b[12]m ago\b", hint) and "written by another program" in hint and "ClaudeWork" in hint and "#cl-live" in hint, hint
            w.move(*_centre(here["rect"]))
            hint = wait_for(lambda: (h := w.ui()["hint"]) and "open" in h and h)
            assert " \u00b7 open" in hint, hint
