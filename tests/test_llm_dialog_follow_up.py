"""M13-P02 in the window: the dialog keeps the last result of each (host, all users) pair in the window's memory with its age and shows it again without a scan (Enter or the
Refresh button scans again), folds the servers that answered but list no model behind one line, says `via ssh` for a row owned by an ssh process and never hides a row by that name,
reads every long row (two lines per row, a selection with Up and Down and a detail area with the full text, the host name wrapped and never cut), and says in one plain line what
"no model listed" means. A private Xvfb; fake servers on ephemeral loopback ports listed by a process table the test wrote; the fake ssh for the long rows."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO
from lico_measure.rig import private_dir
from tests.llm_fixtures import FakeServer, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def _find(w: ProductWindow) -> dict | None:
    return w.ui()["sidebar"]["llm_find"]


def _open(w: ProductWindow) -> dict:
    _key(w, "alt+s")
    menu = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and all(b["rect"] for b in m["items"]) and m)
    w.click(*_centre(next(i["rect"] for i in menu["items"] if i["label"] == "Find Local LLM servers...")))
    return wait_for(lambda: (d := _find(w)) and d["open"] and d)


def _start(w: ProductWindow) -> None:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()
    wait_for(lambda: (u := w.ui()) and u["sidebar"]["shown"] is True)


def _done(w: ProductWindow) -> dict:
    return wait_for(lambda: (d := _find(w)) and d["state"] == "done" and d)


def test_the_last_result_is_kept_with_its_age_shown_again_without_a_scan_and_scanned_again_on_refresh():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        servers = [FakeServer(openai_models(("tiny-llama",))), FakeServer(ollama_tags(("llama3:8b",)))]
        try:
            write_proc(root / "fake_proc", [{"port": s.port, "address": "127.0.0.1", "inode": 10 + i} for i, s in enumerate(servers)], {7: ("server-process", [10, 11])})
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}) as w:
                _start(w)
                idle = _open(w)
                assert idle["state"] == "idle" and idle["cached"] is False and idle["scan_label"] == "Scan", idle
                _key(w, "Return")
                first = _done(w)
                assert first["cached"] is False and first["scan_label"] == "Refresh" and first["found"], first
                scans = sum(s.connections for s in servers)
                assert scans > 0
                _key(w, "Escape")
                wait_for(lambda: _find(w) is None)
                time.sleep(2.2)  # the result gets older while the dialog is closed
                again = _open(w)
                assert again["state"] == "done" and again["cached"] is True and again["age_s"] >= 2, again
                assert "ago" in again["found"] or "just now" in again["found"], again["found"]
                time.sleep(0.8)
                assert sum(s.connections for s in servers) == scans, "opening the dialog again shows the kept result and scans nothing"
                assert any("tiny-llama" in r for r in again["rows"]), again["rows"]
                # a result belongs to its pair: all users' ports have none yet
                _key(w, "a")
                wait_for(lambda: _find(w)["all_users"] is True and _find(w)["state"] == "idle")
                _key(w, "a")
                back = wait_for(lambda: (d := _find(w)) and d["all_users"] is False and d["state"] == "done" and d)
                assert back["cached"] is True
                # Refresh: Enter and the button scan again
                _key(w, "Return")
                wait_for(lambda: sum(s.connections for s in servers) > scans)
                fresh = wait_for(lambda: (d := _find(w)) and d["state"] == "done" and d["cached"] is False and d)
                assert fresh["age_s"] <= 2
                n = sum(s.connections for s in servers)
                w.click(*_centre(fresh["rects"]["scan"]))
                wait_for(lambda: sum(s.connections for s in servers) > n)
        finally:
            for s in servers:
                s.close()


def test_servers_that_list_no_model_are_folded_behind_one_line_and_a_row_owned_by_ssh_says_so_and_is_never_hidden_by_that_name():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with_models = FakeServer(openai_models(("tiny-llama", "qwen-small")))
        ssh_with_models = FakeServer(openai_models(("forwarded-model",)))
        empty_a, empty_b = FakeServer(openai_models(())), FakeServer(ollama_tags(()))
        try:
            servers = [with_models, ssh_with_models, empty_a, empty_b]
            owners = {7: ("python", [10]), 8: ("ssh", [11]), 9: ("mcp-server", [12]), 10: ("ssh", [13])}
            write_proc(root / "fake_proc", [{"port": s.port, "address": "127.0.0.1", "inode": 10 + i} for i, s in enumerate(servers)], owners)
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}) as w:
                _start(w)
                _open(w)
                _key(w, "Return")
                d = _done(w)
                assert "4 servers found" in d["message"], d["message"]
                assert [i["folded"] for i in d["items"]] == [False, False], d["items"]
                assert any("tiny-llama" in r for r in d["rows"]) and any("forwarded-model" in r and "via ssh" in r for r in d["rows"]), d["rows"]
                assert d["folded"]["count"] == 2 and d["folded"]["open"] is False and "2 more servers answered but list no model" in d["folded"]["label"], d["folded"]
                assert "lists no model" in d["note"] and "nothing can be run on it yet" in d["note"], d["note"]
                # a click opens the fold, a key closes it
                w.click(*_centre(d["rects"]["more"]))
                opened = wait_for(lambda: (x := _find(w)) and x["folded"]["open"] is True and x)
                assert [i["folded"] for i in opened["items"]] == [False, False, True, True], opened["items"]
                assert any("via ssh" in r and "no model listed" in r for r in opened["rows"]), "an ssh-owned server that lists no model is folded by that rule only"
                _key(w, "f")
                closed = wait_for(lambda: (x := _find(w)) and x["folded"]["open"] is False and x)
                assert len(closed["items"]) == 2
        finally:
            for s in (with_models, ssh_with_models, empty_a, empty_b):
                s.close()


def test_long_rows_of_an_ssh_host_are_read_in_two_lines_and_a_detail_area_and_the_host_name_is_wrapped_not_cut():
    from tests.test_llm_discover_ssh import _rig, _tables

    alias = "gpu-workstation-" + "very-long-host-alias-" * 3 + "end"  # 80 characters
    assert len(alias) >= 70
    ids = [f"org-name/Some-Very-Long-Model-Name-{n}-instruct-q4_k_m-" + "x" * 30 for n in range(3)]  # about 80 characters each
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        server = FakeServer(openai_models(ids))
        try:
            cfg, mock = _rig(root)
            cfg.write_text(cfg.read_text() + f"Host {alias}\n  HostName 10.20.0.99\n  User u99\n\n")
            (root / "remote_hosts" / alias / "user_home").mkdir(parents=True, exist_ok=True)
            _tables(root, None, [])
            _tables(root, alias, [server])
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--ssh", str(mock), "--ssh-config", str(cfg), "--log-dir", str(root / "logs")],
                               env_extra={"LANG": "C.UTF-8"}) as w:
                _start(w)
                d = _open(w)
                for _ in range(len(d["hosts"])):
                    if _find(w)["host"] == alias:
                        break
                    _key(w, "Right")
                    time.sleep(0.15)
                assert _find(w)["host"] == alias
                _key(w, "Return")
                done = _done(w)
                assert "".join(done["place_lines"]).replace(" ", "").endswith(alias.replace(" ", "")) and len(done["place_lines"]) >= 2, done["place_lines"]
                assert len(done["items"]) == 1 and done["selected"] == 0
                item = done["items"][0]
                assert len(item["lines"]) == 2 and item["clipped"] is True, item
                assert all(len(line) <= done["cols"] for line in item["lines"]), (done["cols"], item["lines"])
                joined = "".join(done["detail"]).replace(" ", "")
                assert all(i in joined for i in ids), "the detail area has every model id in full"
                assert alias in joined or alias in "".join(done["place_lines"]).replace(" ", ""), "the host name is whole somewhere on screen"
        finally:
            server.close()
