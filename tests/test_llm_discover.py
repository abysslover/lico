"""M12-P01: discovery of OpenAI-compatible Local LLM endpoints on this machine (`llm.discover`, `lico llm discover`, `POST /v1/llm/discover`). The host is started with
`--llm-proc-root`, a process table that the test wrote around its own fake servers (ephemeral loopback ports), so nothing real is ever listed or probed. Every fake server
records what it was sent: the probe may send only `GET /v1/models` and `GET /api/tags` as HTTP/1.0 with no body, no credential and no cookie, never follows a redirect, and
sends nothing at all to a server that speaks first."""

from __future__ import annotations

import json
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.llm_fixtures import FakeServer, keyed_server, normalised, ollama_tags, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http

ALLOWED = {"/v1/models", "/api/tags"}


def _start(directory: str, root: Path, sockets: list[dict], processes: dict, unreadable: tuple[int, ...] = (), http: bool = False):
    proc = root / "fake_proc"
    write_proc(proc, sockets, processes, unreadable)
    extra = ["--llm-proc-root", str(proc)]
    if http:
        extra += ["--http-port", "auto", "--http-token-file", str(root / "http.token")]
    host, _log = start(directory, extra=extra, sessions=0, producing=0)
    return host


def _by_port(reply: dict) -> dict:
    return {e["port"]: e for e in reply["endpoints"]}


def _assert_polite(server: FakeServer) -> None:
    """Only the two GETs, as HTTP/1.0, with no body, no credential and no cookie."""
    for method, path, headers in server.requests():
        assert method == "GET" and path in ALLOWED, (method, path)
        assert headers["version"] == "HTTP/1.0" and "authorization" not in headers and "cookie" not in headers and "content-length" not in headers, headers


def test_every_class_is_reported_from_fake_servers_and_the_probe_stays_polite():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        target = FakeServer(openai_models())  # a redirect must never lead here
        servers = {
            "openai": FakeServer(openai_models(("tiny-llama", "qwen-small"))),
            "ollama": FakeServer(ollama_tags(("llama3:8b", "mistral:7b"))),
            "keyed": FakeServer(keyed_server()),
            "html": FakeServer({"/": ("200 OK", {"Content-Type": "text/html"}, "<html>hello</html>")}),
            "redirect": FakeServer({"/v1/models": ("302 Found", {"Location": f"http://127.0.0.1:{target.port}/v1/models"}, "")}),
            "banner": FakeServer(mode="banner"),
            "closer": FakeServer(mode="close"),
        }
        big_ids = [f"model-{i:04d}-" + "x" * 40 for i in range(1500)]  # about 100 KiB of ids: more than the probe reads
        servers["big"] = FakeServer(openai_models(big_ids))
        try:
            sockets = [{"port": s.port, "address": "127.0.0.1", "inode": 1000 + i} for i, s in enumerate(servers.values())]
            procs = {4242: ("ollama-fake", [1000 + i for i in range(len(servers))])}
            host = _start(directory, root, sockets, procs)
            try:
                conn = Conn(host.socket)
                req = {"op": "llm.discover"}
                assert v1.validate_request("llm.discover", req) == []
                reply = conn.call(**req)
                assert reply["ok"] == 1 and v1.validate_response("llm.discover", reply) == [], reply
                assert (reply["host"], reply["os"], reply["scope"], reply["supported"]) == (None, "linux", "same_user", True), reply
                assert reply["listening"] == len(servers) and reply["probed"] == len(servers) and reply["skipped"] == 0, reply
                found = _by_port(reply)

                oa = found[servers["openai"].port]
                assert (oa["kind"], oa["auth"], oa["models"], oa["models_total"], oa["bind"], oa["address"], oa["process"]) == (
                    "openai", "none_needed", ["tiny-llama", "qwen-small"], 2, "loopback", "127.0.0.1", "ollama-fake"), oa
                ol = found[servers["ollama"].port]
                assert (ol["kind"], ol["auth"], ol["models"]) == ("ollama", "none_needed", ["llama3:8b", "mistral:7b"]) and "ollama" in ol["apis"], ol
                assert (found[servers["keyed"].port]["kind"], found[servers["keyed"].port]["auth"]) == ("openai", "key_needed"), found[servers["keyed"].port]
                assert found[servers["html"].port]["kind"] == "http-other"
                assert found[servers["redirect"].port]["kind"] == "http-other", "a redirect is an answer, not a road"
                assert found[servers["banner"].port]["kind"] == "not-http" and found[servers["closer"].port]["kind"] == "not-http"
                bg = found[servers["big"].port]
                assert bg["kind"] == "openai" and bg["models_truncated"] is True and len(bg["models"]) == 50 and bg["models_total"] > 50, {k: bg[k] for k in ("kind", "models_truncated", "models_total")}

                # what the servers were sent
                assert target.received == [] and target.connections == 0, "the redirect target was never requested"
                assert servers["banner"].received in ([], [b""]), "a server that speaks first receives nothing"
                for name in ("openai", "ollama", "keyed", "html", "redirect", "big"):
                    assert servers[name].requests(), name
                    _assert_polite(servers[name])
                for name in ("openai", "ollama", "keyed", "html", "redirect", "big"):
                    assert {p for _m, p, _h in servers[name].requests()} == ALLOWED, (name, "the two paths and no other")
            finally:
                host.stop()
        finally:
            for s in [target, *servers.values()]:
                s.close()


