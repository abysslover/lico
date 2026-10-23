"""The M01-P01 experiments. Each returns scoped measurement records and plain
observations; nothing here judges a candidate, report.build_measurement does."""

from __future__ import annotations

import os
import subprocess
import threading
import time

from .latency import measure_switch
from .procscope import collect_scopes, cpu_seconds, tree_pids
from .report import build_measurement
from .rig import Client, Host, HostConn, Xvfb, private_dir, session_rgb
from .xpixel import PixelReader

VIEWPORT = "1200x800"
SETTLE_S = 3.0
SWITCHES = 100
RESTARTS = 100
COLD_RESTARTS = 10
IDLE_INTERVAL_S = 60
TIMEOUT_MS = 2000.0


def _mib(kb: float) -> float:
    return kb / 1024.0


class Rig:
    """Private X server, synthetic host and (optionally) one candidate client."""

    def __init__(self, candidate: str, sessions: int, producing: int, history: int = 5000,
                 rate: float = 100.0, with_client: bool = True) -> None:
        self.candidate, self.with_client = candidate, with_client
        self.args = (sessions, producing, history, rate)
        self.client = None

    def __enter__(self) -> "Rig":
        self.tmp = private_dir()
        self.xvfb = Xvfb(f"{VIEWPORT}x24")
        self.host = self.reader = self.conn = None
        try:
            sessions, producing, history, rate = self.args
            self.host = Host(self.tmp.name, sessions, producing, rate=rate, history=history)
            self.reader = PixelReader(self.xvfb.display)
            self.conn = HostConn(self.host.socket)
            if self.with_client:
                self.client = Client(self.candidate, self.host.socket, self.xvfb.display)
        except BaseException:
            self.__exit__()
            raise
        return self

    def __exit__(self, *exc) -> None:
        if self.reader:
            self.reader.close()
        if self.conn:
            self.conn.close()
        if self.client:
            self.client.quit()
        if self.host:
            self.host.stop()
        self.xvfb.stop()
        self.tmp.cleanup()

    def scope(self, name: str, extra: dict | None = None) -> dict:
        scope = {"candidate": self.candidate, "workload": name, "process_scope": "gui process tree"}
        scope.update(extra or {})
        return scope

    def show(self, session_id: str) -> None:
        self.client.send(f"show {session_id}")
        self.client.expect("state", "subscribed")
        deadline = time.monotonic() + 5
        while self.reader.pixel(8, 8) != session_rgb(session_id):
            if time.monotonic() > deadline:
                raise TimeoutError("view not shown")
            time.sleep(0.01)

    def seqs(self) -> dict[str, int]:
        return {s["session"]: s["seq"] for s in self.conn.call(op="status")["sessions"]}

    def trees(self) -> dict:
        return collect_scopes({"gui": self.client.proc.pid, "host": self.host.pid,
                               "display": self.xvfb.proc.pid})


def _load_info() -> dict:
    return {"loadavg": os.getloadavg(), "t": time.time()}


def idle_memory(candidate: str) -> dict:
    """PERF-01 and PERF-07: one visible synthetic session, idle."""
    with Rig(candidate, sessions=1, producing=0) as rig:
        sid = rig.host.sessions[0]
        rig.client.send("load")
        rig.client.expect("state", "loaded")
        rig.show(sid)
        time.sleep(SETTLE_S)
        gui_rss, gui_pss, host_rss, display_rss, peaks, anon, filed = [], [], [], [], [], [], []
        for _ in range(5):
            scopes = rig.trees()
            if any(s.unreadable for s in scopes.values()):
                raise RuntimeError("a process in scope could not be read")
            gui_rss.append(_mib(scopes["gui"].rss_kb))
            gui_pss.append(_mib(scopes["gui"].pss_kb))
            host_rss.append(_mib(scopes["host"].rss_kb))
            display_rss.append(_mib(scopes["display"].rss_kb))
            peaks.append(_mib(sum(p.hwm_kb or 0 for p in scopes["gui"].processes)))
            anon.append(_mib(sum(p.anon_kb for p in scopes["gui"].processes)))
            filed.append(_mib(sum(p.file_kb for p in scopes["gui"].processes)))
            time.sleep(1.0)
        gui_pids = [p.pid for p in scopes["gui"].processes]
        names = []
        for pid in gui_pids:
            with open(f"/proc/{pid}/comm") as handle:
                names.append(handle.read().strip())
        scope = rig.scope("one visible session, idle", {"viewport": VIEWPORT, "settle_s": SETTLE_S})
        return {
            "measurements": [
                build_measurement("gui_idle_rss_mib", scope, gui_rss),
                build_measurement("gui_idle_pss_mib", scope, gui_pss),
                build_measurement("gui_peak_rss_mib", scope, peaks),
                build_measurement("gui_idle_rss_anon_mib", scope, anon),
                build_measurement("gui_idle_rss_file_mib", scope, filed),
                build_measurement("host_idle_rss_mib", dict(scope, process_scope="host process tree"), host_rss),
                build_measurement("display_server_idle_rss_mib",
                                  dict(scope, process_scope="virtual display server"), display_rss),
            ],
            "observations": {"gui_processes": names, "gui_process_count": len(names),
                             "composition_note": "anon and file parts come from the status counters, which can differ from the smaps total by about 0.4 MiB per process", "peak_note": "sum of per-process VmHWM; peaks need not coincide and the kernel high-water mark can read below an instantaneous RSS",
                             "load": _load_info()},
        }


