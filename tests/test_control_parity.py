import json
import os
import socket
import time
import urllib.parse
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, echoes, start
from lico_measure.rig import private_dir
from lico_protocol import v1


def _http(port: int, method: str, path: str, token: str, body: dict | None = None) -> tuple[int, dict | None]:
    raw = json.dumps(body).encode() if body is not None else b""
    headers = [
        f"{method} {path} HTTP/1.1",
        f"Host: 127.0.0.1:{port}",
        f"Authorization: Bearer {token}",
        "Connection: close",
    ]
    if raw:
        headers.append("Content-Type: application/json")
        headers.append(f"Content-Length: {len(raw)}")
    sock = socket.create_connection(("127.0.0.1", port), timeout=10)
    sock.sendall(("\r\n".join(headers) + "\r\n\r\n").encode() + raw)
    data = b""
    try:
        while chunk := sock.recv(65536):
            data += chunk
    except OSError:
        pass
    sock.close()
    status = int(data.split(b" ", 2)[1]) if data.startswith(b"HTTP/") else 0
    payload = data.split(b"\r\n\r\n", 1)[1] if b"\r\n\r\n" in data else b""
    return status, (json.loads(payload) if payload.startswith(b"{") else None)


def _wait_until(predicate, timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.05)
    assert predicate()


def test_capabilities_matches_across_socket_cli_and_http():
    with private_dir() as directory:
        token_file = os.path.join(directory, "http.token")
        host, _ = start(
            directory,
            sessions=1,
            producing=0,
            history=5,
            retain=50,
            extra=["--text", "--http-port", "auto", "--http-token-file", token_file],
        )
        try:
            port = host.http_port
            token = Path(token_file).read_text().strip()
            conn = Conn(host.socket)

            sock_cap = conn.call(op="capabilities", id="cap-sock")
            assert v1.validate_response("capabilities", sock_cap) == []
            assert sock_cap["id"] == "cap-sock"
            assert sock_cap["protocol"] == 1
            assert sock_cap["version"] == "0.3.2"
            assert "capabilities" in sock_cap["ops"]
            assert "account.rename" in sock_cap["ops"]
            assert "session.mode" in sock_cap["ops"]
            for call_op in ("call.list", "call.get", "call.start", "call.say", "call.hangup", "call.close"):
                assert call_op in sock_cap["ops"], call_op
            assert sock_cap["socket_only_ops"] == [
                "subscribe",
                "unsubscribe",
                "view.attach",
                "clipboard.write",
                "clipboard.done",
                "picture.slot",
            ]
            assert set(sock_cap["mcp_tools"]) == {
                "sessions",
                "send",
                "create",
                "history",
                "read_handoff",
                "rename",
                "capture",
                "clipboard_read",
                "clipboard_write",
                "computer",
                "llm_endpoints",
            }
            for route in (
                "POST /v1",
                "GET /v1/capabilities",
                "GET /v1/accounts",
                "POST /v1/accounts",
                "GET /v1/sessions/<id>/history",
                "GET /v1/search",
                "GET /v1/calls",
                "POST /v1/calls",
                "GET /v1/calls/<id>",
                "POST /v1/calls/<id>/say",
                "POST /v1/calls/<id>/hangup",
                "POST /v1/calls/<id>/close",
            ):
                assert route in sock_cap["http_routes"], route
            assert sock_cap["limits"]["max_request"] == v1.MAX_REQUEST
            assert sock_cap["limits"]["max_input_text"] == v1.MAX_INPUT_TEXT
            assert sock_cap["limits"]["retain"] == 50
            assert sock_cap["limits"]["font_px_min"] == 12
            assert sock_cap["limits"]["font_px_max"] == 20

            cli_run = cli(host, "capabilities")
            assert cli_run.returncode == 0, cli_run.stderr
            cli_cap = json.loads(cli_run.stdout.strip())
            assert v1.validate_response("capabilities", cli_cap) == []

            status_get, get_cap = _http(port, "GET", "/v1/capabilities", token)
            assert status_get == 200 and get_cap is not None
            assert v1.validate_response("capabilities", get_cap) == []

            status_post, post_cap = _http(port, "POST", "/v1", token, {"op": "capabilities", "id": "cap-http"})
            assert status_post == 200 and post_cap is not None
            assert v1.validate_response("capabilities", post_cap) == []
            assert post_cap["id"] == "cap-http"

            base = {k: v for k, v in sock_cap.items() if k != "id"}
            assert cli_cap == base
            assert get_cap == base
            assert {k: v for k, v in post_cap.items() if k != "id"} == base
        finally:
            host.stop()


