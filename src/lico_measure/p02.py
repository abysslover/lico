"""M01-P02 scenarios and experiments against the owned host.

Each function returns plain observations and is used by the tests, by the
measurement command and by the inspection bundle, so the same code is run
everywhere.
"""

from __future__ import annotations

import json
import os
import random
import socket
import struct
import subprocess
import threading
import time
from pathlib import Path

from .procscope import collect_scopes, tree_pids
from .report import build_measurement
from .rig import REPO, Client, Host, Xvfb, private_dir

BUILD = Path(os.environ.get("LICO_BUILD_ROOT", REPO / "private" / "build"))
HOSTD_DIR = BUILD / "host_hostd" / "release"
HOSTD = HOSTD_DIR / "lico-host"
LICO = HOSTD_DIR / "lico"
AGENT = HOSTD_DIR / "lico-synth-agent"


class ProtoConn:
    """A v1 protocol client: replies are found among the events that may precede them."""

    def __init__(self, path: str, timeout: float = 10.0) -> None:
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(path)
        self.sock.settimeout(timeout)
        self.buf = b""

    def send(self, **request) -> None:
        self.sock.sendall((json.dumps(request) + "\n").encode())

    def read(self) -> dict:
        while b"\n" not in self.buf:
            chunk = self.sock.recv(1 << 20)
            if not chunk:
                raise EOFError
            self.buf += chunk
        line, self.buf = self.buf.split(b"\n", 1)
        return json.loads(line)

    def call(self, **request) -> dict:
        self.send(**request)
        while True:
            message = self.read()
            if "ok" in message:
                return message

    def close(self) -> None:
        self.sock.close()


def start(directory: str, extra: list[str] | None = None, **kw) -> tuple[Host, Path]:
    """The owned host with its durable log in the run directory."""
    log = os.path.join(directory, "log")
    args = ["--log-dir", log, "--agent", str(AGENT)] + (extra or [])
    return Host(directory, impl="hostd", extra_args=args, **kw), Path(log)


def read_log(log_dir: Path, session_id: str) -> list[tuple[int, bytes]]:
    """Parse one session's durable log straight from disk: (sequence, payload) in file order."""
    out = []
    for path in sorted(log_dir.glob(f"{session_id}.*.seg")):
        data = path.read_bytes()
        at = 0
        while at + 12 <= len(data):
            length, seq = struct.unpack_from("<IQ", data, at)
            if at + 12 + length > len(data):
                break
            out.append((seq, data[at + 12: at + 12 + length]))
            at += 12 + length
    return out


def echoes(log_dir: Path, session_id: str) -> list[str]:
    """Texts the synthetic agent echoed, read from the durable log (independent of the host)."""
    return [p[5:].decode() for _, p in read_log(log_dir, session_id) if p.startswith(b"echo ")]


