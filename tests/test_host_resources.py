import os
import time

from lico_measure.procscope import collect_scopes, tree_pids
from lico_measure.rig import private_dir

from lico_measure.p02 import ProtoConn as Conn, start
from lico_protocol import v1


def test_resource_report_matches_process_scope():
    with private_dir() as directory:
        host, log = start(directory, sessions=3, producing=0, history=300, retain=100)   # idle: the log size is exact
        try:
            conn = Conn(host.socket)
            watcher = Conn(host.socket)
            watcher.call(op="subscribe", session=host.sessions[0], after=-1, snapshot=True)
            view = conn.call(op="view.attach", session=host.sessions[1], kind="cli")
            time.sleep(1.0)
            report = conn.call(op="resources")
            assert v1.validate_response("resources", report) == []
            # read the same process independently from /proc
            scope = collect_scopes({"host": host.pid})["host"]
            host_proc = [p for p in scope.processes if p.pid == host.pid][0]
            assert abs(report["rss_kb"] - host_proc.rss_kb) <= 2048, (report["rss_kb"], host_proc.rss_kb)
            assert abs(report["pss_kb"] - host_proc.pss_kb) <= 2048
            tasks = len(os.listdir(f"/proc/{host.pid}/task"))
            assert abs(report["threads"] - tasks) <= 3, (report["threads"], tasks)
            # the agents named in the report are exactly the host's running child processes
            children = {pid for pid in tree_pids(host.pid) if pid != host.pid}
            assert {a["pid"] for a in report["agents"]} == children
            for agent in report["agents"]:
                actual = [p for p in scope.processes if p.pid == agent["pid"]][0].rss_kb
                assert abs(agent["rss_kb"] - actual) <= 2048
            # counts agree with the host's own status, and the log size with the files on disk
            status = conn.call(op="status")["sessions"]
            assert report["sessions"] == len(status) == 3
            assert report["subscribers"] == sum(s["subscribers"] for s in status) == 2
            assert report["views"] == 1 and view["ok"] == 1 and v1.validate_response("view.attach", view) == []
            on_disk = sum(os.path.getsize(log / name) for name in os.listdir(log))
            assert report["log_bytes"] == on_disk, (report["log_bytes"], on_disk)
            assert report["running_sessions"] == 3 - sum(1 for s in status if s["state"] == "exited")
        finally:
            host.stop()
