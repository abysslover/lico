import json
import os
import random
import socket
import time
from pathlib import Path

from lico_measure.rig import private_dir
from lico_protocol import v1

from lico_measure.p02 import AGENT, LICO, ProtoConn as Conn, cli, echoes, read_log, start  # noqa: F401

from tests.test_codex_adapter import wait_for
from tests.test_folders import long_text


def test_stale_target_input_is_refused_without_dispatch():
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=0, history=5)
        try:
            sid = host.sessions[0]
            conn = Conn(host.socket)
            good = conn.call(op="input", session=sid, generation=1, request_id="r-good", text="alpha")
            assert good["ok"] == 1 and good["state"] in ("accepted", "submitted", "observed")
            assert v1.validate_response("input", good) == []
            refused = [
                dict(op="input", session=sid, generation=99, request_id="r-stale", text="bravo"),
                dict(op="input", session="s" + "0" * 16, generation=1, request_id="r-foreign", text="charlie"),
                dict(op="input", session=sid, generation=1, request_id="bad id!", text="delta"),
                dict(op="input", session=sid, generation=1, request_id="r-empty", text=""),
                dict(op="input", session=sid, generation=1, request_id="r-extra", text="echo", extra=1),
            ]
            for request in refused:
                reply = conn.call(**request)
                assert reply["ok"] == 0 and reply["error"] in ("stale_generation", "unknown_session", "bad_request"), reply
            # the command line speaks the same contract: it sends one more valid input
            done = cli(host, "session", "send", sid, "omega")
            assert done.returncode == 0, done.stderr
            # an exited session accepts nothing
            made = conn.call(op="session.create", kind="synthetic", history=3, rate=0)
            time.sleep(0.5)
            exited = conn.call(op="input", session=made["session"], generation=1, request_id="r-late", text="zulu")
            assert exited["ok"] == 0 and exited["error"] == "session_exited"
            time.sleep(1.0)
            seen = echoes(log, sid)
            assert seen == ["alpha", "omega"], seen          # nothing refused ever reached the agent
            assert not echoes(log, made["session"])
        finally:
            host.stop()


def test_reconnect_never_resends_ambiguous_input():
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=0, history=1)
        try:
            sid = host.sessions[0]
            rng = random.Random(7)
            ids = [f"in-{k:03d}" for k in range(200)]
            for rid in ids:
                conn = Conn(host.socket)
                conn.send(op="input", session=sid, generation=1, request_id=rid, text=f"text-{rid}")
                if rng.random() < 0.5:
                    conn.close()                       # the reply is never seen: the sender cannot know
                    retry = Conn(host.socket)          # so it reconnects and sends the same request again
                    reply = retry.call(op="input", session=sid, generation=1, request_id=rid, text=f"text-{rid}")
                    # either request may be handled first; the contract is that exactly one is dispatched
                    assert reply["ok"] == 1, reply
                    retry.close()
                else:
                    reply = conn.read()
                    assert reply["ok"] == 1 and reply["duplicate"] is False, reply
                    conn.close()
            time.sleep(1.5)
            seen = echoes(log, sid)
            assert sorted(seen) == sorted(f"text-{rid}" for rid in ids), "an input was lost or sent twice"
            probe = Conn(host.socket)
            state = probe.call(op="input.status", session=sid, request_id=ids[0])
            assert state["ok"] == 1 and state["state"] == "observed"
            assert v1.validate_response("input.status", state) == []
            # more inputs than the old tracking window of 4096: an early id is still not dispatched twice
            for k in range(4200):
                probe.send(op="input", session=sid, generation=1, request_id=f"bulk-{k:05d}", text="x")
            replies = 0
            while replies < 4200:
                if "ok" in probe.read():
                    replies += 1
            again = probe.call(op="input", session=sid, generation=1, request_id="bulk-00000", text="x")
            assert again["ok"] == 1 and again["duplicate"] is True, again
            assert probe.call(op="input.status", session=sid, request_id="never-sent")["error"] == "unknown_request"
        finally:
            host.stop()


def _http(port, method, path, headers=None, body=b"", raw_host=True):
    """One raw HTTP request with full control of the headers; returns (status, json body)."""
    sock = socket.create_connection(("127.0.0.1", port), timeout=10)
    head = [f"{method} {path} HTTP/1.1"]
    given = dict(headers or {})
    if raw_host and "Host" not in given:
        given["Host"] = f"127.0.0.1:{port}"
    given["Connection"] = "close"
    if body:
        given.setdefault("Content-Length", str(len(body)))
    head += [f"{k}: {v}" for k, v in given.items()]
    sock.sendall(("\r\n".join(head) + "\r\n\r\n").encode() + body)
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


