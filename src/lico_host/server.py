"""Reference oracle for protocol v1: owns synthetic sessions independently of any GUI.

One JSON object per line over an owner-only Unix socket (see lico_protocol.v1).
Output records are pushed only to subscribed connections; a hidden view
unsubscribes and keeps its cursor. This is the behavior every candidate host must
reproduce; it is not the shipped host.
"""

from __future__ import annotations

import asyncio
import json
import os
import secrets
import socket
import struct
import time
from collections import deque

from lico_measure.workload import record
from lico_protocol import v1

SNAPSHOT_LINES = 40


def dumps(message: dict) -> str:
    return json.dumps(message, separators=(",", ":"))  # compact, one fixed shape


def _line_text(seq: int, payload: bytes) -> str:
    return f"{seq:08d} {payload[:44].hex()}"


class Session:
    def __init__(self, producing: bool, history: int, retain: int) -> None:
        self.id = "s" + secrets.token_hex(8)       # opaque and random, not an index
        self.generation = 1
        self.producing = producing
        self.ring: deque[tuple[int, bytes]] = deque(maxlen=retain)
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
        return {"session": self.id, "generation": self.generation, "seq": self.last_seq,
                "lines": [[seq, _line_text(seq, data)] for seq, data in tail]}


class Subscriber:
    def __init__(self, writer: asyncio.StreamWriter, queue_size: int) -> None:
        self.writer = writer
        self.queue: asyncio.Queue[bytes | None] = asyncio.Queue(queue_size)
        self.task = asyncio.ensure_future(self._pump())
        self.overflowed: set[str] = set()

    def send(self, message: dict) -> bool:
        try:
            self.queue.put_nowait((dumps({"v": v1.PROTOCOL, **message}) + "\n").encode())
            return True
        except asyncio.QueueFull:
            return False

    def offer(self, session: Session, seq: int, payload: bytes) -> None:
        if session.id in self.overflowed:
            return
        if not self.send({"ev": "rec", "session": session.id, "generation": session.generation,
                          "seq": seq, "data": payload.hex()}):
            # never stall the host: drop this view's stream, it must re-snapshot
            self.overflowed.add(session.id)
            session.subscribers.discard(self)
            self.writer.write((dumps({"v": v1.PROTOCOL, "ev": "overflow", "session": session.id}) + "\n").encode())

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
    def __init__(self, sessions: int, producing: int, rate: float, history: int, retain: int) -> None:
        self.sessions = [Session(i < producing, history, retain) for i in range(sessions)]
        self.by_id = {s.id: s for s in self.sessions}
        self.rate, self.retain = rate, retain

    async def produce(self, session: Session) -> None:
        interval = 1.0 / self.rate
        next_at = time.monotonic()
        while True:
            next_at += interval
            session.append()
            delay = next_at - time.monotonic()
            await asyncio.sleep(delay if delay > 0 else 0)

    def handle(self, request: object, sub: Subscriber) -> None:
        """Queue exactly one reply on the connection; replayed records follow it."""
        def fail(code: str, rid: object = None) -> None:
            sub.send({"ok": 0, "error": code, **({"id": rid} if isinstance(rid, str) and len(rid) <= 64 else {})})

        if not isinstance(request, dict):
            return fail("bad_request")
        rid = request.get("id")
        if "v" in request and request["v"] != v1.PROTOCOL and isinstance(request["v"], int):
            return fail("version_unsupported", rid)
        op = request.get("op")
        if not isinstance(op, str):
            return fail("bad_request", rid)
        if op not in v1.REQUESTS:
            return fail("unknown_op", rid)
        if v1.validate_request(op, request):
            return fail("bad_request", rid)

        def reply(body: dict) -> None:
            sub.send({"ok": 1, **({"id": rid} if rid is not None else {}), **body})

        if op == "hello":
            return reply({"protocol": v1.PROTOCOL, "host": "lico-oracle", "max_request": v1.MAX_REQUEST,
                          "retain": self.retain, "ops": list(v1.ORACLE_OPS)})
        if op == "status":
            return reply({"pid": os.getpid(), "sessions": [
                {"session": s.id, "generation": s.generation, "seq": s.last_seq, "producing": s.producing,
                 "subscribers": len(s.subscribers), "bytes_total": s.bytes_total} for s in self.sessions]})
        session = self.by_id.get(request["session"])
        if session is None:
            return fail("unknown_session", rid)
        generation = request.get("generation")
        if generation is not None and generation != session.generation:
            return fail("stale_generation", rid)
        if op == "snapshot":
            return reply(session.snapshot())
        if op == "unsubscribe":
            session.subscribers.discard(sub)
            sub.overflowed.discard(session.id)
            return reply({"session": session.id, "seq": session.last_seq})
        after = request["after"]
        oldest = session.ring[0][0] if session.ring else session.next_seq
        body: dict = {"session": session.id, "generation": session.generation}
        if request.get("snapshot") is True:
            # reactivation: the latest screen now, live records after it; the cursor only
            # tells the client whether it missed anything the host still retains
            snap = session.snapshot()
            valid = oldest <= after + 1 and after <= session.last_seq
            body.update({"gap": not valid, "snapshot": snap, "from": snap["seq"] + 1,
                         "missed": snap["seq"] - after if valid else None})
            reply(body)
            sub.overflowed.discard(session.id)
            session.subscribers.add(sub)
            return
        if after + 1 < oldest or after > session.last_seq:
            body["gap"] = True                  # cursor unusable: client redraws from a snapshot
            body["snapshot"] = session.snapshot()
            after = session.last_seq
        else:
            body["gap"] = False
        body["from"] = after + 1
        reply(body)
        sub.overflowed.discard(session.id)
        replay = [(seq, data) for seq, data in session.ring if seq > after]
        session.subscribers.add(sub)  # live records follow the replay with no gap
        for seq, payload in replay:
            sub.offer(session, seq, payload)


async def serve(path: str, sessions: int, producing: int, rate: float, history: int, retain: int) -> None:
    host = Host(sessions, producing, rate, history, retain)
    tasks = [asyncio.ensure_future(host.produce(s)) for s in host.sessions if s.producing]

    async def connection(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        sock = writer.get_extra_info("socket")
        uid = struct.unpack("3i", sock.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))[1]
        if uid != os.getuid():
            writer.close()
            return
        sub = Subscriber(writer, retain + 1024)  # a full replay always fits; only a slow reader overflows
        try:
            while True:
                try:
                    line = await reader.readline()
                except (ConnectionError, ValueError):
                    break  # peer vanished or sent an over-long line
                if not line:
                    break
                if len(line) > v1.MAX_REQUEST:
                    sub.send({"ok": 0, "error": "too_large"})
                    await asyncio.sleep(0)
                    break
                try:
                    request = json.loads(line)
                except ValueError:
                    request = None
                host.handle(request, sub)
        finally:
            for s in host.sessions:
                s.subscribers.discard(sub)
            sub.task.cancel()
            writer.close()

    if os.path.exists(path):
        os.unlink(path)
    previous = os.umask(0o077)  # the socket is never group- or world-accessible, even briefly
    try:
        server = await asyncio.start_unix_server(connection, path=path)
    finally:
        os.umask(previous)
    os.chmod(path, 0o600)
    print(dumps({"ready": 1, "pid": os.getpid(), "sessions": [s.id for s in host.sessions]}), flush=True)
    async with server:
        await asyncio.gather(server.serve_forever(), *tasks)
