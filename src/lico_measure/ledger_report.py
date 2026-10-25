"""The ledger of a test run as a table: python -m lico_measure.ledger_report LEDGER.json [TOP]

The class of a test is read from what its process tree held, not from its name:
  A   acceptance: a build or a clone ran (cargo, git)
  L3  end to end on a real X server: an Xvfb and a window or a candidate client
  L2  a window client without a display (none exists: the window needs X)
  L1  protocol: a host or another program, no display
  L0  unit: no process besides pytest"""
import json
import sys
from collections import defaultdict


def classify(kinds):
    names = set(kinds)
    if names & {"cargo", "git", "rustc"}:
        return "A"
    if "Xvfb" in names:
        return "L3"
    if any(k.startswith("lico gui") or k.startswith("lico-candidate") for k in names):
        return "L2"
    return "L1" if names else "L0"


def table(document, top=15):
    tests, total = document["tests"], document["total_s"]
    lines = [f"{len(tests)} tests, {total} s in all (collection {document['collect_s']} s), peak {document['peak_rss_mib']} MiB at {document['peak_at']}", ""]
    lines += ["| s | share | peak MiB | class | test |", "|---:|---:|---:|---|---|"]
    for t in tests[:top]:
        lines.append(f"| {t['wall_s']:.1f} | {100 * t['wall_s'] / total:.1f} % | {t['peak_rss_mib']:.0f} | {classify(t['kinds'])} | {t['nodeid'].removeprefix('tests/')[:90]} |")
    by = defaultdict(lambda: [0, 0.0, 0.0])
    for t in tests:
        row = by[classify(t["kinds"])]
        row[0] += 1
        row[1] += t["wall_s"]
        row[2] = max(row[2], t["peak_rss_mib"])
    lines += ["", "| class | tests | s | share | max peak MiB |", "|---|---:|---:|---:|---:|"]
    for name in ("A", "L3", "L2", "L1", "L0"):
        if name in by:
            n, s, m = by[name]
            lines.append(f"| {name} | {n} | {s:.1f} | {100 * s / total:.1f} % | {m:.0f} |")
    files = defaultdict(lambda: [0, 0.0])
    for t in tests:
        row = files[t["nodeid"].split("::")[0].removeprefix("tests/")]
        row[0] += 1
        row[1] += t["wall_s"]
    lines += ["", "| file | tests | s |", "|---|---:|---:|"]
    lines += [f"| {name} | {n} | {s:.1f} |" for name, (n, s) in sorted(files.items(), key=lambda kv: -kv[1][1])[:12]]
    return "\n".join(lines)


if __name__ == "__main__":
    print(table(json.load(open(sys.argv[1])), int(sys.argv[2]) if len(sys.argv) > 2 else 15))
