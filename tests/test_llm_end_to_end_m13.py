"""M13-P04: the whole of the discovery dialog as M13 left it, in one window on fake servers, a fake ssh and a private Xvfb (no real service, no real host): the servers that list models
or ask for a key are listed, one that lists none is folded behind a line and opens on a click, an ssh-owned server says `via ssh`, the command gives the same servers as the dialog, the
last result of each (host, all users) choice is kept in the window and shown again without a scan, Refresh scans again, an ssh host with a very long name is chosen, looked at once and
shown again from memory when it is chosen again. Every fake server records what it was sent: only the two GETs, as HTTP/1.0, with no body, credential or cookie, for the whole run."""

from __future__ import annotations

import json
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, cli
from lico_measure.rig import private_dir
from tests.llm_fixtures import FakeServer, keyed_server, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_llm_dialog_follow_up import _done, _find, _key, _open, _start
from tests.test_llm_discover_ssh import _calls, _rig, _tables
from tests.test_new_session_screen import claude_args

ALLOWED = {"/v1/models", "/api/tags"}


def test_the_dialog_keeps_folds_and_refreshes_and_the_command_agrees_with_it():
    alias = "gpu-workstation-" + "very-long-host-alias-" * 3 + "end"
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        local = {"llama": FakeServer(openai_models(("qwen-small", "tiny-llama"))), "ollama": FakeServer(ollama_tags(("llama3:8b",))), "forward": FakeServer(openai_models(("forwarded",))),
                 "empty": FakeServer(openai_models(())), "keyed": FakeServer(keyed_server()), "banner": FakeServer(mode="banner"), "page": FakeServer({"/": ("200 OK", {}, "<html>")})}
        remote = FakeServer(openai_models([f"org/Very-Long-Model-Name-{n}-" + "x" * 50 for n in range(3)]))
        everything = [*local.values(), remote]
        owners = {7: ("llama-server", [10]), 8: ("ollama", [11]), 9: ("ssh", [12]), 10: ("python", [13]), 11: ("keyed-server", [14]), 12: ("sshd-like", [15]), 13: ("web", [16])}
        try:
            cfg, mock = _rig(root)
            cfg.write_text(cfg.read_text() + f"Host {alias}\n  HostName 10.20.0.99\n  User u99\n\n")
            (root / "remote_hosts" / alias / "user_home").mkdir(parents=True, exist_ok=True)
            write_proc(root / "fake_proc", [{"port": s.port, "address": "127.0.0.1", "inode": 10 + i} for i, s in enumerate(local.values())], owners)
            _tables(root, alias, [remote])
            c_args, _c = claude_args(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--ssh", str(mock), "--ssh-config", str(cfg), "--log-dir", str(root / "logs")],
                               env_extra={"LANG": "C.UTF-8"}) as w:
                _start(w)
                touched = lambda: sum(s.connections for s in everything)  # noqa: E731

                # 1. nothing before Enter; then the servers, the fold, `via ssh`
                _open(w)
                time.sleep(0.5)
                assert touched() == 0
                _key(w, "Return")
                done = _done(w)
                assert [i["folded"] for i in done["items"]] == [False] * 4, done["items"]
                assert any("via ssh" in r and "forwarded" in r for r in done["rows"]) and any("key needed" in r for r in done["rows"]), done["rows"]
                assert done["folded"]["count"] == 1 and "1 more server answered but list no model" in done["folded"]["label"], done["folded"]
                assert done["cached"] is False and done["scan_label"] == "Refresh"

                # 2. the command gives the same servers as the dialog
                host_like = type("H", (), {"socket": w.socket})()
                via_cli = json.loads(cli(host_like, "llm", "discover").stdout)
                servers = {e["port"] for e in via_cli["endpoints"] if e["kind"] in ("openai", "ollama")}
                assert servers == {e["port"] for e in done["endpoints"] if e["kind"] in ("openai", "ollama")} and len(servers) == 5, servers

                # 3. closed and opened again: the kept result, no scan; the fold opens on a click; Refresh scans again
                _key(w, "Escape")
                wait_for(lambda: _find(w) is None)
                after_first = touched()
                again = _open(w)
                assert again["state"] == "done" and again["cached"] is True and again["found"], again
                time.sleep(0.5)
                assert touched() == after_first, "opening the dialog again scans nothing"
                w.click((again["rects"]["more"][0] + again["rects"]["more"][2]) / 2.0, (again["rects"]["more"][1] + again["rects"]["more"][3]) / 2.0)
                opened = wait_for(lambda: (x := _find(w)) and x["folded"]["open"] and x)
                assert len(opened["items"]) == 5 and opened["items"][-1]["folded"] is True
                w.click((opened["rects"]["scan"][0] + opened["rects"]["scan"][2]) / 2.0, (opened["rects"]["scan"][1] + opened["rects"]["scan"][3]) / 2.0)
                wait_for(lambda: touched() > after_first)
                fresh = wait_for(lambda: (x := _find(w)) and x["state"] == "done" and x["cached"] is False and x)
                assert fresh["folded"]["open"] is False and len(fresh["items"]) == 4, "a new look starts with the fold closed"

                # 4. an ssh host with a very long name: looked at once, then shown from memory when it is chosen again
                for _ in range(len(fresh["hosts"])):
                    if _find(w)["host"] == alias:
                        break
                    _key(w, "Right")
                    time.sleep(0.15)
                assert _find(w)["state"] == "idle" and _calls(root) == [], "choosing a host does not call it"
                _key(w, "Return")
                far = _done(w)
                assert far["items"][0]["clipped"] is True and "".join(far["detail"]).replace(" ", "").count("Very-Long-Model-Name-") == 3, far["detail"]
                assert alias in "".join(far["place_lines"]).replace(" ", "")
                _key(w, "Left")
                wait_for(lambda: _find(w)["host"] != alias)
                _key(w, "Right")
                back = wait_for(lambda: (x := _find(w)) and x["host"] == alias and x["state"] == "done" and x)
                assert back["cached"] is True and _calls(root) == [alias], "the host was called once"
                _key(w, "Escape")
                wait_for(lambda: _find(w) is None)

            # 5. what every fake server was ever sent: the two GETs as HTTP/1.0, nothing more
            for server in [s for s in everything if s is not local["banner"]]:
                for method, path, headers in server.requests():
                    assert method == "GET" and path in ALLOWED and headers["version"] == "HTTP/1.0", (method, path, headers)
                    assert not {"authorization", "cookie", "content-length"} & set(headers), headers
            assert all(r == b"" for r in local["banner"].received), "a server that speaks first was sent nothing"
        finally:
            for s in everything:
                s.close()
