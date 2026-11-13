"""M12-P03: the whole M12 flow in one window on fake servers, a fake ssh and a private Xvfb (no real service, no real host): the Session menu opens the dialog, nothing happens until
Enter, this machine's servers are listed with their models and where they listen, a server that wants a key says so and a server that speaks first is not asked anything, all
users' ports are an explicit step, an ssh host is chosen and looked at through the bounded remote script (its loopback servers say so), an unreachable host is told in a sentence,
and `lico llm discover` gives the same answers. Every fake server records what it was sent: only the two GETs, as HTTP/1.0, with no body, credential or cookie, for the whole
run; the ssh calls are exactly the hosts that were chosen."""

from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli
from lico_measure.rig import private_dir
from tests.llm_fixtures import FakeServer, keyed_server, normalised, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_llm_discover_ssh import _calls, _rig, _tables
from tests.test_new_session_screen import claude_args

ALLOWED = {"/v1/models", "/api/tags"}


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_find_servers_here_and_on_an_ssh_host_from_the_dialog_and_the_command_and_probe_nothing_else():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        local = {"llama": FakeServer(openai_models(("qwen-small", "tiny-llama"))), "ollama": FakeServer(ollama_tags(("llama3:8b", "mistral:7b"))), "keyed": FakeServer(keyed_server()),
                 "banner": FakeServer(mode="banner"), "page": FakeServer({"/": ("200 OK", {}, "<html>")})}
        others = FakeServer(openai_models(("someone-elses",)))
        remote = FakeServer(openai_models(("remote-llama",)))
        everything = [*local.values(), others, remote]
        try:
            cfg, mock = _rig(root)
            # a socket of another user: in the table, held by no process of this user
            write_proc(root / "fake_proc", [{"port": s.port, "address": "127.0.0.1", "inode": 10 + i} for i, s in enumerate(local.values())] + [{"port": others.port, "address": "127.0.0.1", "inode": 99}],
                       {7: ("local-server", [10 + i for i in range(len(local))])})
            _tables(root, "gpu-box", [remote], ["127.0.0.1"])
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--ssh", str(mock), "--ssh-config", str(cfg), "--log-dir", str(root / "logs")],
                               env_extra={"LANG": "C.UTF-8"}) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                wait_for(lambda: (u := w.ui()) and u["sidebar"]["shown"] is True)

                # 1. the menu opens the dialog; nothing is looked at until Enter
                _key(w, "alt+s")
                menu = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and all(b["rect"] for b in m["items"]) and m)
                w.click(*_centre(next(i["rect"] for i in menu["items"] if i["label"] == "Find Local LLM servers...")))
                d = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["open"] and x)
                assert d["state"] == "idle" and d["host"] is None, d
                time.sleep(1.0)
                assert all(s.connections == 0 for s in everything) and _calls(root) == []

                # 2. this machine: the servers, where they listen, what they want; the other user's socket is not touched
                _key(w, "Return")
                done = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["state"] == "done" and x)
                found = {e["port"]: e for e in done["endpoints"]}
                assert (found[local["llama"].port]["kind"], found[local["llama"].port]["models"], found[local["llama"].port]["bind"], found[local["llama"].port]["process"]) == ("openai", ["qwen-small", "tiny-llama"], "loopback", "local-server")
                assert found[local["ollama"].port]["kind"] == "ollama" and found[local["keyed"].port]["auth"] == "key_needed"
                assert found[local["banner"].port]["kind"] == "not-http" and found[local["page"].port]["kind"] == "http-other"
                assert "3 servers found; 2 other ports did not answer like one" in done["message"], done["message"]
                assert others.connections == 0, "a socket of another user is not looked at"
                assert len(done["rows"]) == 3

                # 3. the command and the socket agree with the dialog
                host_like = type("H", (), {"socket": w.socket})()
                via_cli = json.loads(cli(host_like, "llm", "discover").stdout)
                assert normalised(via_cli)["endpoints"] == normalised(done["endpoints"] and {"endpoints": done["endpoints"]})["endpoints"]

                # 4. all users' ports are an explicit step
                _key(w, "a")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"]["all_users"] is True)
                _key(w, "Return")
                wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["state"] == "done" and "4 servers found" in x["message"])

                # 5. an ssh host: chosen, not called until Enter, its servers say where they listen
                _key(w, "Right")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"]["host"] == "gpu-box" and w.ui()["sidebar"]["llm_find"]["state"] == "idle")
                assert _calls(root) == [], "choosing a host does not call it"
                _key(w, "Return")
                remote_done = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["state"] == "done" and x)
                assert [(e["kind"], e["bind"], e["models"]) for e in remote_done["endpoints"]] == [("openai", "loopback", ["remote-llama"])], remote_done["endpoints"]
                via_remote = json.loads(cli(host_like, "llm", "discover", "--ssh-host", "gpu-box", "--all-users").stdout)
                assert via_remote["host"] == "gpu-box" and via_remote["source"] == "remote_script" and via_remote["scope"] == "all_users", via_remote

                # 6. an unreachable host is a sentence, and the dialog closes with Esc
                _key(w, "Right")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"]["host"] == "offline-box")
                _key(w, "Return")
                failed = wait_for(lambda: (x := w.ui()["sidebar"]["llm_find"]) and x["state"] == "failed" and x)
                assert "could not be reached over ssh" in failed["message"], failed
                _key(w, "Escape")
                wait_for(lambda: w.ui()["sidebar"]["llm_find"] is None)
                assert Conn(w.socket).call(op="status")["ok"] == 1

            # 7. what every fake server was ever sent: the two GETs as HTTP/1.0, nothing more
            for server in [s for s in everything if s is not local["banner"]]:
                for method, path, headers in server.requests():
                    assert method == "GET" and path in ALLOWED and headers["version"] == "HTTP/1.0", (method, path, headers)
                    assert not {"authorization", "cookie", "content-length"} & set(headers), headers
            assert all(r == b"" for r in local["banner"].received), "a server that speaks first was sent nothing"
            assert set(_calls(root)) == {"gpu-box", "offline-box"}, _calls(root)
        finally:
            for s in everything:
                s.close()
