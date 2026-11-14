"""M13-P02: the scan waits for the quiet period once for all sockets, not once per round of eight. A fixture of 46 fake sockets (a process table the test wrote, fake servers on
ephemeral loopback ports; nothing real is probed) of the kind a real machine has: a few servers that answer, many HTTP servers that answer 404 or a page, ssh-like ports that speak first,
and ports that close or that nobody serves. The bounds are the same as before (64 sockets, 1.5 s a request, 8 s in all, GET only, a speak-first server is sent nothing); only the
waiting is shared. The upper bounds below are far over what a run takes on a quiet machine and under what the old rounds took (about 600 ms for this fixture)."""

from __future__ import annotations

import statistics
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import private_dir
from tests.llm_fixtures import FakeServer, keyed_server, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask
from tests.test_llm_discover import _assert_polite
from tests.test_llm_discover_ssh import _host, _tables

PAGE = {"/": ("200 OK", {"Content-Type": "text/html"}, "<html>x</html>")}


def _fixture() -> tuple[list[FakeServer], list[FakeServer], list[dict]]:
    """46 sockets: 26 quiet HTTP servers (4 rounds of 150 ms at 8 at a time), 8 that speak first, 6 that close, 6 that nobody serves."""
    http = [FakeServer(openai_models(("tiny-llama", "qwen-small"))) for _ in range(2)] + [FakeServer(ollama_tags(("llama3:8b",)))] + [FakeServer(keyed_server())]
    http += [FakeServer(openai_models(())) for _ in range(2)] + [FakeServer(PAGE) for _ in range(20)]
    other = [FakeServer(mode="banner") for _ in range(8)] + [FakeServer(mode="close") for _ in range(6)]
    gone = [FakeServer(mode="close") for _ in range(6)]
    for s in gone:
        s.close()  # listed in the table, refused when asked
    sockets = [{"port": s.port, "address": "127.0.0.1", "inode": 1000 + i} for i, s in enumerate([*http, *other, *gone])]
    return http, other, sockets


def test_a_scan_of_46_sockets_waits_for_the_quiet_period_once_and_keeps_every_bound():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        http, other, sockets = _fixture()
        try:
            proc = root / "fake_proc"
            write_proc(proc, sockets, {4242: ("fixture", [s["inode"] for s in sockets])})
            host, _log = start(directory, extra=["--llm-proc-root", str(proc)], sessions=0, producing=0)
            try:
                conn = Conn(host.socket)
                took = []
                for _ in range(5):
                    reply = conn.call(op="llm.discover")
                    assert reply["ok"] == 1 and reply["listening"] == 46 and reply["probed"] == 46 and reply["skipped"] == 0, reply
                    took.append(reply["elapsed_ms"])
                    kinds = {}
                    for e in reply["endpoints"]:
                        kinds[e["kind"]] = kinds.get(e["kind"], 0) + 1
                    assert kinds == {"openai": 5, "ollama": 1, "not-http": 14, "http-other": 20, "unknown": 6}, kinds
                assert statistics.median(took) < 450, f"median {statistics.median(took)} ms of {took}: the quiet period is waited for once, not once per round"
                for s in http:
                    _assert_polite(s)
                assert all(r == b"" for s in other if s.mode == "banner" for r in s.received), "a server that speaks first is sent nothing (five scans)"
            finally:
                host.stop()
        finally:
            for s in [*http, *other]:
                s.close()


def test_the_remote_script_waits_for_the_quiet_period_once_too():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        servers = [FakeServer(PAGE) for _ in range(48)]  # three rounds of 150 ms at 16 at a time before
        try:
            _tables(root, "gpu-box", servers)
            host = _host(directory, root)
            try:
                conn = Conn(host.socket)
                took = []
                for _ in range(3):
                    reply = conn.call(op="llm.discover", ssh_host="gpu-box")
                    assert reply["ok"] == 1 and reply["listening"] == 48 and reply["probed"] == 48, reply
                    assert {e["kind"] for e in reply["endpoints"]} == {"http-other"}
                    took.append(reply["elapsed_ms"])
                assert statistics.median(took) < 300, f"median {statistics.median(took)} ms of {took}"
            finally:
                host.stop()
        finally:
            for s in servers:
                s.close()