def test_http_requires_credential_host_and_origin():
    with private_dir() as directory:
        token_file = os.path.join(directory, "http.token")
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        sock.close()
        host, log = start(directory, sessions=1, producing=0, history=3,
                          extra=["--http-port", str(port), "--http-token-file", token_file])
        try:
            sid = host.sessions[0]
            time.sleep(0.5)
            token = Path(token_file).read_text().strip()
            assert oct(os.stat(token_file).st_mode & 0o777) == "0o600"
            auth = {"Authorization": f"Bearer {token}"}
            assert token not in host.ready_line
            with open(f"/proc/{host.pid}/cmdline", "rb") as handle:
                assert token not in handle.read().decode(errors="replace")
            assert "Bearer" not in open(os.path.join(directory, "host.err"), errors="replace").read()
            # no or wrong credential
            assert _http(port, "GET", "/v1/status")[0] == 401
            assert _http(port, "GET", "/v1/status", {"Authorization": "Bearer nope"})[0] == 401
            assert _http(port, "GET", "/v1/status", {"Authorization": f"Basic {token}"})[0] == 401
            # right credential, wrong Host (DNS rebinding) or wrong Origin (a web page)
            assert _http(port, "GET", "/v1/status", {**auth, "Host": "evil.example"})[0] in (400, 403, 421)
            assert _http(port, "GET", "/v1/status", {**auth, "Origin": "http://evil.example"})[0] == 403
            # routes: unknown, wrong method, oversize
            assert _http(port, "GET", "/v1/nothing", auth)[0] == 404
            assert _http(port, "DELETE", "/v1/status", auth)[0] == 405
            assert _http(port, "POST", f"/v1/sessions/{sid}/input", {**auth, "Content-Length": str(v1.MAX_REQUEST + 1)}, b"x" * 100)[0] == 413      # refused on the declared length, before the body is read
            # a rejected mutation dispatches nothing
            status, body = _http(port, "POST", f"/v1/sessions/{sid}/input", {**auth, "Origin": "http://evil.example"},
                                 json.dumps({"generation": 1, "request_id": "h-bad", "text": "evil"}).encode())
            assert status == 403
            status, body = _http(port, "POST", f"/v1/sessions/{sid}/input", auth,
                                 json.dumps({"generation": 99, "request_id": "h-stale", "text": "stale"}).encode())
            assert status == 409 and body["error"] == "stale_generation"
            # the permitted request works and returns the documented fields
            status, body = _http(port, "POST", f"/v1/sessions/{sid}/input", auth,
                                 json.dumps({"generation": 1, "request_id": "h-ok", "text": "fine"}).encode())
            assert status == 200 and body["request_id"] == "h-ok"
            status, body = _http(port, "GET", "/v1/status", auth)
            assert status == 200 and body["sessions"][0]["session"] == sid
            status, body = _http(port, "GET", f"/v1/sessions/{sid}/events?after=-1", auth)
            assert status == 200 and body["gap"] is False and len(body["events"]) >= 3
            status, body = _http(port, "GET", f"/v1/sessions/{sid}/events?after=999999", auth)
            assert status == 200 and body["gap"] is True and "snapshot" in body
            time.sleep(1.0)
            assert echoes(log, sid) == ["fine"]
            # IN-01: the limit is the same here as on the socket. The largest text is accepted and all of it arrived (the echo's
            # marker counts the bytes the host received); one byte more is refused and nothing is delivered
            text = long_text(v1.MAX_INPUT_TEXT)
            status, body = _http(port, "POST", f"/v1/sessions/{sid}/input", auth,
                                 json.dumps({"generation": 1, "request_id": "h-big", "text": text}).encode())
            assert status == 200 and body["request_id"] == "h-big"
            shown = sum(len(line.encode()) + 1 for line in text.split("\n")[:12])
            total = text.count("\n") + (0 if text.endswith("\n") else 1)
            marker = f">> [{total - 12} more lines, {len(text.encode()) - shown} bytes not shown; the agent received all of it]"
            shown_back = lambda: [p for _, p in read_log(log, sid) if p.startswith(b">> ")]
            wait_for(lambda: marker.encode() in shown_back())
            before = len(shown_back())
            status, body = _http(port, "POST", f"/v1/sessions/{sid}/input", auth,
                                 json.dumps({"generation": 1, "request_id": "h-over", "text": long_text(v1.MAX_INPUT_TEXT + 1)}).encode())
            assert status == 400 and body["error"] == "bad_request"
            time.sleep(0.3)
            assert len(shown_back()) == before                        # nothing delivered, nothing echoed
        finally:
            host.stop()
def test_http_answers_401_and_403_from_the_head_alone_without_waiting_for_a_body():
    """F6: a request that declares a body but never sends it, with no credential or a wrong Host or Origin, is answered at once; the body is read only after
    the head has passed (it used to be read first, until the read timed out, so a caller without the token held a connection for five seconds and answered 400)."""
    with private_dir() as directory:
        token_file = os.path.join(directory, "http.token")
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        sock.close()
        host, log = start(directory, sessions=1, producing=0, history=3, extra=["--http-port", str(port), "--http-token-file", token_file])
        try:
            sid = host.sessions[0]
            wait_for(lambda: _http(port, "GET", "/v1/status")[0] == 401)
            token = Path(token_file).read_text().strip()
            auth = {"Authorization": f"Bearer {token}"}
            declared = {"Content-Length": str(v1.MAX_REQUEST)}                     # the head of a request whose body never comes
            for what, headers, want in [("no credential", declared, 401), ("a wrong credential", {**declared, "Authorization": "Bearer nope"}, 401),
                                        ("a wrong Host", {**auth, **declared, "Host": "evil.example"}, 403), ("a wrong Origin", {**auth, **declared, "Origin": "http://evil.example"}, 403),
                                        ("no credential, more than the limit declared", {"Content-Length": str(v1.MAX_REQUEST + 1)}, 401)]:
                started = time.monotonic()
                got = _http(port, "POST", f"/v1/sessions/{sid}/input", headers)[0]
                took = time.monotonic() - started
                assert got == want and took < 3, (what, got, round(took, 1))
            status, body = _http(port, "POST", f"/v1/sessions/{sid}/input", auth, json.dumps({"generation": 1, "request_id": "h-after", "text": "fine"}).encode())
            assert status == 200 and body["request_id"] == "h-after"               # the body of a request that passes is still read and used
        finally:
            host.stop()


