"""The tools that measure and keep the suite's memory down: who held the peak (lico_measure/ledger.py) and the sweep of a module's host (lico_measure/sweep.py).

The first is checked on a made-up process table and once on a real child; the second against a host that is only a recording."""
import os
import subprocess
import sys

from lico_measure.ledger import PAGE, Tree
from lico_measure.sweep import end_all


def table(*rows):
    """pid -> (ppid, start, rss pages, comm), the shape of ledger.scan()."""
    return {pid: (ppid, 0, pages, name) for pid, ppid, pages, name in rows}


def test_the_peak_is_split_by_the_direct_children_of_the_root_with_everything_below_them():
    tree = Tree(1)
    found = table((1, 0, 5, "suite"), (2, 1, 10, "cargo"), (4, 2, 7, "rustc"), (5, 4, 1, "lld"), (3, 1, 20, "pytest"), (6, 3, 2, "lico"), (9, 8, 99, "stranger"))
    counted = {1, 2, 3, 4, 5, 6}
    assert tree.split(found, counted) == {2: (10 + 7 + 1) * PAGE, 3: (20 + 2) * PAGE}, "the root itself and a process outside the tree are not in it"


def test_a_real_child_is_found_in_the_split_of_a_new_peak():
    child = subprocess.Popen([sys.executable, "-c", "import time; x = bytearray(20_000_000); time.sleep(30)"])
    try:
        tree = Tree(os.getpid())
        for _ in range(100):
            tree.sample()
            if tree.run_peak_split.get(child.pid, 0) > 15_000_000:
                break
            subprocess.run(["sleep", "0.05"])
        assert tree.run_peak_split.get(child.pid, 0) > 15_000_000, tree.run_peak_split
        assert sum(tree.run_peak_split.values()) <= tree.run_peak
    finally:
        child.kill()
        child.wait()


class Recording:
    def __init__(self, sessions):
        self.sessions, self.calls = sessions, []

    def call(self, **request):
        self.calls.append(request)
        return {"ok": 1, "sessions": self.sessions}


def test_the_sweep_purges_every_session_the_host_has_with_the_generation_it_is_on_and_nothing_else():
    conn = Recording([{"session": "a1", "generation": 1}, {"session": "b2", "generation": 3}])
    end_all(conn)
    assert conn.calls == [{"op": "status"}, {"op": "session.purge", "session": "a1", "generation": 1}, {"op": "session.purge", "session": "b2", "generation": 3}]


def test_the_sweep_of_an_empty_host_asks_once_and_purges_nothing():
    conn = Recording([])
    end_all(conn)
    assert conn.calls == [{"op": "status"}]
