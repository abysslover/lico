"""Where does the host's transient peak come from? VmRSS and VmHWM of a host read at five points: after the start, after three sessions are made, after their first turns, after the first `status` and after a second one.

Run:  LICO_BUILD_ROOT=<build root> PYTHONPATH=src:. python3 status_peak.py [RUNS]
Prints one JSON line per run. Needs the repository's venv and the test helpers; no model turn (the simulator is the program).
"""
import json
import sys
import time

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.test_claude_adapter import claude_host, new_claude, records
from tests.test_codex_adapter import wait_for


def proc(pid):
    fields = {}
    for line in open(f"/proc/{pid}/status"):
        key, _, value = line.partition(":")
        fields[key] = value.split()
    return {"rss_kib": int(fields["VmRSS"][0]), "peak_kib": int(fields["VmHWM"][0])}


def one_run():
    with private_dir() as directory:
        host, log, _ = claude_host(directory)
        try:
            points = {}
            time.sleep(1)
            points["after_start"] = proc(host.pid)
            conn = Conn(host.socket)
            sessions = [new_claude(conn, name=f"s-{i}") for i in range(3)]
            time.sleep(1)
            points["after_three_sessions_made"] = proc(host.pid)
            for n, (sid, generation) in enumerate(sessions):
                assert conn.call(op="input", session=sid, generation=generation, request_id=f"f{n}", text="hi")["ok"] == 1
            for sid, _ in sessions:
                wait_for(lambda: "[turn completed]" in records(log, sid))
            time.sleep(2)
            points["after_first_turns"] = proc(host.pid)
            reply = conn.call(op="status")
            points["after_first_status"] = proc(host.pid)
            points["status_reply_chars"] = len(str(reply))
            conn.call(op="status")
            points["after_second_status"] = proc(host.pid)
            return points
        finally:
            host.stop()


if __name__ == "__main__":
    for _ in range(int(sys.argv[1]) if len(sys.argv) > 1 else 3):
        print(json.dumps(one_run()))
