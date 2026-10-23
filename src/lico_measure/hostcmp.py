"""Compare host implementations of protocol v1 by size and resident memory.

Every implementation passes the same conformance suite first; this module only
measures. Nothing here is a target: the comparison informs the language decision.
"""

from __future__ import annotations

import os
import threading
import time

from .procscope import collect_scopes, cpu_seconds, tree_pids
from .report import build_measurement
from .rig import Host, HostConn, _host_command, host_impls, private_dir

HISTORY = 500
STREAM_S = 10


def _mib(kb: float) -> float:
    return kb / 1024.0


def _rss_pss(pid: int) -> tuple[float, float]:
    scope = collect_scopes({"host": pid})["host"]
    if scope.unreadable:
        raise RuntimeError("host process could not be read")
    return _mib(scope.rss_kb), _mib(scope.pss_kb)


def _one(impl: str, sessions: int, producing: int, subscribers: int) -> dict:
    with private_dir() as directory:
        host = Host(directory, sessions, producing, rate=100.0, history=HISTORY, impl=impl)
        stop = threading.Event()
        conns: list[HostConn] = []
        try:
            time.sleep(2.0)
            idle = _rss_pss(host.pid)
            out = {"idle_rss": idle[0], "idle_pss": idle[1]}
            if subscribers:
                for k in range(subscribers):
                    conn = HostConn(host.socket)
                    conn.sock.sendall(
                        (f'{{"op":"subscribe","session":"{host.sessions[k % producing]}","after":-1,"snapshot":true}}\n').encode())
                    conns.append(conn)

                def drain(conn: HostConn) -> None:
                    conn.sock.settimeout(0.5)
                    while not stop.is_set():
                        try:
                            if not conn.sock.recv(65536):
                                return
                        except OSError:
                            continue

                threads = [threading.Thread(target=drain, args=(c,), daemon=True) for c in conns]
                for t in threads:
                    t.start()
                pids = tree_pids(host.pid)
                c0, w0 = cpu_seconds(pids, strict=True), time.monotonic()
                time.sleep(STREAM_S)
                wall = time.monotonic() - w0
                out["cpu_percent"] = (cpu_seconds(pids, strict=True) - c0) / wall * 100
                loaded = _rss_pss(host.pid)
                out["loaded_rss"], out["loaded_pss"] = loaded
            return out
        finally:
            stop.set()
            for c in conns:
                c.close()
            host.stop()


def compare(repetitions: int = 3) -> dict:
    results = {}
    for impl in host_impls():
        exe = _host_command(impl)[0]
        size = os.path.getsize(exe) if impl != "oracle" else None
        rows = {"10 sessions": [], "100 sessions": [], "100 sessions, 5 producing, 10 streaming": []}
        for _ in range(repetitions):
            rows["10 sessions"].append(_one(impl, 10, 0, 0))
            rows["100 sessions"].append(_one(impl, 100, 0, 0))
            rows["100 sessions, 5 producing, 10 streaming"].append(_one(impl, 100, 5, 10))
        scope = {"candidate": f"host:{impl}", "workload": "synthetic host, history 500, retain 2048",
                 "process_scope": "host process"}
        measurements = []
        for name, runs in rows.items():
            s = dict(scope, workload=f"{name}, history {HISTORY}")
            measurements.append(build_measurement("host_idle_rss_mib", s, [r["idle_rss"] for r in runs]))
            measurements.append(build_measurement("host_idle_pss_mib", s, [r["idle_pss"] for r in runs]))
            if "loaded_rss" in runs[0]:
                measurements.append(build_measurement("host_loaded_rss_mib", s, [r["loaded_rss"] for r in runs]))
                measurements.append(build_measurement("host_loaded_cpu_percent", dict(s, interval_s=STREAM_S),
                                                      [r["cpu_percent"] for r in runs]))
        results[impl] = {"binary_bytes": size, "measurements": measurements}
    return results