def test_loopback_http_parity_for_accounts_sessions_history_search_and_post_v1():
    with private_dir() as directory:
        token_file = os.path.join(directory, "http.token")
        host, log = start(
            directory,
            sessions=1,
            producing=0,
            history=6,
            retain=30,
            extra=["--text", "--http-port", "auto", "--http-token-file", token_file],
        )
        try:
            sid = host.sessions[0]
            port = host.http_port
            token = Path(token_file).read_text().strip()
            conn = Conn(host.socket)

            status, created = _http(port, "POST", "/v1/accounts", token, {"provider": "claude", "label": "Ops Initial"})
            assert status == 200 and created is not None
            assert v1.validate_response("account.create", created) == []
            aid = created["account"]

            status, renamed = _http(port, "POST", "/v1", token, {"op": "account.rename", "account": aid, "label": "Ops Primary"})
            assert status == 200 and renamed is not None
            assert v1.validate_response("account.rename", renamed) == []
            assert renamed["label"] == "Ops Primary"

            status, listed = _http(port, "GET", "/v1/accounts", token)
            assert status == 200 and listed is not None
            assert v1.validate_response("account.list", listed) == []
            assert any(a["account"] == aid and a["label"] == "Ops Primary" for a in listed["accounts"])

            status, settings_upd = _http(port, "POST", "/v1", token, {"op": "settings.set", "group_by": "workspace", "font_px": 16})
            assert status == 200 and settings_upd is not None
            assert v1.validate_response("settings.set", settings_upd) == []
            assert settings_upd["settings"]["group_by"] == "workspace"
            assert settings_upd["settings"]["font_px"] == 16

            status, s_ren = _http(port, "POST", "/v1", token, {"op": "session.rename", "session": sid, "name": "parity session"})
            assert status == 200 and s_ren is not None
            assert v1.validate_response("session.rename", s_ren) == []

            status, inp = _http(
                port,
                "POST",
                "/v1",
                token,
                {"op": "input", "session": sid, "generation": 1, "request_id": "v1-in-1", "text": "needle phrase 42"},
            )
            assert status == 200 and inp is not None
            assert v1.validate_response("input", inp) == []
            _wait_until(lambda: "needle phrase 42" in echoes(log, sid))

            status, hist_get = _http(port, "GET", f"/v1/sessions/{sid}/history?before=9999&limit=20", token)
            assert status == 200 and hist_get is not None
            assert v1.validate_response("history", hist_get) == []
            status, hist_post = _http(port, "POST", "/v1", token, {"op": "history", "session": sid, "before": 9999, "limit": 20})
            assert status == 200 and hist_post == hist_get
            assert hist_get == conn.call(op="history", session=sid, before=9999, limit=20)

            query = urllib.parse.urlencode({"q": "needle phrase 42", "session": sid, "limit": 10})
            status, search_get = _http(port, "GET", f"/v1/search?{query}", token)
            assert status == 200 and search_get is not None
            assert v1.validate_response("session.search", search_get) == []
            assert len(search_get["matches"]) >= 1
            status, search_post = _http(port, "POST", "/v1", token, {"op": "session.search", "query": "needle phrase 42", "session": sid, "limit": 10})
            assert status == 200 and search_post == search_get

            status, ev_ok = _http(port, "GET", f"/v1/sessions/{sid}/events?after=-1", token)
            assert status == 200 and ev_ok is not None and ev_ok["gap"] is False and len(ev_ok["events"]) >= 1
            status, ev_gap = _http(port, "GET", f"/v1/sessions/{sid}/events?after=999999", token)
            assert status == 200 and ev_gap is not None and ev_gap["gap"] is True and "snapshot" in ev_gap
        finally:
            host.stop()


