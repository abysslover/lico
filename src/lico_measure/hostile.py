"""Hostile-input cases against the synthetic host, with observable effects."""

from __future__ import annotations

import socket

from .rig import HostConn

CANARY = "LICO_SYNTH_CANARY_00000000000000000000000000000000"


def _subscribers(conn: HostConn) -> int:
    return sum(s["subscribers"] for s in conn.call(op="status")["sessions"])


def _case(name: str, rejected: bool, no_effect: bool, detail: str) -> dict:
    return {"case": name, "rejected": rejected, "no_effect": no_effect, "detail": detail}


def run_hostile_cases(host) -> list[dict]:
    conn = HostConn(host.socket)
    session = host.sessions[0]
    results = []
    baseline = _subscribers(conn)

    reply = conn.call(op="snapshot", session="s_foreign")
    results.append(_case("unknown_session", reply.get("ok") == 0, _subscribers(conn) == baseline,
                         reply.get("error", "")))

    guesses = [conn.call(op="subscribe", session=f"s{i:010x}", after=0) for i in range(50)]
    results.append(_case("guessed_id", all(g.get("ok") == 0 for g in guesses),
                         _subscribers(conn) == baseline, "50 sequential guesses refused"))

    reply = conn.call(op="subscribe", session=session, generation=99, after=0)
    results.append(_case("stale_generation", reply.get("error") == "stale_generation",
                         _subscribers(conn) == baseline, reply.get("error", "")))

    raw = socket.socket(socket.AF_UNIX)
    raw.connect(host.socket)
    raw.sendall(b"x" * 8192 + b"\n")
    raw.settimeout(5)
    try:
        while raw.recv(4096):          # an error line may come first; the stream must then end
            pass
        closed = True
    except ConnectionResetError:
        closed = True
    except socket.timeout:
        closed = False
    raw.close()
    results.append(_case("oversize_request", closed, _subscribers(conn) == baseline,
                         "8192-byte line closes the connection"))

    raw = HostConn(host.socket)
    raw.sock.sendall(b"not json\n")
    bad = raw.next_event()
    raw.close()
    results.append(_case("malformed_json", bad.get("error") == "bad_request",
                         _subscribers(conn) == baseline, bad.get("error", "")))

    reply = conn.call(op="shutdown")
    results.append(_case("unknown_op", reply.get("error") == "unknown_op",
                         conn.call(op="status").get("ok") == 1, reply.get("error", "")))

    with open(f"/proc/{host.pid}/cmdline", "rb") as handle:
        argv = handle.read().decode(errors="replace")
    # the canary is sent in a request field and must not come back anywhere the host writes
    replies = [repr(conn.call(op="snapshot", session=CANARY)), repr(conn.call(op="subscribe", session=CANARY, after=0)),
               repr(conn.call(op="status")), repr(conn.call(op="snapshot", session=session))]
    sinks = [argv, host.ready_line] + replies
    leaked = any(CANARY in sink for sink in sinks)
    results.append(_case("canary_in_sinks", not leaked, _subscribers(conn) == baseline,
                         "canary sent in a request; argv, ready line and replies scanned"))
    conn.close()
    return results


# ------------------------------------------------------------- the owned host
def http_request(port: int, method: str, path: str, headers: dict | None = None, body: bytes = b"",
                 send_host: bool = True) -> tuple[int, dict | None, str]:
    """One raw HTTP request with full control of the headers: (status, json body, raw text)."""
    import json as _json

    sock = socket.create_connection(("127.0.0.1", port), timeout=10)
    given = dict(headers or {})
    if send_host and "Host" not in given:
        given["Host"] = f"127.0.0.1:{port}"
    given["Connection"] = "close"
    if body:
        given["Content-Length"] = str(len(body))
    head = [f"{method} {path} HTTP/1.1"] + [f"{k}: {v}" for k, v in given.items()]
    sock.sendall(("\r\n".join(head) + "\r\n\r\n").encode() + body)
    data = b""
    try:
        while chunk := sock.recv(65536):
            data += chunk
    except OSError:
        pass
    sock.close()
    text = data.decode(errors="replace")
    status = int(text.split(" ", 2)[1]) if text.startswith("HTTP/") else 0
    payload = text.split("\r\n\r\n", 1)[1] if "\r\n\r\n" in text else ""
    return status, (_json.loads(payload) if payload.startswith("{") else None), text