def _cycle_and_measure(candidate: str, sessions: int) -> tuple[float, float]:
    with Rig(candidate, sessions=sessions, producing=0, history=500) as rig:
        rig.client.send("load")
        rig.client.expect("state", "loaded")
        for sid in rig.host.sessions:        # every session is shown once, then released
            rig.show(sid)
            rig.client.send("hide")
            rig.client.expect("state", "hidden")
        rig.show(rig.host.sessions[0])
        time.sleep(SETTLE_S)
        scopes = rig.trees()
        return _mib(scopes["gui"].rss_kb), _mib(scopes["host"].rss_kb)


def hidden_growth(candidate: str, repetitions: int = 3) -> dict:
    """PERF-02: GUI RSS growth from 10 to 100 background sessions, views released."""
    small, large, host_small, host_large = [], [], [], []
    for _ in range(repetitions):
        gui, host = _cycle_and_measure(candidate, 10)
        small.append(gui)
        host_small.append(host)
        gui, host = _cycle_and_measure(candidate, 100)
        large.append(gui)
        host_large.append(host)
    scope = {"candidate": candidate, "workload": "10 vs 100 sessions, each shown once then hidden",
             "process_scope": "gui process tree", "viewport": VIEWPORT}
    return {
        "measurements": [
            build_measurement("hidden_growth_rss_mib", scope, [b - a for a, b in zip(small, large)]),
            build_measurement("gui_rss_10_sessions_mib", scope, small),
            build_measurement("gui_rss_100_sessions_mib", scope, large),
            build_measurement("host_rss_10_sessions_mib", dict(scope, process_scope="host process tree"), host_small),
            build_measurement("host_rss_100_sessions_mib", dict(scope, process_scope="host process tree"), host_large),
        ],
        "observations": {"pairing": "delta i = repetition i at 100 minus repetition i at 10",
                         "load": _load_info()},
    }


def _switch_samples(rig: Rig, order: list[int]) -> tuple[list[float], int]:
    samples, timeouts = [], 0
    for index in order:
        sid = rig.host.sessions[index]
        min_seq = rig.seqs()[sid]
        try:
            ms = measure_switch(lambda: rig.client.send(f"show {sid}"), rig.reader.pixel,
                                session_rgb(sid), min_seq, timeout_s=TIMEOUT_MS / 1000)
        except TimeoutError:
            ms, timeouts = TIMEOUT_MS, timeouts + 1   # counted at the cap, never dropped
        samples.append(ms)
        time.sleep(0.04)
    return samples, timeouts


def activation(candidate: str) -> dict:
    """PERF-03: first correct frame on the display when switching sessions."""
    with Rig(candidate, sessions=100, producing=5, history=5000) as rig:
        rig.client.send("load")
        rig.client.expect("state", "loaded")
        first, first_timeouts = _switch_samples(rig, list(range(100)))
        order = [(i * 37) % 100 for i in range(SWITCHES)]
        warm, warm_timeouts = _switch_samples(rig, order)
        scope = rig.scope("100 sessions (5 producing 100 rec/s while hidden), history 5000 records",
                          {"viewport": VIEWPORT, "clock": "monotonic", "frame_source": "X server pixels",
                           "p95_method": "nearest rank"})
        return {
            "measurements": [
                build_measurement("activation_p95_ms", scope, warm),
                build_measurement("activation_first_visit_ms", scope, first),
            ],
            "observations": {"timeouts_revisit": warm_timeouts, "timeouts_first_visit": first_timeouts,
                             "timeout_cap_ms": TIMEOUT_MS, "revisit_order": "stride 37 over 100 sessions",
                             "load": _load_info()},
        }


