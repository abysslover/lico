"""Receipts: a test file that passed need not run again while nothing it depends on has changed.

A receipt is private/receipts/<test file>.json: {"key": ..., "tests": n, "seconds": s}. The key is a hash of everything the file's
result can depend on, and of nothing else:
  the test file, and every Python file it imports (directly or not)
  the files it reads by path: the fixtures named in those sources, and the [[sources]] of the map that its tests cover
  the executables under the build root, if the file starts any process of the product (tests/testmap.toml, [areas])
  pyproject.toml (the pytest options) and the Python version
A file that failed has no receipt and is also written to failed.json, which no key can override: it runs again. `--cold` ignores
receipts altogether (and still writes the ones it earns)."""
import hashlib
import json
import os
import sys
import time
from pathlib import Path

from . import testmap

DIRECTORY = testmap.REPO / "private" / "receipts"
# the executables the tests start (see rig.HOST_BUILD and rig.candidate_command): relative to the build root
BINARIES = ("host_hostd/release/lico", "host_hostd/release/lico-synth-agent", "b_gtk/candidate-b", "c_qt/candidate-c",
            "d_webkit/candidate-d", "host_c/lico-host-c", "host_rust_std/release/lico-host", "host_rust_serde/release/lico-host")


def build_root():
    return Path(os.environ.get("LICO_BUILD_ROOT", testmap.REPO / "private" / "build"))


def digest(path):
    hasher = hashlib.sha256()
    try:
        with open(path, "rb") as handle:
            for block in iter(lambda: handle.read(1 << 20), b""):
                hasher.update(block)
    except OSError:
        return "absent"
    return hasher.hexdigest()


def binaries():
    """name -> sha256 of every executable the tests may start that exists."""
    root = build_root()
    return {name: digest(root / name) for name in BINARIES if (root / name).is_file()}


def reach(graph, start):
    """Every file `start` imports, however indirectly (start included)."""
    found, todo = {start}, [start]
    while todo:
        for edge in graph.get(todo.pop(), ()):
            if edge not in found:
                found.add(edge)
                todo.append(edge)
    return found


def key(test_file, graph, tmap, files, hashes, binary_hashes, root=testmap.REPO):
    """The hash of what `test_file`'s result depends on. `hashes` caches the sha256 of the files read, by path."""
    closure = reach(graph, test_file)
    texts = " ".join((Path(root) / f).read_text(errors="replace") for f in sorted(closure))
    read = set(closure)
    read |= {f for f in files if f.startswith("tests/fixtures/") and Path(f).name in texts}
    for entry in tmap.get("sources", []):
        if set(entry["tests"]) & closure:
            read |= {f for f in files if testmap.glob_match(f, entry["glob"])}
    if test_file in tmap.get("always", {}).get("tests", []):
        read |= {f for f in files if f.endswith((".rs", ".py", ".toml")) and f.startswith(("src/", "tests/"))}      # it reads the whole tree
    read.add("pyproject.toml")
    parts = [f"python {sys.version_info[:3]}"]
    for f in sorted(read):
        if f not in hashes:
            hashes[f] = digest(Path(root) / f)
        parts.append(f"{f} {hashes[f]}")
    if tmap.get("areas", {}).get(test_file, ["*"]):         # a file with no area never starts the product
        parts += [f"binary {name} {value}" for name, value in sorted(binary_hashes.items())]
    return hashlib.sha256("\n".join(parts).encode()).hexdigest()


def path_of(test_file):
    return DIRECTORY / (test_file.replace("/", "__") + ".json")


def valid(test_file, current_key):
    """Is there a receipt for this exact key, and was the file not failing the last time it ran?"""
    if test_file in failed():
        return False
    try:
        return json.loads(path_of(test_file).read_text())["key"] == current_key
    except (OSError, ValueError, KeyError):
        return False


def failed():
    try:
        return set(json.loads((DIRECTORY / "failed.json").read_text()))
    except (OSError, ValueError):
        return set()


def record(test_file, current_key, tests, seconds):
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    path_of(test_file).write_text(json.dumps({"key": current_key, "tests": tests, "seconds": round(seconds, 1), "at": time.strftime("%F %T")}) + "\n")
    mark_failed(test_file, False)


def forget(test_file):
    path_of(test_file).unlink(missing_ok=True)
    mark_failed(test_file, True)


def mark_failed(test_file, bad):
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    names = failed()
    names = names | {test_file} if bad else names - {test_file}
    (DIRECTORY / "failed.json").write_text(json.dumps(sorted(names)) + "\n")


def durations():
    try:
        return json.loads((DIRECTORY / "durations.json").read_text())
    except (OSError, ValueError):
        return {}


def save_durations(seen):
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    now = durations()
    now.update({name: round(value, 1) for name, value in seen.items()})
    (DIRECTORY / "durations.json").write_text(json.dumps(dict(sorted(now.items())), indent=1) + "\n")