def run_hostd_suite() -> list[dict]:
    """Hostile requests against the owned host over its local socket and its HTTP listener."""
    import json as _json
    import os
    import time

    from .p02 import ProtoConn, echoes, start
    from .rig import private_dir

    cap = 40
    results: list[dict] = []
    with private_dir() as directory:
        probe = socket.socket()
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
        probe.close()
        token_file = os.path.join(directory, "http.token")
        host, log = start(directory, sessions=2, producing=0, history=3,
                          extra=["--http-port", str(port), "--http-token-file", token_file, "--max-connections", str(cap),
                                 "--max-sessions", "6"])
        sinks: list[str] = []
        try:
            time.sleep(0.5)
            token = open(token_file).read().strip()
            auth = {"Authorization": f"Bearer {token}"}
            sid = host.sessions[0]
            ipc = ProtoConn(host.socket)

            def state() -> tuple:
                status = ipc.call(op="status")["sessions"]
                return (len(status), sum(s["subscribers"] for s in status),
                        sum(len(echoes(log, s["session"])) for s in status))

            base = state()

            def case(name: str, rejected: bool, detail: str) -> None:
                results.append({"case": name, "rejected": rejected, "no_effect": state() == base, "detail": detail})

            def ipc_error(name: str, **request) -> None:
                reply = ipc.call(**request)
                sinks.append(repr(reply))
                case(name, reply.get("ok") == 0, reply.get("error", "accepted"))

            ipc_error("ipc_foreign_target_input", op="input", session="s" + "0" * 16, generation=1, request_id="h1", text="x")
            ipc_error("ipc_stale_generation_input", op="input", session=sid, generation=99, request_id="h2", text="x")
            ipc_error("ipc_injected_request_id", op="input", session=sid, generation=1, request_id="h3\nI forged hex", text="x")
            ipc_error("ipc_request_id_with_space", op="input", session=sid, generation=1, request_id="h4 forged", text="x")
            ipc_error("ipc_control_characters_in_text", op="input", session=sid, generation=1, request_id="h5", text="a\nb")
            ipc_error("ipc_unknown_view", op="view.detach", view="w" + "0" * 16)
            ipc_error("ipc_unknown_approval", op="approval.answer", session=sid, generation=1, approval_id=7, answer="allow")
            ipc_error("ipc_unknown_request_status", op="input.status", session=sid, request_id="never-sent")
            ipc_error("ipc_canary_as_session", op="snapshot", session=CANARY)
            ipc_error("ipc_canary_as_request_id", op="input", session=sid, generation=1, request_id=CANARY + "!", text="x")
            created = ipc.call(op="session.create", kind="synthetic", history=2, rate=0)
            time.sleep(0.6)
            reply = ipc.call(op="input", session=created["session"], generation=1, request_id="h6", text="late")
            flood = [ipc.call(op="session.create", kind="synthetic", history=1, rate=0) for _ in range(10)]
            refused_creates = sum(1 for r in flood if r.get("error") == "busy")
            results.append({"case": "ipc_session_flood", "rejected": refused_creates >= 1 and len(ipc.call(op="status")["sessions"]) <= 6,
                            "no_effect": True, "detail": f"{refused_creates} of 10 session.create refused once the cap (6) was reached"})
            results.append({"case": "ipc_input_to_exited_session", "rejected": reply.get("error") == "session_exited",
                            "no_effect": not echoes(log, created["session"]), "detail": reply.get("error", "accepted")})
            base = state()

            # a request that never ends its line cannot make the host buffer without bound
            slow = ProtoConn(host.socket)
            slow.sock.sendall(b"x" * 3000)
            time.sleep(0.5)
            slow.sock.sendall(b"y" * 3000)
            try:
                data = slow.sock.recv(256)
                closed = b"too_large" in data or data == b""
            except OSError:
                closed = True
            case("ipc_unterminated_request", closed, "refused once the line passes 4096 bytes")

            # more local connections than the cap: the extra ones are told the host is busy
            held = [ProtoConn(host.socket) for _ in range(cap + 10)]
            time.sleep(0.5)
            busy = 0
            for conn in held:
                conn.sock.settimeout(0.3)
                try:
                    if b"busy" in conn.sock.recv(64):
                        busy += 1
                except OSError:
                    pass
            alive = ipc.call(op="hello")["ok"] == 1
            for conn in held:
                conn.close()
            case("ipc_connection_cap", busy >= 1 and alive, f"{busy} of {cap + 10} extra connections told busy; an existing one still served")

            def http(name: str, expect: int, method: str, path: str, headers=None, body=b"", send_host=True) -> None:
                status, parsed, text = http_request(port, method, path, headers, body, send_host)
                sinks.append(text)
                case(name, status == expect, f"HTTP {status}")

            http("http_no_credential", 401, "GET", "/v1/status")
            http("http_wrong_credential", 401, "GET", "/v1/status", {"Authorization": "Bearer " + CANARY})
            http("http_wrong_scheme", 401, "GET", "/v1/status", {"Authorization": "Basic " + token})
            http("http_wrong_host", 403, "GET", "/v1/status", {**auth, "Host": "evil.example"})
            http("http_canary_as_host", 403, "GET", "/v1/status", {**auth, "Host": CANARY})
            http("http_wrong_origin", 403, "GET", "/v1/status", {**auth, "Origin": "http://evil.example"})
            http("http_canary_as_origin", 403, "GET", "/v1/status", {**auth, "Origin": "http://" + CANARY})
            http("http_unknown_route", 404, "GET", "/v1/nothing", auth)
            http("http_canary_in_path", 404, "GET", f"/v1/sessions/{CANARY}", auth)
            http("http_canary_in_query", 400, "GET", f"/v1/sessions/{sid}/events?after={CANARY}", auth)
            http("http_wrong_method", 405, "DELETE", "/v1/status", auth)
            http("http_oversize_body", 413, "POST", f"/v1/sessions/{sid}/input", auth, b"x" * 9000)
            http("http_stale_generation", 409, "POST", f"/v1/sessions/{sid}/input", auth,
                 _json.dumps({"generation": 99, "request_id": "hh1", "text": "x"}).encode())
            http("http_cross_site_mutation", 403, "POST", f"/v1/sessions/{sid}/input", {**auth, "Origin": "http://evil.example"},
                 _json.dumps({"generation": 1, "request_id": "hh2", "text": "x"}).encode())
            # a burst is throttled, and the host serves normally afterwards
            statuses = [http_request(port, "GET", "/v1/status", auth)[0] for _ in range(300)]
            time.sleep(1.2)
            after = http_request(port, "GET", "/v1/status", auth)[0]
            case("http_throttle", 429 in statuses and after == 200, f"{statuses.count(429)} of 300 throttled; then HTTP {after}")
            # more idle connections than the HTTP cap: the extra ones are refused at once
            idle = [socket.create_connection(("127.0.0.1", port), timeout=2) for _ in range(40)]
            refused = 0
            for s in idle:
                s.settimeout(0.5)
                try:
                    if s.recv(32).startswith(b"HTTP/1.1 503"):
                        refused += 1
                except OSError:
                    pass
            for s in idle:
                s.close()
            case("http_connection_cap", refused >= 1, f"{refused} of 40 idle connections refused with 503")

            # canaries sent in requests, and the real credential, must be absent from every sink
            with open(f"/proc/{host.pid}/cmdline", "rb") as handle:
                argv = handle.read().decode(errors="replace")
            logs = "".join(open(log / name, "rb").read().decode(errors="replace") for name in os.listdir(log))
            sinks += [argv, host.ready_line, open(os.path.join(directory, "host.err"), errors="replace").read(), logs]
            leaked = any(CANARY in s for s in sinks)
            token_leaked = any(token in s for s in sinks)
            results.append({"case": "canary_in_every_sink", "rejected": not leaked, "no_effect": True,
                            "detail": "argv, ready line, standard error, durable log and every reply and HTTP body scanned"})
            results.append({"case": "credential_in_every_sink", "rejected": not token_leaked, "no_effect": True,
                            "detail": "the control credential is only in its private file"})
            results.append({"case": "peer_of_another_user", "rejected": False, "no_effect": True,
                            "detail": "untested: no second user is available; the check is in the code"})
        finally:
            host.stop()
    return results