def _library_paths(pid: int) -> list[str]:
    paths = set()
    with open(f"/proc/{pid}/maps") as handle:
        for line in handle:
            parts = line.split(None, 5)
            if len(parts) == 6 and parts[5].startswith("/") and "(deleted)" not in parts[5]:
                paths.add(parts[5].strip())
    return sorted(paths)


def _evict(paths: list[str]) -> int:
    done = 0
    for path in paths:
        try:
            fd = os.open(path, os.O_RDONLY)
        except OSError:
            continue
        try:
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
            done += 1
        finally:
            os.close(fd)
    return done


def _launch_and_time(rig: Rig, index: int) -> float:
    sid = rig.host.sessions[index]
    min_seq = rig.seqs()[sid]
    box = {}

    def issue() -> None:
        box["client"] = Client(rig.candidate, rig.host.socket, rig.xvfb.display, wait_ready=False)
        box["client"].send(f"show {sid}")

    try:
        return measure_switch(issue, rig.reader.pixel, session_rgb(sid), min_seq, timeout_s=10.0)
    finally:
        client = box.get("client")
        if client:
            client.kill()
        time.sleep(0.15)


def restart(candidate: str) -> dict:
    """PERF-04: first correct frame after starting the GUI process again."""
    with Rig(candidate, sessions=100, producing=5, history=5000, with_client=False) as rig:
        # discover the files a running client maps, for the best-effort cold runs
        probe = Client(candidate, rig.host.socket, rig.xvfb.display)
        mapped = _library_paths(probe.proc.pid)
        probe.kill()
        time.sleep(0.3)
        warm = []
        for i in range(RESTARTS):
            warm.append(_launch_and_time(rig, i % 100))
        cold, evicted = [], 0
        for i in range(COLD_RESTARTS):
            evicted = _evict(mapped)
            time.sleep(0.2)
            cold.append(_launch_and_time(rig, i % 100))
        scope = rig.scope("start GUI process, attach to one of 100 sessions (5 producing), snapshot path",
                          {"viewport": VIEWPORT, "clock": "monotonic", "frame_source": "X server pixels",
                           "filesystem": "page cache warm after first launch; others not controlled",
                           "p95_method": "nearest rank"})
        cold_scope = dict(scope, filesystem="best-effort cold: posix_fadvise DONTNEED on mapped files; "
                                            "pages mapped by other processes stay resident")
        return {
            "measurements": [
                build_measurement("restart_p95_ms", scope, warm),
                build_measurement("cold_start_ms", cold_scope, cold),
            ],
            "observations": {"files_evicted": evicted, "files_mapped": len(mapped),
                             "cold_runs": COLD_RESTARTS, "load": _load_info()},
        }


def _cpu_percent(rig: Rig, seconds: int) -> tuple[float, float]:
    gui = tree_pids(rig.client.proc.pid)
    display = tree_pids(rig.xvfb.proc.pid)
    g0, d0, w0 = cpu_seconds(gui), cpu_seconds(display), time.monotonic()
    time.sleep(seconds)
    wall = time.monotonic() - w0
    return (cpu_seconds(gui) - g0) / wall * 100, (cpu_seconds(display) - d0) / wall * 100


def idle_cpu(candidate: str) -> dict:
    """PERF-05: idle GUI CPU, and a noisy background-agent case reported separately."""
    out = []
    for name, producing in (("idle visible session", 0), ("visible session producing 100 rec/s", 1)):
        with Rig(candidate, sessions=1, producing=producing, history=500) as rig:
            rig.show(rig.host.sessions[0])
            time.sleep(SETTLE_S)
            gui, display = _cpu_percent(rig, IDLE_INTERVAL_S)
            scope = rig.scope(name, {"viewport": VIEWPORT, "interval_s": IDLE_INTERVAL_S})
            metric = "idle_cpu_percent" if producing == 0 else "noisy_cpu_percent"
            out.append(build_measurement(metric, scope, [gui]))
            out.append(build_measurement(
                "display_server_" + metric, dict(scope, process_scope="virtual display server"), [display]))
    return {"measurements": out, "observations": {"load": _load_info()}}


