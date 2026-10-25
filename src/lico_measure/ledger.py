"""A pytest plugin that records, for every test, the wall time and the peak resident memory of the test's whole process tree
(pytest itself, the host, Xvfb, the window, whatever they started), plus the kinds of process the tree held.

    PYTHONPATH=src python -m pytest -p lico_measure.ledger --ledger private/t025/ledger.json tests

Without --ledger the plugin does nothing. It does not change what a test sees: no environment variable, no subreaper, no signal.
The memory is the sum of the resident sets of the tree, sampled every 100 ms and at the edges of each test; pages that
several processes share (the binary's text, libc) are counted once per process, so the figure is an upper bound.
A process stays in the tree after its parent died if it was seen in the tree before (a detached host); one that is born and
orphaned between two samples is missed."""
import json
import os
import threading
import time
from pathlib import Path

PAGE = os.sysconf("SC_PAGE_SIZE")
INTERVAL = 0.1


def scan():
    """pid -> (ppid, start, rss pages, comm) for every process; a process that vanishes while it is read is skipped."""
    found = {}
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            with open(f"/proc/{name}/stat", "rb") as handle:
                data = handle.read()
        except OSError:
            continue
        close = data.rfind(b")")
        rest = data[close + 2:].split()
        found[int(name)] = (int(rest[1]), int(rest[19]), int(rest[21]), data[data.find(b"(") + 1:close].decode("utf-8", "replace"))
    return found


def kind(pid, comm):
    """What a process is, for the class of a test: `lico host`, `lico gui`, `Xvfb`, `cargo`, ... A name only, never an argument value."""
    if comm != "lico":
        return comm
    try:
        words = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
    except OSError:
        return comm
    sub = words[1].decode("utf-8", "replace") if len(words) > 1 else ""
    return f"lico {sub}" if sub in ("host", "gui", "cred", "login") else comm


class Tree:
    """The processes below one root, sampled."""

    def __init__(self, root):
        self.root = root
        self.known = {root: None}          # pid -> start time, for the pids already counted
        self.kinds = {}                    # (pid, start) -> kind, read once
        self.lock = threading.Lock()
        self.window_peak = 0               # bytes, since the last reset
        self.window_kinds = set()
        self.run_peak = 0
        self.run_peak_at = ""
        self.where = ""

    def sample(self):
        found = scan()
        with self.lock:
            counted = {self.root} | {pid for pid, start in self.known.items() if pid in found and (start is None or found[pid][1] == start)}
            grew = True
            while grew:                                      # children of anything counted
                grew = False
                for pid, (ppid, start, _, _) in found.items():
                    if pid not in counted and ppid in counted:
                        counted.add(pid)
                        grew = True
            total = 0
            for pid in counted:
                ppid, start, rss, comm = found[pid]
                total += rss * PAGE
                self.known[pid] = start
                if pid != self.root and (pid, start) not in self.kinds:
                    self.kinds[(pid, start)] = kind(pid, comm)
                if pid != self.root:
                    self.window_kinds.add(self.kinds[(pid, start)])
            self.known = {pid: start for pid, start in self.known.items() if pid in counted}
            self.window_peak = max(self.window_peak, total)
            if total > self.run_peak:
                self.run_peak, self.run_peak_at = total, self.where
            return total

    def reset_window(self):
        with self.lock:
            self.window_peak, self.window_kinds = 0, set()

    def take_window(self):
        with self.lock:
            return self.window_peak, sorted(self.window_kinds)


class Ledger:
    def __init__(self, out):
        self.out = Path(out)
        self.tree = Tree(os.getpid())
        self.rows = {}
        self.stop = threading.Event()
        self.started = time.monotonic()
        self.collect_s = 0.0
        self.sampler = threading.Thread(target=self.loop, name="ledger-sampler", daemon=True)

    def loop(self):
        while not self.stop.wait(INTERVAL):
            self.tree.sample()

    # --- pytest hooks (wired below) ---
    def collection_finished(self):
        self.collect_s = time.monotonic() - self.started

    def start_test(self, nodeid):
        self.tree.where = nodeid
        self.tree.sample()
        self.tree.reset_window()
        self.rows[nodeid] = {"nodeid": nodeid, "begin": time.monotonic()}

    def report(self, rep):
        row = self.rows.get(rep.nodeid)
        if row is None:
            return
        row[rep.when + "_s"] = round(rep.duration, 3)
        if rep.outcome != "passed" or rep.when == "call":
            row["outcome"] = row.get("outcome") if row.get("outcome") not in (None, "passed") else rep.outcome

    def finish_test(self, nodeid):
        self.tree.sample()
        row = self.rows[nodeid]
        row["wall_s"] = round(time.monotonic() - row.pop("begin"), 3)
        peak, kinds = self.tree.take_window()
        row["peak_rss_mib"] = round(peak / 2 ** 20, 1)
        row["kinds"] = kinds
        row.setdefault("outcome", "unknown")
        with open(self.out.with_suffix(".jsonl"), "a") as handle:      # survives a run that is killed
            handle.write(json.dumps(row) + "\n")

    def finish(self):
        self.stop.set()
        self.tree.sample()
        total = round(time.monotonic() - self.started, 1)
        document = {"collect_s": round(self.collect_s, 2), "total_s": total, "peak_rss_mib": round(self.tree.run_peak / 2 ** 20, 1),
                    "peak_at": self.tree.run_peak_at, "tests": sorted(self.rows.values(), key=lambda r: -r.get("wall_s", 0))}
        self.out.parent.mkdir(parents=True, exist_ok=True)
        self.out.write_text(json.dumps(document, indent=1) + "\n")
        return document


_ledger = None


def pytest_addoption(parser):
    parser.addoption("--ledger", default=None, help="write the per-test time and peak memory (process tree) to this JSON file")


def pytest_configure(config):
    global _ledger
    out = config.getoption("--ledger")
    if out:
        Path(out).with_suffix(".jsonl").unlink(missing_ok=True)
        _ledger = Ledger(out)
        _ledger.sampler.start()


def pytest_collection_finish(session):
    if _ledger:
        _ledger.collection_finished()


def pytest_runtest_logstart(nodeid, location):
    if _ledger:
        _ledger.start_test(nodeid)


def pytest_runtest_logreport(report):
    if _ledger:
        _ledger.report(report)


def pytest_runtest_logfinish(nodeid, location):
    if _ledger:
        _ledger.finish_test(nodeid)


def pytest_sessionfinish(session):
    if _ledger:
        document = _ledger.finish()
        print(f"\nledger: {len(document['tests'])} tests, {document['total_s']} s, collection {document['collect_s']} s, "
              f"peak {document['peak_rss_mib']} MiB at {document['peak_at']}")
