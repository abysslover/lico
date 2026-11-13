"""M10-P03: the "+ New chat" row above the `[..]` parent row of the Directories tab and its centered modal: choose an account, then Resume (default)
or New; Resume shows `Last Session : <provider> / <YYYY-MM-DD HH:MM> / <title> (<N> turns)` and continues the latest chat through the host's one rule;
"Options..." opens the full form. Keys: Up/Down account, Left/Right Resume/New, Enter, Esc. Mocked providers (no billable turn)."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_the_new_chat_row_and_modal_resume_new_options_and_the_keys():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        c_args, c_rec = claude_args(root)
        x_args, _x_rec = codex_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc_c, acc_x = account(conn, "ClaudeWork", "claude"), account(conn, "CodexWork", "codex")
            home = Path(w.directory) / "state" / "lico" / "accounts" / acc_c / "claude"
            _seed_claude_jsonl(home, "proj-alpha", "cl-gui-old", str(proj), "Seeded older task", "older answer", 1_760_000_100)
            _seed_claude_jsonl(home, "proj-alpha", "cl-gui-newest", str(proj), "Seeded newest task", "newest answer", 1_760_000_900)
            made.update(acc_c=acc_c, acc_x=acc_x)
            opened = conn.call(op="session.create", kind="claude", account=acc_c, mode="auto", cwd=str(proj), name="Here")
            assert opened["ok"] == 1, opened
            made["here"] = opened["session"]

        with ProductWindow(host_binary=LICO, host_args=[*c_args, *x_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u["sidebar"]["shown"] is True and u)
            tab = u0["tabs"]["items"][0]
            w.click((tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0)
            sb = wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(proj) and u["sidebar"]["parent"] and u["sidebar"])

            # the row is above the parent row, enabled, and the modal is closed
            assert sb["new_chat_rect"] and sb["parent_rect"] and sb["new_chat_rect"][3] <= sb["parent_rect"][1], sb
            assert sb["new_chat_enabled"] is True and sb["new_chat"] is None
            w.click(*_centre(sb["new_chat_rect"]))
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["open"] and u)
            assert [a["label"] for a in nc["accounts"]] == ["ClaudeWork", "CodexWork"] and nc["selected"] == 0 and nc["resume"] is True, nc
            assert nc["accounts"][0]["default"] is True and nc["rects"]["ok"] and nc["rects"]["cancel"] and nc["rects"]["options"], nc

            # Resume shows the Last Session line of the latest chat of this folder and account
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["last_line"].startswith("Last Session : claude / ") and u)
            assert re.fullmatch(r"Last Session : claude / \d{4}-\d\d-\d\d \d\d:\d\d / Seeded newest task \(2 turns\)", nc["last_line"]), nc["last_line"]

            # Down: the Codex account has no chat here; Up: back; Right and Left toggle Resume and New
            _key(w, "Down")
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["selected"] == 1 and u["last_line"].startswith("No earlier chat") and u)
            _key(w, "Up")
            wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["selected"] == 0 and u["last_line"].startswith("Last Session") and u)
            _key(w, "Right")
            assert wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["resume"] is False and u)
            _key(w, "Left")
            assert wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["resume"] is True and u)

            # Enter on Resume: a new tab that continues the latest chat (the program is started with --resume)
            before = len(argv_of(c_rec))
            _key(w, "Return")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None and len(w.ui()["tabs"]["items"]) == 2)
            wait_for(lambda: len(argv_of(c_rec)) == before + 1 and "--resume" in argv_of(c_rec)[-1] and "cl-gui-newest" in argv_of(c_rec)[-1])

            # Again, and now Resume finds that chat open in a tab: no new session
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["last_line"].startswith("Last Session") and u)
            _key(w, "Return")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None)
            assert len(w.ui()["tabs"]["items"]) == 2 and len(argv_of(c_rec)) == before + 1

            # New: a new session that continues nothing
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["new_chat"])
            _key(w, "Right")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"]["resume"] is False)
            _key(w, "Return")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None and len(w.ui()["tabs"]["items"]) == 3)
            wait_for(lambda: len(argv_of(c_rec)) == before + 2)
            assert "--resume" not in argv_of(c_rec)[-1], argv_of(c_rec)[-1]

            # Esc closes the modal and makes nothing; Options... opens the full form
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["new_chat"])
            _key(w, "Escape")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None)
            assert len(w.ui()["tabs"]["items"]) == 3
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u)
            w.click(*_centre(nc["rects"]["options"]))
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None and w.ui()["panel"] is not None)
