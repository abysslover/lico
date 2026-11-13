"""M11-P01: the first-run question about the provider's own default home (`Use your existing Claude/Codex login and its chats?`: Yes, No, Ask later), the Settings menu
entries, `Native` in the `+ New chat` dialog, the Chats tab, and the name beside the input target. A private Xvfb, synthetic homes under the window's own HOME, mocked
providers; the owner's real homes are never read."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl, _seed_codex_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args

CODEX_NAT = "019a4444-5555-7000-8000-0000000000d4"


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def _window(root: Path, proj: Path, c_args, x_args, made: dict):
    def setup(w: ProductWindow) -> None:
        conn = Conn(w.socket)
        made["acc_c"], made["acc_x"] = account(conn, "ClaudeWork", "claude"), account(conn, "CodexWork", "codex")
        home = Path(w.directory) / "home"  # the window's own HOME: its provider default homes are inside it
        _seed_claude_jsonl(home / ".claude", "p", "cl-nat-old", str(proj), "native old task", "old answer", 1_760_000_100)
        _seed_claude_jsonl(home / ".claude", "p", "cl-nat-new", str(proj), "native new task", "new answer", 1_760_000_500)
        _seed_codex_jsonl(home / ".codex", CODEX_NAT, str(proj), "codex native task", "codex native answer", 1_760_000_600)
        opened = conn.call(op="session.create", kind="claude", account=made["acc_c"], mode="auto", cwd=str(proj), name="Here")
        assert opened["ok"] == 1, opened

    return ProductWindow(host_binary=LICO, host_args=[*c_args, *x_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup)


def _show_proj(w: ProductWindow, proj: Path) -> None:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()
    u = wait_for(lambda: (x := w.ui()) and len(x["tabs"]["items"]) == 1 and x["sidebar"]["shown"] is True and x)
    tab = u["tabs"]["items"][0]
    w.click((tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0)
    wait_for(lambda: (x := w.ui()) and x["sidebar"]["cwd"] == str(proj) and x["sidebar"]["parent"] and x)


def test_ask_later_keeps_the_choice_open_and_the_settings_menu_turns_native_on_and_off():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        c_args, _c = claude_args(root)
        x_args, _x = codex_args(root)
        made: dict[str, str] = {}
        with _window(root, proj, c_args, x_args, made) as w:
            _show_proj(w, proj)
            prompt = wait_for(lambda: (p := w.ui()["sidebar"]["native_prompt"]) and p["rects"]["later"] and p)
            assert {e["provider"]: e["chats"] for e in prompt["entries"]} == {"claude": 2, "codex": 1}, prompt
            assert all(e["label"].startswith("Native (~/.") for e in prompt["entries"]), prompt
            _key(w, "l")
            wait_for(lambda: w.ui()["sidebar"]["native_prompt"] is None)
            conn = Conn(w.socket)
            assert {n["provider"]: n["state"] for n in conn.call(op="account.list")["native"]} == {"claude": "ask", "codex": "ask"}, "Ask later stores nothing"
            time.sleep(2.5)  # a negative: the question does not come back during this window start
            assert w.ui()["sidebar"]["native_prompt"] is None

            # the dialog lists the managed accounts only while Native is not open
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u)
            assert [a["label"] for a in nc["accounts"]] == ["ClaudeWork", "CodexWork"], nc
            _key(w, "Escape")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None)

            # the View menu: Native Claude is not chosen; one click turns it on, the next turns it off
            _key(w, "alt+v")
            menu = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 2 and all(b["rect"] for b in m["items"]) and m)
            item = next(i for i in menu["items"] if "Use existing Claude Code login" in i["label"])
            assert item["label"].endswith("not chosen"), item
            w.click(*_centre(item["rect"]))
            wait_for(lambda: {n["provider"]: n["state"] for n in conn.call(op="account.list")["native"]}["claude"] == "on")
            wait_for(lambda: next(n for n in w.ui()["sidebar"]["native"] if n["provider"] == "claude")["state"] == "on")
            _key(w, "alt+v")
            menu = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 2 and all(b["rect"] for b in m["items"]) and m)
            item = next(i for i in menu["items"] if "Use existing Claude Code login" in i["label"])
            assert item["label"].endswith(": on"), item
            w.click(*_centre(item["rect"]))
            wait_for(lambda: {n["provider"]: n["state"] for n in conn.call(op="account.list")["native"]}["claude"] == "off")


def test_yes_opens_native_in_the_dialog_the_chats_tab_and_the_header_and_resume_runs_in_the_default_home():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        c_args, c_rec = claude_args(root)
        x_args, _x = codex_args(root)
        made: dict[str, str] = {}
        with _window(root, proj, c_args, x_args, made) as w:
            _show_proj(w, proj)
            prompt = wait_for(lambda: (p := w.ui()["sidebar"]["native_prompt"]) and p["rects"]["yes"] and p)
            w.click(*_centre(prompt["rects"]["yes"]))
            wait_for(lambda: w.ui()["sidebar"]["native_prompt"] is None)
            conn = Conn(w.socket)
            wait_for(lambda: {n["provider"]: n["state"] for n in conn.call(op="account.list")["native"]} == {"claude": "on", "codex": "on"})

            # the Chats tab shows the chats of the default home, named Native
            sb = w.ui()["sidebar"]
            w.click(*_centre(sb["chats_tab_rect"]))
            listed = wait_for(lambda: (u := w.ui()["sidebar"]) and u["tab"] == "chats" and u["chats_loading"] is False and u["chats_total"] >= 3 and u)
            native = [e for e in listed["chats_entries"] if e["provider_session"] in ("cl-nat-new", "cl-nat-old", CODEX_NAT)]
            assert len(native) == 3 and {e["account_label"] for e in native} == {"Native (~/.claude)", "Native (~/.codex)"}, native
            w.click(*_centre(listed["dir_tab_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["tab"] == "directories")

            # the dialog lists the two managed accounts and the two Native entries
            w.click(*_centre(w.ui()["sidebar"]["new_chat_rect"]))
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u)
            assert [a["label"] for a in nc["accounts"]] == ["ClaudeWork", "CodexWork", "Native (~/.claude)", "Native (~/.codex)"], nc
            _key(w, "Down", "Down")
            nc = wait_for(lambda: (u := w.ui()["sidebar"]["new_chat"]) and u["selected"] == 2 and u["last_line"].startswith("Last Session") and u)
            assert nc["last_line"].endswith("native new task (2 turns)"), nc["last_line"]
            before = len(argv_of(c_rec))
            _key(w, "Return")
            wait_for(lambda: w.ui()["sidebar"]["new_chat"] is None and len(w.ui()["tabs"]["items"]) == 2)
            wait_for(lambda: len(argv_of(c_rec)) == before + 1 and "cl-nat-new" in argv_of(c_rec)[-1])
            # the name beside the input target says where this session runs
            wait_for(lambda: "Native (~/.claude)" in w.ui()["header"])
