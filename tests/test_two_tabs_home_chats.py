"""M22-P03: the left side of the window has two tabs (Directories, Chats), one virtual directory called Home (`lico://home`), and a Chats tab that lists every past session of this
machine newest first, whatever folder the Directories tab is on. A card names its folder; a click on it goes to that folder (Home for a session without one) at once.

What this pins down: the tab labels and that the Calls tab is gone; the Home pill's words (the window never says Global); that the list is fetched when the window starts, so the tab opens
with it; the list asks for `all` and is the same list in every folder; the second line of a card says where the session is; the click moves Directories to the card's folder and a Home
card turns Home on; the window state no longer carries the question about the provider's own home. A private Xvfb, synthetic homes, mocked providers: no real home is read."""

from __future__ import annotations

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
from tests.test_running_elsewhere_window import _centre


def test_two_tabs_a_home_pill_and_one_list_of_every_session_whose_cards_name_their_folder_and_whose_click_goes_there():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj_a, proj_b = (root / "proj-alpha").resolve(), (root / "proj-beta").resolve()
        for p in (proj_a, proj_b):
            p.mkdir(mode=0o700)
        c_args, _rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            work = account(conn, "ClaudeWork", "claude")
            state = Path(w.directory) / "state" / "lico"
            home = state / "accounts" / work / "claude"
            workspace = (state / "workspaces" / "sanon1").resolve()
            workspace.mkdir(mode=0o700, parents=True)
            now = int(time.time())
            _seed_claude_jsonl(home, "p-alpha", "cl-alpha", str(proj_a), "alpha question", "answer", now - 1200)
            _seed_claude_jsonl(home, "p-beta", "cl-beta", str(proj_b), "beta question", "answer", now - 900)
            _seed_claude_jsonl(home, "workspaces", "cl-home", str(workspace), "home question", "answer", now - 600)

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            # the list is fetched when the window starts: the tab is not even open and the list is there
            s = wait_for(lambda: (x := w.ui()["sidebar"]) and x["shown"] is True and x["tab"] == "directories" and x["chats_total"] >= 3 and x)
            assert s["chats_load_count"] >= 1 and s["chats_loading"] is False, s

            # two tabs, no Calls; the pill says Home and its virtual path, never the old word
            assert s["tab_labels"] == ["Directories", "Chats"], s["tab_labels"]
            assert s.get("calls_tab_rect") is None and "call_entries" not in s and "call_peer_starters" not in s
            assert s["home_label"] == "Home  lico://home", s["home_label"]
            assert "Global" not in " ".join([*s["tab_labels"], s["home_label"]])
            assert "native_prompt" not in s, "the question about the provider's own home is gone: the setting is on by default"

            w.click(*_centre(s["chats_tab_rect"]))
            listed = wait_for(lambda: (x := w.ui()["sidebar"]) and x["tab"] == "chats" and x["chats_loading"] is False and len(x["chats_entries"]) == 3 and x)
            assert listed["chats_scope"] == "all" and listed["chats_total"] == 3, listed
            by_id = {e["provider_session"]: e for e in listed["chats_entries"]}
            assert [e["provider_session"] for e in listed["chats_entries"]] == ["cl-home", "cl-beta", "cl-alpha"], "newest first, every folder in one list"
            assert re.fullmatch(r"10m ago · lico://home · ClaudeWork · #cl-home", by_id["cl-home"]["sub"]), by_id["cl-home"]["sub"]
            assert re.fullmatch(r"15m ago · \S+/proj-beta · ClaudeWork · #cl-beta", by_id["cl-beta"]["sub"]), by_id["cl-beta"]["sub"]
            assert re.fullmatch(r"20m ago · \S+/proj-alpha · ClaudeWork · #cl-alpha", by_id["cl-alpha"]["sub"]), by_id["cl-alpha"]["sub"]
            assert "global_rect" not in listed or listed["global_rect"] is None, "the scope rows of the old tab are gone from Chats"

            # a click goes to the card's folder at once, and the list does not change with the folder
            w.click(*_centre(by_id["cl-beta"]["rect"]))
            there = wait_for(lambda: (x := w.ui()["sidebar"]) and x["cwd"] == str(proj_b) and x["global_scope"] is False and x)
            assert there["chats_total"] == 3 and there["chats_scope"] == "all", there
            w.click(*_centre(w.ui()["sidebar"]["chats_entries"][-1]["rect"]))  # cl-alpha, the oldest, last in the list
            wait_for(lambda: (x := w.ui()["sidebar"]) and x["cwd"] == str(proj_a) and x["global_scope"] is False and x)
            home_card = next(e for e in w.ui()["sidebar"]["chats_entries"] if e["provider_session"] == "cl-home")
            w.click(*_centre(home_card["rect"]))
            home = wait_for(lambda: (x := w.ui()["sidebar"]) and x["global_scope"] is True and x["global_highlighted"] is True and x)
            assert home["chats_total"] == 3 and home["chats_scope"] == "all", home