def test_the_scope_is_the_users_own_sockets_unless_all_users_is_asked_and_a_foreign_socket_is_never_probed_by_default():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        mine = FakeServer(openai_models(("mine",)))
        foreign = FakeServer(openai_models(("foreign",)))
        unreadable = FakeServer(openai_models(("unreadable",)))
        wild = FakeServer(openai_models(("wild",)), address="0.0.0.0")
        try:
            sockets = [
                {"port": mine.port, "address": "127.0.0.1", "inode": 11},
                {"port": foreign.port, "address": "127.0.0.1", "inode": 22},  # nobody of this user holds it
                {"port": unreadable.port, "address": "127.0.0.1", "inode": 33},  # held by a process whose fd folder cannot be read
                {"port": wild.port, "address": "0.0.0.0", "inode": 44},
                {"port": 1, "address": "127.0.0.1", "inode": 55, "state": "01"},  # not listening
            ]
            procs = {100: ("llama-server", [11, 44]), 200: ("someone-else", [33])}
            host = _start(directory, root, sockets, procs, unreadable=(200,))
            try:
                conn = Conn(host.socket)
                mine_only = conn.call(op="llm.discover")
                assert sorted(_by_port(mine_only)) == sorted([mine.port, wild.port]), mine_only
                assert _by_port(mine_only)[wild.port]["bind"] == "all" and _by_port(mine_only)[mine.port]["process"] == "llama-server"
                assert foreign.connections == 0 and unreadable.connections == 0, "a socket of another user is not probed by default"
                everyone = conn.call(op="llm.discover", all_users=True)
                assert everyone["ok"] == 1 and everyone["scope"] == "all_users" and everyone["listening"] == 4, everyone
                assert sorted(_by_port(everyone)) == sorted([mine.port, foreign.port, unreadable.port, wild.port])
                assert _by_port(everyone)[foreign.port]["process"] is None and _by_port(everyone)[foreign.port]["models"] == ["foreign"]
                assert conn.call(op="llm.discover", all_users="yes")["error"] == "bad_request"
                assert conn.call(op="llm.discover", ssh_host="bad host!")["error"] == "bad_request"
            finally:
                host.stop()
        finally:
            (root / "fake_proc" / "200" / "fd").chmod(0o700) if (root / "fake_proc" / "200" / "fd").exists() else None
            for s in (mine, foreign, unreadable, wild):
                s.close()


def test_the_run_is_bounded_in_ports_and_in_time_and_a_silent_server_does_not_hang_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        silent = FakeServer(mode="silent", hold=8.0)
        closers = [FakeServer(mode="close") for _ in range(70)]
        try:
            sockets = [{"port": s.port, "address": "127.0.0.1", "inode": 100 + i} for i, s in enumerate(closers)] + [{"port": silent.port, "address": "127.0.0.1", "inode": 9}]
            host = _start(directory, root, sockets, {7: ("many", [9] + [100 + i for i in range(70)])})
            try:
                conn = Conn(host.socket)
                t0 = time.monotonic()
                reply = conn.call(op="llm.discover")
                took = time.monotonic() - t0
                assert reply["ok"] == 1 and reply["listening"] == 71 and reply["probed"] == 64 and reply["skipped"] == 7, reply
                assert took < 8.5 and reply["elapsed_ms"] < 8500, (took, reply["elapsed_ms"])
            finally:
                host.stop()
        finally:
            silent.close()
            for s in closers:
                s.close()


def test_a_server_that_reads_the_request_and_never_answers_is_unknown_inside_the_per_request_budget():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        silent = FakeServer(mode="silent", hold=8.0)
        try:
            host = _start(directory, root, [{"port": silent.port, "address": "127.0.0.1", "inode": 9}], {7: ("quiet", [9])})
            try:
                reply = Conn(host.socket).call(op="llm.discover")
                found = _by_port(reply)[silent.port]
                assert found["kind"] == "unknown" and reply["elapsed_ms"] < 4000, reply
            finally:
                host.stop()
        finally:
            silent.close()


def test_the_command_the_socket_and_the_http_route_give_the_same_answer_and_a_missing_table_is_said_not_guessed():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        server = FakeServer(openai_models(("one",)))
        try:
            host = _start(directory, root, [{"port": server.port, "address": "127.0.0.1", "inode": 1}], {1: ("srv", [1])}, http=True)
            try:
                wait_for(lambda: Path(root / "http.token").exists() and host.http_port is not None)
                token = (root / "http.token").read_text().strip()
                conn = Conn(host.socket)
                via_socket = normalised(conn.call(op="llm.discover"))
                via_cli = normalised(json.loads(cli(host, "llm", "discover").stdout))
                status, via_http = _http(host.http_port, "POST", "/v1/llm/discover", token, {})
                assert status == 200 and normalised(via_http) == via_socket == via_cli, (via_socket, via_cli, via_http)
                assert json.loads(cli(host, "llm", "discover", "--all-users").stdout)["scope"] == "all_users"
                assert cli(host, "llm", "discover", "--bogus").returncode != 0
                caps = json.loads(cli(host, "capabilities").stdout)
                assert "llm.discover" in json.dumps(caps) and "/v1/llm/discover" in json.dumps(caps)
            finally:
                host.stop()
        finally:
            server.close()


def test_a_process_table_that_cannot_be_read_is_reported_as_not_supported_and_lists_nothing():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        empty = root / "empty_proc"
        empty.mkdir()
        host, _log = start(directory, extra=["--llm-proc-root", str(empty)], sessions=0, producing=0)
        try:
            reply = Conn(host.socket).call(op="llm.discover")
            assert reply["ok"] == 1 and reply["supported"] is False and reply["endpoints"] == [] and reply["listening"] == 0 and reply["reason"] == "proc_unreadable", reply
            assert v1.validate_response("llm.discover", reply) == [], reply
        finally:
            host.stop()
