"""The test suite as one command. Run it inside the heavy lane:

    pri-heavy --label lico --cap 5G --wait 3600 -- python -m lico_measure.suite run [--changed [BASE]] [--receipts | --cold] [--workers N]
    python -m lico_measure.suite select --changed [BASE]      # print what a change needs, run nothing
    pri-heavy --label lico --cap 5G --wait 3600 -- python -m lico_measure.suite acceptance
    pri-heavy --label lico --cap 5G --wait 3600 -- python -m lico_measure.suite legacy        # the retired prototypes: never in `run`

`run` builds the product once (not when the hash of its sources is the one the binary was built from), then runs the test files
on up to 3 workers, a file at a time, the longest first. Each worker has its own client home; every test already has its own
socket, run directory and display. The summary names the tests over their budget (tests/budget.toml); a run that goes over a
budget, over the cold-run limit or over the memory bound exits 3, a run with a failing test exits 1.
Exit: 0 all passed, 1 a test failed, 2 the run could not start, 3 a guard (budget, cold-run limit, memory) was broken."""
import argparse
import hashlib
import json
import os
import queue
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import tomllib
from pathlib import Path

from . import receipts, testmap
from .ledger import INTERVAL, Tree

REPO = testmap.REPO
LOGS = REPO / "private" / "suite"
COMPILE_JOBS = "2"        # rustc at opt-level 2 on the text libraries takes over a GiB each: a rebuild of the dependencies on all cores broke the memory bound


def budget(root=REPO):
    return tomllib.loads((Path(root) / "tests" / "budget.toml").read_text())


def allowed_seconds(nodeid, table):
    """The longest entry of the budget table that is a prefix of the node id (parameters ignored), else the default."""
    best = max((e for e in table.get("allow", []) if nodeid.startswith(e["test"])), key=lambda e: len(e["test"]), default=None)
    return best["seconds"] if best else table["default_s"]


def over_budget(rows, table):
    return sorted(((r["wall_s"], r["nodeid"], allowed_seconds(r["nodeid"], table)) for r in rows if r["wall_s"] > allowed_seconds(r["nodeid"], table)), reverse=True)


# --- build once -----------------------------------------------------------------------------------------------

def source_hash(files):
    """A hash of everything the product binary is built from: the crate, the license text compiled in."""
    hasher = hashlib.sha256()
    for path in sorted(f for f in files if f.startswith("src/host/hostd/") or f == "LICENSE"):
        hasher.update(f"{path} {receipts.digest(REPO / path)}\n".encode())
    return hasher.hexdigest()


def ensure_build(files):
    """Build the binaries the tests start unless the stamp says they were built from exactly these sources. Returns (what happened, seconds)."""
    out = receipts.build_root() / "host_hostd"
    binary, stamp = out / "release" / "lico", out / "release" / ".tree"
    current = source_hash(files)
    try:
        recorded = json.loads(stamp.read_text())
    except (OSError, ValueError):
        recorded = {}
    if recorded.get("tree") == current and recorded.get("lico") == receipts.digest(binary) and (out / "release" / "lico-synth-agent").is_file():
        return "build: skipped (the sources hash to the one the binary was built from)", 0.0
    env = dict(os.environ, CARGO_HOME=str(REPO / "private" / "cargo-home"), CARGO_TARGET_DIR=str(out), CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS=COMPILE_JOBS)
    began = time.monotonic()
    done = subprocess.run(["cargo", "build", "--release", "--locked", "--offline", "--manifest-path", "src/host/hostd/Cargo.toml", "--bins"],
                          cwd=REPO, env=env, capture_output=True, text=True)
    seconds = time.monotonic() - began
    if done.returncode:
        raise SystemExit(f"the build failed:\n{done.stderr[-3000:]}")
    stamp.write_text(json.dumps({"tree": current, "lico": receipts.digest(binary)}) + "\n")
    return f"build: {seconds:.1f} s", seconds


# --- one job --------------------------------------------------------------------------------------------------

class Job:
    def __init__(self, name, argv, cwd, extra_env):
        self.name, self.argv, self.cwd, self.extra_env = name, argv, cwd, extra_env
        self.rc = None
        self.seconds = 0.0
        self.slot = None
        self.rows = []
        self.ledger = None
        self.log = None


