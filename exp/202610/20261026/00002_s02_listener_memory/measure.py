"""S02: what the tool listener costs the host in memory, threads and descriptors.

Question. Does a host with 10 Claude sessions (the simulator, one process each) stay within 2 MiB of resident memory of the same host built without the
tool connection (stop line of the P03 plan), and what does a burst of connections to the listener add?

Workload. A private host; 10 Claude sessions, one turn each (so every session has its process); idle 3 s; then host RSS (VmRSS), peak (VmHWM), threads and
open descriptors from /proc. With the tool connection: then every session makes one `tools` turn (initialize, initialized, list, call over HTTP) and 32
connections are held open at once to the listener (its cap), and the same is read again, and again after the connections close.

Run. `PYTHONPATH=src:. LICO_BUILD_ROOT=<root> python3 measure.py before|after [REPEATS]` prints one JSON line; the build root holds `host_hostd/release/`. Run it once per binary
(a host built from the commit before the tool connection and one built with it) and compare the lines. Needs the repository's venv and the test helpers.
"""
import json
import socket
import sys
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import claude_host, new_claude, records
from tests.test_codex_adapter import wait_for


def proc(pid):
    fields = {}
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        key, _, value = line.partition(":")
        fields[key] = value.split()
    return {"rss_kib": int(fields["VmRSS"][0]), "peak_kib": int(fields["VmHWM"][0]), "threads": int(fields["Threads"][0]), "fds": len(list(Path(f"/proc/{pid}/fd").iterdir()))}


def listener_port(directory):
    for path in sorted((Path(directory) / "tools").glob("*.mcp.json")):
        return int(json.loads(path.read_text())["mcpServers"]["lico"]["url"].split(":")[2].split("/")[0])
    return None


def one_run(with_tools):
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sessions = [new_claude(conn) for _ in range(10)]
            for number, (sid, generation) in enumerate(sessions):
                assert conn.call(op="input", session=sid, generation=generation, request_id=f"m-{number}", text="hi")["ok"] == 1
            for sid, _ in sessions:
                wait_for(lambda: "[turn completed]" in records(log, sid))
            time.sleep(3)
            out = {"idle": proc(host.pid)}
            if with_tools:
                for number, (sid, generation) in enumerate(sessions):
                    assert conn.call(op="input", session=sid, generation=generation, request_id=f"t-{number}", text="tools")["ok"] == 1
                for sid, _ in sessions:
                    wait_for(lambda: any(r.startswith("call 200") for r in records(log, sid)))
                time.sleep(1)
                out["after_tool_turns"] = proc(host.pid)
                port = listener_port(directory)
                held = []
                for _ in range(32):
                    sock = socket.create_connection(("127.0.0.1", port))
                    sock.sendall(b"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:%d\r\n" % port)      # a head that never ends: the connection and its thread are held
                    held.append(sock)
                time.sleep(1)
                out["32_held"] = proc(host.pid)
                for sock in held:
                    sock.close()
                time.sleep(2)
                out["after_close"] = proc(host.pid)
            return out
        finally:
            host.stop()


if __name__ == "__main__":
    label, repeats = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 3
    with_tools = label == "after"
    print(json.dumps({"label": label, "runs": [one_run(with_tools) for _ in range(repeats)]}))
