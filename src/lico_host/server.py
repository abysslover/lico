"""Synthetic host: owns sessions and their output independently of any GUI.

Protocol: one JSON object per line over an owner-only Unix socket. Requests:
status, snapshot, subscribe, unsubscribe. Output records are pushed only to
subscribed connections; a hidden view unsubscribes and keeps its cursor.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import socket
import struct
import time
from collections import deque

from lico_measure.workload import record

RETAIN = 2048        # records kept per session for replay after a cursor
SNAPSHOT_LINES = 40  # lines in the latest-screen snapshot
SUBSCRIBER_QUEUE = 1024
MAX_REQUEST = 4096


def dumps(message: dict) -> str:
    return json.dumps(message, separators=(",", ":"))  # compact: clients parse it directly


def _line_text(seq: int, payload: bytes) -> str:
    return f"{seq:08d} {payload[:44].hex()}"


class Session:
    def __init__(self, index: int, producing: bool, history: int) -> None:
        digest = hashlib.sha256(f"lico-session-{index}".encode()).hexdigest()
        self.id = "s" + digest[:10]            # opaque, not an index
        self.generation = 1
        self.producing = producing
        self.ring: deque[tuple[int, bytes]] = deque(maxlen=RETAIN)
        self.next_seq = 0
        self.bytes_total = 0
        self.subscribers: set[Subscriber] = set()
        for _ in range(history):
            self.append()

    @property
    def last_seq(self) -> int:
        return self.next_seq - 1

    def append(self) -> None:
        rec = record(self.next_seq)
        self.ring.append((rec.seq, rec.payload))
        self.next_seq += 1
        self.bytes_total += len(rec.payload)
        for sub in list(self.subscribers):
            sub.offer(self, rec.seq, rec.payload)

    def snapshot(self) -> dict:
        tail = list(self.ring)[-SNAPSHOT_LINES:]
        return {
            "session": self.id, "generation": self.generation, "seq": self.last_seq,
            "lines": [[seq, _line_text(seq, data)] for seq, data in tail],
        }


class Subscriber:
    def __init__(self, writer: asyncio.StreamWriter) -> None:
        self.writer = writer
        self.queue: asyncio.Queue[bytes | None] = asyncio.Queue(SUBSCRIBER_QUEUE)
        self.task = asyncio.ensure_future(self._pump())
        self.overflowed: set[str] = set()

    def send(self, message: dict) -> bool:
        try:
            self.queue.put_nowait((dumps(message) + "\n").encode())
            return True
        except asyncio.QueueFull:
            return False

    def offer(self, session: Session, seq: int, payload: bytes) -> None:
        if session.id in self.overflowed:
            return
        message = {"ev": "rec", "session": session.id, "generation": session.generation,
                   "seq": seq, "data": payload.hex()}
        if not self.send(message):
            # never stall the host: drop this view's stream, it must re-snapshot
            self.overflowed.add(session.id)
            session.subscribers.discard(self)
            self.writer.write((dumps({"ev": "overflow", "session": session.id}) + "\n").encode())

    async def _pump(self) -> None:
        try:
            while True:
                data = await self.queue.get()
                if data is None:
                    return
                self.writer.write(data)
                await self.writer.drain()
        except (ConnectionError, asyncio.CancelledError):
            return


class Host:
    def __init__(self, sessions: int, producing: int, rate: float, history: int) -> None:
        self.sessions = [Session(i, i < producing, history) for i in range(sessions)]
        self.by_id = {s.id: s for s in self.sessions}
        self.rate = rate

    async def produce(self, session: Session) -> None:
        interval = 1.0 / self.rate
        next_at = time.monotonic()
        while True:
            next_at += interval
            session.append()
            delay = next_at - time.monotonic()
            await asyncio.sleep(delay if delay > 0 else 0)

    def status(self) -> dict:
        return {"ok": 1, "pid": os.getpid(), "sessions": [
            {"session": s.id, "generation": s.generation, "seq": s.last_seq,
             "producing": s.producing, "subscribers": len(s.subscribers),
             "bytes_total": s.bytes_total} for s in self.sessions]}

    def handle(self, request: dict, sub: Subscriber) -> None:
        """Queue exactly one reply on the connection; replayed records follow it."""
        op = request.get("op")
        if op == "status":
            sub.send(self.status())
            return
        if op not in ("snapshot", "subscribe", "unsubscribe"):
            sub.send({"ok": 0, "error": "unknown_op"})
            return
        session = self.by_id.get(request.get("session"))
        if session is None:
            sub.send({"ok": 0, "error": "unknown_session"})
            return
        generation = request.get("generation")
        if generation is not None and generation != session.generation:
            sub.send({"ok": 0, "error": "stale_generation"})
            return
        if op == "snapshot":
            sub.send({"ok": 1, **session.snapshot()})
            return
        if op == "unsubscribe":
            session.subscribers.discard(sub)
            sub.overflowed.discard(session.id)
            sub.send({"ok": 1, "session": session.id, "seq": session.last_seq})
            return
        after = request.get("after")
        oldest = session.ring[0][0] if session.ring else session.next_seq
        reply: dict = {"ok": 1, "session": session.id, "generation": session.generation}
        if not isinstance(after, int) or after + 1 < oldest or after > session.last_seq:
            reply["gap"] = True                 # cursor unusable: client redraws from a snapshot
            reply["snapshot"] = session.snapshot()
            after = session.last_seq
        else:
            reply["gap"] = False
        reply["from"] = after + 1
        sub.send(reply)
        sub.overflowed.discard(session.id)
        replay = [(seq, data) for seq, data in session.ring if seq > after]
        session.subscribers.add(sub)  # live records follow the replay with no gap
        for seq, payload in replay:
            sub.offer(session, seq, payload)


async def serve(path: str, sessions: int, producing: int, rate: float, history: int) -> None:
    host = Host(sessions, producing, rate, history)
    tasks = [asyncio.ensure_future(host.produce(s)) for s in host.sessions if s.producing]

    async def connection(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        sock = writer.get_extra_info("socket")
        uid = struct.unpack("3i", sock.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))[1]
        if uid != os.getuid():
            writer.close()
            return
        sub = Subscriber(writer)
        try:
            while True:
                try:
                    line = await reader.readline()
                except (ConnectionError, ValueError):
                    break  # peer vanished or sent an over-long line
                if not line or len(line) > MAX_REQUEST:
                    break
                try:
                    request = json.loads(line)
                except ValueError:
                    request = None
                if isinstance(request, dict):
                    host.handle(request, sub)
                else:
                    sub.send({"ok": 0, "error": "bad_request"})
        finally:
            for s in host.sessions:
                s.subscribers.discard(sub)
            sub.task.cancel()
            writer.close()

    if os.path.exists(path):
        os.unlink(path)
    server = await asyncio.start_unix_server(connection, path=path)
    os.chmod(path, 0o600)
    print(json.dumps({"ready": 1, "pid": os.getpid(), "sessions": [s.id for s in host.sessions]}), flush=True)
    async with server:
        await asyncio.gather(server.serve_forever(), *tasks)
