"""M11-P03: the whole M11 flow in one window on synthetic homes, a private socket and mocked providers (no billable turn; the owner's real homes are never read): a person who
used Claude Code before Lico finds those chats again. The first-run question, Yes, the account list with Native, the latest Native chat, a Claude process in the folder
(a fake: a script named `claude`) that is writing it (listed `[elsewhere]`, Resume refused, New allowed), a closed chat of the same folder that resumes in the default home
with the provider's own login (the process environment is the default home's, nothing of a managed account), and the hint of an account that has no chat here."""

from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_accounts import environ_of
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_earlier_chats_are_found_again_through_native_and_a_chat_another_terminal_writes_cannot_be_resumed():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        c_args, c_rec = claude_args(root)
        fake = root / "fakebin" / "claude"
        fake.parent.mkdir()
        fake.write_text("#!/bin/sh\ni=0\nwhile [ $i -lt 600 ]; do sleep 1; i=$((i+1)); done\n")
        fake.chmod(0o755)
        terminal = subprocess.Popen([str(fake)], cwd=proj)  # "Claude Code in another terminal", in the project folder
        made: dict[str, str] = {}
        try:
            def setup(w: ProductWindow) -> None:
                conn = Conn(w.socket)
                made["work"], made["empty"] = account(conn, "ClaudeWork", "claude"), account(conn, "EmptyOne", "claude")
                native = Path(w.directory) / "home" / ".claude"
                _seed_claude_jsonl(native, "p", "cl-before-lico", str(proj), "work I did before Lico", "an answer", 1_760_000_100)
                live = _seed_claude_jsonl(native, "p", "cl-open-terminal", str(proj), "work the other terminal writes", "an answer", int(time.time()))
                os.utime(live, (time.time(), time.time()))
                opened = conn.call(op="session.create", kind="claude", account=made["work"], mode="auto", cwd=str(proj), name="Here")
                assert opened["ok"] == 1, opened

            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                u = wait_for(lambda: (x := w.ui()) and len(x["tabs"]["items"]) == 1 and x["sidebar"]["shown"] is True and x)
                tab = u["tabs"]["items"][0]
                w.click((tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0)
                wait_for(lambda: (x := w.ui()) and x["sidebar"]["cwd"] == str(proj) and x["sidebar"]["parent"] and x)
                conn = Conn(w.socket)

                # 1. before the answer nothing of the default home is listed; the question names the count and reads nothing else
                assert conn.call(op="session.chats", cwd=str(proj))["total"] == 0 or all(i["account"] != "native-claude" for i in conn.call(op="session.chats", cwd=str(proj))["items"])
                prompt = wait_for(lambda: (p := w.ui()["sidebar"]["native_prompt"]) and p["rects"]["yes"] and p)
                assert [(e["provider"], e["chats"]) for e in prompt["entries"]] == [("claude", 2)], prompt
                w.click(*_centre(prompt["rects"]["yes"]))
                wait_for(lambda: w.ui()["sidebar"]["native_prompt"] is None)

                # 2. the Chats tab lists the chats from before Lico; only the one the other terminal writes is marked
                w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
                listed = wait_for(lambda: (s := w.ui()["sidebar"]) and s["tab"] == "chats" and s["chats_loading"] is False and s["chats_total"] >= 2 and s)
                by_id = {e["provider_session"]: e for e in listed["chats_entries"]}
                assert (by_id["cl-before-lico"]["running_elsewhere"], by_id["cl-open-terminal"]["running_elsewhere"]) == (False, True), by_id
                assert by_id["cl-before-lico"]["account_label"] == "Native (~/.claude)", by_id
                w.click(*_centre(listed["dir_tab_rect"]))
                wait_for(lambda: w.ui()["sidebar"]["tab"] == "directories")

                # 3. the dialog: Native is in the account list; its latest chat is the live one: Resume is refused, nothing starts, New is allowed
                w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
                nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u)
                assert [a["label"] for a in nc["accounts"]] == ["ClaudeWork", "EmptyOne", "Native (~/.claude)", "Native (~/.codex)"], nc  # Codex has no managed account here: its default home is the only one, and open
                _key(w, "Down", "Down")
                nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["selected"] == 2 and u["last_line"].startswith("Last Session") and u)
                assert "work the other terminal writes" in nc["last_line"] and "running in another terminal; it cannot be resumed from here" in nc["last_line"], nc["last_line"]
                tabs_before, starts_before = len(w.ui()["tabs"]["items"]), len(argv_of(c_rec))
                _key(w, "Return")
                wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["last_line"].startswith("This chat is running in another terminal"))
                assert len(w.ui()["tabs"]["items"]) == tabs_before and len(argv_of(c_rec)) == starts_before
                _key(w, "Escape")
                wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None)

                # 4. the closed chat of the same folder resumes from the Chats tab, in the default home, with the provider's own login
                w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
                listed = wait_for(lambda: (s := w.ui()["sidebar"]) and s["tab"] == "chats" and s["chats_loading"] is False and s["chats_total"] >= 2 and s)
                closed_entry = next(e for e in listed["chats_entries"] if e["provider_session"] == "cl-before-lico")
                w.click(*_centre(closed_entry["rect"]))
                wait_for(lambda: len(w.ui()["tabs"]["items"]) == tabs_before + 1 and len(argv_of(c_rec)) == starts_before + 1)
                assert "--resume" in argv_of(c_rec)[-1] and "cl-before-lico" in argv_of(c_rec)[-1], argv_of(c_rec)[-1]
                native_session = next(t["session"] for t in w.ui()["tabs"]["items"] if t["session"] != u["tabs"]["items"][0]["session"])
                env = environ_of(None, conn, native_session)
                home = Path(w.directory) / "home"
                assert env["HOME"] == str(home) and env.get("CLAUDE_CONFIG_DIR", str(home / ".claude")) == str(home / ".claude"), env  # the host sets no CLAUDE_CONFIG_DIR here: the program uses $HOME/.claude
                assert not any(made["work"] in v or made["empty"] in v for v in env.values()), "nothing of a managed account"
                wait_for(lambda: "Native (~/.claude)" in w.ui()["header"])

                # 5. an account that has no chat here says where the others are
                w.click(*_centre(w.ui()["sidebar"]["dir_tab_rect"]))
                wait_for(lambda: w.ui()["sidebar"]["tab"] == "directories")
                w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
                wait_for(lambda: w.ui()["sidebar"]["new_chat"])
                _key(w, "Down")
                hint = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["selected"] == 1 and u["last_line"].startswith("No earlier chat") and u)
                assert "2 chats of this folder exist under other accounts or homes (Native (~/.claude): 2)" in hint["last_line"], hint["last_line"]
        finally:
            terminal.kill()
            terminal.wait()
