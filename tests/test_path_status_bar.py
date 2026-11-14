"""M14-P01 in the window: a status bar across the bottom edge shows the FULL path of what the person points at (a directory or file row of the sidebar, a chat card's folder), else of the
Files panel row or the directory the sidebar is on, else the working directory of the active session; `host:path` on an ssh host. A path too long for the bar keeps its end behind a leading
ellipsis and a click copies the whole text. The bar does not overlap the hint line, the telemetry strip or the lists, and it draws nothing by itself while the window is idle. A private
Xvfb, synthetic homes, the fake ssh of the ssh tests: no real terminal, home or host is looked at."""

from __future__ import annotations

import os
import subprocess
import time
import unicodedata
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_running_elsewhere_window import _centre
from tests.test_ssh_sessions_and_files import _make_remote_workbench_rig

DEEP = "한글로-아주-긴-디렉터리-이름-하나/두번째-긴-디렉터리-이름-둘/세번째-긴-디렉터리-이름-셋"


def _cols(text: str) -> int:
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 1 for c in text)


def _bar(w: ProductWindow) -> dict:
    return w.ui()["path_bar"]


def _start(w: ProductWindow, proj: Path) -> None:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()
    u = wait_for(lambda: (x := w.ui()) and len(x["tabs"]["items"]) == 1 and x["sidebar"]["shown"] is True and x)
    tab = u["tabs"]["items"][0]
    w.click((tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0)  # no session tab: the sidebar stays on the folder
    wait_for(lambda: (x := w.ui()) and x["sidebar"]["cwd"] == str(proj) and x["sidebar"]["parent"] and x)


def _setup(proj: Path, deep: Path | None = None):
    def setup(w: ProductWindow) -> None:
        conn = Conn(w.socket)
        work = account(conn, "ClaudeWork", "claude")
        home = Path(w.directory) / "state" / "lico" / "accounts" / work / "claude"
        _seed_claude_jsonl(home, "p", "cl-closed", str(proj), "Chat that is closed", "answer", 1_760_000_100)
        opened = conn.call(op="session.create", kind="claude", account=work, mode="auto", cwd=str(deep or proj), name="Here")
        assert opened["ok"] == 1, opened

    return setup


def test_the_bar_is_the_last_row_at_full_width_shows_the_pointed_row_and_falls_back_to_the_folder():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        (proj / "sub").mkdir(mode=0o700, parents=True)
        (proj / "notes.txt").write_text("hello\n")
        c_args, _rec = claude_args(root)
        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=_setup(proj)) as w:
            _start(w, proj)
            u = w.ui()
            width, height = u["layout"]["window_size"]
            bar = u["path_bar"]
            assert bar["rect"][0] == 0 and bar["rect"][2] == width and bar["rect"][3] == height and bar["rect"][3] - bar["rect"][1] >= 16, (bar["rect"], width, height)
            assert bar["text"] == str(proj) and bar["source"] in ("selection", "session") and bar["cut"] is False, bar
            # no overlap: the bar is the last row, the hint line is above it, the telemetry strip above that, and no list reaches into them
            assert bar["hint_rect"][3] == bar["rect"][1] and bar["telem_rect"][3] <= bar["hint_rect"][1], bar
            entries = u["sidebar"]["entries"]
            assert entries and all(e["rect"][3] <= bar["hint_rect"][1] for e in entries), (entries[-1]["rect"], bar["hint_rect"])

            # the pointer: a directory row, then a file row, then away
            by_name = {e["name"]: e for e in entries}
            w.move(*_centre(by_name["sub"]["rect"]))
            pointed = wait_for(lambda: (b := _bar(w)) and b["source"] == "pointer" and b)
            assert pointed["text"] == str(proj / "sub"), pointed
            w.move(*_centre(by_name["notes.txt"]["rect"]))
            assert wait_for(lambda: (b := _bar(w)) and b["text"] == str(proj / "notes.txt") and b)["source"] == "pointer"
            w.move(width - 40, 300)  # the transcript: nothing is pointed at
            away = wait_for(lambda: (b := _bar(w)) and b["source"] != "pointer" and b)
            assert away["text"] == str(proj), away

            # a chat card shows the folder of the chat
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            listed = wait_for(lambda: (s := w.ui()["sidebar"]) and s["tab"] == "chats" and s["chats_loading"] is False and s["chats_total"] >= 1 and s)
            w.move(*_centre(listed["chats_entries"][0]["rect"]))
            card = wait_for(lambda: (b := _bar(w)) and b["source"] == "pointer" and b)
            assert card["text"] == str(proj), card
            assert all(e["rect"][3] <= card["hint_rect"][1] for e in listed["chats_entries"]), "the cards end above the hint line"

            # two idle seconds draw nothing
            w.move(width - 40, 300)
            time.sleep(0.5)
            before = w.frames()
            time.sleep(2.0)
            assert w.frames() == before, "the bar draws only when its text changes"