def test_post_v1_rejects_socket_only_attachment_and_malformed_queries():
    with private_dir() as directory:
        token_file = os.path.join(directory, "http.token")
        host, _ = start(
            directory,
            sessions=1,
            producing=0,
            history=2,
            extra=["--text", "--http-port", "auto", "--http-token-file", token_file],
        )
        try:
            sid = host.sessions[0]
            port = host.http_port
            token = Path(token_file).read_text().strip()

            socket_only_payloads = [
                {"op": "subscribe", "session": sid, "after": -1},
                {"op": "unsubscribe", "session": sid},
                {"op": "view.attach", "session": sid, "kind": "cli"},
                {"op": "clipboard.write", "kind": "text", "text": "secret"},
                {"op": "clipboard.done", "req": "r1", "ok": True},
                {"op": "picture.slot"},
                {"op": "input", "session": sid, "generation": 1, "request_id": "att-1", "text": "hi", "attachment": "paste0001.png"},
            ]
            for payload in socket_only_payloads:
                status, body = _http(port, "POST", "/v1", token, payload)
                assert status == 400 and body is not None and body["error"] == "bad_request", (payload, status, body)

            status, body = _http(port, "POST", "/v1", token, {"v": 2, "op": "status"})
            assert status == 400 and body is not None and body["error"] == "version_unsupported"

            status, body = _http(port, "POST", "/v1", token, {"op": "no.such.op"})
            assert status == 404 and body is not None and body["error"] == "unknown_op"

            status, _ = _http(port, "GET", "/v1", token)
            assert status == 405

            for bad_path in (
                "/v1/search",
                "/v1/search?q=%ZZ",
                "/v1/search?q=ok&extra=1",
                f"/v1/sessions/{sid}/history",
                f"/v1/sessions/{sid}/history?before=-1",
                f"/v1/sessions/{sid}/history?before=10&limit=999",
            ):
                status, body = _http(port, "GET", bad_path, token)
                assert status == 400 and body is not None and body["error"] == "bad_request", (bad_path, status, body)
        finally:
            host.stop()


def test_call_room_parity_across_socket_cli_and_http():
    with private_dir() as directory:
        token_file = os.path.join(directory, "http.token")
        host, _ = start(
            directory,
            sessions=2,
            producing=0,
            history=2,
            extra=["--text", "--http-port", "auto", "--http-token-file", token_file],
        )
        try:
            s1, s2 = host.sessions
            port = host.http_port
            token = Path(token_file).read_text().strip()
            conn = Conn(host.socket)
            assert conn.call(op="session.rename", session=s1, name="Pri-Lico")["ok"] == 1
            assert conn.call(op="session.rename", session=s2, name="Pri-Avatar")["ok"] == 1

            status, started = _http(port, "POST", "/v1/calls", token, {"from": s1, "to": s2, "text": "초기 호출 테스트"})
            assert status == 200 and started is not None
            assert v1.validate_response("call.start", started) == []
            cid = started["call"]["call_id"]
            assert started["call"]["title"] == "Pri-Lico · Pri-Avatar"

            cli_list = cli(host, "call", "list")
            assert cli_list.returncode == 0, cli_list.stderr
            listed = json.loads(cli_list.stdout.strip())
            assert v1.validate_response("call.list", listed) == []
            assert [c["call_id"] for c in listed["calls"]] == [cid]

            status, said = _http(port, "POST", f"/v1/calls/{cid}/say", token, {"text": "사람 중재 메시지"})
            assert status == 200 and said is not None
            assert v1.validate_response("call.say", said) == []

            cli_show = cli(host, "call", "show", cid[:7])
            assert cli_show.returncode == 0, cli_show.stderr
            shown = json.loads(cli_show.stdout.strip())
            assert v1.validate_response("call.get", shown) == []
            assert len(shown["call"]["entries"]) >= 2

            cli_hang = cli(host, "call", "hangup", cid)
            assert cli_hang.returncode == 0, cli_hang.stderr
            hung = json.loads(cli_hang.stdout.strip())
            assert v1.validate_response("call.hangup", hung) == []
            assert hung["call"]["status"] == "paused"

            status, closed = _http(port, "POST", f"/v1/calls/{cid}/close", token)
            assert status == 200 and closed is not None
            assert v1.validate_response("call.close", closed) == []
            assert closed["closed"] is True

            status, missing = _http(port, "GET", "/v1/calls/c00000000", token)
            assert status == 404 and missing is not None and missing["error"] == "unknown_call"
        finally:
            host.stop()
