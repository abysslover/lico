"""M12-P01 in the window: the Session menu opens a centred dialog that looks for Local LLM servers only when the person presses Enter (nothing before), lists the servers
with their models, toggles all users' ports, and closes with Esc. A private Xvfb, fake servers on ephemeral loopback ports listed by a process table the test wrote."""

from __future__ import annotations

import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.llm_fixtures import FakeServer, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_the_dialog_scans_only_on_enter_lists_the_servers_and_closes():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        servers = [FakeServer(openai_models(("tiny-llama", "qwen-small"))), FakeServer(ollama_tags(("llama3:8b",))), FakeServer(mode="banner")]
        foreign = FakeServer(openai_models(("not-mine",)))
        try:
            write_proc(root / "fake_proc", [{"port": s.port, "address": "127.0.0.1", "inode": 10 + i} for i, s in enumerate(servers)] + [{"port": foreign.port, "address": "127.0.0.1", "inode": 99}],
                       {7: ("server-process", [10, 11, 12])})
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                wait_for(lambda: (u := w.ui()) and u["sidebar"]["shown"] is True)
                _key(w, "alt+s")
                menu = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and all(b["rect"] for b in m["items"]) and m)
                item = next(i for i in menu["items"] if i["label"] == "Find Local LLM servers...")
                w.click(*_centre(item["rect"]))
                idle = wait_for(lambda: (d := w.ui()["sidebar"]["llm_find"]) and d["open"] and d)
                assert idle["state"] == "idle" and idle["all_users"] is False and idle["rects"]["scan"] and idle["rects"]["close"], idle
                import time
                time.sleep(1.0)  # a negative: it does not start by itself
                assert all(s.connections == 0 for s in servers) and w.ui()["sidebar"]["llm_find"]["state"] == "idle"

                _key(w, "Return")
                done = wait_for(lambda: (d := w.ui()["sidebar"]["llm_find"]) and d["state"] == "done" and d)
                assert "2 servers found; 1 other port did not answer like one" in done["message"], done["message"]
                assert any("tiny-llama, qwen-small" in r and "openai" in r and "(server-process)" in r for r in done["rows"]), done["rows"]
                assert any("llama3:8b" in r and "ollama" in r for r in done["rows"]), done["rows"]
                assert foreign.connections == 0, "a socket of another user is not looked at"

                _key(w, "a")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"]["all_users"] is True)
                _key(w, "Return")
                wait_for(lambda: (d := w.ui()["sidebar"]["llm_find"]) and d["state"] == "done" and "3 servers found" in d["message"])
                _key(w, "Escape")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"] is None)
                assert Conn(w.socket).call(op="status")["ok"] == 1
        finally:
            for s in [*servers, foreign]:
                s.close()


def test_the_dialog_chooses_an_ssh_host_scans_it_and_says_why_an_unreachable_one_fails():
    from tests.test_llm_discover_ssh import _rig, _tables

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        server = FakeServer(openai_models(("remote-llama",)))
        try:
            cfg, mock = _rig(root)
            _tables(root, None, [])
            _tables(root, "gpu-box", [server])
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--ssh", str(mock), "--ssh-config", str(cfg), "--log-dir", str(root / "logs")],
                               env_extra={"LANG": "C.UTF-8"}) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                wait_for(lambda: (u := w.ui()) and u["sidebar"]["shown"] is True)
                _key(w, "alt+s")
                menu = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and all(b["rect"] for b in m["items"]) and m)
                w.click(*_centre(next(i["rect"] for i in menu["items"] if i["label"] == "Find Local LLM servers...")))
                d = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["open"] and x)
                assert d["host"] is None and d["hosts"][:3] == [None, "gpu-box", "offline-box"], d["hosts"]
                _key(w, "Right")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"]["host"] == "gpu-box")
                assert (root / "ssh_calls.log").exists() is False, "choosing a host does not call it"
                _key(w, "Return")
                done = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["state"] == "done" and x)
                assert any("remote-llama" in r for r in done["rows"]) and "1 server found" in done["message"], done
                _key(w, "Right")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"]["host"] == "offline-box" and w.ui()["sidebar"]["llm_find"]["state"] == "idle")
                _key(w, "Return")
                failed = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["state"] == "failed" and x)
                assert "could not be reached over ssh" in failed["message"], failed
        finally:
            server.close()
