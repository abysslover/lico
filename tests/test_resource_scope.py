import os
import subprocess
import sys
import time

import pytest

from lico_measure.procscope import ScopeError, collect_scopes
from lico_measure.workload import RECORD_BYTES, record, records

TREE_SCRIPT = (
    "import subprocess, sys, time\n"
    "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])\n"
    "print(child.pid, flush=True)\n"
    "time.sleep(30)\n"
)


def _spawn_tree():
    root = subprocess.Popen(
        [sys.executable, "-c", TREE_SCRIPT], stdout=subprocess.PIPE, text=True
    )
    helper_pid = int(root.stdout.readline())
    time.sleep(0.3)
    return root, helper_pid


def _stop(*procs):
    for proc in procs:
        proc.kill()
        proc.wait()


def test_resource_process_tree_scope():
    gui, gui_helper = _spawn_tree()
    host = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
    time.sleep(0.3)
    try:
        scopes = collect_scopes({"gui": gui.pid, "host": host.pid})
        gui_pids = {p.pid for p in scopes["gui"].processes}
        # the helper descendant must not be omitted, the host must not leak in
        assert gui_helper in gui_pids and gui.pid in gui_pids
        assert host.pid not in gui_pids
        assert {p.pid for p in scopes["host"].processes} == {host.pid}
        # each process is counted once; totals are the sums of their parts
        assert len(gui_pids) == len(scopes["gui"].processes)
        assert scopes["gui"].rss_kb == sum(p.rss_kb for p in scopes["gui"].processes)
        assert scopes["gui"].pss_kb == sum(p.pss_kb for p in scopes["gui"].processes)
        assert all(0 < p.pss_kb <= p.rss_kb for p in scopes["gui"].processes)
        # a process reachable from two categories is refused, not double counted
        with pytest.raises(ScopeError):
            collect_scopes({"gui": gui.pid, "overlap": gui_helper})
    finally:
        _stop(gui, host)
        os.kill(gui_helper, 9)


def test_workload_records_are_sequenced_and_bounded():
    first = list(records(start=0, count=100))
    assert [r.seq for r in first] == list(range(100))
    assert all(len(r.payload) == RECORD_BYTES for r in first)
    # deterministic and resumable from a cursor: no gap, no duplicate
    assert record(57).payload == first[57].payload
    assert [r.seq for r in records(start=50, count=10)] == list(range(50, 60))
    with pytest.raises(ValueError):
        list(records(start=0, count=-1))
