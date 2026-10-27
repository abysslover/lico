"""S08: does ten minutes of traffic on the tool connection grow the host or the window?

Question. The plan's stop line is a leak of 10 MiB in 10 minutes with the views hidden (no session shown). Do the host and the window stay within it while three
Claude sessions (the simulator) use all five tools all the time, the person answers every card, and the window only shows the list?

Workload. A private host with three sessions; the window (when asked) shows the session list and nothing else. Every 4 s one cycle, the caller and the target rotating over
the three sessions: `sessions`; `send` (a text of 200 bytes or of 6,000, so that half of them are kept as a file) and the person's yes (and, first, a word of the person to the caller, which starts the count of the chain of agents again); `history` of the target and the
person's yes (the answer is over 4,096 bytes soon, so it is a file in the caller's folder); `read_handoff` of the newest file in that folder; every eighth cycle a `create`
with a yes and the purge of the session it made. Host and window are read from /proc every 30 s: VmRSS, RssAnon, VmHWM, threads, descriptors. When the run is over, the
credentials of the three sessions are searched for in every file of the run folder except their config files, and the texts that were sent in the host's own log.

Run. `PYTHONPATH=src:. LICO_BUILD_ROOT=<root> python3 measure.py host|window [SECONDS]` prints JSON lines, the last one is the summary. Needs the repository's venv and the
test helpers.
"""
import json
import os
import sys
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import new_claude, records
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args
from tests.test_tool_connection import config_of, rpc
from tests.test_tool_send import host_of

CYCLE_S, SAMPLE_S = 4.0, 30.0


def proc(pid):
    fields = {}
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        key, _, value = line.partition(":")
        fields[key] = value.split()
    return {"rss_kib": int(fields["VmRSS"][0]), "anon_kib": int(fields["RssAnon"][0]), "peak_kib": int(fields["VmHWM"][0]), "threads": int(fields["Threads"][0]),
            "fds": len(list(Path(f"/proc/{pid}/fd").iterdir()))}


def call(config, tool, **arguments):
    """A call of a tool; the listener lets a burst through and then 100 a second, and a cycle is far below that."""
    status, body = rpc(config, "tools/call", {"name": tool, "arguments": arguments})
    assert status == 200, (tool, status, body)
    result = body["result"]
    return result["isError"], result["content"][0]["text"]


class Traffic:
    def __init__(self, conn, state, sessions):
        self.conn, self.state, self.sessions = conn, Path(state), sessions
        self.configs = {}
        for sid, _ in sessions:
            config = config_of(self.state, sid)                              # the file goes at the first initialize: read before
            status, body = rpc(config, "initialize", {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "s08", "version": "0"}})
            assert status == 200 and "result" in body, (status, body)
            self.configs[sid] = config
        self.cycles, self.sent = 0, []

    def decide(self, caller, number):
        sid, generation = caller
        done = self.conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="allow")
        assert done["ok"] == 1 and done["outcome"] == "done", done

    def ask(self, caller, tool, **arguments):
        failed, answer = call(self.configs[caller[0]], tool, **arguments)
        assert not failed, (tool, answer)
        reply = json.loads(answer)
        assert reply["status"] == "pending", answer
        self.decide(caller, reply["approval_id"])

    def cycle(self):
        n = self.cycles
        caller, target = self.sessions[n % 3], self.sessions[(n + 1) % 3]
        config = self.configs[caller[0]]
        assert self.conn.call(op="input", session=caller[0], generation=caller[1], request_id=f"p-{n}", text="go")["ok"] == 1                 # a person's word starts the count of a chain again
        call(config, "sessions")
        text = f"s08 cycle {n} " + "y" * (200 if n % 2 else 6000)
        self.sent.append(text[:40])
        self.ask(caller, "send", to=target[0], text=text)
        self.ask(caller, "history", session=target[0])
        files = sorted((self.state / "handoff" / caller[0]).glob("*/*.md"))
        if files:
            failed, answer = call(config, "read_handoff", path=str(files[-1]), offset=0, limit=32768)
            assert not failed, answer
        if n % 8 == 7:
            failed, answer = call(config, "create", prompt="s08 check", name="s08")
            assert not failed and json.loads(answer)["status"] == "pending", answer
            self.decide(caller, json.loads(answer)["approval_id"])
            for row in self.conn.call(op="status")["sessions"]:
                if "made by" in row.get("name", ""):
                    wait_for(lambda: self.conn.call(op="session.purge", session=row["session"], generation=row["generation"])["ok"] == 1)
        self.cycles += 1


