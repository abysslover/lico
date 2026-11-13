"""Fixtures for the Local LLM discovery tests (M12): fake servers on ephemeral loopback ports that record what they were sent, and a fake process table root that lists
them, so the host reads a table and a set of socket inodes that the test wrote and never the real machine's. Nothing here opens a connection to anything but the test's
own servers."""

from __future__ import annotations

import json
import os
import socket
import socketserver
import threading
import time
from pathlib import Path


class _Handler(socketserver.BaseRequestHandler):
    def handle(self) -> None:
        server: FakeServer = self.server.owner  # type: ignore[attr-defined]
        server.connections += 1
        self.request.settimeout(2.0)
        if server.mode == "banner":  # speaks first, like sshd or a database
            self.request.sendall(b"SSH-2.0-fake_banner\r\n")
            try:
                data = self.request.recv(4096)
            except OSError:
                data = b""
            server.received.append(data)
            return
        if server.mode == "close":  # accepts and closes without a word
            return
        data = b""
        try:
            while b"\r\n\r\n" not in data and len(data) < 65536:
                chunk = self.request.recv(4096)
                if not chunk:
                    break
                data += chunk
        except OSError:
            pass
        server.received.append(data)
        if server.mode == "silent":  # reads the request and never answers
            time.sleep(server.hold)
            return
        path = data.split(b" ", 2)[1].decode("latin-1", "replace") if data.count(b" ") >= 2 else ""
        status, headers, body = server.answer(path)
        head = f"HTTP/1.0 {status}\r\n" + "".join(f"{k}: {v}\r\n" for k, v in headers.items()) + f"Content-Length: {len(body)}\r\n\r\n"
        try:
            self.request.sendall(head.encode() + body)
        except OSError:
            pass


class _TCP(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class _TCP6(_TCP):
    address_family = socket.AF_INET6


class FakeServer:
    """A server on an ephemeral port of `address`. `mode`: `http` (answers by `routes`), `banner`, `close`, `silent`."""

    def __init__(self, routes: dict | None = None, mode: str = "http", address: str = "127.0.0.1", hold: float = 6.0) -> None:
        self.routes, self.mode, self.address, self.hold = routes or {}, mode, address, hold
        self.received: list[bytes] = []
        self.connections = 0
        self._srv = (_TCP6 if ":" in address else _TCP)((address, 0), _Handler)
        self._srv.owner = self  # type: ignore[attr-defined]
        self.port = self._srv.server_address[1]
        self._thread = threading.Thread(target=lambda: self._srv.serve_forever(poll_interval=0.02), daemon=True)  # a short poll: closing 70 servers must not take half a minute
        self._thread.start()

    def answer(self, path: str):
        route = self.routes.get(path)
        if route is None:
            return "404 Not Found", {"Content-Type": "text/plain"}, b"not found"
        status, headers, body = route
        if isinstance(body, (dict, list)):
            body = json.dumps(body).encode()
            headers = {"Content-Type": "application/json", **headers}
        elif isinstance(body, str):
            body = body.encode()
        return status, headers, body

    def requests(self) -> list[tuple[str, str, dict]]:
        """(method, path, headers) of every request head the server received."""
        out = []
        for raw in self.received:
            head = raw.split(b"\r\n\r\n", 1)[0].decode("latin-1")
            lines = head.split("\r\n")
            method, path, version = (lines[0].split(" ") + ["", "", ""])[:3]
            out.append((method, path, {"version": version, **{k.lower(): v for k, v in (line.split(": ", 1) for line in lines[1:] if ": " in line)}}))
        return out

    def close(self) -> None:
        self._srv.shutdown()
        self._srv.server_close()


def openai_models(ids=("tiny-llama", "qwen-small")) -> dict:
    return {"/v1/models": ("200 OK", {}, {"object": "list", "data": [{"id": i, "object": "model", "owned_by": "local"} for i in ids]})}


def ollama_tags(names=("llama3:8b", "mistral:7b")) -> dict:
    return {"/api/tags": ("200 OK", {}, {"models": [{"name": n, "model": n, "size": 1} for n in names]})}


def keyed_server() -> dict:
    return {"/v1/models": ("401 Unauthorized", {}, {"error": {"message": "Incorrect API key provided", "type": "invalid_request_error", "code": "invalid_api_key"}})}


def _hex_v4(address: str) -> str:
    return "".join(f"{int(p):02X}" for p in reversed(address.split(".")))


def _hex_v6(address: str) -> str:
    raw = socket.inet_pton(socket.AF_INET6, address)
    return "".join(raw[i:i + 4][::-1].hex().upper() for i in range(0, 16, 4))


TCP_HEADER = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n"


def _row(n: int, local: str, state: str, uid: int, inode: int) -> str:
    remote = "00000000:0000" if len(local) < 20 else "00000000000000000000000000000000:0000"
    return f"{n:4d}: {local} {remote} {state} 00000000:00000000 00:00000000 00000000 {uid:5d}        0 {inode} 1 0000000000000000 100 0 0 10 0\n"


def write_proc(root: Path, sockets: list[dict], processes: dict[int, tuple[str, list[int]]], unreadable: tuple[int, ...] = ()) -> None:
    """A process table root that lists `sockets` (`port`, `address`, `inode`, optional `state`, `uid`) in `net/tcp` and `net/tcp6`, and the processes `{pid: (comm, [inodes])}`
    with a `socket:[inode]` link per socket under `<pid>/fd`; a pid in `unreadable` gets an fd folder that nobody may read (what another user's process looks like)."""
    net = root / "net"
    net.mkdir(parents=True, exist_ok=True)
    v4, v6 = [TCP_HEADER], [TCP_HEADER]
    for n, s in enumerate(sockets):
        address, state, uid = s["address"], s.get("state", "0A"), s.get("uid", os.getuid())
        if ":" in address:
            v6.append(_row(n, f"{_hex_v6(address)}:{s['port']:04X}", state, uid, s["inode"]))
        else:
            v4.append(_row(n, f"{_hex_v4(address)}:{s['port']:04X}", state, uid, s["inode"]))
    (net / "tcp").write_text("".join(v4))
    (net / "tcp6").write_text("".join(v6))
    for pid, (comm, inodes) in processes.items():
        fd = root / str(pid) / "fd"
        fd.mkdir(parents=True, exist_ok=True)
        (root / str(pid) / "comm").write_text(comm + "\n")
        for i, inode in enumerate(inodes):
            os.symlink(f"socket:[{inode}]", fd / str(3 + i))
        if pid in unreadable:
            fd.chmod(0)


def normalised(reply: dict) -> dict:
    """A reply without what differs from run to run (times)."""
    out = {k: v for k, v in reply.items() if k not in ("elapsed_ms", "v", "id")}
    out["endpoints"] = [{k: v for k, v in e.items() if k != "probe_ms"} for e in reply.get("endpoints", [])]
    return out