def pytest_job(test_file, out):
    job = Job(test_file, None, REPO, {})
    job.ledger = out / (test_file.replace("/", "__") + ".json")
    job.log = out / (test_file.replace("/", "__") + ".log")
    job.argv = [sys.executable, "-m", "pytest", "-p", "lico_measure.ledger", "--ledger", str(job.ledger), "-q", "-p", "no:cacheprovider", test_file]
    return job


def cargo_job(filters, out):
    """The Rust unit tests: one run of the test binary, its own target directory (the tests build in the dev profile)."""
    job = Job("cargo test --bin lico" + "".join(f" {f}" for f in filters), None, REPO / "src" / "host" / "hostd", {})
    job.log = out / "cargo.log"
    job.extra_env = {"CARGO_NET_OFFLINE": "true", "CARGO_HOME": str(REPO / "private" / "cargo-home"),
                     "CARGO_TARGET_DIR": str(receipts.build_root() / "host_hostd_test"), "CARGO_BUILD_JOBS": COMPILE_JOBS}
    job.argv = ["cargo", "test", "--bin", "lico", *(["--", *filters] if filters else [])]      # cargo takes one filter; libtest takes any number after --
    return job


def execute(job, slot, env):
    job.slot = slot
    began = time.monotonic()
    with open(job.log, "wb") as log:
        proc = subprocess.Popen(job.argv, cwd=job.cwd, env={**env, **job.extra_env, "LICO_WORKER": str(slot)}, stdout=log, stderr=subprocess.STDOUT,
                                start_new_session=True)
        job.pid = proc.pid
        job.rc = proc.wait()
    job.seconds = time.monotonic() - began
    if job.ledger and job.ledger.exists():
        job.rows = json.loads(job.ledger.read_text())["tests"]
    elif job.ledger and job.ledger.with_suffix(".jsonl").exists():       # the run was killed: what finished is in the journal
        job.rows = [json.loads(line) for line in job.ledger.with_suffix(".jsonl").read_text().splitlines() if line]
    return job


def passed(job, acceptance_only):
    """rc 0; or pytest's 5 (nothing collected) for a file the map names as acceptance-only, whose every test the daily run leaves out.
    Anywhere else nothing collected is a mistake - a mistyped marker, an addopts that deselects everything - and not a pass."""
    return job.rc == 0 or (job.rc == 5 and job.ledger is not None and job.name in acceptance_only)


# --- run ------------------------------------------------------------------------------------------------------

def use_receipts(args, selection):
    """(whether a receipt may skip a file, what to tell the reader). A selection of EVERYTHING means a changed file nobody declared:
    such a file is in no receipt key, so a receipt cannot say it is safe to skip - the run behaves as `--cold`."""
    if not args.receipts or args.cold:
        return False, None
    if selection is not None and selection.everything:
        return False, "a changed file nobody declared selects everything (default-deny): receipts are ignored, as with --cold"
    return True, None