def test_a_path_on_an_ssh_host_is_host_colon_path():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700)
        ssh_cfg, mock_ssh, remote_root = _make_remote_workbench_rig(root)
        c_args, _rec = claude_args(root)
        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--ssh", str(mock_ssh), "--ssh-config", str(ssh_cfg), "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"},
                           before_window=_setup(proj)) as w:
            _start(w, proj)
            w.click(*_centre(w.ui()["sidebar"]["host_rect"]))
            popup = wait_for(lambda: (s := w.ui()["sidebar"]) and s["host_popup_open"] and s["host_items"] and s)
            item = next(i for i in popup["host_items"] if i["label"].endswith("ssh: gpu-box"))
            w.click(*_centre(item["rect"]))
            remote = wait_for(lambda: (s := w.ui()["sidebar"]) and s["ssh_host"] == "gpu-box" and s["loading"] is None and s["entries"] and s)
            entry = remote["entries"][0]
            w.move(*_centre(entry["rect"]))
            bar = wait_for(lambda: (b := _bar(w)) and b["source"] == "pointer" and b)
            assert bar["text"].startswith("gpu-box:/") and bar["text"].endswith("/" + entry["name"]), bar
            assert bar["text"] == f"gpu-box:{remote['cwd'].rstrip('/')}/{entry['name']}", (bar["text"], remote["cwd"])


def test_a_long_path_with_wide_characters_keeps_its_end_and_a_click_copies_the_whole_text():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        deep = proj / DEEP
        deep.mkdir(mode=0o700, parents=True)
        c_args, _rec = claude_args(root)
        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=_setup(proj, deep)) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u["sidebar"]["shown"] is True)
            subprocess.run(["xdotool", "windowsize", w.window_id(), "640", "560"], env=w.env, check=True)
            narrow = wait_for(lambda: (u := w.ui()) and u["layout"]["window_size"][0] <= 700 and u["path_bar"]["rect"][2] <= 700 and u["path_bar"]["text"].endswith(DEEP) and u)  # drawn at the new size
            bar = narrow["path_bar"]
            assert bar["cut"] is True and bar["shown"].startswith("…") and bar["text"].endswith(bar["shown"][1:]), bar
            assert _cols(bar["shown"]) <= bar["cols"] and _cols(bar["text"]) > bar["cols"], (bar["cols"], _cols(bar["shown"]))
            # the telemetry strip, the hint line and the bar are three rows that do not overlap, at this width too
            assert bar["hint_rect"][3] == bar["rect"][1] and (bar["telem_rect"] is None or bar["telem_rect"][3] <= bar["hint_rect"][1]), bar
            w.click(*_centre(bar["rect"]))
            got = wait_for(lambda: subprocess.run(["xclip", "-o", "-selection", "clipboard", "-t", "UTF8_STRING"], env={"DISPLAY": w.xvfb.display, "PATH": "/usr/bin:/bin"}, capture_output=True,
                                                  text=True).stdout or None)
            assert got == bar["text"], (got, bar["text"])
