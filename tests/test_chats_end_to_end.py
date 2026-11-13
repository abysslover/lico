"""M10-P04: the whole M10 flow in one window on mocked providers (no billable turn): the three tabs are named Directories, Chats and Calls; the Chats tab, `lico chat
latest` and the New chat dialog read the same latest chat of the folder; Resume opens a tab whose transcript is that chat and the Chats tab then shows it open; the old
name `session.past` is an ordinary unknown operation."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _records, _seed_claude_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


class _Host:
    """`cli` wants a host with a socket: the window's host has one."""

    def __init__(self, socket: str) -> None:
        self.socket = socket


def test_the_chats_tab_the_command_and_the_new_chat_dialog_agree_and_resume_opens_the_latest_chat():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-e2e").resolve()
        proj.mkdir(mode=0o700)
        log_dir = root / "logs"
        c_args, c_rec = claude_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "ClaudeWork", "claude")
            home = Path(w.directory) / "state" / "lico" / "accounts" / acc / "claude"
            _seed_claude_jsonl(home, "proj-e2e", "cl-e2e-old", str(proj), "Older e2e task", "older answer", 1_760_000_100)
            _seed_claude_jsonl(home, "proj-e2e", "cl-e2e-new", str(proj), "Newest e2e task", "newest answer", 1_760_000_900)
            opened = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(proj), name="Here")
            assert opened["ok"] == 1, opened
            made["acc"] = acc

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(log_dir)], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u["sidebar"]["shown"] is True and u)
            tab = u0["tabs"]["items"][0]
            w.click((tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0)
            wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(proj) and u)

            # 1. the three tabs and what the Chats tab lists for this folder
            sb = w.ui()["sidebar"]
            assert sb["tab_labels"] == ["Directories", "Chats", "Calls"]
            w.click(*_centre(sb["chats_tab_rect"]))
            listed = wait_for(lambda: (u := w.ui()["sidebar"]) and u["tab"] == "chats" and u["chats_loading"] is False and u["chats_total"] >= 2 and u)
            ids = [e["provider_session"] for e in listed["chats_entries"] if e["provider_session"].startswith("cl-e2e")]
            assert ids == ["cl-e2e-new", "cl-e2e-old"], ids
            w.click(*_centre(listed["dir_tab_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["tab"] == "directories")

            # 2. the command's latest chat is the newest one that has a transcript (the open blank tab is not a chat to continue)
            latest = json.loads(cli(_Host(w.socket), "chat", "latest", "--cwd", str(proj), "--account", made["acc"]).stdout)
            assert [i["provider_session"] for i in latest["items"]] == ["cl-e2e-new"] and latest["items"][0]["turns"] == 2, latest

            # 3. the dialog shows the same chat and Resume opens a tab whose transcript is that chat
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["last_line"].startswith("Last Session") and u)
            assert nc["last_line"].endswith("Newest e2e task (2 turns)"), nc["last_line"]
            before = len(argv_of(c_rec))
            subprocess.run(["xdotool", "key", "Return"], env=w.env, check=True)
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None and len(w.ui()["tabs"]["items"]) == 2)
            wait_for(lambda: len(argv_of(c_rec)) == before + 1 and "cl-e2e-new" in argv_of(c_rec)[-1])
            opened = next(t for t in w.ui()["tabs"]["items"] if t["session"] != u0["tabs"]["items"][0]["session"])
            wait_for(lambda: any(">> Newest e2e task" in r for r in _records(log_dir, opened["session"])))

            # 4. the Chats tab now shows that chat as open, and the old name is gone
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            again = wait_for(lambda: (u := w.ui()["sidebar"]) and u["tab"] == "chats" and u["chats_loading"] is False
                             and any(e["provider_session"] == "cl-e2e-new" and e["open_session"] == opened["session"] for e in u["chats_entries"]) and u)
            assert again["tab"] == "chats"
            gone = Conn(w.socket).call(op="session.past", **{"global": True})
            assert gone["ok"] == 0 and gone["error"] == "unknown_op", gone
