import json
import socket
import time

import pytest

from lico_measure import legacy
from lico_measure.rig import Host, host_impls, private_dir
from lico_protocol import v1


class Conn:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(path)
        self.sock.settimeout(5)
        self.buf = b""

    def send_raw(self, data):
        self.sock.sendall(data)

    def read(self):
        while b"\n" not in self.buf:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise EOFError
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)

    def call(self, **request):
        self.send_raw((json.dumps(request) + "\n").encode())
        return self.read()

    def read_until_eof(self):
        data = self.buf
        try:
            while True:
                chunk = self.sock.recv(65536)
                if not chunk:
                    return data, True
                data += chunk
        except ConnectionResetError:
            return data, True          # the host closed with request bytes still unread
        except socket.timeout:
            return data, False

    def close(self):
        self.sock.close()


@pytest.mark.parametrize("impl", legacy.hosts(host_impls()))
def test_protocol_rejects_unknown_fields_and_oversize(impl):
    with private_dir() as directory:
        host = Host(directory, sessions=2, producing=0, history=100, retain=50, impl=impl)
        try:
            conn = Conn(host.socket)
            hello = conn.call(op="hello")
            assert v1.validate_response("hello", hello) == []
            assert hello["protocol"] == 1 and hello["max_request"] == (4096 if impl in legacy.HOSTS else v1.MAX_REQUEST) and hello["retain"] == 50
            status = conn.call(op="status")
            assert v1.validate_response("status", status) == []
            ids = [s["session"] for s in status["sessions"]]
            # ids are opaque and random: unique, long, and not an index
            assert len(set(ids)) == 2 and all(len(i) >= 17 for i in ids)
            assert conn.call(op="status", id="req-1")["id"] == "req-1"
            sid = ids[0]
            expected = [
                (dict(op="status", extra=1), "bad_request"),
                (dict(op="subscribe", session=sid, after=0, snapshot="yes"), "bad_request"),
                (dict(op="subscribe", session=sid), "bad_request"),
                (dict(op="snapshot"), "bad_request"),
                (dict(op="subscribe", session=sid, after=True), "bad_request"),
                (dict(v=2, op="status"), "version_unsupported"),
                (dict(op="nope"), "unknown_op"),
                (dict(op="snapshot", session="s" + "0" * 16), "unknown_session"),
                (dict(op="subscribe", session=sid, after=0, generation=99), "stale_generation"),
            ]
            for request, code in expected:
                reply = conn.call(**request)
                assert v1.validate_error(reply) == [], (request, reply)
                assert reply["ok"] == 0 and reply["error"] == code, (request, reply)
            # rejected requests had no effect
            assert all(s["subscribers"] == 0 for s in conn.call(op="status")["sessions"])
            bad = Conn(host.socket)
            bad.send_raw(b"not json\n")
            assert bad.read()["error"] == "bad_request"
            big = Conn(host.socket)
            big.send_raw(b"x" * (v1.MAX_REQUEST + 4096) + b"\n")
            data, closed = big.read_until_eof()
            assert closed, "an oversize request must close the connection"
            if data:
                assert json.loads(data.splitlines()[0])["error"] == "too_large"
        finally:
            host.stop()


@pytest.mark.parametrize("impl", legacy.hosts(host_impls()))
def test_cursor_contract_no_gap_no_duplicate(impl):
    with private_dir() as directory:
        host = Host(directory, sessions=1, producing=1, rate=200, history=300, retain=50, impl=impl)
        try:
            sid = host.sessions[0]
            conn = Conn(host.socket)
            reply = conn.call(op="subscribe", session=sid, after=-1, snapshot=True)
            assert v1.validate_response("subscribe", reply) == []
            assert reply["gap"] is True and reply["from"] == reply["snapshot"]["seq"] + 1
            seqs = []
            while len(seqs) < 60:
                event = conn.read()
                assert v1.validate_event(event) == [], event
                if event["ev"] == "rec":
                    seqs.append(event["seq"])
            assert seqs[0] == reply["from"] and seqs == list(range(seqs[0], seqs[0] + len(seqs)))
            conn.send_raw((json.dumps({"op": "unsubscribe", "session": sid}) + "\n").encode())
            while True:                      # records already queued may precede the reply
                message = conn.read()
                if "ok" in message:
                    break
            assert v1.validate_response("unsubscribe", message) == []
            # reattach after the last seen sequence: replay starts right after it, nothing twice
            conn2 = Conn(host.socket)
            cursor = seqs[-1]
            again = conn2.call(op="subscribe", session=sid, after=cursor)
            assert again["gap"] is False and again["from"] == cursor + 1
            replay = []
            while len(replay) < 30:
                event = conn2.read()
                if event["ev"] == "rec":
                    replay.append(event["seq"])
            assert replay == list(range(cursor + 1, cursor + 1 + len(replay)))
            # a cursor older than the retained window, or from the future, is a gap with a snapshot
            time.sleep(0.5)
            for stale in (0, 10**9):
                conn3 = Conn(host.socket)
                gap = conn3.call(op="subscribe", session=sid, after=stale)
                assert gap["gap"] is True and "snapshot" in gap and gap["from"] == gap["snapshot"]["seq"] + 1
        finally:
            host.stop()
