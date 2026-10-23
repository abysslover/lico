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
        closed = raw.recv(64) == b""        # a real end-of-stream, not a receive timeout
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
