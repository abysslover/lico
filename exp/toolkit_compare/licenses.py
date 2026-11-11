"""List the crates of one experiment for the Linux target with the license each declares (from the locked dependency graph).

usage: licenses.py NAME   (NAME in egui, iced, slint)  -> prints a summary and writes data/licenses_NAME.txt
Needs CARGO_HOME to point at the crate cache (offline) or the network.
"""

from __future__ import annotations

import collections
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
PERMISSIVE = ("MIT", "Apache-2.0", "Zlib", "ISC", "BSD-2-Clause", "BSD-3-Clause", "Unlicense", "0BSD", "CC0-1.0", "MPL-2.0",
              "Unicode-3.0", "Unicode-DFS-2016", "BSL-1.0", "OFL-1.1", "LicenseRef-UFL-1.0", "Ubuntu-font-1.0", "NCSA")


def main() -> int:
    name = sys.argv[1]
    env = dict(os.environ, CARGO_HOME=os.environ.get("CARGO_HOME", str(REPO / "private" / "cargo_home")))
    manifest = REPO / ("src/host/hostd/Cargo.toml" if name == "reference" else f"exp/toolkit_{name}/Cargo.toml")
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", "x86_64-unknown-linux-gnu",
         "--manifest-path", str(manifest)], capture_output=True, text=True, env=env, check=True).stdout
    meta = json.loads(out)
    # only crates that are reachable from the root through normal (non-dev, non-build-only) edges are shipped in the binary;
    # build-only crates (proc macros, code generators) are listed separately
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    packages = {p["id"]: p for p in meta["packages"]}
    root = meta["resolve"]["root"]
    shipped, seen = set(), set()
    stack = [root]
    while stack:
        current = stack.pop()
        if current in shipped:
            continue
        shipped.add(current)
        for dep in nodes[current]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    rows = []
    for pid in sorted(shipped, key=lambda p: packages[p]["name"]):
        pkg = packages[pid]
        rows.append((pkg["name"], pkg["version"], pkg.get("license") or pkg.get("license_file") or "UNDECLARED"))
    total = len(meta["packages"]) - 0
    lines = [f"# {name}: {len(rows)} crates reach the binary through normal dependencies (including the root crate); "
             f"{total} packages are in the resolved graph for the target (build-time tools included)"]
    counts = collections.Counter(r[2] for r in rows)
    lines.append("# license expressions: " + "; ".join(f"{k} x{v}" for k, v in counts.most_common()))
    flagged = [r for r in rows if not any(tok in PERMISSIVE for tok in r[2].replace("(", " ").replace(")", " ").split())]
    lines.append("# crates whose expression names no known permissive license: " + (", ".join(f"{r[0]} {r[1]} [{r[2]}]" for r in flagged) or "none"))
    lines += [f"{n} {v}\t{lic}" for n, v, lic in rows]
    text = "\n".join(lines) + "\n"
    (HERE / "data").mkdir(exist_ok=True)
    (HERE / "data" / f"licenses_{name}.txt").write_text(text)
    print("\n".join(lines[:3]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
