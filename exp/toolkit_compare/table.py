"""Print the comparison tables from raw rig output.  usage: table.py RAW.json [RAW.json ...]"""

import json
import sys

ORDER = ["a2", "egui", "iced", "slint"]
LABEL = {"a2": "reference (hand-drawn)", "egui": "egui", "iced": "iced", "slint": "slint"}


def rows(raw):
    out = {}
    for cand, exps in raw["candidates"].items():
        for exp, body in exps.items():
            for m in body.get("measurements", []):
                out.setdefault((exp, m["metric"], m.get("statistic")), {})[cand] = m
    return out


def fmt(m):
    if m is None:
        return "-"
    cls = m.get("classification")
    return f"{m['value']:.2f}" + (f" ({cls})" if cls and cls != "reported" else "")


for path in sys.argv[1:]:
    raw = json.load(open(path))
    print(f"\n## {path}\n")
    print("| experiment | metric | statistic | " + " | ".join(LABEL[c] for c in ORDER) + " |")
    print("|---|---|---|" + "---|" * len(ORDER))
    for (exp, metric, stat), by in rows(raw).items():
        print(f"| {exp} | {metric} | {stat} | " + " | ".join(fmt(by.get(c)) for c in ORDER) + " |")
    print()
    for cand in ORDER:
        body = raw["candidates"].get(cand, {})
        inv = body.get("inventory", {}).get("observations", {})
        kor = body.get("korean_ime", {}).get("observations", {})
        runs = {k: v.get("exact") for k, v in kor.get("runs", {}).items()}
        print(f"{cand}: binary_bytes={inv.get('binary_bytes')} mapped_files={inv.get('mapped_file_count')} "
              f"mapped_bytes={inv.get('mapped_file_bytes_total')} korean_exact={kor.get('all_exact')} runs={runs}")