def run(args):
    files = testmap.tracked()
    every = sorted(f for f in files if re.fullmatch(r"tests/test_[^/]+\.py", f))
    selection, cargo_filters = None, []
    if args.changed is not None:
        selection = testmap.select(testmap.changed(args.changed or None), files=files)
        wanted = selection.tests(every)
        cargo_filters = None if "" in selection.cargo else sorted(selection.cargo)
        print(testmap.render(selection, every), flush=True)
    else:
        wanted = [f for f in (args.paths or every) if f in every or Path(f).exists()]
        cargo_filters = None if not args.paths else []
    full = args.changed is None and not args.paths
    use_receipt, why_not = use_receipts(args, selection)
    if why_not:
        print(why_not, flush=True)
    jobs = []
    tree = Tree(os.getpid())            # the one tree and the one sampler: the whole run is measured from here, the build included
    tree.where = "the build"
    stop = threading.Event()
    clock0 = time.monotonic()

    def sampler():
        while not stop.wait(INTERVAL):
            tree.where = f"{time.monotonic() - clock0:.0f} s into the run: " + (", ".join(j.name for j in jobs if j.slot and j.rc is None) or "the build")
            tree.sample()                                    # a new peak keeps the text above: what was running when it was reached

    threading.Thread(target=sampler, name="suite-sampler", daemon=True).start()
    build_line, build_s = ensure_build(files)
    print(build_line, flush=True)

    graph = testmap.python_graph(files)
    tmap, hashes, binary_hashes = testmap.load_map(), {}, receipts.binaries()
    acceptance_only = set(tmap.get("acceptance", {}).get("only", []))
    keys = {f: receipts.key(f, graph, tmap, files, hashes, binary_hashes) for f in wanted}
    skipped = []
    if use_receipt:
        skipped = [f for f in wanted if receipts.valid(f, keys[f])]
        for f in skipped:
            print(f"skipped (receipt) {f}", flush=True)
    todo = [f for f in wanted if f not in skipped]

    stamp = time.strftime("%Y%m%d-%H%M%S")
    out = LOGS / stamp
    out.mkdir(parents=True, exist_ok=True)
    estimate = receipts.durations()
    jobs.extend(pytest_job(f, out) for f in sorted(todo, key=lambda f: -estimate.get(f, 10.0)))
    cargo_key = hashlib.sha256((source_hash(files) + repr(cargo_filters)).encode()).hexdigest()
    run_cargo = cargo_filters is None or bool(cargo_filters)
    cargo_skipped = False
    if run_cargo:
        if use_receipt and receipts.valid("cargo", cargo_key):
            cargo_skipped = True
            print("skipped (receipt) cargo test --bin lico", flush=True)
        else:
            jobs.insert(0, cargo_job([] if cargo_filters is None else cargo_filters, out))
    if not jobs:
        stop.set()
        print(f"nothing to run: {len(skipped)} file(s) skipped by receipt")
        return 0

    env = dict(os.environ, PYTHONPATH=f"{REPO / 'src'}:{REPO}", LICO_BUILD_ROOT=str(receipts.build_root()))
    pending, finished = queue.Queue(), []
    for job in jobs:
        pending.put(job)
    slots = queue.Queue()
    workers = max(1, min(args.workers, len(jobs)))
    for n in range(workers):
        slots.put(n + 1)
    began = time.monotonic()                                 # the cold-run limit counts from here: the tests, not the build

    def worker():
        while True:
            try:
                job = pending.get_nowait()
            except queue.Empty:
                return
            slot = slots.get()
            try:
                execute(job, slot, env)
            finally:
                slots.put(slot)
            finished.append(job)
            count = len(job.rows) or "-"
            print(f"[{time.monotonic() - began:6.1f} s] {'ok  ' if passed(job, acceptance_only) else 'FAIL'} {job.name}  {count} tests  {job.seconds:.1f} s  (worker {job.slot})", flush=True)

    threads = [threading.Thread(target=worker) for _ in range(workers)]
    try:
        for t in threads:
            t.start()
        for t in threads:
            t.join()
    except KeyboardInterrupt:
        for job in jobs:
            if getattr(job, "pid", None) and job.rc is None:
                try:
                    os.killpg(job.pid, signal.SIGTERM)
                except OSError:
                    pass
        raise
    stop.set()
    tree.sample()
    wall = time.monotonic() - began

    rows = [dict(row, worker=job.slot) for job in finished for row in job.rows]
    failing = [job for job in finished if not passed(job, acceptance_only)]
    for job in failing:
        tail = job.log.read_text(errors="replace").splitlines()[-40:]
        print(f"\n--- {job.name} (rc {job.rc}), the end of {job.log.relative_to(REPO)}\n" + "\n".join(tail))
    table = budget()
    slow = over_budget(rows, table)
    peak = round(tree.run_peak / 2 ** 20, 1)
    seen = {job.name: job.seconds for job in finished if job.ledger}
    for job in finished:
        if job.ledger and passed(job, acceptance_only):
            receipts.record(job.name, keys[job.name], len(job.rows), job.seconds)
        elif job.ledger:
            receipts.forget(job.name)
    if run_cargo and not cargo_skipped:
        cargo = next(j for j in finished if j.ledger is None)
        if passed(cargo, acceptance_only):
            receipts.record("cargo", cargo_key, 0, cargo.seconds)
        else:
            receipts.forget("cargo")
    receipts.save_durations(seen)

    document = {"total_s": round(wall, 1), "peak_rss_mib": peak, "peak_at": tree.run_peak_at, "workers": workers,
                "files": len(todo), "skipped_by_receipt": len(skipped) + int(cargo_skipped), "build_s": round(build_s, 1),
                "tests": sorted(rows, key=lambda r: -r.get("wall_s", 0))}
    if args.ledger:
        Path(args.ledger).parent.mkdir(parents=True, exist_ok=True)
        Path(args.ledger).write_text(json.dumps(document, indent=1) + "\n")

    outcomes = {}
    for row in rows:
        outcomes[row.get("outcome", "unknown")] = outcomes.get(row.get("outcome", "unknown"), 0) + 1
    print(f"\n{len(rows)} tests in {len(todo)} files on {workers} worker(s): {', '.join(f'{n} {k}' for k, n in sorted(outcomes.items()))}; "
          f"{len(skipped) + int(cargo_skipped)} skipped (receipt); {wall:.1f} s; peak {peak} MiB (all workers together) {tree.run_peak_at}; {build_line}")
    for seconds, nodeid, limit in slow:
        print(f"over budget: {seconds:.1f} s > {limit} s  {nodeid}")
    broken = []
    if slow:
        broken.append(f"{len(slow)} test(s) over their budget")
    if full and not skipped and wall > table["cold_run_s"]:
        broken.append(f"the cold run took {wall:.0f} s, the limit is {table['cold_run_s']} s")
    if peak > table["peak_rss_mib"]:
        broken.append(f"the peak memory {peak} MiB is over the bound of {table['peak_rss_mib']} MiB")
    for line in broken:
        print("GUARD:", line)
    if failing:
        return 1
    return 3 if broken else 0