def first_turns(conn, log, sessions):
    """A session has its process, and so its credential, from its first turn."""
    for number, (sid, generation) in enumerate(sessions):
        assert conn.call(op="input", session=sid, generation=generation, request_id=f"first-{number}", text="hi")["ok"] == 1
    for sid, _ in sessions:
        wait_for(lambda: "[turn completed]" in records(log, sid))


def scan(directory, tokens, markers):
    """Where a credential or a sent text is found in the run folder: the credentials anywhere but in a config file, the texts in the host's own log."""
    found = {"credential": [], "text": []}
    for path in Path(directory).rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        data = path.read_bytes()
        if path.parent.name != "tools" and any(t.encode() in data for t in tokens):
            found["credential"].append(path.name)
        if path.name in ("host.err",) and any(m.encode() in data for m in markers):
            found["text"].append(path.name)
    return found


def run(kind, seconds):
    with private_dir() as directory:
        if kind == "host":
            host, log, _received = host_of(directory)
            try:
                conn = Conn(host.socket)
                sessions = [new_claude(conn, name=f"s08-{i}") for i in range(3)]
                first_turns(conn, log, sessions)
                drive(conn, directory, directory, sessions, host.pid, None, seconds)
            finally:
                host.stop()
        else:
            args, _ = claude_args(directory)
            made = {}
            with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(Path(directory) / "log")],
                               before_window=lambda w: made.update(ids=[new_claude(Conn(w.socket), name=f"s08-{i}") for i in range(3)])) as w:
                wait_for(lambda: len(w.ui()["rows"]) == 3)
                first_turns(Conn(w.socket), Path(directory) / "log", made["ids"])
                drive(Conn(w.socket), w.directory, Path(w.directory) / "state" / "lico", made["ids"], w.old_pid, w.proc.pid, seconds)


def drive(conn, directory, state, sessions, host_pid, window_pid, seconds):
    traffic = Traffic(conn, state, sessions)
    tokens = [c["authorization"] for c in traffic.configs.values()]
    series, begin, last = [], time.monotonic(), None
    while True:
        now = time.monotonic() - begin
        if last is None or now - last >= SAMPLE_S or now >= seconds:
            row = {"t": round(now), "host": proc(host_pid), "cycles": traffic.cycles}
            if window_pid:
                row["window"] = proc(window_pid)
            series.append(row)
            print(json.dumps(row), flush=True)
            last = now
        if now >= seconds:
            break
        started = time.monotonic()
        traffic.cycle()
        time.sleep(max(0.0, CYCLE_S - (time.monotonic() - started)))
    found = scan(directory, [t.split(" ", 1)[-1] for t in tokens], traffic.sent[:5])

    def mib(rows, who, key="rss_kib"):
        return [r[who][key] / 1024 for r in rows]

    summary = {"summary": True, "seconds": seconds, "cycles": traffic.cycles, "found": found}
    for who in ("host", "window"):
        if who == "window" and not window_pid:
            continue
        rss = mib(series, who)
        after_minute = mib([r for r in series if r["t"] >= 60], who)
        summary[who] = {"first_mib": round(rss[0], 2), "last_mib": round(rss[-1], 2), "growth_mib": round(rss[-1] - rss[0], 2), "min_mib": round(min(rss), 2), "max_mib": round(max(rss), 2),
                        "growth_after_minute_mib": round(after_minute[-1] - after_minute[0], 2), "threads": [series[0][who]["threads"], series[-1][who]["threads"]],
                        "fds": [series[0][who]["fds"], series[-1][who]["fds"]], "peak_mib": round(series[-1][who]["peak_kib"] / 1024, 2)}
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    run(sys.argv[1], float(sys.argv[2]) if len(sys.argv) > 2 else 600.0)