def continuity(candidate: str) -> dict:
    """PERF-06: sequenced output while the GUI is absent, then reattach by cursor."""
    seqs: list[int] = []
    with Rig(candidate, sessions=2, producing=1, history=0, with_client=False) as rig:
        sid = rig.host.sessions[0]
        recorder = HostConn(rig.host.socket)
        recorder.call(op="subscribe", session=sid, after=-1)
        stop = threading.Event()

        def record() -> None:
            recorder.sock.settimeout(0.5)
            while not stop.is_set():
                try:
                    seqs.append(recorder.next_event()["seq"])
                except (TimeoutError, OSError):
                    continue
                except ValueError:
                    return

        thread = threading.Thread(target=record, daemon=True)
        thread.start()
        started = time.monotonic()
        first = Client(candidate, rig.host.socket, rig.xvfb.display)
        first.send(f"show {sid}")
        first.expect("state", "subscribed")
        time.sleep(5 - (time.monotonic() - started))
        cursor = first.state()["cursor"]
        first.kill()                                  # the GUI is absent from here
        time.sleep(20 - (time.monotonic() - started))
        second = Client(candidate, rig.host.socket, rig.xvfb.display)
        second.send(f"show {sid} {cursor}")
        second.expect("state", "subscribed")
        time.sleep(30 - (time.monotonic() - started))
        final = second.state()
        late = HostConn(rig.host.socket)
        stale = late.call(op="subscribe", session=sid, after=cursor)   # 25 s old: beyond retention
        stop.set()
        thread.join(timeout=2)
        status = [s for s in rig.conn.call(op="status")["sessions"] if s["session"] == sid][0]
        second.kill()
        late.close()
        recorder.close()
    contiguous = seqs == list(range(len(seqs)))
    return {
        "measurements": [],
        "observations": {
            "records_emitted_at_end": status["seq"] + 1, "records_recorded_by_independent_subscriber": len(seqs),
            "recorder_contiguous_no_gap_no_duplicate": contiguous,
            "bytes_total": status["bytes_total"], "record_bytes": 1024, "rate_per_second": 100,
            "duration_s": 30, "gui_absent_s": 15, "reattach_cursor": cursor,
            "client_gaps": final["gaps"], "client_dups": final["dups"],
            "client_final_cursor_at_30s": final["cursor"], "host_seq_after_recorder_stopped": status["seq"],
            "stale_cursor_reports_gap_and_snapshot": bool(stale.get("gap")) and "snapshot" in stale,
            "load": _load_info(),
        },
    }


def inventory(candidate: str) -> dict:
    """PERF-07 items that are not timings."""
    from .rig import candidate_command

    exe = candidate_command(candidate, "x")[0]
    with Rig(candidate, sessions=1, producing=0, history=100) as rig:
        rig.show(rig.host.sessions[0])
        tree = subprocess.run(["xwininfo", "-display", rig.xvfb.display, "-root", "-tree"],
                              capture_output=True, text=True).stdout
        scopes = rig.trees()
        began = time.perf_counter_ns()
        for _ in range(500):
            rig.reader.pixel(8, 8)
        poll_us = (time.perf_counter_ns() - began) / 500 / 1000
        libs = _library_paths(rig.client.proc.pid)
        sizes = {p: os.path.getsize(p) for p in libs if os.path.isfile(p)}
        return {
            "measurements": [],
            "observations": {
                "binary_bytes": os.path.getsize(exe),
                "mapped_file_count": len(libs), "mapped_file_bytes_total": sum(sizes.values()),
                "package_size": "unverified: no package is produced in this phase",
                "window_1200x800_present": "1200x800" in tree,
                "gui_process_count": len(scopes["gui"].processes),
                "display": {"server": "Xvfb (X11, no compositor, software rendering)",
                            "screen": f"{VIEWPORT}x24", "scale": 1, "wayland": "unverified"},
                "gpu": "none observable under Xvfb; GPU allocation unverified",
                "fonts": ["DejaVu Sans Mono (Latin)", "NanumSquare (Hangul fallback)"],
                "cpu_logical": os.cpu_count(),
                "screen_read_period_us": round(poll_us, 1),
                "latency_resolution_note": "two screen reads per poll; latency values cannot resolve below this period",
            },
        }


EXPERIMENTS = {
    "idle_memory": idle_memory, "hidden_growth": hidden_growth, "activation": activation,
    "restart": restart, "idle_cpu": idle_cpu, "continuity": continuity, "inventory": inventory,
}