def cli(host: Host, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run([str(LICO), "--socket", host.socket, *args], capture_output=True, text=True, timeout=20)


def _mib(kb: float) -> float:
    return kb / 1024.0


# ------------------------------------------------------------------ scenarios
def gui_kill(name: str) -> dict:
    """A GUI killed with SIGKILL while the host works; then a new GUI attaches."""
    with private_dir() as directory:
        xvfb = Xvfb()
        host = None
        clients = []
        try:
            host, log = start(directory, sessions=1, producing=1, rate=100, history=0, retain=100000)
            sid = host.sessions[0]
            conn = ProtoConn(host.socket)
            agent_pid = conn.call(op="resources")["agents"][0]["pid"]
            first = Client(name, host.socket, xvfb.display)
            clients.append(first)
            first.send(f"show {sid}")
            first.expect("state", "subscribed")
            time.sleep(2.0)
            attached = conn.call(op="resources")
            before = conn.call(op="status")["sessions"][0]["seq"]
            first.kill()
            time.sleep(5.0)
            detached = conn.call(op="resources")
            after = conn.call(op="status")["sessions"][0]["seq"]
            seqs = [s for s, _ in read_log(log, sid)]
            alive = True
            try:
                os.kill(agent_pid, 0)
            except OSError:
                alive = False
            second = Client(name, host.socket, xvfb.display)
            clients.append(second)
            second.send(f"show {sid}")
            second.expect("state", "subscribed")
            time.sleep(1.5)
            state = second.state()
            current = conn.call(op="status")["sessions"][0]["seq"]
            return {
                "views_attached": attached["views"], "subscribers_attached": attached["subscribers"],
                "views_after_kill": detached["views"], "subscribers_after_kill": detached["subscribers"],
                "agent_pid_unchanged": [a["pid"] for a in detached["agents"]] == [agent_pid],
                "agent_alive": alive, "records_while_gui_absent": after - before,
                "log_contiguous": seqs == list(range(len(seqs))), "log_records": len(seqs),
                "restarted_gaps": state["gaps"], "restarted_dups": state["dups"], "restarted_resyncs": state.get("resyncs", 0),
                "restarted_cursor": state["cursor"], "host_seq": current, "views_after_restart": conn.call(op="resources")["views"],
                "view_resident": state["view_resident"],
            }
        finally:
            for c in clients:
                if c.proc.poll() is None:
                    c.kill()
            if host:
                host.stop()
            xvfb.stop()


def view_switch(name: str) -> dict:
    """A view switched in the middle of a Korean composition, through a real input method."""
    from .ime import InputMethod, _focus

    def keys(display: str, *names: str) -> None:
        subprocess.run(["xdotool", "key", "--delay", "60", *names], env=dict(os.environ, DISPLAY=display), check=True)

    with private_dir() as directory:
        xvfb = Xvfb()
        host = method = client = None
        try:
            host, log = start(directory, sessions=2, producing=0, history=3)
            s1, s2 = host.sessions
            method = InputMethod(xvfb.display)
            client = Client(name, host.socket, xvfb.display, extra_env=method.client_env)
            client.send(f"show {s1}")
            client.expect("state", "subscribed")
            time.sleep(1.0)
            _focus(xvfb.display)
            time.sleep(0.5)
            keys(xvfb.display, "shift+space")
            time.sleep(0.5)
            keys(xvfb.display, "d", "k", "s", "s", "u", "d")      # 안 committed, 녕 composing
            time.sleep(0.5)
            client.send(f"show {s2}")                              # the switch, mid-composition
            client.expect("state", "subscribed")
            time.sleep(0.8)
            _focus(xvfb.display)
            time.sleep(0.8)
            keys(xvfb.display, "g", "k", "Return")                 # 하, sent from the second session
            time.sleep(1.0)
            client.send(f"show {s1}")
            client.expect("state", "subscribed")
            time.sleep(0.8)
            _focus(xvfb.display)
            time.sleep(0.8)
            keys(xvfb.display, "Return")                           # sends the restored draft of the first
            time.sleep(1.5)
            return {"first_session_inputs": echoes(log, s1), "second_session_inputs": echoes(log, s2),
                    "expected_first": ["안녕"], "expected_second": ["하"]}
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if method:
                method.stop()
            if host:
                host.stop()
            xvfb.stop()


# ---------------------------------------------------------------- experiments
def _host_rss(host: Host) -> tuple[float, float]:
    scope = collect_scopes({"host": host.pid})["host"]
    if scope.unreadable:
        raise RuntimeError("host process could not be read")
    one = [p for p in scope.processes if p.pid == host.pid][0]
    return _mib(one.rss_kb), _mib(one.pss_kb)


def drain(conn: ProtoConn, stop: threading.Event) -> None:
    conn.sock.settimeout(0.5)
    while not stop.is_set():
        try:
            if not conn.sock.recv(1 << 20):
                return
        except OSError:
            continue


def host_resources(repetitions: int = 3) -> dict:
    """Host memory at 10 and 100 sessions, idle, producing, and with streaming views."""
    rows = {"10 sessions": (10, 0, 0), "100 sessions": (100, 0, 0), "100 sessions, 5 producing, 10 streaming": (100, 5, 10)}
    measurements, observations = [], {}
    for label, (sessions, producing, streaming) in rows.items():
        rss, pss, agent_rss, reported = [], [], [], []
        for _ in range(repetitions):
            with private_dir() as directory:
                host, _log = start(directory, sessions=sessions, producing=producing, rate=100, history=500)
                conns, stop = [], threading.Event()
                try:
                    time.sleep(2.0)
                    for k in range(streaming):
                        conn = ProtoConn(host.socket)
                        conn.send(op="subscribe", session=host.sessions[k % producing], after=-1, snapshot=True)
                        conns.append(conn)
                        threading.Thread(target=drain, args=(conn, stop), daemon=True).start()
                    time.sleep(10.0 if streaming else 0.5)
                    r, p = _host_rss(host)
                    report = ProtoConn(host.socket).call(op="resources")
                    rss.append(r), pss.append(p)
                    agent_rss.append(_mib(sum(a["rss_kb"] for a in report["agents"])))
                    reported.append(_mib(report["rss_kb"]))
                finally:
                    stop.set()
                    for c in conns:
                        c.close()
                    host.stop()
        scope = {"candidate": "host:hostd", "workload": f"{label}, history 500, window 64", "process_scope": "host process"}
        measurements += [
            build_measurement("host_rss_mib", scope, rss), build_measurement("host_pss_mib", scope, pss),
            build_measurement("host_reported_rss_mib", dict(scope, process_scope="host's own resource report"), reported),
            build_measurement("agents_total_rss_mib", dict(scope, process_scope="agent processes"), agent_rss),
        ]
    return {"measurements": measurements, "observations": observations}


def backpressure(seconds: float = 6.0) -> dict:
    """One stalled subscriber must not slow the host or another subscriber."""
    def run(stalled: bool) -> dict:
        with private_dir() as directory:
            host, _log = start(directory, extra=["--queue-bytes", "65536"], sessions=1, producing=1, rate=1000, history=0,
                               retain=100000)
            try:
                sid = host.sessions[0]
                slow = None
                if stalled:
                    slow = ProtoConn(host.socket)
                    slow.call(op="subscribe", session=sid, after=-1, snapshot=True)
                fast, probe = ProtoConn(host.socket), ProtoConn(host.socket)
                fast.call(op="subscribe", session=sid, after=-1, snapshot=True)
                first = probe.call(op="status")["sessions"][0]["seq"]
                started, seqs = time.monotonic(), []
                while time.monotonic() - started < seconds:
                    message = fast.read()
                    if message.get("ev") == "rec":
                        seqs.append(message["seq"])
                elapsed = time.monotonic() - started
                last = probe.call(op="status")["sessions"][0]
                out = {"records_per_second": (last["seq"] - first) / elapsed, "fast_contiguous": seqs == list(range(seqs[0], seqs[0] + len(seqs))),
                       "fast_received": len(seqs), "subscribers_left": last["subscribers"]}
                if slow:
                    slow.sock.settimeout(5)
                    told = False
                    try:
                        while True:
                            if slow.read().get("ev") == "overflow":
                                told = True
                                break
                    except (EOFError, socket.timeout):
                        pass
                    out["stalled_told_to_resync"] = told
                return out
            finally:
                host.stop()

    alone, with_stalled = run(False), run(True)
    return {"measurements": [], "observations": {
        "without_stalled_subscriber": alone, "with_stalled_subscriber": with_stalled,
        "rate_ratio": round(with_stalled["records_per_second"] / alone["records_per_second"], 3)}}


def input_integrity(inputs: int = 1000, stale: int = 100, foreign: int = 50) -> dict:
    """Many inputs, with replies lost and retried, stale and foreign targets: each lands once, in its own session."""
    rng = random.Random(11)
    with private_dir() as directory:
        host, log = start(directory, sessions=2, producing=0, history=1)
        try:
            ids = host.sessions
            expected: dict[str, list[str]] = {sid: [] for sid in ids}
            refused = unsent_retries = 0
            for k in range(inputs):
                sid = ids[k % 2]
                rid, text = f"in-{k:05d}", f"{k % 2}:text-{k:05d}"
                conn = ProtoConn(host.socket)
                conn.send(op="input", session=sid, generation=1, request_id=rid, text=text)
                if rng.random() < 0.5:
                    conn.close()                              # the reply is lost, so the sender sends it again
                    retry = ProtoConn(host.socket)
                    retry.call(op="input", session=sid, generation=1, request_id=rid, text=text)
                    retry.close()
                    unsent_retries += 1
                else:
                    conn.read()
                    conn.close()
                expected[sid].append(text)
            probe = ProtoConn(host.socket)
            for k in range(stale):
                reply = probe.call(op="input", session=ids[k % 2], generation=99, request_id=f"stale-{k}", text="stale-target")
                refused += reply["ok"] == 0 and reply["error"] == "stale_generation"
            for k in range(foreign):
                reply = probe.call(op="input", session="s" + "0" * 16, generation=1, request_id=f"foreign-{k}", text="foreign-target")
                refused += reply["ok"] == 0 and reply["error"] == "unknown_session"
            time.sleep(3.0)
            seen = {sid: echoes(log, sid) for sid in ids}
            states = [probe.call(op="input.status", session=ids[k % 2], request_id=f"in-{k:05d}")["state"] for k in range(0, inputs, 10)]
            result = {
                "inputs": inputs, "retried_after_lost_reply": unsent_retries,
                "stale_and_foreign_attempts": stale + foreign, "refused_correctly": refused,
                "lost": sum(len(set(expected[s]) - set(seen[s])) for s in ids),
                "duplicated": sum(len(seen[s]) - len(set(seen[s])) for s in ids),
                "misrouted": sum(len(set(seen[s]) - set(expected[s])) for s in ids),
                "refused_inputs_that_reached_the_agent": sum(1 for s in ids for t in seen[s] if "target" in t),
                "sampled_input_states_observed": all(s == "observed" for s in states),
            }
            return {"measurements": [], "observations": result}
        finally:
            host.stop()


def cli_transcript() -> dict:
    """The command line against a live host: every command and what it printed."""
    steps = []
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=1, rate=50, history=20)
        try:
            sid = host.sessions[0]

            def run(*args: str) -> None:
                done = cli(host, *args)
                steps.append({"command": "lico " + " ".join(a if " " not in a else repr(a) for a in args),
                              "exit": done.returncode, "stdout": done.stdout.strip()[:1200], "stderr": done.stderr.strip()[:200]})

            run("status")
            run("session", "list")
            run("session", "create", "--rate", "0", "--history", "5")
            run("session", "show", sid)
            run("session", "send", sid, "hello from the command line")
            run("session", "send", sid, "ask to run the build")
            time.sleep(1.0)
            approvals = ProtoConn(host.socket).call(op="status")["sessions"][0].get("approvals", [])
            if approvals:
                run("approval", "answer", sid, str(approvals[0]["approval_id"]), "allow")
            run("session", "watch", sid, "--seconds", "1")
            run("view", "attach", sid)
            run("resources")
            run("session", "show", "s" + "0" * 16)             # an error exits 2
            return {"steps": steps, "echoes": echoes(log, sid)}
        finally:
            host.stop()