def marked(marker):
    done = subprocess.run([sys.executable, "-m", "pytest", "-o", "addopts=", "-m", marker, "-q", "-p", "no:cacheprovider", "tests"],
                          cwd=REPO, env=dict(os.environ, PYTHONPATH=f"{REPO / 'src'}:{REPO}", LICO_BUILD_ROOT=str(receipts.build_root())))
    return done.returncode


def acceptance(args):
    """The clean-clone build: marked `acceptance`, left out of the daily run. Due when a trigger of tests/testmap.toml changes, and before a merge."""
    return marked("acceptance")


def legacy(args):
    """The retired prototypes (candidate clients a b c c2 d, host prototypes): marked `legacy_candidates`, left out of the daily run, receipts and
    budget. They need their binaries built by hand under the build root (the GTK, Qt and WebKitGTK development packages); a missing one fails its test."""
    return marked("legacy_candidates")


def select(args):
    files = testmap.tracked()
    every = sorted(f for f in files if re.fullmatch(r"tests/test_[^/]+\.py", f))
    result = testmap.select(testmap.changed(args.changed or None), files=files)
    if args.json:
        print(json.dumps({"everything": result.everything, "cargo": sorted(result.cargo), "tests": result.tests(every), "acceptance": result.acceptance}, indent=1))
    else:
        print(testmap.render(result, every))
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(prog="python -m lico_measure.suite", description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    runner = sub.add_parser("run", help="build once, run the tests on up to 3 workers")
    runner.add_argument("paths", nargs="*", help="test files; none: the whole suite")
    runner.add_argument("--changed", nargs="?", const="", default=None, metavar="BASE", help="only what the changes since BASE (default HEAD: staged, unstaged, new) need")
    runner.add_argument("--receipts", action="store_true", help="skip a test file whose receipt is current (it says: skipped (receipt))")
    runner.add_argument("--cold", action="store_true", help="ignore receipts")
    runner.add_argument("--workers", type=int, default=3)
    runner.add_argument("--ledger", help="write the per-test time and peak memory of the run to this file")
    chooser = sub.add_parser("select", help="print the tests a change needs")
    chooser.add_argument("--changed", nargs="?", const="", default="", metavar="BASE")
    chooser.add_argument("--json", action="store_true")
    sub.add_parser("acceptance", help="the clean-clone build")
    sub.add_parser("legacy", help="the retired prototype clients and hosts (marker legacy_candidates; hand-built binaries)")
    args = parser.parse_args(argv)
    return {"run": run, "select": select, "acceptance": acceptance, "legacy": legacy}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
